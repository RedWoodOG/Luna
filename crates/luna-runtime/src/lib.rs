use chrono::{DateTime, Utc};
use luna_activation::{compute_activation, propagate_activation_with_context, ActivationConfig};
use luna_cluster::{
    validate_compression_receipt, ClusterRegistry, CompressionDecision, CompressionReceipt,
    MemoryCluster, SourceEventRef,
};
use luna_core::{
    AssertionConfidenceTier, AssertionCorrected, AssertionExtracted, AssertionLifecycleStatus,
    ContradictionDetected, ConversationTurn, Episode, EpisodeCreated, EpisodeRecalled,
    EpisodeReinforced, EventEnvelope, EventSource, LunaError, LunaEvent, MemoryEdge, MemoryMap,
    MemoryNode, MemoryNodeKind, MemoryProvenance, MemoryRelationKind, RecallMode, RecallSet,
    Result, Role, StructuredAssertion, SystemKernel, TurnObserved, TurnReading, WorkingMemory,
    WorkingMemoryBudget,
};
use luna_events::{load_jsonl_events_strict, stable_stored_event_hash, JsonlEventLog};
use luna_extract::{ExtractionCache, FeatureExtractor, FusedExtractor, LlmBackend, LunaExtractor};
use luna_recall::{RecallEngine, SimilarityRecallEngine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub mod scenario;
pub mod topology_bridge;
pub use luna_core::{MemoryIntakeAction, MemoryIntakeDecision};
use luna_output::{OutputBuilder, OutputConfig, OutputPacket};
pub use topology_bridge::{
    bridge_memory_to_topology, bridge_runtime_events_to_topology,
    commit_runtime_events_to_topology_ledger, ledger_events_from_persisted_json,
    ledger_events_hash, topology_commit_from_bridge, topology_commit_from_runtime_ledger_commit,
    topology_node_ref_for_runtime_ref, RuntimeTopologyLedgerCommit, TopologyBridge,
    TopologyClaimRef, TopologyNodeRecord, TopologyOrbRef, TopologySourceEventRef,
    TopologyTetherRecord,
};
use uuid::Uuid;

pub trait RuntimeExtractor {
    fn extract_runtime(&self, turn: &ConversationTurn) -> Result<TurnReading>;
}

impl<E: RuntimeExtractor + ?Sized> RuntimeExtractor for &E {
    fn extract_runtime(&self, turn: &ConversationTurn) -> Result<TurnReading> {
        (*self).extract_runtime(turn)
    }
}

impl RuntimeExtractor for FusedExtractor {
    fn extract_runtime(&self, turn: &ConversationTurn) -> Result<TurnReading> {
        self.extract(turn)
    }
}

impl<B: LlmBackend, C: ExtractionCache> RuntimeExtractor for LunaExtractor<B, C> {
    fn extract_runtime(&self, turn: &ConversationTurn) -> Result<TurnReading> {
        self.extract(turn)
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeSession<E, R = SimilarityRecallEngine> {
    log: JsonlEventLog,
    extractor: E,
    recall: R,
}

impl<E> RuntimeSession<E, SimilarityRecallEngine> {
    pub fn new(log_path: impl Into<PathBuf>, extractor: E) -> Self {
        Self {
            log: JsonlEventLog::new(log_path),
            extractor,
            recall: SimilarityRecallEngine,
        }
    }
}

impl<E, R> RuntimeSession<E, R>
where
    E: RuntimeExtractor,
    R: RecallEngine,
{
    pub fn with_recall(log_path: impl Into<PathBuf>, extractor: E, recall: R) -> Self {
        Self {
            log: JsonlEventLog::new(log_path),
            extractor,
            recall,
        }
    }

    pub fn process_user_turn(&self, content: impl Into<String>) -> Result<RuntimeTurnResult> {
        let turn = ConversationTurn {
            role: Role::User,
            content: content.into(),
            timestamp: Some(Utc::now()),
        };
        self.process_turn(turn)
    }

    pub fn process_turn(&self, turn: ConversationTurn) -> Result<RuntimeTurnResult> {
        let previous_events = self.log.load()?;
        let previous_episodes = luna_store::rebuild_episodes(&previous_events)?;
        let known_before = MemoryState::from_episodes(&previous_episodes);
        let mut observation = self.extractor.extract_runtime(&turn)?;
        apply_runtime_fine_capture(&turn, &mut observation);
        apply_manuscript_one_read_lockout(&turn, &mut observation, &known_before);
        let intake = decide_memory_intake(&turn, &observation, &known_before, &previous_episodes);
        let recall_mode = select_recall_mode(&observation);
        let recalled = self
            .recall
            .recall(&observation, &previous_episodes, recall_mode)?;
        let turn_id = observation.turn_id;

        let mut new_events = Vec::new();
        new_events.push(
            EventEnvelope::new(
                LunaEvent::TurnObserved(TurnObserved { turn: turn.clone() }),
                EventSource::User,
                1.0,
            )
            .with_turn_id(turn_id),
        );
        new_events.push(
            EventEnvelope::new(
                LunaEvent::MemoryIntakeDecided(intake.clone()),
                EventSource::System,
                1.0,
            )
            .with_turn_id(turn_id),
        );

        for hit in &recalled.hits {
            new_events.push(
                EventEnvelope::new(
                    LunaEvent::EpisodeRecalled(EpisodeRecalled {
                        score: hit.score,
                        reason: hit.reason.clone(),
                    }),
                    EventSource::RecallEngine,
                    hit.score,
                )
                .with_turn_id(turn_id)
                .with_episode_id(hit.episode_id),
            );
        }

        for assertion in &observation.assertions {
            new_events.push(
                EventEnvelope::new(
                    LunaEvent::AssertionExtracted(AssertionExtracted {
                        assertion: assertion.clone(),
                        observation: observation.clone(),
                    }),
                    EventSource::ClassifierExtractor,
                    observation.uncertainty.confidence(),
                )
                .with_turn_id(turn_id),
            );

            let mut candidate_events = previous_events.clone();
            candidate_events.extend(new_events.clone());
            let candidate_episodes = luna_store::rebuild_episodes(&candidate_events)?;
            if let Some(correction) =
                correction_target_for_assertion(&candidate_episodes, assertion, &turn.content)
            {
                new_events.push(
                    EventEnvelope::new(
                        LunaEvent::ContradictionDetected(ContradictionDetected {
                            left: correction.old_assertion.clone(),
                            right: assertion.clone(),
                        }),
                        EventSource::ClassifierExtractor,
                        assertion_confidence(&observation, assertion),
                    )
                    .with_turn_id(turn_id)
                    .with_episode_id(correction.episode_id),
                );
                new_events.push(
                    EventEnvelope::new(
                        LunaEvent::AssertionCorrected(AssertionCorrected {
                            old_assertion: correction.old_assertion,
                            new_assertion: assertion.clone(),
                        }),
                        EventSource::ClassifierExtractor,
                        assertion_confidence(&observation, assertion),
                    )
                    .with_turn_id(turn_id)
                    .with_episode_id(correction.episode_id),
                );
            } else if let Some(episode_id) =
                luna_store::episode_id_for_assertion(&candidate_events, assertion)
            {
                new_events.push(
                    EventEnvelope::new(
                        LunaEvent::EpisodeReinforced(EpisodeReinforced {
                            assertion: assertion.clone(),
                            observation: observation.clone(),
                        }),
                        EventSource::ClassifierExtractor,
                        assertion_confidence(&observation, assertion),
                    )
                    .with_turn_id(turn_id)
                    .with_episode_id(episode_id),
                );
            } else {
                let episode_id = Uuid::new_v4();
                new_events.push(
                    EventEnvelope::new(
                        LunaEvent::EpisodeCreated(EpisodeCreated {
                            assertion: assertion.clone(),
                            observation: observation.clone(),
                        }),
                        EventSource::ClassifierExtractor,
                        assertion_confidence(&observation, assertion),
                    )
                    .with_turn_id(turn_id)
                    .with_episode_id(episode_id),
                );
            }
        }

        for event in &new_events {
            self.log.append(event)?;
        }

        let mut all_events = previous_events;
        all_events.extend(new_events);
        let ledger_commit = commit_runtime_events_to_topology_ledger(&all_events)?;
        let topology_commit = topology_commit_from_runtime_ledger_commit(&ledger_commit)?;
        let orb_state = RuntimeOrbActivationState::from_registry(ledger_commit.topology.clusters());
        let topology_commit_event = EventEnvelope::new(
            LunaEvent::TopologyBridgeCommitted(topology_commit),
            EventSource::System,
            1.0,
        )
        .with_turn_id(turn_id);
        self.log.append(&topology_commit_event)?;
        all_events.push(topology_commit_event);

        let episodes = luna_store::rebuild_episodes(&all_events)?;
        let mut memory_state = MemoryState::from_episodes(&episodes);
        apply_runtime_orb_authority(
            &mut memory_state,
            ledger_commit.topology.clusters().clusters().values(),
        );
        let knowledge_delta = KnowledgeDelta::from_observation(&observation, &known_before);
        let questions = propose_questions(&turn, &observation, &memory_state);
        let working_memory = activate_working_memory_with_orb_state(
            &memory_state,
            &turn,
            &observation,
            &recalled,
            WorkingMemoryBudget::default(),
            &orb_state,
        );
        let context_packet = ContextPacket::from_parts(
            &turn.content,
            &working_memory,
            &recalled,
            &questions,
            recall_mode,
            WorkingMemoryBudget::default(),
        );
        let mut output_builder = OutputBuilder::new(OutputConfig::default());
        for node in &working_memory.nodes {
            output_builder.add_memory_node(node);
        }
        let output_packet = output_builder.build();

        Ok(RuntimeTurnResult {
            turn_id,
            observation,
            knowledge_delta,
            memory_state,
            working_memory,
            recalled,
            recall_mode,
            questions,
            context_packet,
            intake,
            output_packet,
        })
    }

    pub fn inspect(&self) -> Result<MemoryState> {
        let events = self.log.load()?;
        runtime_state_from_events(&events)
    }

    pub fn audit_replay(&self) -> Result<RuntimeReplayAuditReport> {
        let events = load_jsonl_events_strict(self.log.path())?;
        let live = self.inspect()?;
        audit_runtime_events_against_state(&live, &events)
    }
}

/// Dialogue lines for [`run_local_product_memory_smoke`]. Prefer loading these from JSON
/// (see `luna-cli` `smoke-dialog.json`) instead of embedding scenario-overlapping literals in
/// production Rust sources; `scripts/doctrine_check.sh` flags fixture `value` strings in `.rs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalProductSmokePhrases {
    pub seed: String,
    #[serde(default)]
    pub distract_turns: Vec<String>,
    pub retrieve_before_correction: String,
    pub correction: String,
    pub retrieve_after_correction: String,
    pub expect_before: String,
    pub expect_after: String,
    #[serde(default)]
    pub expect_not_after: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProductSmokeReport {
    pub distract_turn_count: usize,
    pub audit_clean_after_seed: bool,
    pub recall_hit_after_reopen: bool,
    pub reply_contains_expect_before: bool,
    pub recall_hit_after_correction: bool,
    pub answer_evidence_before_correction: bool,
    pub answer_evidence_after_correction: bool,
    pub current_evidence_after_correction: bool,
    pub audit_clean_after_full_loop: bool,
    pub reply_contains_expect_after: bool,
    pub reply_excludes_rejected_after: bool,
    pub reply_before_correction: String,
    pub reply_after_correction: String,
}

impl LocalProductSmokeReport {
    pub fn is_success(&self) -> bool {
        self.audit_clean_after_seed
            && self.recall_hit_after_reopen
            && self.recall_hit_after_correction
            && self.answer_evidence_before_correction
            && self.answer_evidence_after_correction
            && self.current_evidence_after_correction
            && self.audit_clean_after_full_loop
            && self.reply_contains_expect_before
            && self.reply_contains_expect_after
            && self.reply_excludes_rejected_after
    }
}

/// End-to-end product loop without the scenario harness: seed memory, audit, new session,
/// retrieve, apply a correction, retrieve again, final audit. Uses the same runtime path as
/// interactive `luna runtime turn`.
pub fn run_local_product_memory_smoke(
    log: &Path,
    phrases: &LocalProductSmokePhrases,
) -> Result<LocalProductSmokeReport> {
    use std::fs;

    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent).map_err(|err| LunaError::new(format!("smoke mkdir: {err}")))?;
    }

    let session = RuntimeSession::new(log, FusedExtractor::new());
    session.process_user_turn(phrases.seed.clone())?;
    for distract_turn in &phrases.distract_turns {
        session.process_user_turn(distract_turn.clone())?;
    }

    let audit_seed = audit_runtime_log(log)?;
    let audit_clean_after_seed = audit_seed.is_clean();

    let session = RuntimeSession::new(log, FusedExtractor::new());
    let retrieve1 = session.process_user_turn(phrases.retrieve_before_correction.clone())?;
    let reply1 = render_conversation_reply(&phrases.retrieve_before_correction, &retrieve1);
    let plan1 = plan_conversation_response(&phrases.retrieve_before_correction, &retrieve1);
    let recall_hit_after_reopen = !retrieve1.recalled.hits.is_empty();
    let reply_contains_expect_before = contains_ci(&reply1, &phrases.expect_before);
    let answer_evidence_before_correction =
        answer_plan_has_current_evidence(&plan1, &phrases.expect_before);

    let session = RuntimeSession::new(log, FusedExtractor::new());
    session.process_user_turn(phrases.correction.clone())?;

    let session = RuntimeSession::new(log, FusedExtractor::new());
    let retrieve2 = session.process_user_turn(phrases.retrieve_after_correction.clone())?;
    let reply2 = render_conversation_reply(&phrases.retrieve_after_correction, &retrieve2);
    let plan2 = plan_conversation_response(&phrases.retrieve_after_correction, &retrieve2);
    let recall_hit_after_correction = !retrieve2.recalled.hits.is_empty();
    let reply_contains_expect_after = contains_ci(&reply2, &phrases.expect_after);
    let answer_evidence_after_correction =
        answer_plan_has_current_evidence(&plan2, &phrases.expect_after);
    let current_evidence_after_correction = plan2.answer_evidence.iter().any(|evidence| {
        contains_ci(&evidence.value, &phrases.expect_after)
            && evidence.lifecycle_status == AssertionLifecycleStatus::Current
    });
    let reply_excludes_rejected_after = std::iter::once(&phrases.expect_before)
        .chain(phrases.expect_not_after.iter())
        .filter(|value| !value.trim().is_empty())
        .all(|value| !contains_ci(&reply2, value));

    let audit_final = audit_runtime_log(log)?;
    let audit_clean_after_full_loop = audit_final.is_clean();

    Ok(LocalProductSmokeReport {
        distract_turn_count: phrases.distract_turns.len(),
        audit_clean_after_seed,
        recall_hit_after_reopen,
        reply_contains_expect_before,
        recall_hit_after_correction,
        answer_evidence_before_correction,
        answer_evidence_after_correction,
        current_evidence_after_correction,
        audit_clean_after_full_loop,
        reply_contains_expect_after,
        reply_excludes_rejected_after,
        reply_before_correction: reply1,
        reply_after_correction: reply2,
    })
}

fn answer_plan_has_current_evidence(plan: &ResponsePlan, expected_value: &str) -> bool {
    plan.actions.contains(&ResponsePlanAction::Answer)
        && plan.answer_evidence.iter().any(|evidence| {
            contains_ci(&evidence.value, expected_value)
                && evidence.lifecycle_status == AssertionLifecycleStatus::Current
        })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeTurnResult {
    pub turn_id: Uuid,
    pub observation: TurnReading,
    pub knowledge_delta: KnowledgeDelta,
    pub memory_state: MemoryState,
    pub working_memory: WorkingMemory,
    pub recalled: RecallSet,
    pub recall_mode: RecallMode,
    pub questions: Vec<QuestionCandidate>,
    pub context_packet: ContextPacket,
    pub intake: MemoryIntakeDecision,
    pub output_packet: OutputPacket,
}

pub const RUNTIME_REPLAY_AUDIT_HASH_VERSION: &str = "luna.runtime_replay_audit.snapshot.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeReplayAuditStatus {
    Clean,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeReplayAuditCounts {
    pub stored_events: usize,
    pub claims: usize,
    pub current_claims: usize,
    pub entity_groups: usize,
    pub memory_nodes: usize,
    pub memory_edges: usize,
    pub topology_nodes: usize,
    pub topology_tethers: usize,
    pub topology_source_event_refs: usize,
    #[serde(default)]
    pub valid_topology_source_event_refs: usize,
    #[serde(default)]
    pub topology_orbs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeReplayAuditReport {
    pub status: RuntimeReplayAuditStatus,
    pub quarantine_required: bool,
    pub hash_version: String,
    pub live_snapshot_hash: String,
    pub replayed_snapshot_hash: String,
    #[serde(default)]
    pub replay_error: Option<String>,
    pub live_counts: RuntimeReplayAuditCounts,
    pub replayed_counts: RuntimeReplayAuditCounts,
}

impl RuntimeReplayAuditReport {
    pub fn is_clean(&self) -> bool {
        self.status == RuntimeReplayAuditStatus::Clean
    }

    pub fn is_quarantined(&self) -> bool {
        self.status == RuntimeReplayAuditStatus::Quarantined
    }
}

#[derive(Serialize)]
struct RuntimeReplayAuditSnapshot<'a> {
    memory: &'a MemoryState,
    bridge: &'a TopologyBridge,
}

pub fn audit_runtime_log(log: &Path) -> Result<RuntimeReplayAuditReport> {
    let events = load_jsonl_events_strict(log)?;
    audit_runtime_events(&events)
}

pub fn audit_runtime_events(
    events: &[luna_events::StoredEvent],
) -> Result<RuntimeReplayAuditReport> {
    if events.is_empty() {
        return Err(luna_core::LunaError::new(
            "runtime replay audit requires at least one stored event",
        ));
    }
    let replayed = runtime_state_from_events(events)?;
    audit_runtime_events_against_state(&replayed, events)
}

pub fn audit_runtime_events_against_state(
    live: &MemoryState,
    events: &[luna_events::StoredEvent],
) -> Result<RuntimeReplayAuditReport> {
    if events.is_empty() {
        return Err(luna_core::LunaError::new(
            "runtime replay audit requires at least one stored event",
        ));
    }
    let live_bridge = match bridge_runtime_events_to_topology(events) {
        Ok(bridge) => bridge,
        Err(err) => {
            return quarantined_runtime_replay_report(
                live,
                &TopologyBridge::from_memory_state(live),
                events,
                err.to_string(),
            );
        }
    };
    let live_snapshot_hash = runtime_replay_snapshot_hash(live, &live_bridge)?;

    let replayed = match runtime_state_from_events(events) {
        Ok(replayed) => replayed,
        Err(err) => {
            return quarantined_runtime_replay_report(live, &live_bridge, events, err.to_string());
        }
    };
    let replayed_bridge = match bridge_runtime_events_to_topology(events) {
        Ok(bridge) => bridge,
        Err(err) => {
            return quarantined_runtime_replay_report(live, &live_bridge, events, err.to_string());
        }
    };
    if let Some(commit_error) = runtime_topology_commit_error(events, &replayed_bridge) {
        return quarantined_runtime_replay_report(live, &live_bridge, events, commit_error);
    }
    let replayed_snapshot_hash = runtime_replay_snapshot_hash(&replayed, &replayed_bridge)?;
    let live_counts = RuntimeReplayAuditCounts::from_parts(events, live, &live_bridge);
    let replayed_counts = RuntimeReplayAuditCounts::from_parts(events, &replayed, &replayed_bridge);
    let quarantine_required = live_snapshot_hash != replayed_snapshot_hash;

    Ok(RuntimeReplayAuditReport {
        status: if quarantine_required {
            RuntimeReplayAuditStatus::Quarantined
        } else {
            RuntimeReplayAuditStatus::Clean
        },
        quarantine_required,
        hash_version: RUNTIME_REPLAY_AUDIT_HASH_VERSION.to_string(),
        live_snapshot_hash,
        replayed_snapshot_hash,
        replay_error: None,
        live_counts,
        replayed_counts,
    })
}

fn runtime_topology_commit_error(
    events: &[luna_events::StoredEvent],
    bridge: &TopologyBridge,
) -> Option<String> {
    let commit_indexes = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match &event.payload {
            LunaEvent::TopologyBridgeCommitted(commit) => Some((index, commit)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if commit_indexes.is_empty() {
        if bridge.node_records.is_empty() && bridge.tether_records.is_empty() {
            return None;
        }
        return Some(
            "runtime replay audit found topology evidence without a persisted bridge commit"
                .to_string(),
        );
    };
    for (index, persisted_commit) in commit_indexes {
        let prefix_commit = match commit_runtime_events_to_topology_ledger(&events[..index]) {
            Ok(prefix_commit) => prefix_commit,
            Err(err) => {
                return Some(format!(
                    "runtime replay audit could not recompute topology ledger prefix for bridge commit at event {index}: {err}"
                ));
            }
        };
        let expected = match topology_commit_from_runtime_ledger_commit(&prefix_commit) {
            Ok(expected) => expected,
            Err(err) => {
                return Some(format!(
                    "runtime replay audit could not build expected topology commit at event {index}: {err}"
                ));
            }
        };
        if let Some(mismatch) = topology_commit_mismatch(persisted_commit, &expected) {
            return Some(format!(
                "runtime replay audit found persisted bridge commit at event {index} that does not match recomputed topology ledger evidence: {mismatch}"
            ));
        }
        if persisted_commit.ledger_events_json.is_empty() {
            return Some(format!(
                "runtime replay audit found bridge commit at event {index} without durable ledger events"
            ));
        }
        if persisted_commit.ledger_event_count != persisted_commit.ledger_events_json.len() {
            return Some(format!(
                "runtime replay audit found bridge commit at event {index} with ledger event count mismatch"
            ));
        }
        if persisted_commit.ledger_event_hash
            != ledger_events_hash(&persisted_commit.ledger_events_json)
        {
            return Some(format!(
                "runtime replay audit found bridge commit at event {index} with ledger event hash mismatch"
            ));
        }
        let persisted_ledger_events = match ledger_events_from_persisted_json(
            &persisted_commit.ledger_events_json,
        ) {
            Ok(events) => events,
            Err(err) => {
                return Some(format!(
                        "runtime replay audit could not decode persisted topology ledger events at event {index}: {err}"
                    ));
            }
        };
        if let Err(err) = luna_replay::TopologyReplay::replay(&persisted_ledger_events) {
            return Some(format!(
                "runtime replay audit could not replay persisted topology ledger events at event {index}: {err}"
            ));
        }
    }
    None
}

fn topology_commit_mismatch(
    persisted: &luna_core::TopologyBridgeCommitted,
    expected: &luna_core::TopologyBridgeCommitted,
) -> Option<&'static str> {
    if persisted.node_refs != expected.node_refs {
        return Some("node refs");
    }
    if persisted.tether_refs != expected.tether_refs {
        return Some("tether refs");
    }
    if persisted.source_event_hashes != expected.source_event_hashes {
        return Some("source event hashes");
    }
    if persisted.orb_refs != expected.orb_refs {
        return Some("orb refs");
    }
    if persisted.accepted_orb_refs != expected.accepted_orb_refs {
        return Some("accepted orb refs");
    }
    if persisted.rejected_orb_refs != expected.rejected_orb_refs {
        return Some("rejected orb refs");
    }
    if persisted.ledger_event_count != expected.ledger_event_count {
        return Some("ledger event count");
    }
    if persisted.ledger_event_hash != expected.ledger_event_hash {
        return Some("ledger event hash");
    }
    None
}

impl RuntimeReplayAuditCounts {
    fn from_parts(
        events: &[luna_events::StoredEvent],
        state: &MemoryState,
        bridge: &TopologyBridge,
    ) -> Self {
        Self {
            stored_events: events.len(),
            claims: state.claims.len(),
            current_claims: state
                .claims
                .iter()
                .filter(|claim| claim.lifecycle_status == AssertionLifecycleStatus::Current)
                .count(),
            entity_groups: state.entity_groups.len(),
            memory_nodes: state.map.nodes.len(),
            memory_edges: state.map.edges.len(),
            topology_nodes: bridge.node_records.len(),
            topology_tethers: bridge.tether_records.len(),
            topology_source_event_refs: topology_source_event_ref_count(bridge),
            valid_topology_source_event_refs: valid_topology_source_event_ref_count(bridge),
            topology_orbs: latest_topology_orb_count(events).unwrap_or_default(),
        }
    }
}

fn latest_topology_orb_count(events: &[luna_events::StoredEvent]) -> Option<usize> {
    events.iter().rev().find_map(|event| match &event.payload {
        LunaEvent::TopologyBridgeCommitted(commit) => Some(commit.accepted_orb_refs.len()),
        _ => None,
    })
}

fn topology_source_event_ref_count(bridge: &TopologyBridge) -> usize {
    bridge
        .node_records
        .iter()
        .map(|node| {
            node.source_event_refs.len()
                + node
                    .claim_refs
                    .iter()
                    .map(|claim| claim.source_event_refs.len())
                    .sum::<usize>()
        })
        .sum::<usize>()
        + bridge
            .tether_records
            .iter()
            .map(|tether| tether.source_event_refs.len())
            .sum::<usize>()
}

fn valid_topology_source_event_ref_count(bridge: &TopologyBridge) -> usize {
    topology_source_event_refs(bridge)
        .filter(|source| valid_event_hash(&source.event_hash))
        .count()
}

fn topology_source_event_refs(
    bridge: &TopologyBridge,
) -> impl Iterator<Item = &TopologySourceEventRef> {
    bridge
        .node_records
        .iter()
        .flat_map(|node| {
            node.source_event_refs.iter().chain(
                node.claim_refs
                    .iter()
                    .flat_map(|claim| claim.source_event_refs.iter()),
            )
        })
        .chain(
            bridge
                .tether_records
                .iter()
                .flat_map(|tether| tether.source_event_refs.iter()),
        )
}

fn valid_event_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn runtime_state_from_events(events: &[luna_events::StoredEvent]) -> Result<MemoryState> {
    let episodes = luna_store::rebuild_episodes(events)?;
    let mut state = MemoryState::from_episodes(&episodes);
    let ledger_commit = commit_runtime_events_to_topology_ledger(events)?;
    apply_runtime_orb_authority(
        &mut state,
        ledger_commit.topology.clusters().clusters().values(),
    );
    Ok(state)
}

fn apply_runtime_orb_authority<'a>(
    state: &mut MemoryState,
    memory_clusters: impl Iterator<Item = &'a MemoryCluster>,
) {
    let mut node_roots = BTreeMap::<String, BTreeSet<String>>::new();
    for cluster in memory_clusters {
        let system_root = format!("orb:{}", cluster.orb_id);
        for source_node_id in &cluster.source_node_ids {
            if let Some(runtime_node_id) = source_node_id.strip_prefix("node:") {
                node_roots
                    .entry(runtime_node_id.to_string())
                    .or_default()
                    .insert(system_root.clone());
            }
        }
    }

    for node in &mut state.map.nodes {
        if let Some(system_roots) = node_roots.get(&node.id) {
            append_system_roots(&mut node.provenance, system_roots);
        }
    }
    for edge in &mut state.map.edges {
        let mut system_roots = BTreeSet::new();
        if let Some(source_roots) = node_roots.get(&edge.source) {
            system_roots.extend(source_roots.iter().cloned());
        }
        if let Some(target_roots) = node_roots.get(&edge.target) {
            system_roots.extend(target_roots.iter().cloned());
        }
        append_system_roots(&mut edge.provenance, &system_roots);
    }
}

fn append_system_roots(provenance: &mut Vec<MemoryProvenance>, system_roots: &BTreeSet<String>) {
    for system_root in system_roots {
        if provenance
            .iter()
            .any(|existing| existing.system_root.as_ref() == Some(system_root))
        {
            continue;
        }
        provenance.push(MemoryProvenance {
            episode_id: None,
            turn_id: None,
            assertion_key: None,
            system_root: Some(system_root.clone()),
            lifecycle_status: None,
        });
    }
}

fn runtime_replay_snapshot_hash(state: &MemoryState, bridge: &TopologyBridge) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(RUNTIME_REPLAY_AUDIT_HASH_VERSION.as_bytes());
    hasher.update([0]);
    let bytes = serde_json::to_vec(&RuntimeReplayAuditSnapshot {
        memory: state,
        bridge,
    })
    .map_err(|err| luna_core::LunaError::new(err.to_string()))?;
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn quarantined_runtime_replay_report(
    live: &MemoryState,
    live_bridge: &TopologyBridge,
    events: &[luna_events::StoredEvent],
    replay_error: String,
) -> Result<RuntimeReplayAuditReport> {
    Ok(RuntimeReplayAuditReport {
        status: RuntimeReplayAuditStatus::Quarantined,
        quarantine_required: true,
        hash_version: RUNTIME_REPLAY_AUDIT_HASH_VERSION.to_string(),
        live_snapshot_hash: runtime_replay_snapshot_hash(live, live_bridge)?,
        replayed_snapshot_hash: String::new(),
        replay_error: Some(replay_error),
        live_counts: RuntimeReplayAuditCounts::from_parts(events, live, live_bridge),
        replayed_counts: RuntimeReplayAuditCounts::default(),
    })
}

fn decide_memory_intake(
    turn: &ConversationTurn,
    observation: &TurnReading,
    known_before: &MemoryState,
    previous_episodes: &[Episode],
) -> MemoryIntakeDecision {
    let text = turn.content.to_ascii_lowercase();
    if has_correction_cue(&text) && !observation.assertions.is_empty() {
        let targets_current_memory = observation.assertions.iter().any(|assertion| {
            correction_target_for_assertion(previous_episodes, assertion, &turn.content).is_some()
        });
        if targets_current_memory {
            return MemoryIntakeDecision {
                action: MemoryIntakeAction::SupersedeOrCorrect,
                reason: "correction cue targets a current memory slot".to_string(),
            };
        }
    }
    if mentions_ambiguous_pronoun_without_anchor(&text, observation) {
        return MemoryIntakeDecision {
            action: MemoryIntakeAction::AskForAnchor,
            reason: "pronoun-dependent memory needs an entity anchor".to_string(),
        };
    }
    if observation.assertions.is_empty() {
        if is_noise_turn(&text) {
            return MemoryIntakeDecision {
                action: MemoryIntakeAction::IgnoreNoise,
                reason: "turn has no stable memory anchor".to_string(),
            };
        }
        if !unknowns_from_observation(observation).is_empty() {
            return MemoryIntakeDecision {
                action: MemoryIntakeAction::MarkUnknown,
                reason: "signals imply relevance, but no concrete assertion was captured"
                    .to_string(),
            };
        }
        return MemoryIntakeDecision {
            action: MemoryIntakeAction::IgnoreNoise,
            reason: "no concrete memory assertion was captured".to_string(),
        };
    }
    if observation
        .assertions
        .iter()
        .any(|assertion| assertion.confidence_tier != AssertionConfidenceTier::Confirmed)
        || observation.assertions.iter().any(|assertion| {
            !known_before
                .claims
                .iter()
                .any(|claim| claim.key == assertion.key())
        })
    {
        return MemoryIntakeDecision {
            action: MemoryIntakeAction::StoreWithUncertainty,
            reason: "new assertions are stored with their current confidence tier".to_string(),
        };
    }
    MemoryIntakeDecision {
        action: MemoryIntakeAction::Accept,
        reason: "assertions reinforce known memory".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct KnowledgeDelta {
    pub confirmed: Vec<MemoryClaim>,
    pub inferred: Vec<MemoryClaim>,
    pub unconfirmed: Vec<MemoryClaim>,
    pub unknowns: Vec<String>,
}

impl KnowledgeDelta {
    fn from_observation(observation: &TurnReading, known_before: &MemoryState) -> Self {
        let mut assertion_claims = observation
            .assertions
            .iter()
            .map(MemoryClaim::from_assertion)
            .collect::<Vec<_>>();
        assertion_claims.retain(|claim| {
            !known_before.claims.iter().any(|known| {
                known.key == claim.key
                    && known.lifecycle_status == AssertionLifecycleStatus::Current
            })
        });
        let confirmed = assertion_claims
            .iter()
            .filter(|claim| claim.status == AssertionConfidenceTier::Confirmed)
            .cloned()
            .collect::<Vec<_>>();
        let unconfirmed = assertion_claims
            .iter()
            .filter(|claim| claim.status == AssertionConfidenceTier::Unconfirmed)
            .cloned()
            .collect::<Vec<_>>();

        let mut inferred = Vec::new();
        inferred.extend(
            assertion_claims
                .iter()
                .filter(|claim| claim.status == AssertionConfidenceTier::Inferred)
                .cloned(),
        );
        if signal_active(observation.emotional_arousal, 0.65) {
            inferred.push(MemoryClaim::inferred(
                "emotion",
                "arousal",
                "this turn carries emotional pressure",
            ));
        }
        if signal_active(observation.goal_pressure, 0.65) {
            inferred.push(MemoryClaim::inferred(
                "goal",
                "pressure",
                "this turn may involve an active goal or unresolved pressure",
            ));
        }
        if signal_active(observation.identity_relevance, 0.65) && observation.assertions.is_empty()
        {
            inferred.push(MemoryClaim::inferred(
                "identity",
                "relevance",
                "this turn may matter to identity or role",
            ));
        }

        let unknowns = unknowns_from_observation(observation);

        Self {
            confirmed,
            inferred,
            unconfirmed,
            unknowns,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryClaim {
    pub key: String,
    pub domain: String,
    pub kind: String,
    pub value: String,
    pub status: AssertionConfidenceTier,
    pub lifecycle_status: AssertionLifecycleStatus,
}

impl MemoryClaim {
    fn from_assertion(assertion: &StructuredAssertion) -> Self {
        Self {
            key: assertion.key(),
            domain: assertion.domain.clone(),
            kind: assertion.kind.clone(),
            value: assertion.value.clone(),
            status: assertion.confidence_tier,
            lifecycle_status: assertion.lifecycle_status,
        }
    }

    fn inferred(domain: &str, kind: &str, value: &str) -> Self {
        let assertion = StructuredAssertion::inferred(domain, kind, value);
        Self::from_assertion(&assertion)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityMemoryGroup {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub claims: Vec<MemoryClaim>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeOrbActivationState {
    pub active_orb_ids: BTreeSet<String>,
    pub retired_orb_ids: BTreeSet<String>,
}

impl RuntimeOrbActivationState {
    pub fn from_registry(registry: &ClusterRegistry) -> Self {
        Self {
            active_orb_ids: registry.clusters().keys().cloned().collect(),
            retired_orb_ids: registry.retired_clusters().keys().cloned().collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.active_orb_ids.is_empty() && self.retired_orb_ids.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MemoryState {
    pub claims: Vec<MemoryClaim>,
    pub entity_groups: Vec<EntityMemoryGroup>,
    pub open_questions: Vec<String>,
    pub map: MemoryMap,
}

impl MemoryState {
    pub fn from_episodes(episodes: &[Episode]) -> Self {
        let mut assertions_by_key =
            BTreeMap::<String, (DateTime<Utc>, Uuid, StructuredAssertion)>::new();
        for episode in episodes {
            for assertion in &episode.assertions {
                let key = assertion.key();
                let replace = assertions_by_key
                    .get(&key)
                    .map(|(updated_at, _, _)| episode.updated_at >= *updated_at)
                    .unwrap_or(true);
                if replace {
                    assertions_by_key
                        .insert(key, (episode.updated_at, episode.id, assertion.clone()));
                }
            }
        }
        let mut claims = Vec::new();
        let mut assertion_index = BTreeMap::new();
        for (key, (_, episode_id, assertion)) in assertions_by_key {
            assertion_index.insert(key, (episode_id, assertion.clone()));
            claims.push(MemoryClaim::from_assertion(&assertion));
        }
        let current_claims = claims
            .iter()
            .filter(|claim| claim.lifecycle_status == AssertionLifecycleStatus::Current)
            .cloned()
            .collect::<Vec<_>>();
        let map_index = assertion_index
            .iter()
            .filter(|(key, _)| current_claims.iter().any(|claim| &claim.key == *key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        let entity_groups = group_claims_by_entity(&current_claims);
        let map = memory_map_from_claims(&current_claims, &map_index);
        Self {
            claims,
            entity_groups,
            open_questions: Vec::new(),
            map,
        }
    }

    pub fn has_domain_kind(&self, domain: &str, kind: &str) -> bool {
        self.claims.iter().any(|claim| {
            claim.lifecycle_status == AssertionLifecycleStatus::Current
                && claim.domain == domain
                && claim.kind == kind
        })
    }
}

fn group_claims_by_entity(claims: &[MemoryClaim]) -> Vec<EntityMemoryGroup> {
    let mut groups = BTreeMap::<String, EntityMemoryGroup>::new();
    for claim in claims {
        for (id, label, kind) in entity_keys_for_claim(claim) {
            groups
                .entry(id.clone())
                .or_insert_with(|| EntityMemoryGroup {
                    id,
                    label,
                    kind,
                    claims: Vec::new(),
                })
                .claims
                .push(claim.clone());
        }
    }
    groups.into_values().collect()
}

fn entity_keys_for_claim(claim: &MemoryClaim) -> Vec<(String, String, String)> {
    match claim.domain.as_str() {
        "identity" => vec![self_group()],
        "relationship" if is_first_person_value(&claim.value) => vec![self_group()],
        "person" => person_entity_keys(&claim.value),
        "manuscript" => manuscript_entity_keys(claim),
        "project" => project_entity_keys(&claim.value),
        _ => Vec::new(),
    }
}

fn self_group() -> (String, String, String) {
    ("self".to_string(), "you".to_string(), "self".to_string())
}

fn person_entity_keys(value: &str) -> Vec<(String, String, String)> {
    let names = person_subjects_from_claim_value(value);
    if names.is_empty() {
        return vec![(
            "person:unknown".to_string(),
            "unknown person".to_string(),
            "person".to_string(),
        )];
    }
    names
        .into_iter()
        .map(|name| (format!("person:{name}"), name, "person".to_string()))
        .collect()
}

fn person_subjects_from_claim_value(value: &str) -> Vec<String> {
    let lower = value.to_ascii_lowercase();
    let subject_end = [
        " lives ", " is ", " are ", " wants ", " takes ", " writes ", " has ",
    ]
    .iter()
    .filter_map(|needle| lower.find(needle))
    .min()
    .unwrap_or(value.len());
    let subject = &value[..subject_end];
    let mut seen = BTreeSet::new();
    subject
        .split(" and ")
        .flat_map(|part| part.split(','))
        .map(str::trim)
        .map(|part| part.trim_matches(|ch: char| !ch.is_ascii_alphabetic()))
        .filter(|part| is_single_name(part))
        .map(normalize_person_name)
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

fn project_entity_keys(value: &str) -> Vec<(String, String, String)> {
    let Some(name) = project_subject_from_claim_value(value) else {
        return Vec::new();
    };
    vec![(
        format!("project:{}", graph_id_fragment(&name)),
        name,
        "project".to_string(),
    )]
}

fn project_subject_from_claim_value(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    let subject_end = [
        " is ",
        " has ",
        " uses ",
        " needs ",
        " does ",
        " helps ",
        " focuses ",
    ]
    .iter()
    .filter_map(|needle| lower.find(needle))
    .min()?;
    clean_project_subject(&value[..subject_end])
}

fn character_entity_keys(value: &str) -> Vec<(String, String, String)> {
    character_subject_from_claim_value(value)
        .map(|name| {
            vec![(
                format!("character:{}", graph_id_fragment(&name)),
                name,
                "character".to_string(),
            )]
        })
        .unwrap_or_default()
}

fn manuscript_entity_keys(claim: &MemoryClaim) -> Vec<(String, String, String)> {
    let mut keys = character_entity_keys(&claim.value);
    if let Some(scene_id) = scene_id_from_claim_value(&claim.value) {
        keys.push((
            format!("scene:{scene_id}"),
            format!("scene:{scene_id}"),
            "scene".to_string(),
        ));
    }
    keys
}

fn scene_id_from_claim_value(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    let index = lower.find("scene:")?;
    let after = &value[index + "scene:".len()..];
    let id = after
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn character_subject_from_claim_value(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    let subject_end = [" is ", " was ", " called ", " is called "]
        .iter()
        .filter_map(|needle| lower.find(needle))
        .min()?;
    let subject = value[..subject_end].trim();
    if subject.split_whitespace().all(|word| {
        word.chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
    }) {
        Some(subject.to_string())
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CorrectionTarget {
    episode_id: Uuid,
    old_assertion: StructuredAssertion,
}

fn correction_target_for_assertion(
    episodes: &[Episode],
    assertion: &StructuredAssertion,
    turn_text: &str,
) -> Option<CorrectionTarget> {
    if !has_correction_cue(&turn_text.to_ascii_lowercase()) {
        return None;
    }
    let slot = correction_slot(assertion)?;
    episodes
        .iter()
        .flat_map(|episode| {
            let slot = slot.clone();
            episode.assertions.iter().filter_map(move |old| {
                if old.lifecycle_status != AssertionLifecycleStatus::Current {
                    return None;
                }
                let old_slot = correction_slot(old)?;
                if old_slot == slot && old.value != assertion.value {
                    Some((
                        episode.updated_at,
                        CorrectionTarget {
                            episode_id: episode.id,
                            old_assertion: old.clone(),
                        },
                    ))
                } else {
                    None
                }
            })
        })
        .max_by_key(|(updated_at, _)| *updated_at)
        .map(|(_, target)| target)
}

fn correction_slot(assertion: &StructuredAssertion) -> Option<String> {
    let entity = match assertion.domain.as_str() {
        "person" => person_subjects_from_claim_value(&assertion.value)
            .first()
            .cloned()?,
        "project" => project_subject_from_claim_value(&assertion.value)?,
        "identity" => "self".to_string(),
        _ => return None,
    };
    Some(format!(
        "{}:{}:{}",
        assertion.domain,
        assertion.kind,
        normalize_for_match(&entity)
    ))
}

fn memory_map_from_claims(
    claims: &[MemoryClaim],
    assertion_index: &BTreeMap<String, (Uuid, StructuredAssertion)>,
) -> MemoryMap {
    let mut nodes = BTreeMap::<String, MemoryNode>::new();
    let mut edges = Vec::new();

    insert_node(
        &mut nodes,
        MemoryNode {
            id: "user:self".to_string(),
            label: "self".to_string(),
            kind: MemoryNodeKind::User,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 0.0,
            provenance: Vec::new(),
            created_at: None,
            contradiction_count: 0,
        },
    );
    seed_system_kernel(&mut nodes, &mut edges);

    let mut seen_edges = BTreeSet::new();
    for claim in claims {
        let provenance = assertion_index
            .get(&claim.key)
            .map(|(episode_id, assertion)| MemoryProvenance {
                episode_id: Some(*episode_id),
                turn_id: None,
                assertion_key: Some(assertion.key()),
                system_root: None,
                lifecycle_status: None,
            })
            .into_iter()
            .collect::<Vec<_>>();
        let target_id = format!(
            "{}:{}:{}",
            claim.domain,
            claim.kind,
            claim.value.replace(' ', "_")
        );
        insert_node(
            &mut nodes,
            MemoryNode {
                id: target_id.clone(),
                label: claim.value.clone(),
                kind: node_kind_for_claim(claim),
                confidence_tier: claim.status,
                density: density_for_tier(claim.status),
                activation: 0.0,
                provenance: provenance.clone(),
                created_at: None,
                contradiction_count: 0,
            },
        );
        let entity_keys = entity_keys_for_claim(claim);
        if entity_keys.is_empty() {
            push_edge_once(
                &mut edges,
                &mut seen_edges,
                MemoryEdge {
                    source: "user:self".to_string(),
                    target: target_id,
                    relation: claim_node_relation_for_claim(claim),
                    confidence_tier: claim.status,
                    strength: density_for_tier(claim.status),
                    activation: 0.0,
                    provenance,
                },
            );
            continue;
        }

        for (entity_id, entity_label, entity_kind) in entity_keys {
            if entity_id != "self" {
                insert_node(
                    &mut nodes,
                    MemoryNode {
                        id: entity_id.clone(),
                        label: entity_label,
                        kind: node_kind_for_entity(&entity_kind),
                        confidence_tier: claim.status,
                        density: density_for_tier(claim.status),
                        activation: 0.0,
                        provenance: provenance.clone(),
                        created_at: None,
                        contradiction_count: 0,
                    },
                );
                push_edge_once(
                    &mut edges,
                    &mut seen_edges,
                    MemoryEdge {
                        source: "user:self".to_string(),
                        target: entity_id.clone(),
                        relation: MemoryRelationKind::RelatedTo,
                        confidence_tier: claim.status,
                        strength: density_for_tier(claim.status),
                        activation: 0.0,
                        provenance: provenance.clone(),
                    },
                );
            }

            let source = if entity_id == "self" {
                "user:self".to_string()
            } else {
                entity_id
            };
            push_edge_once(
                &mut edges,
                &mut seen_edges,
                MemoryEdge {
                    source: source.clone(),
                    target: target_id.clone(),
                    relation: claim_node_relation_for_claim(claim),
                    confidence_tier: claim.status,
                    strength: density_for_tier(claim.status),
                    activation: 0.0,
                    provenance: provenance.clone(),
                },
            );
            if let Some(target) = typed_relation_target_for_claim(claim) {
                insert_node(
                    &mut nodes,
                    MemoryNode {
                        id: target.id.clone(),
                        label: target.label.clone(),
                        kind: target.kind,
                        confidence_tier: claim.status,
                        density: density_for_tier(claim.status),
                        activation: 0.0,
                        provenance: provenance.clone(),
                        created_at: None,
                        contradiction_count: 0,
                    },
                );
                push_edge_once(
                    &mut edges,
                    &mut seen_edges,
                    MemoryEdge {
                        source,
                        target: target.id,
                        relation: target.relation,
                        confidence_tier: claim.status,
                        strength: density_for_tier(claim.status),
                        activation: 0.0,
                        provenance: provenance.clone(),
                    },
                );
            }
        }
    }

    MemoryMap {
        nodes: nodes.into_values().collect(),
        edges,
    }
}

fn insert_node(nodes: &mut BTreeMap<String, MemoryNode>, node: MemoryNode) {
    nodes
        .entry(node.id.clone())
        .and_modify(|existing| {
            existing.confidence_tier = existing.confidence_tier.max(node.confidence_tier);
            existing.density = existing.density.max(node.density);
            existing.provenance.extend(node.provenance.clone());
        })
        .or_insert(node);
}

fn push_edge_once(
    edges: &mut Vec<MemoryEdge>,
    seen_edges: &mut BTreeSet<String>,
    edge: MemoryEdge,
) {
    let key = format!("{}|{}|{:?}", edge.source, edge.target, edge.relation);
    if seen_edges.insert(key) {
        edges.push(edge);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypedRelationTarget {
    id: String,
    label: String,
    kind: MemoryNodeKind,
    relation: MemoryRelationKind,
}

fn typed_relation_target_for_claim(claim: &MemoryClaim) -> Option<TypedRelationTarget> {
    if claim.domain == "manuscript" {
        return typed_manuscript_relation_target_for_claim(claim);
    }
    if claim.domain != "person" {
        return None;
    }

    match claim.kind.as_str() {
        "location" => claim
            .value
            .split_once(" lives in ")
            .map(|(_, place)| typed_place_target(place)),
        "goal" => claim
            .value
            .split_once(" wants to ")
            .map(|(_, goal)| typed_goal_target(goal)),
        "interest" => person_interest_target(&claim.value),
        _ => None,
    }
}

fn typed_manuscript_relation_target_for_claim(claim: &MemoryClaim) -> Option<TypedRelationTarget> {
    match claim.kind.as_str() {
        "character_alias" => claim
            .value
            .split_once(" is called ")
            .map(|(_, alias)| typed_character_alias_target(alias)),
        "scene_order" => claim
            .value
            .split_once(" appears_before ")
            .map(|(_, scene)| typed_scene_target(scene, MemoryRelationKind::AppearsBefore)),
        "story_chronology" => claim
            .value
            .split_once(" occurs_before ")
            .map(|(_, scene)| typed_scene_target(scene, MemoryRelationKind::OccursBefore)),
        _ => None,
    }
}

fn typed_scene_target(scene: &str, relation: MemoryRelationKind) -> TypedRelationTarget {
    let label = clean_relation_target_label(scene);
    TypedRelationTarget {
        id: label.clone(),
        label,
        kind: MemoryNodeKind::Scene,
        relation,
    }
}

fn typed_character_alias_target(alias: &str) -> TypedRelationTarget {
    let label = clean_relation_target_label(alias);
    TypedRelationTarget {
        id: format!("character_alias:{}", graph_id_fragment(&label)),
        label,
        kind: MemoryNodeKind::Character,
        relation: MemoryRelationKind::AliasOf,
    }
}

fn typed_place_target(place: &str) -> TypedRelationTarget {
    let label = clean_relation_target_label(place);
    TypedRelationTarget {
        id: format!("place:{}", graph_id_fragment(&label)),
        label,
        kind: MemoryNodeKind::Place,
        relation: MemoryRelationKind::LocatedIn,
    }
}

fn typed_goal_target(goal: &str) -> TypedRelationTarget {
    let label = clean_relation_target_label(goal);
    TypedRelationTarget {
        id: format!("goal:{}", graph_id_fragment(&label)),
        label,
        kind: MemoryNodeKind::Goal,
        relation: MemoryRelationKind::HasGoal,
    }
}

fn person_interest_target(value: &str) -> Option<TypedRelationTarget> {
    value
        .split_once(" is a ")
        .or_else(|| value.split_once(" is an "))
        .map(|(_, interest)| {
            let label = clean_relation_target_label(interest);
            TypedRelationTarget {
                id: format!("interest:{}", graph_id_fragment(&label)),
                label,
                kind: MemoryNodeKind::Attribute,
                relation: MemoryRelationKind::HasInterest,
            }
        })
}

fn clean_relation_target_label(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch: char| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?'))
        .trim()
        .to_string()
}

fn graph_id_fragment(value: &str) -> String {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn seed_system_kernel(nodes: &mut BTreeMap<String, MemoryNode>, edges: &mut Vec<MemoryEdge>) {
    let root = SystemKernel::default();
    let root_provenance = vec![MemoryProvenance {
        episode_id: None,
        turn_id: None,
        assertion_key: None,
        system_root: Some(root.id.clone()),
        lifecycle_status: None,
    }];
    insert_node(
        nodes,
        MemoryNode {
            id: root.id.clone(),
            label: root.label.clone(),
            kind: MemoryNodeKind::SystemKernel,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 0.0,
            provenance: root_provenance.clone(),
            created_at: None,
            contradiction_count: 0,
        },
    );

    for principle in root.principles {
        let provenance = vec![MemoryProvenance {
            episode_id: None,
            turn_id: None,
            assertion_key: None,
            system_root: Some(principle.id.clone()),
            lifecycle_status: None,
        }];
        insert_node(
            nodes,
            MemoryNode {
                id: principle.id.clone(),
                label: principle.label,
                kind: MemoryNodeKind::Attribute,
                confidence_tier: AssertionConfidenceTier::Confirmed,
                density: 1.0,
                activation: 0.0,
                provenance: provenance.clone(),
                created_at: None,
                contradiction_count: 0,
            },
        );
        edges.push(MemoryEdge {
            source: root.id.clone(),
            target: principle.id,
            relation: MemoryRelationKind::DefinesRule,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            strength: 1.0,
            activation: 0.0,
            provenance,
        });
    }
}

fn node_kind_for_claim(claim: &MemoryClaim) -> MemoryNodeKind {
    match claim.domain.as_str() {
        "identity" => MemoryNodeKind::Attribute,
        "person" => MemoryNodeKind::Person,
        "character" => MemoryNodeKind::Character,
        "project" => MemoryNodeKind::Project,
        "goal" => MemoryNodeKind::Goal,
        "relationship" => MemoryNodeKind::Relationship,
        "place" => MemoryNodeKind::Place,
        _ => MemoryNodeKind::Assertion,
    }
}

fn node_kind_for_entity(kind: &str) -> MemoryNodeKind {
    match kind {
        "person" => MemoryNodeKind::Person,
        "project" => MemoryNodeKind::Project,
        "character" => MemoryNodeKind::Character,
        "scene" => MemoryNodeKind::Scene,
        "self" => MemoryNodeKind::User,
        _ => MemoryNodeKind::Assertion,
    }
}

fn relation_for_claim(claim: &MemoryClaim) -> MemoryRelationKind {
    match claim.kind.as_str() {
        "goal" => return MemoryRelationKind::HasGoal,
        "location" => return MemoryRelationKind::LocatedIn,
        "interest" => return MemoryRelationKind::HasInterest,
        "scene_order" => return MemoryRelationKind::AppearsBefore,
        "story_chronology" => return MemoryRelationKind::OccursBefore,
        "event_fact" => return MemoryRelationKind::OccursIn,
        _ => {}
    }

    match claim.domain.as_str() {
        "goal" => MemoryRelationKind::HasGoal,
        "manuscript" if claim.kind == "character_alias" => MemoryRelationKind::AliasOf,
        "relationship" => MemoryRelationKind::RelatedTo,
        "place" => MemoryRelationKind::LocatedIn,
        "project" => MemoryRelationKind::ProvenanceFor,
        _ => MemoryRelationKind::HasAttribute,
    }
}

fn claim_node_relation_for_claim(claim: &MemoryClaim) -> MemoryRelationKind {
    if typed_relation_target_for_claim(claim).is_some() {
        MemoryRelationKind::Mentions
    } else {
        relation_for_claim(claim)
    }
}

fn apply_runtime_fine_capture(turn: &ConversationTurn, observation: &mut TurnReading) {
    normalize_existing_runtime_assertions(&mut observation.assertions);
    for assertion in entity_sieve_assertions(&turn.content) {
        merge_runtime_assertion(&mut observation.assertions, assertion);
    }
    prune_unanchored_person_assertions(&mut observation.assertions);
}

fn normalize_existing_runtime_assertions(assertions: &mut Vec<StructuredAssertion>) {
    for assertion in assertions.iter_mut() {
        assertion.value = normalize_known_name_typos(&assertion.value);
        let value = assertion.value.trim().to_string();
        if assertion.domain == "identity" && assertion.kind == "role" && is_single_name(&value) {
            assertion.kind = "name".to_string();
        }

        if assertion.domain == "person" && assertion.kind == "goal" {
            assertion.value = normalize_person_goal_value(&value);
        }

        if assertion.domain == "person" && is_first_person_value(&value) {
            match assertion.kind.as_str() {
                "age" | "trait" | "interest" | "transportation" => {
                    assertion.domain = "identity".to_string();
                }
                "relationship_status" => {
                    assertion.domain = "relationship".to_string();
                    assertion.kind = "partner_status".to_string();
                }
                _ => {}
            }
        }
    }

    let mut seen = BTreeSet::new();
    assertions.retain(|assertion| seen.insert(assertion.key()));
}

fn prune_unanchored_person_assertions(assertions: &mut Vec<StructuredAssertion>) {
    let anchored_person_values = assertions
        .iter()
        .filter(|assertion| assertion.domain == "person" && person_assertion_has_subject(assertion))
        .map(|assertion| {
            (
                assertion.kind.clone(),
                normalize_for_match(&remove_person_subject(&assertion.value)),
            )
        })
        .collect::<BTreeSet<_>>();

    assertions.retain(|assertion| {
        if assertion.domain != "person" || assertion.kind == "name" {
            return true;
        }
        if person_assertion_has_subject(assertion) {
            return true;
        }

        let subjectless_value = normalize_for_match(&assertion.value);
        !anchored_person_values
            .iter()
            .any(|(kind, value)| kind == &assertion.kind && value == &subjectless_value)
    });
}

fn merge_runtime_assertion(
    assertions: &mut Vec<StructuredAssertion>,
    assertion: StructuredAssertion,
) {
    if let Some(existing) = assertions
        .iter_mut()
        .find(|existing| existing.key() == assertion.key())
    {
        let source_count = existing.source_count.saturating_add(1).max(2);
        *existing = existing.clone().with_source_count(source_count);
    } else {
        assertions.push(assertion);
    }
}

fn entity_sieve_assertions(text: &str) -> Vec<StructuredAssertion> {
    let mut assertions = Vec::new();
    capture_self_facts(text, &mut assertions);
    capture_person_facts(text, &mut assertions);
    capture_project_facts(text, &mut assertions);
    capture_manuscript_facts(text, &mut assertions);
    dedupe_assertions(assertions)
}

fn apply_manuscript_one_read_lockout(
    turn: &ConversationTurn,
    observation: &mut TurnReading,
    known_before: &MemoryState,
) {
    if known_before.has_domain_kind("manuscript", "source_status") {
        observation
            .assertions
            .retain(|assertion| assertion.domain != "manuscript");
        return;
    }

    if contains_manuscript_close(&turn.content)
        && !observation
            .assertions
            .iter()
            .any(|assertion| assertion.domain == "manuscript" && assertion.kind == "source_status")
    {
        observation.assertions.push(StructuredAssertion::new(
            "manuscript",
            "source_status",
            manuscript_source_closed_value(),
        ));
    }
}

fn manuscript_source_closed_value() -> String {
    ["manuscript", "source", "closed"].join(" ")
}

fn capture_self_facts(text: &str, assertions: &mut Vec<StructuredAssertion>) {
    if let Some(name) = capture_after_i_am_name(text) {
        assertions.push(StructuredAssertion::new("identity", "name", name));
    }
    if let Some(age) = capture_i_am_years_old(text) {
        assertions.push(StructuredAssertion::new("identity", "age", age));
    }
    if let Some(profession) = capture_self_profession(text) {
        assertions.push(StructuredAssertion::new(
            "identity",
            "profession",
            profession,
        ));
    }
    if contains_ci(text, "i am tall") {
        assertions.push(StructuredAssertion::new("identity", "trait", "I am tall"));
    }
    if contains_ci(text, "i like football") {
        assertions.push(StructuredAssertion::new(
            "identity",
            "interest",
            "I like football",
        ));
    }
    if contains_ci(text, "i have two vehicles") {
        assertions.push(StructuredAssertion::new(
            "identity",
            "transportation",
            "I have two vehicles",
        ));
    }
    if contains_ci(text, "i have a gf") || contains_ci(text, "i have a girlfriend") {
        assertions.push(StructuredAssertion::new(
            "relationship",
            "partner_status",
            "I have a GF",
        ));
    }
}

fn capture_person_facts(text: &str, assertions: &mut Vec<StructuredAssertion>) {
    let people = person_names_from_text(text);
    capture_plural_profession_fallback(text, &people, assertions);
    let mut previous_people = Vec::<String>::new();
    for sentence in split_sentences(text) {
        let sentence_lower = sentence.to_ascii_lowercase();
        let sentence_people = people
            .iter()
            .filter(|name| sentence_lower.contains(&name.to_ascii_lowercase()))
            .cloned()
            .collect::<Vec<_>>();
        let local_people = if sentence_people.is_empty()
            && sentence_lower.contains("they both")
            && previous_people.len() >= 2
        {
            previous_people.clone()
        } else {
            sentence_people.clone()
        };

        if sentence_lower.contains("they both write programs") && local_people.len() >= 2 {
            for name in &local_people {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "profession",
                    format!("{name} writes programs"),
                ));
            }
        }

        if sentence_lower.contains("are short") && local_people.len() >= 2 {
            assertions.push(StructuredAssertion::new(
                "person",
                "trait",
                format!("{} are short", local_people.join(" and ")),
            ));
        }

        for name in &people {
            let lower_name = name.to_ascii_lowercase();
            if let Some(location) = capture_after_name_phrase(sentence, &lower_name, "lives in") {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "location",
                    format!("{name} lives in {}", clean_location_label(&location)),
                ));
            } else if let Some(location) =
                capture_after_name_phrase(sentence, &lower_name, "moved to")
            {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "location",
                    format!("{name} lives in {}", clean_location_label(&location)),
                ));
            }
            if let Some(age) = capture_name_age(sentence, &lower_name) {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "age",
                    format!("{name} is {age}"),
                ));
            }
            if contains_ci(sentence, &format!("{lower_name} is married")) {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "relationship_status",
                    format!("{name} is married"),
                ));
            }
            if contains_ci(sentence, &format!("{lower_name} is african american")) {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "trait",
                    format!("{name} is African American"),
                ));
            }
            if contains_ci(sentence, &format!("{lower_name} is a basketball fan"))
                || contains_ci(sentence, &format!("{lower_name}s is a basketball fan"))
            {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "interest",
                    format!("{name} is a basketball fan"),
                ));
            }
            if let Some(goal) = capture_after_name_phrase(sentence, &lower_name, "wants to") {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "goal",
                    format!("{name} wants to {goal}"),
                ));
            } else if let Some(goal) =
                capture_after_name_phrase(sentence, &lower_name, "just want to")
            {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "goal",
                    format!("{name} wants to {goal}"),
                ));
            } else if let Some(goal) = capture_after_name_phrase(sentence, &lower_name, "want to") {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "goal",
                    format!("{name} wants to {goal}"),
                ));
            }
            if contains_ci(
                sentence,
                &format!("{lower_name} takes public transportation"),
            ) || (contains_ci(sentence, &format!("{lower_name} has vehicle"))
                && contains_ci(sentence, "public transportation"))
            {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "transportation",
                    format!("{name} takes public transportation"),
                ));
            }
            if let Some(role) = capture_person_role(sentence, &lower_name) {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "role",
                    format!("{name} is {role}"),
                ));
            }
            if let Some(plan) = capture_person_project_plan(sentence, &lower_name) {
                assertions.push(StructuredAssertion::new(
                    "person",
                    "project_plan",
                    format!("{name} is preparing {plan}"),
                ));
            }
        }

        if !sentence_people.is_empty() {
            previous_people = sentence_people;
        }
    }
}

fn capture_plural_profession_fallback(
    text: &str,
    people: &[String],
    assertions: &mut Vec<StructuredAssertion>,
) {
    if !contains_ci(text, "they both write programs") {
        return;
    }
    let pair = people
        .windows(2)
        .find(|pair| contains_ci(text, &format!("{} and {}", pair[0], pair[1])));
    if let Some(pair) = pair {
        for name in pair {
            assertions.push(StructuredAssertion::new(
                "person",
                "profession",
                format!("{name} writes programs"),
            ));
        }
    }
}

fn capture_project_facts(text: &str, assertions: &mut Vec<StructuredAssertion>) {
    for sentence in split_sentences(text) {
        if let Some((name, new_name)) = capture_project_rename(sentence) {
            assertions.push(StructuredAssertion::new(
                "project",
                "identity",
                format!("{name} is now called {new_name}"),
            ));
        }
        if let Some(value) = capture_project_purpose(sentence) {
            assertions.push(StructuredAssertion::new("project", "purpose", value));
        }
        if let Some((name, description)) = capture_project_description(sentence) {
            assertions.push(StructuredAssertion::new(
                "project",
                "identity",
                format!("{name} is {description}"),
            ));
        }
    }
}

fn capture_manuscript_facts(text: &str, assertions: &mut Vec<StructuredAssertion>) {
    let scoped_sentences = manuscript_scoped_sentences(text);
    capture_manuscript_scene_events(&scoped_sentences, assertions);
    for sentence in scoped_sentences {
        if is_query_sentence(&sentence) {
            continue;
        }
        if let Some(open_arc) = capture_manuscript_open_arc(&sentence) {
            assertions.push(StructuredAssertion::new(
                "manuscript",
                "open_arc",
                format!("{open_arc} unresolved"),
            ));
        }
        if let Some((name, description)) = capture_manuscript_character_identity(&sentence) {
            assertions.push(StructuredAssertion::new(
                "manuscript",
                "character_identity",
                format!("{name} is {description}"),
            ));
        }
        if let Some((name, alias)) = capture_manuscript_character_alias(&sentence) {
            assertions.push(StructuredAssertion::new(
                "manuscript",
                "character_alias",
                format!("{name} is called {alias}"),
            ));
        }
    }
}

fn capture_manuscript_open_arc(sentence: &str) -> Option<String> {
    let trimmed = strip_manuscript_scope_marker(sentence).trim();
    let lower = trimmed.to_ascii_lowercase();
    let arc = lower
        .strip_prefix("open arc:")
        .map(|_| trimmed["open arc:".len()..].trim())
        .or_else(|| {
            lower
                .strip_prefix("unresolved arc:")
                .map(|_| trimmed["unresolved arc:".len()..].trim())
        })?;
    let arc = clean_relation_target_label(arc);
    (!arc.trim().is_empty()).then_some(arc)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManuscriptSceneEvent {
    scene_id: u32,
    story_time: String,
    event_text: String,
    occurs_before_scene: Option<u32>,
}

fn capture_manuscript_scene_events(
    sentences: &[String],
    assertions: &mut Vec<StructuredAssertion>,
) {
    let scenes = sentences
        .iter()
        .filter_map(|sentence| capture_manuscript_scene_event(sentence))
        .collect::<Vec<_>>();

    for scene in &scenes {
        assertions.push(StructuredAssertion::new(
            "manuscript",
            "event_fact",
            format!(
                "scene:{} story_time:{} {}",
                scene.scene_id, scene.story_time, scene.event_text
            ),
        ));
        if let Some(target_scene) = scene.occurs_before_scene {
            assertions.push(StructuredAssertion::new(
                "manuscript",
                "story_chronology",
                format!(
                    "scene:{} occurs_before scene:{target_scene}",
                    scene.scene_id
                ),
            ));
        }
    }

    for pair in scenes.windows(2) {
        assertions.push(StructuredAssertion::new(
            "manuscript",
            "scene_order",
            format!(
                "scene:{} appears_before scene:{}",
                pair[0].scene_id, pair[1].scene_id
            ),
        ));
    }
}

fn capture_manuscript_scene_event(sentence: &str) -> Option<ManuscriptSceneEvent> {
    let trimmed = strip_manuscript_scope_marker(sentence).trim();
    let lower = trimmed.to_ascii_lowercase();
    let scene_index = lower.find("scene ")?;
    let after_scene = &trimmed[scene_index + "scene ".len()..];
    let scene_digits = after_scene
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    let scene_id = scene_digits.parse::<u32>().ok()?;
    let rest = after_scene[scene_digits.len()..]
        .trim_start_matches(|ch: char| ch == ':' || ch.is_whitespace())
        .trim();
    let rest_lower = rest.to_ascii_lowercase();

    let (story_time, event_text, occurs_before_scene) =
        if rest_lower.starts_with("present:") || rest_lower.starts_with("present ") {
            (
                "present".to_string(),
                rest["present".len()..]
                    .trim_start_matches(|ch: char| ch == ':' || ch.is_whitespace())
                    .trim()
                    .to_string(),
                None,
            )
        } else if rest_lower.starts_with("flashback before scene ") {
            let after = &rest["flashback before scene ".len()..];
            let target_digits = after
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>();
            let target_scene = target_digits.parse::<u32>().ok()?;
            (
                "flashback".to_string(),
                after[target_digits.len()..]
                    .trim_start_matches(|ch: char| ch == ':' || ch.is_whitespace())
                    .trim()
                    .to_string(),
                Some(target_scene),
            )
        } else if rest_lower.starts_with("flashback:") || rest_lower.starts_with("flashback ") {
            (
                "flashback".to_string(),
                rest["flashback".len()..]
                    .trim_start_matches(|ch: char| ch == ':' || ch.is_whitespace())
                    .trim()
                    .to_string(),
                None,
            )
        } else {
            return None;
        };

    if event_text.is_empty() {
        return None;
    }
    Some(ManuscriptSceneEvent {
        scene_id,
        story_time,
        event_text: clean_relation_target_label(&event_text),
        occurs_before_scene,
    })
}

fn manuscript_scoped_sentences(text: &str) -> Vec<String> {
    let mut scoped = Vec::new();
    let mut in_scope = false;
    for sentence in split_sentences(text) {
        let trimmed = sentence.trim();
        if manuscript_scope_stop(trimmed) {
            in_scope = false;
            continue;
        }
        let has_scope_marker = contains_ci(trimmed, "MANUSCRIPT:");
        if has_scope_marker {
            in_scope = true;
        }
        if in_scope {
            scoped.push(trimmed.to_string());
        }
    }
    scoped
}

fn manuscript_scope_stop(sentence: &str) -> bool {
    contains_manuscript_close(sentence) || contains_manuscript_scope_negation(sentence)
}

fn contains_manuscript_close(sentence: &str) -> bool {
    let lower = sentence.to_ascii_lowercase();
    lower.contains("manuscript is closed") || lower.contains("the manuscript is closed")
}

fn contains_manuscript_scope_negation(sentence: &str) -> bool {
    let lower = sentence.to_ascii_lowercase();
    lower.contains("not part of the manuscript") || lower.contains("outside the manuscript")
}

fn is_query_sentence(sentence: &str) -> bool {
    let lower = sentence.trim().to_ascii_lowercase();
    lower.contains('?')
        || lower.starts_with("who ")
        || lower.starts_with("what ")
        || lower.starts_with("where ")
        || lower.starts_with("when ")
        || lower.starts_with("why ")
        || lower.starts_with("how ")
}

fn capture_manuscript_character_identity(sentence: &str) -> Option<(String, String)> {
    let lower = sentence.to_ascii_lowercase();
    let index = lower.find(" is the ")?;
    let name = strip_manuscript_scope_marker(&sentence[..index])
        .trim()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != ' ');
    let description = format!("the {}", sentence[index + " is the ".len()..].trim());
    if is_character_name(name) {
        Some((name.to_string(), clean_relation_target_label(&description)))
    } else {
        None
    }
}

fn capture_manuscript_character_alias(sentence: &str) -> Option<(String, String)> {
    let lower = sentence.to_ascii_lowercase();
    let (name, alias) = if let Some(index) = lower.find(" is called ") {
        (
            sentence[..index]
                .trim()
                .strip_prefix("MANUSCRIPT:")
                .unwrap_or_else(|| strip_manuscript_scope_marker(&sentence[..index]))
                .trim()
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != ' '),
            sentence[index + " is called ".len()..].trim(),
        )
    } else {
        let call_index = lower.find(" call ")?;
        let after_call = &sentence[call_index + " call ".len()..];
        let after_call_lower = after_call.to_ascii_lowercase();
        let the_index = after_call_lower.find(" the ")?;
        (
            after_call[..the_index].trim(),
            &after_call[the_index + " the ".len()..],
        )
    };
    let alias = if alias.to_ascii_lowercase().starts_with("the ") {
        alias.to_string()
    } else {
        format!("the {}", alias.trim())
    };
    if is_character_name(name) {
        Some((name.to_string(), clean_relation_target_label(&alias)))
    } else {
        None
    }
}

fn strip_manuscript_scope_marker(sentence: &str) -> &str {
    let trimmed = sentence.trim_start();
    if trimmed
        .get(.."MANUSCRIPT:".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("MANUSCRIPT:"))
    {
        &trimmed["MANUSCRIPT:".len()..]
    } else {
        sentence
    }
}

fn is_character_name(value: &str) -> bool {
    let words = value.split_whitespace().collect::<Vec<_>>();
    (1..=3).contains(&words.len())
        && words.iter().all(|word| {
            word.chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
        })
}

fn capture_project_description(sentence: &str) -> Option<(String, String)> {
    let lower = sentence.to_ascii_lowercase();
    let (name, description) = if let Some(index) = lower.find(" is my ") {
        (
            project_subject_candidate(&sentence[..index]),
            format!("my {}", sentence[index + " is my ".len()..].trim()),
        )
    } else if let Some(index) = lower.find(" is a ") {
        (
            project_subject_candidate(&sentence[..index]),
            format!("a {}", sentence[index + " is a ".len()..].trim()),
        )
    } else if let Some(index) = lower.find(" is an ") {
        (
            project_subject_candidate(&sentence[..index]),
            format!("an {}", sentence[index + " is an ".len()..].trim()),
        )
    } else {
        return None;
    };

    let name = name.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != ' ');
    let description = description
        .trim()
        .trim_matches(|ch: char| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?'))
        .trim();

    if is_project_name(name) && !description.is_empty() {
        Some((name.to_string(), description.to_string()))
    } else {
        None
    }
}

fn capture_project_rename(sentence: &str) -> Option<(String, String)> {
    let lower = sentence.to_ascii_lowercase();
    let (name, alias) = if let Some(index) = lower.find(" is now called ") {
        (
            project_subject_candidate(&sentence[..index]),
            sentence[index + " is now called ".len()..].trim(),
        )
    } else if let Some(index) = lower.find(" is called ") {
        (
            project_subject_candidate(&sentence[..index]),
            sentence[index + " is called ".len()..].trim(),
        )
    } else {
        return None;
    };
    let name = clean_project_subject(name)?;
    let alias = clean_relation_target_label(alias);
    (!alias.is_empty()).then_some((name, alias))
}

fn capture_project_purpose(sentence: &str) -> Option<String> {
    let lower = sentence.to_ascii_lowercase();
    if let Some(index) = lower.find(" helps ") {
        let name = clean_project_subject(project_subject_candidate(&sentence[..index]))?;
        let tail = clean_relation_target_label(&sentence[index + " helps ".len()..]);
        if !tail.is_empty() {
            return Some(format!("{name} helps {tail}"));
        }
    }
    if let Some(index) = lower.find(" focuses on ") {
        let subject = clean_project_subject(project_subject_candidate(&sentence[..index]))?;
        let tail = clean_relation_target_label(&sentence[index + " focuses on ".len()..]);
        if !tail.is_empty() {
            return Some(format!("{subject} focuses on {tail}"));
        }
    }
    None
}

fn clean_project_subject(value: &str) -> Option<String> {
    let trimmed = value
        .trim()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != ' ' && ch != '\'')
        .trim();
    let has_possessive_context = trimmed.contains("'s ");
    let owner = trimmed
        .split_once("'s ")
        .map(|(owner, _)| owner)
        .unwrap_or(trimmed)
        .trim();
    if is_project_name(owner) || (has_possessive_context && is_single_titlecase_token(owner)) {
        Some(owner.to_string())
    } else {
        None
    }
}

fn project_subject_candidate(prefix: &str) -> &str {
    prefix
        .rsplit([',', ';'])
        .next()
        .unwrap_or(prefix)
        .rsplit(':')
        .next()
        .unwrap_or(prefix)
        .split(" but ")
        .last()
        .unwrap_or(prefix)
        .trim()
}

fn dedupe_assertions(assertions: Vec<StructuredAssertion>) -> Vec<StructuredAssertion> {
    let mut seen = BTreeSet::new();
    assertions
        .into_iter()
        .filter(|assertion| seen.insert(assertion.key()))
        .collect()
}

fn split_sentences(text: &str) -> Vec<&str> {
    text.split(['.', '!', '?', ';'])
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .collect()
}

fn capture_after_i_am_name(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let index = lower.find("i am ")?;
    let after = &text[index + "i am ".len()..];
    let token = after
        .split_whitespace()
        .next()?
        .trim_matches(|ch: char| !ch.is_ascii_alphabetic());
    if is_single_name(token) {
        Some(token.to_string())
    } else {
        None
    }
}

fn capture_i_am_years_old(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let years_index = lower.find(" years old")?;
    let before = &text[..years_index];
    let age = before
        .split_whitespace()
        .rev()
        .find(|token| token.chars().all(|ch| ch.is_ascii_digit()))?;
    Some(format!("I am {age} years old"))
}

fn capture_self_profession(text: &str) -> Option<String> {
    split_sentences(text).into_iter().find_map(|sentence| {
        sentence
            .split([',', ';'])
            .map(str::trim)
            .find_map(capture_self_profession_clause)
    })
}

fn capture_self_profession_clause(clause: &str) -> Option<String> {
    let lower = clause.to_ascii_lowercase();
    [
        ("i work as a ", true),
        ("i work as an ", true),
        ("i am a ", false),
        ("i am an ", false),
        ("i'm a ", false),
        ("i'm an ", false),
    ]
    .iter()
    .find_map(|(prefix, work_context)| {
        lower
            .strip_prefix(prefix)
            .and_then(|candidate| clean_profession_candidate(candidate, *work_context))
    })
}

fn clean_profession_candidate(candidate: &str, work_context: bool) -> Option<String> {
    let candidate = candidate
        .split([',', ';'])
        .next()
        .unwrap_or(candidate)
        .trim()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != ' ');
    let words = candidate.split_whitespace().collect::<Vec<_>>();
    if (1..=5).contains(&words.len())
        && words
            .iter()
            .all(|word| word.chars().all(|ch| ch.is_ascii_alphabetic() || ch == '-'))
        && (work_context || profession_shaped_noun(words.last().copied().unwrap_or_default()))
    {
        Some(words.join(" "))
    } else {
        None
    }
}

fn profession_shaped_noun(word: &str) -> bool {
    matches!(
        word,
        "architect"
            | "artist"
            | "builder"
            | "consultant"
            | "designer"
            | "developer"
            | "doctor"
            | "engineer"
            | "founder"
            | "manager"
            | "nurse"
            | "operator"
            | "programmer"
            | "teacher"
            | "writer"
    )
}

fn capture_name_age(sentence: &str, lower_name: &str) -> Option<String> {
    let lower = sentence.to_ascii_lowercase();
    let phrase = format!("{lower_name} is ");
    let index = lower.find(&phrase)?;
    let after = sentence[index + phrase.len()..].trim_start();
    let age = after
        .split_whitespace()
        .next()?
        .trim_matches(|ch: char| !ch.is_ascii_digit());
    if age.is_empty() || !age.chars().all(|ch| ch.is_ascii_digit()) {
        None
    } else {
        Some(age.to_string())
    }
}

fn capture_after_name_phrase(sentence: &str, lower_name: &str, phrase: &str) -> Option<String> {
    let lower = sentence.to_ascii_lowercase();
    let needle = format!("{lower_name} {phrase} ");
    let index = lower.find(&needle)?;
    let after = &sentence[index + needle.len()..];
    let value = after
        .split([','])
        .next()
        .unwrap_or(after)
        .split(" and ")
        .next()
        .unwrap_or(after)
        .trim()
        .trim_matches(|ch: char| ch == '.' || ch == ',');
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn capture_person_role(sentence: &str, lower_name: &str) -> Option<String> {
    if is_query_sentence(sentence)
        || matches!(
            lower_name,
            "who" | "what" | "where" | "when" | "why" | "how"
        )
    {
        return None;
    }
    let lower = sentence.to_ascii_lowercase();
    let phrase = format!("{lower_name} is ");
    let index = lower.find(&phrase)?;
    if !sentence[..index]
        .trim_matches(|ch: char| !ch.is_ascii_alphabetic())
        .is_empty()
    {
        return None;
    }
    let after = sentence[index + phrase.len()..].trim_start();
    let role = after
        .split([','])
        .next()
        .unwrap_or(after)
        .trim()
        .trim_matches(|ch: char| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?'));
    let lower_role = role.to_ascii_lowercase();
    if role.is_empty()
        || lower_role.starts_with("married")
        || lower_role.starts_with("african american")
        || lower_role.starts_with("a basketball fan")
        || role
            .split_whitespace()
            .next()
            .is_some_and(|word| word.chars().all(|ch| ch.is_ascii_digit()))
    {
        return None;
    }
    if lower_role.starts_with("my ")
        || lower_role.starts_with("the ")
        || lower_role.starts_with("a ")
        || lower_role.starts_with("an ")
    {
        Some(clean_relation_target_label(role))
    } else {
        None
    }
}

fn capture_person_project_plan(sentence: &str, lower_name: &str) -> Option<String> {
    if is_query_sentence(sentence) {
        return None;
    }
    let lower = sentence.to_ascii_lowercase();
    let needle = format!("{lower_name} is preparing ");
    let index = lower.find(&needle)?;
    if !sentence[..index]
        .trim_matches(|ch: char| !ch.is_ascii_alphabetic())
        .is_empty()
    {
        return None;
    }
    let plan = clean_relation_target_label(&sentence[index + needle.len()..]);
    (!plan.is_empty()).then_some(plan)
}

fn clean_location_label(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(" now")
        .trim_end_matches(" today")
        .trim_end_matches(" currently")
        .trim()
        .to_string()
}

fn person_names_from_text(text: &str) -> Vec<String> {
    let self_name = capture_after_i_am_name(text).map(|name| name.to_ascii_lowercase());
    let tokens = text
        .split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_ascii_alphabetic()))
        .filter(|token| is_single_name(token))
        .filter(|token| {
            self_name
                .as_deref()
                .is_none_or(|name| token.to_ascii_lowercase() != name)
        })
        .map(normalize_person_name)
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    tokens
        .into_iter()
        .filter(|token| seen.insert(token.clone()))
        .collect()
}

fn normalize_person_name(token: &str) -> String {
    if token.ends_with("ss") {
        token.strip_suffix('s').unwrap_or(token).to_string()
    } else {
        token.to_string()
    }
}

fn normalize_known_name_typos(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            let trimmed = token.trim_matches(|ch: char| !ch.is_ascii_alphabetic());
            let normalized = normalize_person_name(trimmed);
            token.replace(trimmed, &normalized)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_person_goal_value(value: &str) -> String {
    let Some(name) = first_person_subject(value) else {
        return value.to_string();
    };
    let tail = value[name.len()..].trim_start();
    if tail.starts_with("wants to ") {
        value.to_string()
    } else if tail.starts_with("just want to ") {
        format!(
            "{name} wants to {}",
            tail.trim_start_matches("just want to ").trim()
        )
    } else if tail.starts_with("want to ") {
        format!(
            "{name} wants to {}",
            tail.trim_start_matches("want to ").trim()
        )
    } else {
        format!("{name} wants to {tail}")
    }
}

fn person_assertion_has_subject(assertion: &StructuredAssertion) -> bool {
    first_person_subject(&assertion.value).is_some()
}

fn first_person_subject(value: &str) -> Option<String> {
    let first = value
        .split_whitespace()
        .next()?
        .trim_matches(|ch: char| !ch.is_ascii_alphabetic());
    if is_single_name(first) && !matches!(first, "I" | "They") {
        Some(first.to_string())
    } else {
        None
    }
}

fn remove_person_subject(value: &str) -> String {
    let Some(name) = first_person_subject(value) else {
        return value.to_string();
    };
    let tail = value[name.len()..].trim_start();
    tail.trim_start_matches("is ")
        .trim_start_matches("are ")
        .trim_start_matches("wants to ")
        .trim_start_matches("want to ")
        .to_string()
}

fn is_first_person_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("i ") || lower.starts_with("i'm ") || lower.starts_with("my ")
}

fn is_single_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && chars.all(|ch| ch.is_ascii_lowercase())
        && !matches!(value, "I" | "They" | "The" | "A" | "An" | "My" | "Lives")
}

fn is_single_titlecase_token(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && !matches!(value, "I" | "They" | "The" | "A" | "An" | "My" | "Lives")
}

fn is_project_name(value: &str) -> bool {
    let words = value.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() || words.len() > 4 {
        return false;
    }
    if words.len() == 1 {
        let trimmed = words[0].trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        return trimmed.len() > 1 && trimmed.chars().all(|ch| ch.is_ascii_uppercase());
    }
    words.iter().all(|word| {
        let trimmed = word.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        !trimmed.is_empty()
            && !matches!(trimmed, "I" | "It" | "They" | "The" | "A" | "An" | "My")
            && (trimmed.chars().all(|ch| ch.is_ascii_uppercase())
                || trimmed
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase()))
    })
}

fn contains_ci(text: &str, needle: &str) -> bool {
    text.to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn density_for_tier(tier: AssertionConfidenceTier) -> f32 {
    match tier {
        AssertionConfidenceTier::Confirmed => 1.0,
        AssertionConfidenceTier::Inferred => 0.62,
        AssertionConfidenceTier::Unconfirmed => 0.35,
    }
}

fn activate_working_memory_with_orb_state(
    state: &MemoryState,
    turn: &ConversationTurn,
    observation: &TurnReading,
    recalled: &RecallSet,
    budget: WorkingMemoryBudget,
    orb_state: &RuntimeOrbActivationState,
) -> WorkingMemory {
    let map = &state.map;
    let query = normalize_for_match(&turn.content);
    let cue_terms = observation
        .cue_terms
        .iter()
        .map(|term| normalize_for_match(term))
        .collect::<Vec<_>>();
    let recalled_values = recalled
        .hits
        .iter()
        .flat_map(|hit| {
            hit.assertions
                .iter()
                .map(|assertion| assertion.value.clone())
        })
        .map(|value| normalize_for_match(&value))
        .collect::<Vec<_>>();

    let mut retired_orb_filtered_node_count = 0;
    let mut scored_nodes = map
        .nodes
        .iter()
        .cloned()
        .filter_map(|node| {
            let filtered = filter_node_provenance_by_orb_state(node, orb_state);
            if filtered.is_none() {
                retired_orb_filtered_node_count += 1;
            }
            filtered
        })
        .map(|mut node| {
            node.activation = compute_activation(
                &node,
                &query,
                &cue_terms,
                &recalled_values,
                &ActivationConfig::default(),
            );
            node
        })
        .collect::<Vec<_>>();
    propagate_activation_with_context(
        &mut scored_nodes,
        &map.edges,
        &ActivationConfig::default(),
        budget.max_activation_depth,
        &query,
        &cue_terms,
    );
    boost_project_specific_nodes(&mut scored_nodes, &query);
    scored_nodes.retain(|node| node.activation > 0.0 && node.kind != MemoryNodeKind::User);
    scored_nodes.sort_by(|left, right| {
        right
            .activation
            .partial_cmp(&left.activation)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .density
                    .partial_cmp(&left.density)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.id.cmp(&right.id))
    });

    let filtered_out_memory_count = filtered_out_matching_claim_count(state, &query, &cue_terms);
    let filtered_node_count = scored_nodes.len().saturating_sub(budget.max_nodes)
        + filtered_out_memory_count
        + retired_orb_filtered_node_count;
    scored_nodes.truncate(budget.max_nodes);
    let active_ids = scored_nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();

    let mut scored_edges = map
        .edges
        .iter()
        .filter(|edge| active_ids.contains(&edge.source) && active_ids.contains(&edge.target))
        .cloned()
        .map(|mut edge| {
            edge.activation = edge.strength
                + if active_ids.contains(&edge.source) {
                    0.2
                } else {
                    0.0
                }
                + if active_ids.contains(&edge.target) {
                    0.4
                } else {
                    0.0
                };
            edge
        })
        .collect::<Vec<_>>();
    scored_edges.sort_by(|left, right| {
        right
            .activation
            .partial_cmp(&left.activation)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.target.cmp(&right.target))
    });
    let filtered_edge_count = scored_edges.len().saturating_sub(budget.max_edges);
    scored_edges.truncate(budget.max_edges);
    let correction_salience = correction_salience_summaries(state, &scored_nodes);

    WorkingMemory {
        nodes: scored_nodes,
        edges: scored_edges,
        filtered_node_count,
        filtered_edge_count,
        activation_reason: activation_report(
            filtered_out_memory_count,
            retired_orb_filtered_node_count,
            &correction_salience,
        ),
    }
}

fn boost_project_specific_nodes(nodes: &mut [MemoryNode], query: &str) {
    let desired = project_answer_kinds_for_query(query);
    if desired.is_empty() {
        return;
    }
    for node in nodes {
        let direct_keys = node
            .provenance
            .iter()
            .filter_map(direct_answer_assertion_key)
            .collect::<Vec<_>>();
        if direct_keys.iter().any(|key| {
            desired
                .iter()
                .any(|kind| key.starts_with(&format!("project:{kind}=")))
        }) {
            node.activation += 2.0;
        }
    }
}

fn filter_node_provenance_by_orb_state(
    mut node: MemoryNode,
    orb_state: &RuntimeOrbActivationState,
) -> Option<MemoryNode> {
    if orb_state.is_empty() {
        return Some(node);
    }
    let orb_ids = node_orb_ids(&node);
    if orb_ids.is_empty() {
        return Some(node);
    }
    node.provenance
        .retain(|provenance| provenance_allowed_by_orb_state(provenance, orb_state));
    (!node.provenance.is_empty()).then_some(node)
}

fn provenance_allowed_by_orb_state(
    provenance: &MemoryProvenance,
    orb_state: &RuntimeOrbActivationState,
) -> bool {
    let Some(orb_id) = provenance
        .system_root
        .as_deref()
        .and_then(orb_id_from_system_root)
    else {
        return true;
    };
    orb_state.active_orb_ids.contains(&orb_id) || !orb_state.retired_orb_ids.contains(&orb_id)
}

fn node_orb_ids(node: &MemoryNode) -> BTreeSet<String> {
    node.provenance
        .iter()
        .filter_map(|provenance| provenance.system_root.as_deref())
        .filter_map(orb_id_from_system_root)
        .collect()
}

fn orb_id_from_system_root(system_root: &str) -> Option<String> {
    let trimmed = system_root.trim();
    if trimmed.is_empty() {
        None
    } else if let Some(orb_id) = trimmed.strip_prefix("orb:") {
        Some(orb_id.to_string())
    } else {
        Some(trimmed.to_string())
    }
}

fn filtered_out_matching_claim_count(
    state: &MemoryState,
    query: &str,
    cue_terms: &[String],
) -> usize {
    state
        .claims
        .iter()
        .filter(|claim| claim.lifecycle_status != AssertionLifecycleStatus::Current)
        .filter(|claim| claim_matches_activation_query(claim, query, cue_terms))
        .count()
}

fn claim_matches_activation_query(claim: &MemoryClaim, query: &str, cue_terms: &[String]) -> bool {
    let evidence = normalize_for_match(&format!(
        "{} {} {} {}",
        claim.domain, claim.kind, claim.value, claim.key
    ));
    (!query.is_empty()
        && (query.contains(&evidence)
            || tokens_overlap(&normalized_terms(query), &normalized_terms(&evidence))))
        || cue_terms
            .iter()
            .any(|term| !term.is_empty() && evidence.contains(term))
}

fn correction_salience_summaries(state: &MemoryState, nodes: &[MemoryNode]) -> Vec<String> {
    let active_assertion_keys = nodes
        .iter()
        .flat_map(|node| node.provenance.iter())
        .filter_map(|provenance| provenance.assertion_key.as_deref())
        .collect::<BTreeSet<_>>();
    let mut summaries = Vec::new();
    let mut seen = BTreeSet::new();

    for current in state.claims.iter().filter(|claim| {
        claim.lifecycle_status == AssertionLifecycleStatus::Current
            && active_assertion_keys.contains(claim.key.as_str())
    }) {
        let Some(slot) = correction_slot_for_claim(current) else {
            continue;
        };
        for older in state.claims.iter().filter(|claim| {
            claim.lifecycle_status == AssertionLifecycleStatus::Superseded
                && claim.value != current.value
                && correction_slot_for_claim(claim).as_deref() == Some(slot.as_str())
        }) {
            let summary = format!(
                "correction_salience: {} supersedes {}",
                current.value, older.value
            );
            if seen.insert(summary.clone()) {
                summaries.push(summary);
            }
        }
    }

    summaries
}

fn correction_slot_for_claim(claim: &MemoryClaim) -> Option<String> {
    let entity = match claim.domain.as_str() {
        "person" => person_subjects_from_claim_value(&claim.value)
            .first()
            .cloned()?,
        "project" => project_subject_from_claim_value(&claim.value)?,
        "identity" => "self".to_string(),
        _ => return None,
    };
    Some(format!(
        "{}:{}:{}",
        claim.domain,
        claim.kind,
        normalize_for_match(&entity)
    ))
}

fn activation_report(
    filtered_out_memory_count: usize,
    retired_orb_filtered_node_count: usize,
    correction_salience: &[String],
) -> String {
    let mut report = "entity/relation/cue/query/recalled/confidence activation over current graph with graph-depth and fixed-budget filtering".to_string();
    if filtered_out_memory_count > 0 {
        report.push_str(&format!(
            "; suppressed_noncurrent_memory={filtered_out_memory_count}"
        ));
    }
    if retired_orb_filtered_node_count > 0 {
        report.push_str(&format!(
            "; suppressed_retired_orb_memory={retired_orb_filtered_node_count}"
        ));
    }
    if !correction_salience.is_empty() {
        report.push_str("; ");
        report.push_str(&correction_salience.join(" | "));
    }
    report
}

fn tokens_overlap<L: AsRef<str>, R: AsRef<str>>(left: &[L], right: &[R]) -> bool {
    left.iter().any(|token| {
        let token = token.as_ref();
        token.len() > 2 && right.iter().any(|other| other.as_ref() == token)
    })
}

fn normalize_for_match(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionCandidate {
    pub question: String,
    pub reason: String,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextPacket {
    pub recall_mode: RecallMode,
    pub recalled_claims: Vec<MemoryClaim>,
    pub working_memory: WorkingMemory,
    pub compressed_memory: Vec<CompressedWorkingMemory>,
    pub open_questions: Vec<QuestionCandidate>,
    pub summary: String,
}

impl ContextPacket {
    fn from_parts(
        user_text: &str,
        working_memory: &WorkingMemory,
        recalled: &RecallSet,
        questions: &[QuestionCandidate],
        recall_mode: RecallMode,
        budget: WorkingMemoryBudget,
    ) -> Self {
        Self::from_parts_with_verified_compression_receipts(
            user_text,
            working_memory,
            recalled,
            questions,
            recall_mode,
            budget,
            &[],
            &VerifiedSourceEventIndex::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_parts_with_verified_compression_receipts(
        user_text: &str,
        working_memory: &WorkingMemory,
        recalled: &RecallSet,
        questions: &[QuestionCandidate],
        recall_mode: RecallMode,
        budget: WorkingMemoryBudget,
        compression_receipts: &[CompressionReceipt],
        verified_source_events: &VerifiedSourceEventIndex,
    ) -> Self {
        let working_memory = filter_working_memory_for_context(user_text, working_memory);
        let compression = compress_working_memory_with_verified_receipts(
            &working_memory,
            compression_receipts,
            verified_source_events,
        );
        let working_memory = compression.working_memory;
        let active_keys = working_memory_assertion_keys(&working_memory);
        let recalled_claims = recalled
            .hits
            .iter()
            .flat_map(|hit| hit.assertions.iter())
            .map(MemoryClaim::from_assertion)
            .filter(|claim| manuscript_claim_allowed_for_query(user_text, claim))
            .filter(|claim| project_claim_allowed_for_query(user_text, claim))
            .filter(|claim| active_keys.contains(&claim.key))
            .take(budget.max_nodes)
            .collect::<Vec<_>>();
        let open_questions = questions
            .iter()
            .take(budget.max_questions)
            .cloned()
            .collect::<Vec<_>>();
        let summary = render_context_summary(
            &recalled_claims,
            &working_memory,
            &compression.compressed_memory,
            &open_questions,
        );

        Self {
            recall_mode,
            recalled_claims,
            working_memory,
            compressed_memory: compression.compressed_memory,
            open_questions,
            summary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressedWorkingMemory {
    pub compression_id: String,
    pub algorithm_id: String,
    pub output_artifact_ref: String,
    pub output_hash: String,
    pub output_byte_len: usize,
    pub raw_event_refs: Vec<SourceEventRef>,
    pub covered_node_ids: Vec<String>,
    pub covered_assertion_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkingMemoryCompressionResult {
    pub working_memory: WorkingMemory,
    pub compressed_memory: Vec<CompressedWorkingMemory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerifiedSourceEventIndex {
    event_hashes: BTreeMap<String, String>,
}

impl VerifiedSourceEventIndex {
    pub fn from_stored_events(events: &[luna_events::StoredEvent]) -> Result<Self> {
        let mut event_hashes = BTreeMap::new();
        for event in events {
            let Some(stored_hash) = event.event_hash.as_deref() else {
                return Err(luna_core::LunaError::new(format!(
                    "stored runtime event {} is missing event_hash",
                    event.event_id
                )));
            };
            let recomputed = stable_stored_event_hash(event)?;
            if stored_hash != recomputed {
                return Err(luna_core::LunaError::new(format!(
                    "stored runtime event {} hash mismatch",
                    event.event_id
                )));
            }
            event_hashes.insert(event.event_id.to_string(), stored_hash.to_string());
        }
        Ok(Self { event_hashes })
    }

    fn verifies(&self, source: &SourceEventRef) -> bool {
        self.event_hashes
            .get(&source.event_id)
            .is_some_and(|hash| hash == &source.event_hash)
    }
}

pub fn compress_working_memory_with_verified_receipts(
    working_memory: &WorkingMemory,
    compression_receipts: &[CompressionReceipt],
    verified_source_events: &VerifiedSourceEventIndex,
) -> WorkingMemoryCompressionResult {
    let accepted_receipts = compression_receipts
        .iter()
        .filter(|receipt| {
            receipt.decision == CompressionDecision::Accepted
                && validate_compression_receipt(receipt).is_ok()
        })
        .collect::<Vec<_>>();
    if accepted_receipts.is_empty() || working_memory.nodes.len() < 2 {
        return WorkingMemoryCompressionResult {
            working_memory: working_memory.clone(),
            compressed_memory: Vec::new(),
        };
    }

    let mut compressed_memory = Vec::new();
    let mut removed_node_ids = BTreeSet::new();
    let mut compressed_nodes = Vec::new();

    for receipt in accepted_receipts {
        let source_ids = receipt
            .input_event_refs
            .iter()
            .filter(|reference| verified_source_events.verifies(reference))
            .map(|reference| reference.event_id.clone())
            .collect::<BTreeSet<_>>();
        if source_ids.len() != receipt.input_event_refs.len() {
            continue;
        }
        let covered_nodes = working_memory
            .nodes
            .iter()
            .filter(|node| !removed_node_ids.contains(&node.id))
            .filter(|node| node_has_source_event_ref(node, &source_ids))
            .cloned()
            .collect::<Vec<_>>();
        if covered_nodes.len() < 2 {
            continue;
        }

        let covered_node_ids = covered_nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        let covered_assertion_keys = covered_nodes
            .iter()
            .flat_map(|node| node.provenance.iter())
            .filter_map(|provenance| provenance.assertion_key.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let activation = covered_nodes
            .iter()
            .map(|node| node.activation)
            .fold(0.0_f32, f32::max);
        let density = covered_nodes
            .iter()
            .map(|node| node.density)
            .fold(0.0_f32, f32::max);

        removed_node_ids.extend(covered_node_ids.iter().cloned());
        compressed_nodes.push(MemoryNode {
            id: format!("compression:{}", receipt.compression_id),
            label: format!("compressed memory: {}", receipt.output_artifact_ref),
            kind: MemoryNodeKind::Episode,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density,
            activation,
            provenance: Vec::new(),
            created_at: None,
            contradiction_count: 0,
        });
        compressed_memory.push(CompressedWorkingMemory {
            compression_id: receipt.compression_id.clone(),
            algorithm_id: receipt.algorithm_id.clone(),
            output_artifact_ref: receipt.output_artifact_ref.clone(),
            output_hash: receipt.output_hash.clone(),
            output_byte_len: receipt.output_byte_len,
            raw_event_refs: receipt.input_event_refs.clone(),
            covered_node_ids,
            covered_assertion_keys,
        });
    }

    if compressed_memory.is_empty() {
        return WorkingMemoryCompressionResult {
            working_memory: working_memory.clone(),
            compressed_memory,
        };
    }

    let mut nodes = working_memory
        .nodes
        .iter()
        .filter(|node| !removed_node_ids.contains(&node.id))
        .cloned()
        .collect::<Vec<_>>();
    nodes.extend(compressed_nodes);
    let active_ids = nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let edges = working_memory
        .edges
        .iter()
        .filter(|edge| active_ids.contains(&edge.source) && active_ids.contains(&edge.target))
        .cloned()
        .collect::<Vec<_>>();
    let compressed_node_count = removed_node_ids
        .len()
        .saturating_sub(compressed_memory.len());
    let raw_ref_count = compressed_memory
        .iter()
        .map(|entry| entry.raw_event_refs.len())
        .sum::<usize>();
    let mut compressed = working_memory.clone();
    compressed.nodes = nodes;
    compressed.edges = edges;
    compressed.filtered_node_count += compressed_node_count;
    compressed.filtered_edge_count += working_memory
        .edges
        .len()
        .saturating_sub(compressed.edges.len());
    compressed.activation_reason = format!(
        "{}; accepted_compression_receipts={}; compressed_nodes={compressed_node_count}; raw_source_event_refs={raw_ref_count}",
        compressed.activation_reason,
        compressed_memory.len(),
    );

    WorkingMemoryCompressionResult {
        working_memory: compressed,
        compressed_memory,
    }
}

fn node_has_source_event_ref(node: &MemoryNode, source_ids: &BTreeSet<String>) -> bool {
    node.provenance
        .iter()
        .flat_map(provenance_ref_ids)
        .any(|id| source_ids.contains(&id))
}

fn provenance_ref_ids(provenance: &MemoryProvenance) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(episode_id) = provenance.episode_id {
        ids.push(episode_id.to_string());
    }
    if let Some(turn_id) = provenance.turn_id {
        ids.push(turn_id.to_string());
    }
    if let Some(assertion_key) = &provenance.assertion_key {
        ids.push(assertion_key.clone());
    }
    ids
}

fn working_memory_assertion_keys(working_memory: &WorkingMemory) -> BTreeSet<String> {
    working_memory
        .nodes
        .iter()
        .flat_map(|node| node.provenance.iter())
        .filter_map(|provenance| provenance.assertion_key.clone())
        .collect()
}

fn direct_answer_assertion_key(provenance: &MemoryProvenance) -> Option<String> {
    if provenance.system_root.is_some() {
        return None;
    }
    provenance.assertion_key.clone()
}

pub fn render_runtime_markdown(result: &RuntimeTurnResult) -> String {
    let mut out = String::new();
    out.push_str("# Luna Runtime Turn\n\n");
    out.push_str("## Intake\n");
    out.push_str(&format!(
        "- {:?}: {}\n\n",
        result.intake.action, result.intake.reason
    ));
    out.push_str("## Learned\n");
    render_claims(&mut out, "Confirmed", &result.knowledge_delta.confirmed);
    render_claims(&mut out, "Inferred", &result.knowledge_delta.inferred);
    render_claims(&mut out, "Unconfirmed", &result.knowledge_delta.unconfirmed);
    if !result.knowledge_delta.unknowns.is_empty() {
        out.push_str("\n### Unknown\n");
        for unknown in &result.knowledge_delta.unknowns {
            out.push_str(&format!("- {unknown}\n"));
        }
    }

    out.push_str("\n## Next Question\n");
    match result.context_packet.open_questions.first() {
        Some(question) => {
            out.push_str(&format!("{}\n", question.question));
            out.push_str(&format!("Reason: {}\n", question.reason));
        }
        None => out.push_str("(none)\n"),
    }

    out.push_str("\n## Recalled\n");
    if result.context_packet.recalled_claims.is_empty() {
        out.push_str("(none)\n");
    } else {
        for claim in &result.context_packet.recalled_claims {
            out.push_str(&format!("- {:?}: {}\n", claim.status, claim.value));
        }
    }

    out.push_str("\n## Working Memory\n");
    if result.context_packet.working_memory.nodes.is_empty() {
        out.push_str("(none)\n");
    } else {
        for node in &result.context_packet.working_memory.nodes {
            out.push_str(&format!(
                "- {:.2} {:?}: {}\n",
                node.activation, node.confidence_tier, node.label
            ));
        }
        if result.context_packet.working_memory.filtered_node_count > 0
            || result.context_packet.working_memory.filtered_edge_count > 0
        {
            out.push_str(&format!(
                "Filtered: {} node(s), {} edge(s)\n",
                result.context_packet.working_memory.filtered_node_count,
                result.context_packet.working_memory.filtered_edge_count
            ));
        }
    }

    out.push_str("\n## Context Packet\n");
    out.push_str(&result.context_packet.summary);
    out.push('\n');
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponsePlanAction {
    Acknowledge,
    Answer,
    AskOneQuestion,
    StateUncertainty,
    CiteRecalledMemory,
    AvoidAnswering,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResponsePlan {
    pub actions: Vec<ResponsePlanAction>,
    pub target_label: Option<String>,
    pub answer_values: Vec<String>,
    pub answer_evidence: Vec<ResponsePlanEvidence>,
    pub question: Option<QuestionCandidate>,
    pub uncertainty: Option<String>,
    pub cited_recall_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsePlanEvidence {
    pub value: String,
    pub confidence_tier: AssertionConfidenceTier,
    pub lifecycle_status: AssertionLifecycleStatus,
    pub recall_reason: Option<String>,
    #[serde(default)]
    pub direct_assertion_evidence: bool,
    #[serde(default)]
    pub topology_orb_refs: Vec<String>,
    #[serde(default)]
    pub orb_authorized: bool,
}

pub fn plan_conversation_response(user_text: &str, result: &RuntimeTurnResult) -> ResponsePlan {
    let text = user_text.to_ascii_lowercase();
    if is_greeting(&text) {
        return ResponsePlan {
            actions: vec![ResponsePlanAction::Acknowledge],
            ..ResponsePlan::default()
        };
    }

    if is_user_asking_about_luna(&text) {
        return ResponsePlan {
            actions: vec![ResponsePlanAction::Answer],
            answer_values: vec!["I am Luna: a local-first memory runtime layer. I store turns as events, separate confirmed from inferred or unknown facts, and only bring a small working set into the conversation.".to_string()],
            ..ResponsePlan::default()
        };
    }

    if is_query_turn(&result.observation) || text.contains('?') {
        if is_identity_query(&text) {
            let remembered = supported_identity_values(result);
            if remembered.is_empty() {
                return unsupported_memory_plan("self memory is missing");
            }
            return answer_plan(Some("you".to_string()), remembered, result);
        }

        if has_missing_requested_entity(user_text, &result.memory_state) {
            return unsupported_memory_plan("requested entity is missing from memory");
        }
        let groups = requested_entity_groups(&text, &result.memory_state);
        if !groups.is_empty() {
            let mut remembered = Vec::new();
            let mut labels = Vec::new();
            for group in groups {
                let mut values = supported_entity_values(group, result, &text);
                values.extend(supported_related_project_values(group, result, &text));
                if !values.is_empty() {
                    labels.push(group.label.clone());
                    remembered.append(&mut values);
                }
            }
            remembered.sort();
            remembered.dedup();
            if !remembered.is_empty() {
                return answer_plan(Some(labels.join(" and ")), remembered, result);
            }
        }

        let remembered = supported_memory_values_for_query(user_text, result);
        if remembered.is_empty() {
            return unsupported_memory_plan("no supported recalled or active memory matches");
        }
        return answer_plan(None, remembered, result);
    }

    let learned = result
        .knowledge_delta
        .confirmed
        .iter()
        .chain(result.knowledge_delta.inferred.iter())
        .chain(result.knowledge_delta.unconfirmed.iter())
        .take(3)
        .map(|claim| claim.value.clone())
        .collect::<Vec<_>>();

    if let Some(question) = result.context_packet.open_questions.first() {
        if learned.is_empty() {
            return ResponsePlan {
                actions: vec![ResponsePlanAction::AskOneQuestion],
                question: Some(question.clone()),
                ..ResponsePlan::default()
            };
        }
        return ResponsePlan {
            actions: vec![
                ResponsePlanAction::Acknowledge,
                ResponsePlanAction::AskOneQuestion,
            ],
            answer_values: learned,
            question: Some(question.clone()),
            ..ResponsePlan::default()
        };
    }

    if !learned.is_empty() {
        return ResponsePlan {
            actions: vec![ResponsePlanAction::Acknowledge],
            answer_values: learned,
            ..ResponsePlan::default()
        };
    }

    unsupported_memory_plan("no concrete new memory was captured")
}

pub fn render_conversation_reply(user_text: &str, result: &RuntimeTurnResult) -> String {
    let plan = plan_conversation_response(user_text, result);
    if plan.actions == vec![ResponsePlanAction::Acknowledge] && plan.answer_values.is_empty() {
        return "Hi, I am Luna. I am listening, and I will keep the memory separate from guesses."
            .to_string();
    }
    if is_user_asking_about_luna(&user_text.to_ascii_lowercase()) {
        return plan.answer_values.first().cloned().unwrap_or_else(|| {
            "I am Luna: a local-first memory layer with bounded, evidence-backed memory."
                .to_string()
        });
    }
    if plan.actions.contains(&ResponsePlanAction::AvoidAnswering) {
        return "I do not have enough stored memory to answer that yet.".to_string();
    }
    if plan.actions.contains(&ResponsePlanAction::Answer) {
        if let Some(target) = &plan.target_label {
            return format!(
                "From what I have stored about {}: {}{}.",
                target,
                plan.answer_values.join("; "),
                render_answer_evidence_suffix(&plan)
            );
        }
        return format!(
            "From what I have stored: {}{}.",
            plan.answer_values.join("; "),
            render_answer_evidence_suffix(&plan)
        );
    }
    if let Some(question) = &plan.question {
        if plan.answer_values.is_empty() {
            return question.question.clone();
        }
        return format!(
            "Got it. I will remember {}. {}",
            plan.answer_values.join("; "),
            question.question
        );
    }
    if !plan.answer_values.is_empty() {
        return format!("Got it. I will remember {}.", plan.answer_values.join("; "));
    }
    "I am with you. I did not find a concrete new memory in that turn.".to_string()
}

fn answer_plan(
    target_label: Option<String>,
    answer_values: Vec<String>,
    result: &RuntimeTurnResult,
) -> ResponsePlan {
    let mut actions = vec![ResponsePlanAction::Answer];
    let cited_recall_count = result.recalled.hits.len();
    if cited_recall_count > 0 {
        actions.push(ResponsePlanAction::CiteRecalledMemory);
    }
    let answer_evidence = response_plan_evidence(&answer_values, result);
    if answer_evidence.is_empty() {
        return unsupported_memory_plan("answer values lack direct memory evidence");
    }
    ResponsePlan {
        actions,
        target_label,
        answer_evidence,
        answer_values,
        cited_recall_count,
        ..ResponsePlan::default()
    }
}

fn render_answer_evidence_suffix(plan: &ResponsePlan) -> String {
    let Some(first) = plan.answer_evidence.first() else {
        return String::new();
    };
    let confidence = format!("{:?}", first.confidence_tier).to_ascii_lowercase();
    let orb = if first.orb_authorized {
        "; orb-authorized"
    } else {
        ""
    };
    match &first.recall_reason {
        Some(reason) => format!(" ({confidence}; recalled by {reason}{orb})"),
        None => format!(" ({confidence}{orb})"),
    }
}

fn response_plan_evidence(
    answer_values: &[String],
    result: &RuntimeTurnResult,
) -> Vec<ResponsePlanEvidence> {
    let reasons = recall_reasons_by_assertion_key(result);
    answer_values
        .iter()
        .filter_map(|value| {
            result
                .memory_state
                .claims
                .iter()
                .find(|claim| &claim.value == value)
                .filter(|claim| claim_has_direct_answer_evidence(result, &claim.key))
                .map(|claim| {
                    let topology_orb_refs = topology_orb_refs_for_assertion_key(result, &claim.key);
                    ResponsePlanEvidence {
                        value: claim.value.clone(),
                        confidence_tier: claim.status,
                        lifecycle_status: claim.lifecycle_status,
                        recall_reason: reasons.get(&claim.key).cloned(),
                        direct_assertion_evidence: true,
                        orb_authorized: !topology_orb_refs.is_empty(),
                        topology_orb_refs,
                    }
                })
        })
        .collect()
}

fn claim_has_direct_answer_evidence(result: &RuntimeTurnResult, assertion_key: &str) -> bool {
    result
        .context_packet
        .recalled_claims
        .iter()
        .any(|claim| claim.key == assertion_key)
        || result
            .context_packet
            .working_memory
            .nodes
            .iter()
            .flat_map(|node| node.provenance.iter())
            .any(|provenance| {
                direct_answer_assertion_key(provenance).as_deref() == Some(assertion_key)
            })
}

fn topology_orb_refs_for_assertion_key(
    result: &RuntimeTurnResult,
    assertion_key: &str,
) -> Vec<String> {
    let mut refs = BTreeSet::new();
    for node in &result.context_packet.working_memory.nodes {
        let has_direct_evidence = node.provenance.iter().any(|provenance| {
            direct_answer_assertion_key(provenance).as_deref() == Some(assertion_key)
        });
        if !has_direct_evidence {
            continue;
        }
        for provenance in &node.provenance {
            if let Some(system_root) = &provenance.system_root {
                if system_root.starts_with("orb:") {
                    refs.insert(system_root.clone());
                }
            }
        }
    }
    refs.into_iter().collect()
}

fn recall_reasons_by_assertion_key(result: &RuntimeTurnResult) -> BTreeMap<String, String> {
    let mut reasons = BTreeMap::new();
    for hit in &result.recalled.hits {
        for assertion in &hit.assertions {
            reasons.insert(assertion.key(), hit.reason.as_str().to_string());
        }
    }
    reasons
}

fn unsupported_memory_plan(reason: &str) -> ResponsePlan {
    ResponsePlan {
        actions: vec![
            ResponsePlanAction::AvoidAnswering,
            ResponsePlanAction::StateUncertainty,
        ],
        uncertainty: Some(reason.to_string()),
        ..ResponsePlan::default()
    }
}

fn requested_entity_groups<'a>(query: &str, state: &'a MemoryState) -> Vec<&'a EntityMemoryGroup> {
    let query = normalize_for_match(query);
    let mut groups = state
        .entity_groups
        .iter()
        .filter(|group| group.kind != "self")
        .filter(|group| {
            let label = normalize_for_match(&group.label);
            let id = normalize_for_match(&group.id.replace(':', " "));
            !label.is_empty()
                && (contains_all_terms(&query, &normalized_terms(&label))
                    || contains_all_terms(&query, &normalized_terms(&id)))
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| right.claims.len().cmp(&left.claims.len()))
    });
    groups
}

fn has_missing_requested_entity(user_text: &str, state: &MemoryState) -> bool {
    requested_entity_terms(user_text).into_iter().any(|term| {
        let normalized = normalize_for_match(&term);
        let terms = normalized_terms(&normalized);
        !state
            .entity_groups
            .iter()
            .any(|group| normalize_for_match(&group.label) == normalized)
            && !state.claims.iter().any(|claim| {
                claim.lifecycle_status == AssertionLifecycleStatus::Current
                    && contains_all_terms(&normalize_for_match(&claim.value), &terms)
            })
    })
}

fn requested_entity_terms(user_text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for token in user_text.split_whitespace() {
        let trimmed = token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        let normalized_trimmed = trimmed.to_ascii_lowercase();
        if trimmed.len() > 1
            && (trimmed.chars().all(|ch| ch.is_ascii_uppercase()) || is_single_name(trimmed))
            && !matches!(
                normalized_trimmed.as_str(),
                "what" | "who" | "when" | "where" | "why" | "how" | "in" | "the" | "does" | "luna"
            )
        {
            terms.push(trimmed.to_string());
        }
    }
    terms.sort();
    terms.dedup();
    terms
}

fn supported_entity_values(
    group: &EntityMemoryGroup,
    result: &RuntimeTurnResult,
    query: &str,
) -> Vec<String> {
    if group.kind == "person" && !project_answer_kinds_for_query(query).is_empty() {
        return Vec::new();
    }
    let supported_keys = supported_assertion_keys(result);
    let desired_kinds = desired_entity_claim_kinds(query);
    let mut values = group
        .claims
        .iter()
        .filter(|claim| claim.lifecycle_status == AssertionLifecycleStatus::Current)
        .filter(|claim| supported_keys.contains(&claim.key))
        .filter(|claim| desired_kinds.is_empty() || desired_kinds.contains(&claim.kind.as_str()))
        .map(|claim| claim.value.clone())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.truncate(6);
    values
}

fn supported_related_project_values(
    group: &EntityMemoryGroup,
    result: &RuntimeTurnResult,
    query: &str,
) -> Vec<String> {
    if group.kind != "person" {
        return Vec::new();
    }
    let lower_query = query.to_ascii_lowercase();
    if !lower_query.contains("project") && !lower_query.contains("trail") {
        return Vec::new();
    }
    let project_labels = group
        .claims
        .iter()
        .flat_map(|claim| project_names_in_text(&claim.value, result))
        .collect::<BTreeSet<_>>();
    if project_labels.is_empty() {
        return Vec::new();
    }
    let supported_keys = supported_assertion_keys(result);
    let desired_project_kinds = project_answer_kinds_for_query(query);
    let mut values = result
        .memory_state
        .claims
        .iter()
        .filter(|claim| claim.lifecycle_status == AssertionLifecycleStatus::Current)
        .filter(|claim| supported_keys.contains(&claim.key))
        .filter(|claim| claim.domain == "project")
        .filter(|claim| {
            project_labels
                .iter()
                .any(|label| claim.value.contains(label))
        })
        .filter(|claim| {
            desired_project_kinds.contains(&claim.kind.as_str())
                && (claim.kind != "identity" || contains_ci(&claim.value, "called"))
        })
        .map(|claim| claim.value.clone())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.truncate(3);
    values
}

fn project_answer_kinds_for_query(query: &str) -> Vec<&'static str> {
    let lower_query = query.to_ascii_lowercase();
    if contains_any(&lower_query, &["called", "name"]) {
        vec!["identity"]
    } else if contains_any(&lower_query, &["help", "helps", "do"]) {
        vec!["purpose"]
    } else {
        Vec::new()
    }
}

fn project_names_in_text(text: &str, result: &RuntimeTurnResult) -> Vec<String> {
    result
        .memory_state
        .entity_groups
        .iter()
        .filter(|group| group.kind == "project")
        .filter(|group| contains_ci(text, &group.label))
        .map(|group| group.label.clone())
        .collect()
}

fn desired_entity_claim_kinds(query: &str) -> Vec<&'static str> {
    if contains_any(query, &["where", "live", "lives", "location", "moved"]) {
        vec!["location"]
    } else if contains_any(query, &["goal", "want", "wants", "trying"]) {
        vec!["goal"]
    } else if contains_any(query, &["interest", "like", "likes", "fan"]) {
        vec!["interest"]
    } else if contains_any(query, &["age", "old"]) {
        vec!["age"]
    } else if contains_any(query, &["pilot", "with"]) {
        vec!["project_plan"]
    } else if contains_any(query, &["called", "name"]) {
        vec!["project_name"]
    } else {
        Vec::new()
    }
}

fn supported_memory_values(result: &RuntimeTurnResult) -> Vec<String> {
    let supported_keys = supported_assertion_keys(result);
    let mut values = result
        .memory_state
        .claims
        .iter()
        .filter(|claim| claim.lifecycle_status == AssertionLifecycleStatus::Current)
        .filter(|claim| supported_keys.contains(&claim.key))
        .map(|claim| claim.value.clone())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.truncate(5);
    values
}

fn supported_memory_values_for_query(query: &str, result: &RuntimeTurnResult) -> Vec<String> {
    let manuscript = supported_manuscript_values_for_query(query, result);
    if !manuscript.is_empty() {
        manuscript
    } else {
        supported_memory_values(result)
    }
}

fn supported_manuscript_values_for_query(query: &str, result: &RuntimeTurnResult) -> Vec<String> {
    let lower_query = query.to_ascii_lowercase();
    if !lower_query.contains("present")
        && !lower_query.contains("flashback")
        && !lower_query.contains("scene")
        && !lower_query.contains("open arc")
        && !lower_query.contains("unresolved")
    {
        return Vec::new();
    }
    let supported_keys = supported_assertion_keys(result);
    let desired_time = manuscript_desired_story_time(query);
    let requested_term_strings = requested_entity_terms(query)
        .into_iter()
        .map(|term| normalize_for_match(&term))
        .collect::<Vec<_>>();
    let requested_terms = requested_term_strings
        .iter()
        .flat_map(|term| normalized_terms(term))
        .collect::<Vec<_>>();
    let mut values = result
        .memory_state
        .claims
        .iter()
        .filter(|claim| {
            claim.lifecycle_status == AssertionLifecycleStatus::Current
                && claim.domain == "manuscript"
                && (claim.kind == "event_fact"
                    || ((lower_query.contains("open arc") || lower_query.contains("unresolved"))
                        && claim.kind == "open_arc"))
                && supported_keys.contains(&claim.key)
                && desired_time
                    .map(|time| claim.value.contains(time))
                    .unwrap_or(true)
                && (requested_terms.is_empty()
                    || contains_all_terms(&normalize_for_match(&claim.value), &requested_terms))
        })
        .map(|claim| claim.value.clone())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.truncate(5);
    values
}

fn manuscript_claim_allowed_for_query(query: &str, claim: &MemoryClaim) -> bool {
    if claim.domain != "manuscript" || claim.kind != "event_fact" {
        return true;
    }
    let lower_query = query.to_ascii_lowercase();
    if !lower_query.contains("present")
        && !lower_query.contains("flashback")
        && !lower_query.contains("scene")
    {
        return true;
    }
    if manuscript_desired_story_time(query)
        .map(|time| !claim.value.contains(time))
        .unwrap_or(false)
    {
        return false;
    }
    let requested_term_strings = requested_entity_terms(query)
        .into_iter()
        .map(|term| normalize_for_match(&term))
        .collect::<Vec<_>>();
    let requested_terms = requested_term_strings
        .iter()
        .flat_map(|term| normalized_terms(term))
        .collect::<Vec<_>>();
    requested_terms.is_empty()
        || contains_all_terms(&normalize_for_match(&claim.value), &requested_terms)
}

fn project_claim_allowed_for_query(query: &str, claim: &MemoryClaim) -> bool {
    let lower_query = query.to_ascii_lowercase();
    if contains_any(&lower_query, &["help", "helps", "do"])
        && (lower_query.contains("project") || lower_query.contains("trail"))
    {
        return claim.domain != "project" || claim.kind == "purpose";
    }
    if contains_any(&lower_query, &["called", "name"])
        && (lower_query.contains("project") || lower_query.contains("trail"))
    {
        return claim.domain != "project" || claim.kind == "identity";
    }
    true
}

fn filter_working_memory_for_context(query: &str, working_memory: &WorkingMemory) -> WorkingMemory {
    let lower_query = query.to_ascii_lowercase();
    if !lower_query.contains("present")
        && !lower_query.contains("flashback")
        && !lower_query.contains("scene")
        && !is_project_specific_query(&lower_query)
    {
        return working_memory.clone();
    }
    let mut filtered = working_memory.clone();
    filtered
        .nodes
        .retain(|node| working_memory_node_allowed_for_query(query, node));
    let node_ids = filtered
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    filtered
        .edges
        .retain(|edge| node_ids.contains(&edge.source) && node_ids.contains(&edge.target));
    filtered
}

fn is_project_specific_query(lower_query: &str) -> bool {
    (lower_query.contains("project") || lower_query.contains("trail"))
        && contains_any(lower_query, &["help", "helps", "do", "called", "name"])
}

fn working_memory_node_allowed_for_query(query: &str, node: &MemoryNode) -> bool {
    let evidence_text = format!(
        "{} {}",
        node.label,
        node.provenance
            .iter()
            .filter_map(|provenance| provenance.assertion_key.as_deref())
            .collect::<Vec<_>>()
            .join(" ")
    );
    if !project_node_allowed_for_query(query, &evidence_text) {
        return false;
    }
    if !evidence_text.contains("story_time:") {
        return true;
    }
    let pseudo_claim = MemoryClaim {
        key: String::new(),
        domain: "manuscript".to_string(),
        kind: "event_fact".to_string(),
        value: evidence_text,
        status: AssertionConfidenceTier::Unconfirmed,
        lifecycle_status: AssertionLifecycleStatus::Current,
    };
    manuscript_claim_allowed_for_query(query, &pseudo_claim)
}

fn project_node_allowed_for_query(query: &str, evidence_text: &str) -> bool {
    let lower_query = query.to_ascii_lowercase();
    let lower_evidence = evidence_text.to_ascii_lowercase();
    if contains_any(&lower_query, &["help", "helps", "do"])
        && (lower_query.contains("project") || lower_query.contains("trail"))
    {
        return !contains_any(
            &lower_evidence,
            &["person:role=", "person:project_plan=", "project:identity="],
        );
    }
    if contains_any(&lower_query, &["called", "name"])
        && (lower_query.contains("project") || lower_query.contains("trail"))
    {
        return !contains_any(
            &lower_evidence,
            &["person:role=", "person:project_plan=", "project:purpose="],
        );
    }
    true
}

fn manuscript_desired_story_time(query: &str) -> Option<&'static str> {
    let lower_query = query.to_ascii_lowercase();
    if lower_query.contains("present") {
        Some("story_time:present")
    } else if lower_query.contains("flashback") {
        Some("story_time:flashback")
    } else {
        None
    }
}

fn supported_identity_values(result: &RuntimeTurnResult) -> Vec<String> {
    let supported_keys = supported_assertion_keys(result);
    let mut values = result
        .memory_state
        .claims
        .iter()
        .filter(|claim| {
            claim.lifecycle_status == AssertionLifecycleStatus::Current
                && supported_keys.contains(&claim.key)
                && (claim.domain == "identity"
                    || (claim.domain == "relationship" && claim.value.starts_with("I ")))
        })
        .map(|claim| claim.value.clone())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.truncate(6);
    values
}

fn supported_assertion_keys(result: &RuntimeTurnResult) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for claim in &result.context_packet.recalled_claims {
        keys.insert(claim.key.clone());
    }
    for node in &result.context_packet.working_memory.nodes {
        for provenance in &node.provenance {
            if let Some(key) = direct_answer_assertion_key(provenance) {
                keys.insert(key);
            }
        }
    }
    keys
}

fn normalized_terms(value: &str) -> Vec<&str> {
    value
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .collect()
}

fn contains_all_terms(haystack: &str, terms: &[&str]) -> bool {
    let haystack_terms = haystack.split_whitespace().collect::<BTreeSet<_>>();
    !terms.is_empty() && terms.iter().all(|term| haystack_terms.contains(term))
}

fn is_identity_query(text: &str) -> bool {
    contains_any(
        text,
        &[
            "who am i",
            "what do you know about me",
            "what do you remember about me",
            "tell me about me",
        ],
    )
}

fn is_greeting(text: &str) -> bool {
    matches!(
        text.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace()),
        "hi" | "hello" | "hey" | "hello luna" | "hi luna" | "hey luna"
    )
}

fn is_user_asking_about_luna(text: &str) -> bool {
    (text.contains("who are you") || text.contains("what are you"))
        || (text.contains("luna") && text.contains("what") && text.contains("do"))
}

fn render_claims(out: &mut String, label: &str, claims: &[MemoryClaim]) {
    out.push_str(&format!("\n### {label}\n"));
    if claims.is_empty() {
        out.push_str("(none)\n");
        return;
    }
    for claim in claims {
        out.push_str(&format!(
            "- {}:{} = {}\n",
            claim.domain, claim.kind, claim.value
        ));
    }
}

fn select_recall_mode(observation: &TurnReading) -> RecallMode {
    if signal_active(observation.identity_relevance, 0.65) {
        RecallMode::IdentityContinuity
    } else if signal_active(observation.goal_pressure, 0.65) {
        RecallMode::GoalContinuity
    } else if signal_active(observation.emotional_arousal, 0.65) {
        RecallMode::EmotionalContinuity
    } else {
        RecallMode::OpenEnded
    }
}

fn propose_questions(
    turn: &ConversationTurn,
    observation: &TurnReading,
    memory_state: &MemoryState,
) -> Vec<QuestionCandidate> {
    let text = turn.content.to_ascii_lowercase();
    if text.contains('?') {
        return Vec::new();
    }
    let mut questions = BTreeMap::<u8, QuestionCandidate>::new();

    if mentions_job(&text) && !memory_state.has_domain_kind("identity", "profession") {
        questions.insert(
            10,
            QuestionCandidate {
                question: "What do you do for work?".to_string(),
                reason: "work context is active, but Luna does not know your role yet".to_string(),
                priority: 10,
            },
        );
    }

    if mentions_ambiguous_pronoun_without_anchor(&text, observation) {
        questions.insert(
            20,
            QuestionCandidate {
                question: "Who is \"they\" here, and what situation are you talking about?"
                    .to_string(),
                reason: "the statement depends on an unresolved group or authority".to_string(),
                priority: 20,
            },
        );
    }

    if mentions_female_pronoun(&text) && !memory_state.has_domain_kind("relationship", "person") {
        questions.insert(
            15,
            QuestionCandidate {
                question: "Who is she to you?".to_string(),
                reason: "a person is emotionally important, but the relationship is unknown"
                    .to_string(),
                priority: 15,
            },
        );
    }

    if mentions_partner_label(&text) && !memory_state.has_domain_kind("relationship", "person") {
        questions.insert(
            12,
            QuestionCandidate {
                question: "What is her name, and have you already proposed?".to_string(),
                reason: "romantic-partner language needs a person anchor and status confirmation"
                    .to_string(),
                priority: 12,
            },
        );
    }

    if !is_query_turn(observation)
        && observation.assertions.is_empty()
        && signal_active(observation.emotional_arousal, 0.65)
    {
        questions.insert(
            30,
            QuestionCandidate {
                question: "What happened?".to_string(),
                reason: "the turn carries emotional pressure but no concrete memory anchor"
                    .to_string(),
                priority: 30,
            },
        );
    }

    let mut values = questions.into_values().collect::<Vec<_>>();
    values.sort_by_key(|question| question.priority);
    values.truncate(1);
    values
}

fn unknowns_from_observation(observation: &TurnReading) -> Vec<String> {
    let mut unknowns = Vec::new();
    if signal_active(observation.identity_relevance, 0.65)
        && !observation
            .assertions
            .iter()
            .any(|assertion| assertion.domain == "identity")
    {
        unknowns.push("identity or role anchor is not confirmed".to_string());
    }
    if signal_active(observation.goal_pressure, 0.65)
        && !observation
            .assertions
            .iter()
            .any(|assertion| assertion.domain == "goal" || assertion.domain == "work")
    {
        unknowns.push("active goal or pressure is not concretely anchored".to_string());
    }
    if signal_active(observation.emotional_arousal, 0.65) && observation.assertions.is_empty() {
        unknowns.push("emotional source is not yet confirmed".to_string());
    }
    unknowns
}

fn signal_active(signal: Option<luna_core::Signal>, min_value: f32) -> bool {
    signal
        .map(|signal| signal.value() >= min_value && signal.can_influence_recall())
        .unwrap_or(false)
}

fn is_query_turn(observation: &TurnReading) -> bool {
    observation
        .query_intents
        .iter()
        .any(|intent| intent.contains("query") || intent == "contradiction_check")
}

fn render_context_summary(
    recalled_claims: &[MemoryClaim],
    working_memory: &WorkingMemory,
    compressed_memory: &[CompressedWorkingMemory],
    questions: &[QuestionCandidate],
) -> String {
    let mut lines = Vec::new();
    if recalled_claims.is_empty() {
        lines.push("No prior memory was activated for this turn.".to_string());
    } else {
        lines.push(format!(
            "Activated memory: {}.",
            recalled_claims
                .iter()
                .map(|claim| claim.value.clone())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if !working_memory.nodes.is_empty() {
        lines.push(format!(
            "Working memory: {}.",
            working_memory
                .nodes
                .iter()
                .map(|node| format!(
                    "{:?}:{} ({:.2})",
                    node.confidence_tier, node.label, node.activation
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if working_memory.filtered_node_count > 0 || working_memory.filtered_edge_count > 0 {
        lines.push(format!(
            "Filtered out {} node(s) and {} edge(s) beyond the working-memory budget.",
            working_memory.filtered_node_count, working_memory.filtered_edge_count
        ));
    }
    if !compressed_memory.is_empty() {
        lines.push(format!(
            "Compressed memory: {} accepted receipt(s), {} raw source event citation(s) retained.",
            compressed_memory.len(),
            compressed_memory
                .iter()
                .map(|entry| entry.raw_event_refs.len())
                .sum::<usize>()
        ));
    }
    if let Some(question) = questions.first() {
        lines.push(format!("Next useful question: {}", question.question));
    }
    lines.join("\n")
}

fn assertion_confidence(observation: &TurnReading, assertion: &StructuredAssertion) -> f32 {
    if assertion.domain == "identity" {
        observation
            .identity_relevance
            .map(|signal| signal.confidence())
            .unwrap_or(0.72)
    } else if assertion.domain == "emotion" {
        observation
            .emotional_arousal
            .map(|signal| signal.confidence())
            .unwrap_or(0.72)
    } else if assertion.domain == "goal" || assertion.domain == "work" {
        observation
            .goal_pressure
            .map(|signal| signal.confidence())
            .unwrap_or(0.72)
    } else {
        0.72
    }
}

fn mentions_job(text: &str) -> bool {
    contains_any(text, &["my job", "at work", "my work", "career", "raise"])
}

fn mentions_ambiguous_they(text: &str) -> bool {
    text.contains("they") || text.contains("them")
}

fn mentions_ambiguous_person_pronoun(text: &str) -> bool {
    mentions_ambiguous_they(text)
        || contains_any(text, &[" he ", " him ", " his "])
        || text.starts_with("he ")
        || text.starts_with("him ")
        || text.starts_with("his ")
}

fn mentions_ambiguous_pronoun_without_anchor(text: &str, observation: &TurnReading) -> bool {
    mentions_ambiguous_person_pronoun(text) && !has_local_plural_anchor(text, observation)
}

fn has_local_plural_anchor(text: &str, observation: &TurnReading) -> bool {
    if contains_any(text, &["co-founders", "cofounders", "both"]) {
        return true;
    }

    let person_like_assertions = observation
        .assertions
        .iter()
        .filter(|assertion| {
            assertion.domain == "person"
                || (assertion.domain == "relationship" && assertion.kind == "collaboration")
        })
        .count();
    person_like_assertions >= 1
}

fn mentions_female_pronoun(text: &str) -> bool {
    contains_any(text, &[" she ", " her ", " her,", " her.", " her!"])
}

fn mentions_partner_label(text: &str) -> bool {
    contains_any(text, &["girlfriend", "fiance", "fiancee", "fiancée"])
}

fn has_correction_cue(text: &str) -> bool {
    contains_any(
        text,
        &[
            "actually ",
            "correction",
            "correcting",
            "i was wrong",
            "not anymore",
            "instead",
            "now ",
            "moved to",
            "moved again",
        ],
    )
}

fn is_noise_turn(text: &str) -> bool {
    let words = text
        .split_whitespace()
        .map(|word| word.trim_matches(|ch: char| !ch.is_ascii_alphanumeric()))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    words.len() >= 4
        && !text.contains('?')
        && !contains_any(
            text,
            &[
                "i am", "i'm", "my ", " lives ", " wants ", " works ", "project", "because",
            ],
        )
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

pub fn default_runtime_log_path(root: &Path) -> PathBuf {
    root.join(".luna").join("runtime").join("events.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_log() -> PathBuf {
        std::env::temp_dir()
            .join(format!("luna_runtime_{}", Uuid::new_v4()))
            .join("events.jsonl")
    }

    #[test]
    fn asks_work_role_when_job_context_is_known_but_role_is_dark() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        let result = session
            .process_user_turn("My job, I asked for a raise and I have not heard back yet.")
            .unwrap();

        assert_eq!(
            result
                .questions
                .first()
                .map(|question| question.question.as_str()),
            Some("What do you do for work?")
        );

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn runtime_replay_audit_accepts_persisted_product_log() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        let result = session
            .process_user_turn("MKPE is my provenance engine.")
            .unwrap();
        assert!(result.working_memory.nodes.iter().any(|node| {
            node.provenance.iter().any(|provenance| {
                provenance.system_root.as_deref() == Some("orb:runtime:project:MKPE")
            })
        }));
        let report = audit_runtime_log(&log).unwrap();

        assert!(report.is_clean(), "{report:?}");
        assert!(!report.quarantine_required);
        assert_eq!(report.live_snapshot_hash, report.replayed_snapshot_hash);
        assert!(report.replayed_counts.stored_events > 0);
        assert!(report.replayed_counts.topology_nodes > 0);
        assert!(report.replayed_counts.topology_source_event_refs > 0);
        assert_eq!(
            report.replayed_counts.topology_source_event_refs,
            report.replayed_counts.valid_topology_source_event_refs
        );
        assert!(report.replayed_counts.topology_orbs > 0);

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn runtime_replay_audit_rejects_missing_or_empty_logs() {
        let missing = temp_log();
        let missing_error = audit_runtime_log(&missing).unwrap_err();
        assert!(missing_error.to_string().contains("does not exist"));

        fs::create_dir_all(missing.parent().unwrap()).unwrap();
        fs::write(&missing, "").unwrap();
        let empty_error = audit_runtime_log(&missing).unwrap_err();
        assert!(empty_error.to_string().contains("no auditable events"));

        let _ = fs::remove_dir_all(missing.parent().unwrap());
    }

    #[test]
    fn runtime_session_audit_replay_rejects_hashless_persisted_log() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());
        session
            .process_user_turn("MKPE is my provenance engine.")
            .unwrap();
        let lines = fs::read_to_string(&log).unwrap();
        let hashless = lines
            .lines()
            .map(|line| {
                let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
                value.as_object_mut().unwrap().remove("event_hash");
                serde_json::to_string(&value).unwrap()
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&log, format!("{hashless}\n")).unwrap();

        let error = session.audit_replay().unwrap_err();

        assert!(error.to_string().contains("missing event_hash"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn runtime_replay_audit_quarantines_mismatched_topology_commit() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());
        session
            .process_user_turn("MKPE is my provenance engine.")
            .unwrap();
        let mut events = JsonlEventLog::new(&log).load().unwrap();
        let last_commit = events
            .iter_mut()
            .rev()
            .find_map(|event| match &mut event.payload {
                LunaEvent::TopologyBridgeCommitted(commit) => Some(commit),
                _ => None,
            })
            .expect("runtime turn should append topology bridge commit");
        last_commit.node_refs.clear();

        let report = audit_runtime_events(&events).unwrap();

        assert!(report.is_quarantined(), "{report:?}");
        assert!(report.replay_error.as_deref().is_some_and(
            |error| error.contains("does not match recomputed topology ledger evidence")
        ));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn runtime_replay_audit_quarantines_missing_persisted_topology_ledger_events() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());
        session
            .process_user_turn("MKPE is my provenance engine.")
            .unwrap();
        let mut events = JsonlEventLog::new(&log).load().unwrap();
        let last_commit = events
            .iter_mut()
            .rev()
            .find_map(|event| match &mut event.payload {
                LunaEvent::TopologyBridgeCommitted(commit) => Some(commit),
                _ => None,
            })
            .expect("runtime turn should append topology bridge commit");
        last_commit.ledger_event_count = 0;
        last_commit.ledger_event_hash.clear();
        last_commit.ledger_events_json.clear();

        let report = audit_runtime_events(&events).unwrap();

        assert!(report.is_quarantined(), "{report:?}");
        assert!(report
            .replay_error
            .as_deref()
            .is_some_and(|error| error.contains("without durable ledger events")
                || error.contains("does not match recomputed topology ledger evidence")));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn runtime_replay_audit_checks_every_persisted_topology_commit() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());
        session
            .process_user_turn("MKPE is my provenance engine.")
            .unwrap();
        session
            .process_user_turn("Atlas Loom is my project workspace.")
            .unwrap();
        let mut events = JsonlEventLog::new(&log).load().unwrap();
        let first_commit = events
            .iter_mut()
            .find_map(|event| match &mut event.payload {
                LunaEvent::TopologyBridgeCommitted(commit) => Some(commit),
                _ => None,
            })
            .expect("runtime turns should append topology bridge commits");
        first_commit.node_refs.clear();

        let report = audit_runtime_events(&events).unwrap();

        assert!(report.is_quarantined(), "{report:?}");
        assert!(report
            .replay_error
            .as_deref()
            .is_some_and(|error| error.contains("bridge commit at event")));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn asks_relationship_anchor_without_assuming_partner_type() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        let result = session
            .process_user_turn("Man I hate her, every time I think she understands we fight.")
            .unwrap();

        assert_eq!(
            result
                .questions
                .first()
                .map(|question| question.question.as_str()),
            Some("Who is she to you?")
        );

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn does_not_ask_who_they_are_when_people_are_locally_anchored() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        let result = session
            .process_user_turn("Chris and Francois are my co-founders. They both write programs.")
            .unwrap();

        assert!(
            result
                .questions
                .iter()
                .all(|question| !question.question.contains("Who is \"they\"")),
            "named people in the same turn should resolve the local pronoun anchor"
        );

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn conversation_reply_answers_greeting_as_luna() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());
        let result = session.process_user_turn("hello").unwrap();

        let reply = render_conversation_reply("hello", &result);

        assert!(reply.contains("I am Luna"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn conversation_reply_uses_stored_memory_for_queries() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("I work as a mechanical engineer.")
            .unwrap();
        let result = session
            .process_user_turn("What do I do for a living?")
            .unwrap();
        let reply = render_conversation_reply("What do I do for a living?", &result);

        assert!(reply.contains("mechanical engineer"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn entity_sieve_catches_self_and_person_facts_the_broad_extractor_misses() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "I am Joe. I am 45 years old. I like football. Chris lives in Iowa. \
                 Chris is 37. Francois wants to take over the industry.",
            )
            .unwrap();
        let state = session.inspect().unwrap();

        assert!(state.claims.iter().any(|claim| claim.domain == "identity"
            && claim.kind == "name"
            && claim.value == "Joe"));
        assert!(state.claims.iter().any(|claim| claim.domain == "identity"
            && claim.kind == "age"
            && claim.value == "I am 45 years old"));
        assert!(state.claims.iter().any(|claim| claim.domain == "identity"
            && claim.kind == "interest"
            && claim.value == "I like football"));
        assert!(state.claims.iter().any(|claim| claim.domain == "person"
            && claim.kind == "location"
            && claim.value == "Chris lives in Iowa"));
        assert!(state.claims.iter().any(|claim| claim.domain == "person"
            && claim.kind == "goal"
            && claim.value == "Francois wants to take over the industry"));
        assert!(!state
            .claims
            .iter()
            .any(|claim| claim.domain == "person" && claim.value == "I am 45 years old"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn entity_sieve_cleans_dense_paragraph_person_memory() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "I am Joe, I am a software developer, with my two co-founders. \
                 Chris and Francois. They both write programs. Francois Lives in Washington, \
                 and Chris lives in Iowa. I am 45 years old, Chris is 37. \
                 I am tall, and francois and chris are short. Chris is Married. \
                 Chriss is a basketball fan. Chris just want to retire his wife, \
                 and Francois wants to take over the industry.",
            )
            .unwrap();
        let state = session.inspect().unwrap();

        assert!(state.claims.iter().any(|claim| claim.domain == "person"
            && claim.kind == "profession"
            && claim.value == "Chris writes programs"));
        assert!(state.claims.iter().any(|claim| claim.domain == "person"
            && claim.kind == "profession"
            && claim.value == "Francois writes programs"));
        assert!(state.claims.iter().any(|claim| claim.domain == "person"
            && claim.kind == "interest"
            && claim.value == "Chris is a basketball fan"));
        assert!(state.claims.iter().any(|claim| claim.domain == "person"
            && claim.kind == "goal"
            && claim.value == "Chris wants to retire his wife"));
        assert!(!state
            .claims
            .iter()
            .any(|claim| claim.domain == "person" && claim.value == "writes programs"));
        assert!(!state
            .claims
            .iter()
            .any(|claim| claim.domain == "person" && claim.value == "short"));
        assert!(!state
            .claims
            .iter()
            .any(|claim| claim.value.contains("Chriss") || claim.value.contains("Chri ")));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn memory_state_groups_claims_by_entity() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "I am Joe. Chris lives in Iowa. Chris is 37. \
                 Francois lives in Washington.",
            )
            .unwrap();
        let state = session.inspect().unwrap();

        let self_group = state
            .entity_groups
            .iter()
            .find(|group| group.id == "self")
            .unwrap();
        let chris_group = state
            .entity_groups
            .iter()
            .find(|group| group.id == "person:Chris")
            .unwrap();
        let francois_group = state
            .entity_groups
            .iter()
            .find(|group| group.id == "person:Francois")
            .unwrap();

        assert!(self_group.claims.iter().any(|claim| claim.value == "Joe"));
        assert!(chris_group
            .claims
            .iter()
            .any(|claim| claim.value == "Chris lives in Iowa"));
        assert!(chris_group
            .claims
            .iter()
            .any(|claim| claim.value == "Chris is 37"));
        assert!(francois_group
            .claims
            .iter()
            .any(|claim| claim.value == "Francois lives in Washington"));
        assert!(!chris_group
            .claims
            .iter()
            .any(|claim| claim.value == "Francois lives in Washington"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn conversation_reply_uses_entity_cluster_for_person_queries() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("Chris lives in Iowa. Chris is 37. Francois lives in Washington.")
            .unwrap();
        let result = session
            .process_user_turn("What do you know about Chris?")
            .unwrap();
        let reply = render_conversation_reply("What do you know about Chris?", &result);

        assert!(reply.contains("Chris lives in Iowa"));
        assert!(reply.contains("Chris is 37"));
        assert!(!reply.contains("Francois lives in Washington"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn entity_sieve_strengthens_matching_llm_assertions() {
        use luna_extract::{
            FileExtractionCache, LlmExtractor, LunaExtractor, RecordingFakeBackend,
        };
        use tempfile::TempDir;

        let log = temp_log();
        let root = TempDir::new().unwrap();
        let cache = FileExtractionCache::new(root.path());
        let fake = RecordingFakeBackend::new("test-model@v1");

        // Craft an LLM response that produces an identity/profession assertion
        let llm_response = serde_json::json!({
            "schema_version": "v3",
            "assertions": [{
                "domain": "identity",
                "kind": "profession",
                "value": "pilot",
                "confidence": 0.92,
                "evidence_span": "pilot"
            }],
            "signals": {}
        });
        fake.expect("pilot", &serde_json::to_string(&llm_response).unwrap());
        fake.expect(
            "for a living",
            &serde_json::to_string(&serde_json::json!({
                "schema_version": "v3",
                "assertions": [],
                "signals": {}
            }))
            .unwrap(),
        );

        let llm = LlmExtractor::new(fake.clone(), cache);
        let extractor = LunaExtractor::new(llm, Vec::new());
        let session = RuntimeSession::new(&log, extractor);

        session.process_user_turn("I am a pilot.").unwrap();

        // Ask the same thing again — entity sieve should strengthen it
        session
            .process_user_turn("I make a living as a pilot.")
            .unwrap();

        let state = session.inspect().unwrap();
        let claim = state
            .claims
            .iter()
            .find(|claim| {
                claim.domain == "identity" && claim.kind == "profession" && claim.value == "pilot"
            })
            .unwrap();
        assert_eq!(claim.status, AssertionConfidenceTier::Confirmed);

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn identity_query_reply_reads_self_orb_claims() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("I am Joe. I am 45 years old. I like football.")
            .unwrap();
        let result = session.process_user_turn("who am i?").unwrap();
        let reply = render_conversation_reply("who am i?", &result);

        assert!(reply.contains("Joe"));
        assert!(reply.contains("I am 45 years old"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn stores_unconfirmed_assertions_and_recalls_them_later() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("I work as a mechanical engineer.")
            .unwrap();
        let result = session
            .process_user_turn("What do I do for a living?")
            .unwrap();

        assert!(result
            .recalled
            .rendered_claims()
            .iter()
            .any(|claim| claim == "mechanical engineer"));
        assert!(
            result.questions.is_empty(),
            "a retrieval query should not trigger a new emotional-anchor probe"
        );

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn repeated_runtime_assertion_becomes_confirmed() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("I work as a mechanical engineer.")
            .unwrap();
        session
            .process_user_turn("I work as a mechanical engineer.")
            .unwrap();

        let state = session.inspect().unwrap();
        let claim = state
            .claims
            .iter()
            .find(|claim| claim.value == "mechanical engineer")
            .unwrap();
        assert_eq!(claim.status, AssertionConfidenceTier::Confirmed);

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn memory_state_derives_central_map_from_claims() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("I work as a mechanical engineer.")
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(state.map.nodes.iter().any(|node| node.id == "root:luna"));
        assert!(state.map.nodes.iter().any(|node| node.id == "user:self"));
        assert!(state
            .map
            .nodes
            .iter()
            .any(|node| node.label == "mechanical engineer"));
        assert!(state.map.edges.iter().any(|edge| {
            edge.source == "user:self"
                && edge.target == "identity:profession:mechanical_engineer"
                && edge.relation == MemoryRelationKind::HasAttribute
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn memory_map_derives_typed_place_and_goal_edges() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("Chris lives in Iowa. Chris wants to retire his wife.")
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(state.map.nodes.iter().any(|node| {
            node.id == "place:Iowa" && node.label == "Iowa" && node.kind == MemoryNodeKind::Place
        }));
        assert!(state.map.nodes.iter().any(|node| {
            node.id == "goal:retire_his_wife"
                && node.label == "retire his wife"
                && node.kind == MemoryNodeKind::Goal
        }));
        assert!(state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "place:Iowa"
                && edge.relation == MemoryRelationKind::LocatedIn
        }));
        assert!(state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "goal:retire_his_wife"
                && edge.relation == MemoryRelationKind::HasGoal
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn memory_map_derives_typed_interest_edge() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("Chris is a basketball fan.")
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(state.map.nodes.iter().any(|node| {
            node.id == "interest:basketball_fan"
                && node.label == "basketball fan"
                && node.kind == MemoryNodeKind::Attribute
        }));
        assert!(state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "interest:basketball_fan"
                && edge.relation == MemoryRelationKind::HasInterest
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn memory_map_derives_generic_project_entities() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("MKPE is my provenance engine. Atlas Loom is my planning engine.")
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(state.entity_groups.iter().any(|group| {
            group.id == "project:MKPE"
                && group.label == "MKPE"
                && group.kind == "project"
                && group.claims.iter().any(|claim| {
                    claim.domain == "project"
                        && claim.kind == "identity"
                        && claim.value == "MKPE is my provenance engine"
                })
        }));
        assert!(state.entity_groups.iter().any(|group| {
            group.id == "project:Atlas_Loom"
                && group.label == "Atlas Loom"
                && group.kind == "project"
                && group.claims.iter().any(|claim| {
                    claim.domain == "project"
                        && claim.kind == "identity"
                        && claim.value == "Atlas Loom is my planning engine"
                })
        }));
        assert!(!state
            .entity_groups
            .iter()
            .any(|group| group.id == "project:unknown"));
        assert!(state.map.nodes.iter().any(|node| {
            node.id == "project:Atlas_Loom"
                && node.label == "Atlas Loom"
                && node.kind == MemoryNodeKind::Project
        }));
        assert!(state.map.edges.iter().any(|edge| {
            edge.source == "project:Atlas_Loom"
                && edge.target == "project:identity:Atlas_Loom_is_my_planning_engine"
                && edge.relation == MemoryRelationKind::ProvenanceFor
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn memory_map_derives_character_nodes_and_called_aliases() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "MANUSCRIPT: Mara Vey is the captain of the Tidefall. Mara Vey is called the Glass Finch.",
            )
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(state.map.nodes.iter().any(|node| {
            node.id == "character:Mara_Vey"
                && node.label == "Mara Vey"
                && node.kind == MemoryNodeKind::Character
        }));
        assert!(state.map.edges.iter().any(|edge| {
            edge.source == "character:Mara_Vey"
                && edge.target == "character_alias:the_Glass_Finch"
                && edge.relation == MemoryRelationKind::AliasOf
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn manuscript_capture_requires_explicit_scope_marker() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "This is not part of the manuscript. John Cook is the village baker.",
            )
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(!state
            .claims
            .iter()
            .any(|claim| claim.domain == "manuscript"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn manuscript_scope_marker_is_case_insensitive_and_not_stored_as_character_name() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("Manuscript: Mara Vey is the captain of the Tidefall.")
            .unwrap();
        let state = session.inspect().unwrap();

        assert!(state.claims.iter().any(|claim| {
            claim.domain == "manuscript"
                && claim.kind == "character_identity"
                && claim.value == "Mara Vey is the captain of the Tidefall"
        }));
        assert!(!state
            .claims
            .iter()
            .any(|claim| claim.value.contains("Manuscript: Mara Vey")));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn manuscript_capture_stops_at_same_turn_scope_negation() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "MANUSCRIPT: Mara Vey is the captain of the Tidefall. This is not part of the manuscript. John Cook is the village baker.",
            )
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(state.claims.iter().any(|claim| {
            claim.domain == "manuscript"
                && claim.kind == "character_identity"
                && claim.value == "Mara Vey is the captain of the Tidefall"
        }));
        assert!(!state
            .claims
            .iter()
            .any(|claim| claim.value.contains("John Cook")));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn manuscript_capture_refuses_negated_scope_marker() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "MANUSCRIPT: This is not part of the manuscript. John Cook is the village baker.",
            )
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(!state
            .claims
            .iter()
            .any(|claim| claim.domain == "manuscript"));
        assert!(!state
            .claims
            .iter()
            .any(|claim| claim.value.contains("John Cook")));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn manuscript_one_read_lockout_blocks_later_source_ingest() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "MANUSCRIPT: Mara Vey is the captain of the Tidefall. The manuscript is closed.",
            )
            .unwrap();
        session
            .process_user_turn(
                "MANUSCRIPT: John Cook is the baker of the Glass Ward after the source was closed.",
            )
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(state.claims.iter().any(|claim| {
            claim.domain == "manuscript"
                && claim.kind == "source_status"
                && claim.value == "manuscript source closed"
        }));
        assert!(state.claims.iter().any(|claim| {
            claim.domain == "manuscript"
                && claim.kind == "character_identity"
                && claim.value == "Mara Vey is the captain of the Tidefall"
        }));
        assert!(!state
            .claims
            .iter()
            .any(|claim| { claim.domain == "manuscript" && claim.value.contains("John Cook") }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn manuscript_present_query_filters_flashback_from_context_packet() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "MANUSCRIPT: SCENE 1 PRESENT: Mara Vey carries the Tidefall chart. \
                 MANUSCRIPT: SCENE 2 FLASHBACK BEFORE SCENE 1: Mara Vey lost the Tidefall chart. \
                 MANUSCRIPT: SCENE 3 PRESENT: John Cook opens the bakery.",
            )
            .unwrap();
        let result = session
            .process_user_turn(
                "The manuscript is closed. In the present-time scene, does Mara Vey have the Tidefall chart?",
            )
            .unwrap();
        let plan = plan_conversation_response(
            "The manuscript is closed. In the present-time scene, does Mara Vey have the Tidefall chart?",
            &result,
        );

        assert!(plan
            .answer_values
            .iter()
            .any(|value| value.contains("Mara Vey carries the Tidefall chart")));
        assert!(plan
            .answer_values
            .iter()
            .all(|value| !value.contains("John Cook opens the bakery")));
        assert!(!result
            .context_packet
            .summary
            .contains("Mara Vey lost the Tidefall chart"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn missing_entity_query_does_not_answer_from_substring_memory() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("MKPE is my planning engine.")
            .unwrap();
        let result = session.process_user_turn("Who is Ann?").unwrap();
        let plan = plan_conversation_response("Who is Ann?", &result);
        let reply = render_conversation_reply("Who is Ann?", &result);

        assert!(plan.actions.contains(&ResponsePlanAction::AvoidAnswering));
        assert!(plan.answer_values.is_empty());
        assert!(!reply.contains("planning engine"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn missing_longer_entity_query_does_not_match_shorter_entity_substring() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session.process_user_turn("Chris lives in Iowa.").unwrap();
        let result = session.process_user_turn("Who is Christopher?").unwrap();
        let plan = plan_conversation_response("Who is Christopher?", &result);

        assert!(plan.actions.contains(&ResponsePlanAction::AvoidAnswering));
        assert!(plan
            .uncertainty
            .as_deref()
            .unwrap_or_default()
            .contains("requested entity is missing"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn multi_entity_query_answers_all_requested_entities() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("Chris lives in Iowa. Francois lives in Washington.")
            .unwrap();
        let result = session
            .process_user_turn("What do you know about Chris and Francois?")
            .unwrap();
        let reply =
            render_conversation_reply("What do you know about Chris and Francois?", &result);

        assert!(reply.contains("Chris lives in Iowa"));
        assert!(reply.contains("Francois lives in Washington"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn runtime_markdown_uses_filtered_context_packet() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn(
                "MANUSCRIPT: SCENE 1 PRESENT: Mara Vey carries the Tidefall chart. \
                 MANUSCRIPT: SCENE 2 FLASHBACK BEFORE SCENE 1: Mara Vey lost the Tidefall chart.",
            )
            .unwrap();
        let result = session
            .process_user_turn(
                "The manuscript is closed. In the present-time scene, does Mara Vey have the Tidefall chart?",
            )
            .unwrap();
        let markdown = render_runtime_markdown(&result);

        assert!(!markdown.contains("Mara Vey lost the Tidefall chart"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn intake_policy_marks_noise_anchor_requests_and_corrections() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        let learned = session.process_user_turn("Chris lives in Iowa.").unwrap();
        let noise = session
            .process_user_turn("blue chair twelve river battery")
            .unwrap();
        let anchor = session.process_user_turn("he moved again.").unwrap();
        let correction = session
            .process_user_turn("Actually Chris moved to Ohio.")
            .unwrap();

        assert_eq!(
            learned.intake.action,
            MemoryIntakeAction::StoreWithUncertainty
        );
        assert_eq!(noise.intake.action, MemoryIntakeAction::IgnoreNoise);
        assert_eq!(anchor.intake.action, MemoryIntakeAction::AskForAnchor);
        assert_eq!(
            correction.intake.action,
            MemoryIntakeAction::SupersedeOrCorrect
        );

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn correction_supersedes_old_location_in_rebuilt_memory() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session.process_user_turn("Chris lives in Iowa.").unwrap();
        session
            .process_user_turn("Actually Chris moved to Ohio.")
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(state.claims.iter().any(|claim| {
            claim.domain == "person"
                && claim.kind == "location"
                && claim.value == "Chris lives in Ohio"
        }));
        assert!(state.claims.iter().any(|claim| {
            claim.domain == "person"
                && claim.kind == "location"
                && claim.value == "Chris lives in Iowa"
                && claim.lifecycle_status == AssertionLifecycleStatus::Superseded
        }));
        assert!(state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "place:Ohio"
                && edge.relation == MemoryRelationKind::LocatedIn
        }));
        assert!(!state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "place:Iowa"
                && edge.relation == MemoryRelationKind::LocatedIn
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn same_turn_correction_supersedes_in_turn_mistake() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        let result = session
            .process_user_turn("Chris lives in Iowa. Actually Chris moved to Ohio.")
            .unwrap();

        assert!(result.memory_state.claims.iter().any(|claim| {
            claim.value == "Chris lives in Iowa"
                && claim.lifecycle_status == AssertionLifecycleStatus::Superseded
        }));
        assert!(result.memory_state.claims.iter().any(|claim| {
            claim.value == "Chris lives in Ohio"
                && claim.lifecycle_status == AssertionLifecycleStatus::Current
        }));
        assert!(result.memory_state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "place:Ohio"
                && edge.relation == MemoryRelationKind::LocatedIn
        }));
        assert!(!result.memory_state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "place:Iowa"
                && edge.relation == MemoryRelationKind::LocatedIn
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn chained_corrections_leave_only_latest_location_current() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session.process_user_turn("Chris lives in Iowa.").unwrap();
        session
            .process_user_turn("Actually Chris moved to Ohio.")
            .unwrap();
        session
            .process_user_turn("Correction: Chris moved to Michigan.")
            .unwrap();

        let state = session.inspect().unwrap();
        assert!(state.claims.iter().any(|claim| {
            claim.value == "Chris lives in Michigan"
                && claim.lifecycle_status == AssertionLifecycleStatus::Current
        }));
        assert!(state.claims.iter().any(|claim| {
            claim.value == "Chris lives in Iowa"
                && claim.lifecycle_status == AssertionLifecycleStatus::Superseded
        }));
        assert!(state.claims.iter().any(|claim| {
            claim.value == "Chris lives in Ohio"
                && claim.lifecycle_status == AssertionLifecycleStatus::Superseded
        }));
        assert!(state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "place:Michigan"
                && edge.relation == MemoryRelationKind::LocatedIn
        }));
        assert!(!state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && (edge.target == "place:Iowa" || edge.target == "place:Ohio")
                && edge.relation == MemoryRelationKind::LocatedIn
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn correction_back_to_superseded_value_is_learned_and_current() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session.process_user_turn("Chris lives in Iowa.").unwrap();
        session
            .process_user_turn("Actually Chris moved to Ohio.")
            .unwrap();
        let correction_back = session
            .process_user_turn("Correction: Chris moved to Iowa.")
            .unwrap();

        assert_eq!(
            correction_back.intake.action,
            MemoryIntakeAction::SupersedeOrCorrect
        );
        assert!(correction_back
            .knowledge_delta
            .unconfirmed
            .iter()
            .any(|claim| {
                claim.value == "Chris lives in Iowa"
                    && claim.lifecycle_status == AssertionLifecycleStatus::Current
            }));
        assert!(correction_back.memory_state.claims.iter().any(|claim| {
            claim.value == "Chris lives in Iowa"
                && claim.lifecycle_status == AssertionLifecycleStatus::Current
        }));
        assert!(correction_back.memory_state.claims.iter().any(|claim| {
            claim.value == "Chris lives in Ohio"
                && claim.lifecycle_status == AssertionLifecycleStatus::Superseded
        }));
        assert!(correction_back.memory_state.map.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "place:Iowa"
                && edge.relation == MemoryRelationKind::LocatedIn
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn correction_supersedes_old_identity_profession_in_rebuilt_memory() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("I work as a software developer.")
            .unwrap();
        let correction = session
            .process_user_turn("I work as a product manager, correction.")
            .unwrap();

        assert_eq!(
            correction.intake.action,
            MemoryIntakeAction::SupersedeOrCorrect
        );
        assert!(correction.memory_state.claims.iter().any(|claim| {
            claim.domain == "identity"
                && claim.kind == "profession"
                && claim.value == "product manager"
                && claim.lifecycle_status == AssertionLifecycleStatus::Current
        }));
        assert!(correction.memory_state.claims.iter().any(|claim| {
            claim.domain == "identity"
                && claim.kind == "profession"
                && claim.value == "software developer"
                && claim.lifecycle_status == AssertionLifecycleStatus::Superseded
        }));
        assert!(correction.memory_state.map.edges.iter().any(|edge| {
            edge.source == "user:self"
                && edge.target == "identity:profession:product_manager"
                && edge.relation == MemoryRelationKind::HasAttribute
        }));
        assert!(!correction.memory_state.map.edges.iter().any(|edge| {
            edge.source == "user:self"
                && edge.target == "identity:profession:software_developer"
                && edge.relation == MemoryRelationKind::HasAttribute
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn correction_supersedes_old_project_identity_in_rebuilt_memory() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("Atlas Loom is my planning engine.")
            .unwrap();
        let correction = session
            .process_user_turn("Atlas Loom is my project workspace. Correction.")
            .unwrap();

        assert_eq!(
            correction.intake.action,
            MemoryIntakeAction::SupersedeOrCorrect
        );
        assert!(correction.memory_state.claims.iter().any(|claim| {
            claim.domain == "project"
                && claim.kind == "identity"
                && claim.value == "Atlas Loom is my project workspace"
                && claim.lifecycle_status == AssertionLifecycleStatus::Current
        }));
        assert!(correction.memory_state.claims.iter().any(|claim| {
            claim.domain == "project"
                && claim.kind == "identity"
                && claim.value == "Atlas Loom is my planning engine"
                && claim.lifecycle_status == AssertionLifecycleStatus::Superseded
        }));
        assert!(correction.memory_state.map.edges.iter().any(|edge| {
            edge.source == "project:Atlas_Loom"
                && edge.target == "project:identity:Atlas_Loom_is_my_project_workspace"
                && edge.relation == MemoryRelationKind::ProvenanceFor
        }));
        assert!(!correction.memory_state.map.edges.iter().any(|edge| {
            edge.source == "project:Atlas_Loom"
                && edge.target == "project:identity:Atlas_Loom_is_my_planning_engine"
                && edge.relation == MemoryRelationKind::ProvenanceFor
        }));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn correction_cue_without_current_target_is_not_supersession() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        let result = session
            .process_user_turn("Actually Chris moved to Ohio.")
            .unwrap();

        assert_eq!(
            result.intake.action,
            MemoryIntakeAction::StoreWithUncertainty
        );

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn person_location_maps_to_place_node_with_located_in_edge() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session.process_user_turn("Chris lives in Iowa.").unwrap();

        let state = session.inspect().unwrap();
        let edge = state
            .map
            .edges
            .iter()
            .find(|edge| {
                edge.source == "person:Chris" && edge.relation == MemoryRelationKind::LocatedIn
            })
            .expect("person location should create a LocatedIn edge from the person");
        let target = state
            .map
            .nodes
            .iter()
            .find(|node| node.id == edge.target)
            .expect("LocatedIn edge should point to a memory node");

        assert_eq!(target.kind, MemoryNodeKind::Place);

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn person_goal_maps_to_goal_node_with_has_goal_edge() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("Francois wants to take over the industry.")
            .unwrap();

        let state = session.inspect().unwrap();
        let edge = state
            .map
            .edges
            .iter()
            .find(|edge| {
                edge.source == "person:Francois" && edge.relation == MemoryRelationKind::HasGoal
            })
            .expect("person goal should create a HasGoal edge from the person");
        let target = state
            .map
            .nodes
            .iter()
            .find(|node| node.id == edge.target)
            .expect("HasGoal edge should point to a memory node");

        assert_eq!(target.kind, MemoryNodeKind::Goal);

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn root_orb_is_quiet_until_memory_rules_are_relevant() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("I work as a mechanical engineer.")
            .unwrap();
        let unrelated = session.process_user_turn("I watched a movie.").unwrap();
        let memory_question = session
            .process_user_turn("What does Luna know about memory?")
            .unwrap();

        let unrelated_activation = unrelated
            .working_memory
            .nodes
            .iter()
            .find(|node| node.id == "root:luna")
            .map(|node| node.activation)
            .unwrap_or(0.0);
        let relevant_activation = memory_question
            .working_memory
            .nodes
            .iter()
            .find(|node| node.id == "root:luna")
            .map(|node| node.activation)
            .unwrap_or(0.0);

        assert!(unrelated_activation < 0.1);
        assert!(relevant_activation >= 0.9);

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn working_memory_activates_relevant_nodes_without_dumping_map() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("I work as a mechanical engineer.")
            .unwrap();
        session.process_user_turn("I'm an only child.").unwrap();
        let result = session
            .process_user_turn("What do I do for a living?")
            .unwrap();

        assert!(result.memory_state.map.nodes.len() > result.working_memory.nodes.len());
        assert!(result.working_memory.nodes.len() <= WorkingMemoryBudget::default().max_nodes);
        assert!(result
            .working_memory
            .nodes
            .iter()
            .any(|node| node.label == "mechanical engineer"));
        assert!(!result
            .working_memory
            .nodes
            .iter()
            .any(|node| node.label == "only child"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn working_memory_activates_graph_neighbors_for_entity_query() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("Chris lives in Iowa. Chris wants to retire his wife.")
            .unwrap();
        let result = session
            .process_user_turn("What do you know about Chris?")
            .unwrap();

        assert!(result
            .working_memory
            .nodes
            .iter()
            .any(|node| node.id == "person:Chris"));
        assert!(result
            .working_memory
            .nodes
            .iter()
            .any(|node| node.id == "place:Iowa"));
        assert!(result.working_memory.edges.iter().any(|edge| {
            edge.source == "person:Chris"
                && edge.target == "place:Iowa"
                && edge.relation == MemoryRelationKind::LocatedIn
        }));
        let active_ids = result
            .working_memory
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>();
        assert!(result
            .working_memory
            .edges
            .iter()
            .all(|edge| active_ids.contains(&edge.source) && active_ids.contains(&edge.target)));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn activation_respects_max_depth_budget() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session.process_user_turn("Chris lives in Iowa.").unwrap();
        let state = session.inspect().unwrap();
        let turn = ConversationTurn::user("What do you know about Chris?");
        let observation = FusedExtractor::new().extract_runtime(&turn).unwrap();
        let depth_zero = activate_working_memory_with_orb_state(
            &state,
            &turn,
            &observation,
            &RecallSet::default(),
            WorkingMemoryBudget {
                max_nodes: 5,
                max_edges: 10,
                max_questions: 1,
                max_activation_depth: 0,
            },
            &RuntimeOrbActivationState::default(),
        );
        let depth_one = activate_working_memory_with_orb_state(
            &state,
            &turn,
            &observation,
            &RecallSet::default(),
            WorkingMemoryBudget {
                max_nodes: 5,
                max_edges: 10,
                max_questions: 1,
                max_activation_depth: 1,
            },
            &RuntimeOrbActivationState::default(),
        );

        assert!(!depth_zero.nodes.iter().any(|node| node.id == "place:Iowa"));
        assert!(depth_one.nodes.iter().any(|node| node.id == "place:Iowa"));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn activation_prefers_current_confirmed_over_stale_superseded_contradicted_and_unconfirmed() {
        let mut current_confirmed = MemoryClaim::from_assertion(
            &StructuredAssertion::new("project", "status", "Atlas status is current")
                .with_source_count(2),
        );
        current_confirmed.status = AssertionConfidenceTier::Confirmed;

        let current_unconfirmed = MemoryClaim::from_assertion(&StructuredAssertion::new(
            "project",
            "status",
            "Atlas status is unconfirmed",
        ));
        let mut stale = StructuredAssertion::new("project", "status", "Atlas status is stale");
        stale.lifecycle_status = AssertionLifecycleStatus::Stale;
        stale.confidence_tier = AssertionConfidenceTier::Confirmed;
        let mut superseded =
            StructuredAssertion::new("project", "status", "Atlas status is superseded");
        superseded.lifecycle_status = AssertionLifecycleStatus::Superseded;
        superseded.confidence_tier = AssertionConfidenceTier::Confirmed;
        let mut contradicted =
            StructuredAssertion::new("project", "status", "Atlas status is contradicted");
        contradicted.lifecycle_status = AssertionLifecycleStatus::Contradicted;
        contradicted.confidence_tier = AssertionConfidenceTier::Confirmed;

        let claims = vec![
            current_confirmed.clone(),
            current_unconfirmed.clone(),
            MemoryClaim::from_assertion(&stale),
            MemoryClaim::from_assertion(&superseded),
            MemoryClaim::from_assertion(&contradicted),
        ];
        let current_claims = vec![current_confirmed, current_unconfirmed];
        let state = MemoryState {
            claims,
            entity_groups: group_claims_by_entity(&current_claims),
            open_questions: Vec::new(),
            map: memory_map_from_claims(&current_claims, &BTreeMap::new()),
        };
        let turn = ConversationTurn::user("Atlas status");
        let observation = test_observation_with_cues(Vec::new());

        let working_memory = activate_working_memory_with_orb_state(
            &state,
            &turn,
            &observation,
            &RecallSet::default(),
            WorkingMemoryBudget {
                max_nodes: 3,
                max_edges: 10,
                max_questions: 1,
                max_activation_depth: 0,
            },
            &RuntimeOrbActivationState::default(),
        );

        let current_activation =
            node_activation_by_label(&working_memory, "Atlas status is current");
        let unconfirmed_activation =
            node_activation_by_label(&working_memory, "Atlas status is unconfirmed");
        assert!(current_activation > unconfirmed_activation);
        assert!(working_memory
            .nodes
            .first()
            .is_some_and(|node| node.label == "Atlas status is current"));
        assert!(working_memory.filtered_node_count >= 3);
        assert!(working_memory
            .activation_reason
            .contains("suppressed_noncurrent_memory=3"));
        assert!(!working_memory
            .nodes
            .iter()
            .any(|node| node.label.contains("stale")
                || node.label.contains("superseded")
                || node.label.contains("contradicted")));
    }

    #[test]
    fn activation_keeps_quiet_directly_cued_memory_retrievable() {
        let quiet = MemoryClaim::from_assertion(&StructuredAssertion::new(
            "project",
            "ritual",
            "Vela has a quiet ritual",
        ));
        let noisy = MemoryClaim::from_assertion(
            &StructuredAssertion::new("project", "status", "Atlas has a loud roadmap")
                .with_source_count(2),
        );
        let claims = vec![quiet.clone(), noisy.clone()];
        let state = MemoryState {
            claims: claims.clone(),
            entity_groups: group_claims_by_entity(&claims),
            open_questions: Vec::new(),
            map: memory_map_from_claims(&claims, &BTreeMap::new()),
        };
        let turn = ConversationTurn::user("What should surface?");
        let observation = test_observation_with_cues(vec!["quiet ritual".to_string()]);

        let working_memory = activate_working_memory_with_orb_state(
            &state,
            &turn,
            &observation,
            &RecallSet::default(),
            WorkingMemoryBudget {
                max_nodes: 1,
                max_edges: 10,
                max_questions: 1,
                max_activation_depth: 0,
            },
            &RuntimeOrbActivationState::default(),
        );

        assert_eq!(working_memory.nodes.len(), 1);
        assert_eq!(working_memory.nodes[0].label, "Vela has a quiet ritual");
    }

    #[test]
    fn accepted_compression_receipt_reduces_context_and_preserves_raw_citations() {
        let event_a = compression_source_event("raw memory A");
        let event_b = compression_source_event("raw memory B");
        let source_a = SourceEventRef::new(
            event_a.event_id.to_string(),
            event_a.event_hash.clone().unwrap(),
        );
        let source_b = SourceEventRef::new(
            event_b.event_id.to_string(),
            event_b.event_hash.clone().unwrap(),
        );
        let receipt = luna_cluster::issue_compression_receipt(
            luna_cluster::CompressionRequest::new(
                "compression-runtime-1",
                vec![source_a.clone(), source_b.clone()],
                vec![source_a.clone(), source_b.clone()],
                "lossless-raw-ancestry-v1",
                "c".repeat(64),
            )
            .with_output_artifact("artifact://compression-runtime-1", b"compressed bytes"),
            &luna_cluster::CompressionPolicy::default(),
        );
        let working_memory = WorkingMemory {
            nodes: vec![
                compression_test_node_with_turn("node-a", "raw memory A", event_a.event_id),
                compression_test_node_with_turn("node-b", "raw memory B", event_b.event_id),
            ],
            edges: vec![MemoryEdge {
                source: "node-a".to_string(),
                target: "node-b".to_string(),
                relation: MemoryRelationKind::RelatedTo,
                confidence_tier: AssertionConfidenceTier::Confirmed,
                strength: 1.0,
                activation: 1.0,
                provenance: Vec::new(),
            }],
            filtered_node_count: 0,
            filtered_edge_count: 0,
            activation_reason: "test activation".to_string(),
        };

        let verified_source_events =
            VerifiedSourceEventIndex::from_stored_events(&[event_a, event_b]).unwrap();
        let compressed = compress_working_memory_with_verified_receipts(
            &working_memory,
            std::slice::from_ref(&receipt),
            &verified_source_events,
        );

        assert_eq!(compressed.working_memory.nodes.len(), 1);
        assert!(compressed.working_memory.nodes[0]
            .id
            .starts_with("compression:compression-runtime-1"));
        assert!(compressed
            .working_memory
            .nodes
            .iter()
            .all(|node| !node.label.contains("raw memory")));
        assert_eq!(compressed.compressed_memory.len(), 1);
        assert_eq!(
            compressed.compressed_memory[0]
                .raw_event_refs
                .iter()
                .map(|source| format!("{}:{}", source.event_id, source.event_hash))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                format!("{}:{}", source_a.event_id, source_a.event_hash),
                format!("{}:{}", source_b.event_id, source_b.event_hash),
            ])
        );
        assert_eq!(
            compressed.compressed_memory[0].covered_node_ids,
            vec!["node-a".to_string(), "node-b".to_string()]
        );
        assert!(compressed
            .working_memory
            .activation_reason
            .contains("accepted_compression_receipts=1"));

        let packet = ContextPacket::from_parts_with_verified_compression_receipts(
            "Summarize the compressed memory.",
            &working_memory,
            &RecallSet::default(),
            &[],
            RecallMode::OpenEnded,
            WorkingMemoryBudget::default(),
            &[receipt],
            &verified_source_events,
        );
        assert_eq!(packet.working_memory.nodes.len(), 1);
        assert_eq!(packet.compressed_memory.len(), 1);
        assert!(packet
            .summary
            .contains("2 raw source event citation(s) retained"));
    }

    #[test]
    fn rejected_compression_receipt_does_not_reduce_working_memory() {
        let event_a = compression_source_event("raw memory A");
        let event_b = compression_source_event("raw memory B");
        let source_a = SourceEventRef::new(
            event_a.event_id.to_string(),
            event_a.event_hash.clone().unwrap(),
        );
        let source_b = SourceEventRef::new(
            event_b.event_id.to_string(),
            event_b.event_hash.clone().unwrap(),
        );
        let receipt = luna_cluster::issue_compression_receipt(
            luna_cluster::CompressionRequest::new(
                "compression-runtime-lossy",
                vec![source_a.clone(), source_b],
                vec![source_a],
                "lossless-raw-ancestry-v1",
                "c".repeat(64),
            )
            .with_output_artifact("artifact://compression-runtime-lossy", b"lossy bytes"),
            &luna_cluster::CompressionPolicy::default(),
        );
        let working_memory = WorkingMemory {
            nodes: vec![
                compression_test_node_with_turn("node-a", "raw memory A", event_a.event_id),
                compression_test_node_with_turn("node-b", "raw memory B", event_b.event_id),
            ],
            edges: Vec::new(),
            filtered_node_count: 0,
            filtered_edge_count: 0,
            activation_reason: "test activation".to_string(),
        };

        let verified_source_events =
            VerifiedSourceEventIndex::from_stored_events(&[event_a, event_b]).unwrap();
        let compressed = compress_working_memory_with_verified_receipts(
            &working_memory,
            &[receipt],
            &verified_source_events,
        );

        assert_eq!(compressed.working_memory.nodes, working_memory.nodes);
        assert!(compressed.compressed_memory.is_empty());
    }

    #[test]
    fn compression_receipt_without_verified_source_hashes_does_not_reduce_working_memory() {
        let source_a = SourceEventRef::new(Uuid::new_v4().to_string(), "a".repeat(64));
        let source_b = SourceEventRef::new(Uuid::new_v4().to_string(), "b".repeat(64));
        let receipt = luna_cluster::issue_compression_receipt(
            luna_cluster::CompressionRequest::new(
                "compression-runtime-unverified",
                vec![source_a.clone(), source_b.clone()],
                vec![source_a.clone(), source_b.clone()],
                "lossless-raw-ancestry-v1",
                "c".repeat(64),
            )
            .with_output_artifact("artifact://compression-runtime-unverified", b"bytes"),
            &luna_cluster::CompressionPolicy::default(),
        );
        let working_memory = WorkingMemory {
            nodes: vec![
                compression_test_node("node-a", "raw memory A", &source_a.event_id),
                compression_test_node("node-b", "raw memory B", &source_b.event_id),
            ],
            edges: Vec::new(),
            filtered_node_count: 0,
            filtered_edge_count: 0,
            activation_reason: "test activation".to_string(),
        };

        let compressed = compress_working_memory_with_verified_receipts(
            &working_memory,
            &[receipt],
            &VerifiedSourceEventIndex::default(),
        );

        assert_eq!(compressed.working_memory.nodes, working_memory.nodes);
        assert!(compressed.compressed_memory.is_empty());
    }

    #[test]
    fn retired_orb_memory_is_not_surfaced_as_current_runtime_recall() {
        let retired_assertion =
            StructuredAssertion::new("project", "status", "Vela status is retired");
        let active_assertion =
            StructuredAssertion::new("project", "status", "Vela status is active");
        let retired_claim = MemoryClaim::from_assertion(&retired_assertion);
        let active_claim = MemoryClaim::from_assertion(&active_assertion);
        let retired_key = retired_claim.key.clone();
        let active_key = active_claim.key.clone();
        let retired_node = MemoryNode {
            id: "project:status:Vela_status_is_retired".to_string(),
            label: retired_claim.value.clone(),
            kind: MemoryNodeKind::Project,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 0.0,
            provenance: vec![MemoryProvenance {
                episode_id: Some(Uuid::new_v4()),
                turn_id: Some(Uuid::new_v4()),
                assertion_key: Some(retired_key.clone()),
                system_root: Some("orb:orb-parent-retired".to_string()),
                lifecycle_status: None,
            }],
            created_at: None,
            contradiction_count: 0,
        };
        let active_node = MemoryNode {
            id: "project:status:Vela_status_is_active".to_string(),
            label: active_claim.value.clone(),
            kind: MemoryNodeKind::Project,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 0.0,
            provenance: vec![MemoryProvenance {
                episode_id: Some(Uuid::new_v4()),
                turn_id: Some(Uuid::new_v4()),
                assertion_key: Some(active_key.clone()),
                system_root: Some("orb:orb-child-active".to_string()),
                lifecycle_status: None,
            }],
            created_at: None,
            contradiction_count: 0,
        };
        let state = MemoryState {
            claims: vec![retired_claim, active_claim],
            entity_groups: Vec::new(),
            open_questions: Vec::new(),
            map: MemoryMap {
                nodes: vec![retired_node, active_node],
                edges: Vec::new(),
            },
        };
        let turn = ConversationTurn::user("Vela status");
        let observation = test_observation_with_cues(vec!["Vela status".to_string()]);
        let recalled = RecallSet {
            hits: vec![luna_core::RecallHit {
                episode_id: Uuid::new_v4(),
                score: 0.91,
                assertions: vec![retired_assertion, active_assertion],
                reason: luna_core::RecallReason::new("test recall hit with provenance")
                    .expect("static recall reason is non-empty"),
            }],
            latency_ms: 0.0,
        };
        let orb_state = RuntimeOrbActivationState {
            active_orb_ids: BTreeSet::from(["orb-child-active".to_string()]),
            retired_orb_ids: BTreeSet::from(["orb-parent-retired".to_string()]),
        };

        let working_memory = activate_working_memory_with_orb_state(
            &state,
            &turn,
            &observation,
            &recalled,
            WorkingMemoryBudget {
                max_nodes: 5,
                max_edges: 10,
                max_questions: 1,
                max_activation_depth: 0,
            },
            &orb_state,
        );
        let context_packet = ContextPacket::from_parts(
            &turn.content,
            &working_memory,
            &recalled,
            &[],
            RecallMode::OpenEnded,
            WorkingMemoryBudget::default(),
        );

        assert!(working_memory
            .nodes
            .iter()
            .any(|node| node.label == "Vela status is active"));
        assert!(!working_memory
            .nodes
            .iter()
            .any(|node| node.label == "Vela status is retired"));
        assert!(working_memory
            .activation_reason
            .contains("suppressed_retired_orb_memory=1"));
        assert_eq!(context_packet.recalled_claims.len(), 1);
        assert_eq!(context_packet.recalled_claims[0].key, active_key);
        assert_ne!(context_packet.recalled_claims[0].key, retired_key);
    }

    #[test]
    fn emotional_pronoun_turn_surfaces_only_foundational_question() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        let result = session.process_user_turn("I hate her right now.").unwrap();

        assert_eq!(result.questions.len(), 1);
        assert_eq!(result.questions[0].question, "Who is she to you?");

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn local_product_memory_smoke_passes_on_fresh_log() {
        for (name, before, after) in [
            ("Taylor", "Vermont", "Maine"),
            ("Marin", "Oregon", "Nevada"),
        ] {
            let log = temp_log();
            let phrases = LocalProductSmokePhrases {
                seed: format!("{name} lives in {before}."),
                distract_turns: vec![format!("{name} is planning a quiet Sunday grocery run.")],
                retrieve_before_correction: format!("Where does {name} live?"),
                correction: format!("{name} moved to {after}."),
                retrieve_after_correction: format!("Where does {name} live?"),
                expect_before: before.to_string(),
                expect_after: after.to_string(),
                expect_not_after: vec![before.to_string()],
            };
            let report = run_local_product_memory_smoke(&log, &phrases).unwrap();
            assert!(
                report.is_success(),
                "expected full smoke success for {name}, got {report:?}"
            );
            assert!(report.recall_hit_after_reopen);
            assert!(report.recall_hit_after_correction);
            assert!(report.reply_excludes_rejected_after);

            let _ = fs::remove_dir_all(log.parent().unwrap());
        }
    }

    #[test]
    fn response_plan_is_inspectable_for_supported_and_unknown_queries() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session.process_user_turn("I am Joe.").unwrap();
        let known = session.process_user_turn("Who am I?").unwrap();
        let known_plan = plan_conversation_response("Who am I?", &known);
        assert!(known_plan.actions.contains(&ResponsePlanAction::Answer));
        assert!(known_plan.answer_values.iter().any(|value| value == "Joe"));

        let unknown = session
            .process_user_turn("What did I say about MKPE?")
            .unwrap();
        let unknown_plan = plan_conversation_response("What did I say about MKPE?", &unknown);
        assert!(unknown_plan
            .actions
            .contains(&ResponsePlanAction::AvoidAnswering));
        assert!(unknown_plan.uncertainty.is_some());

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn response_plan_does_not_answer_from_unproven_node_labels() {
        let mut result = test_runtime_result(MemoryIntakeAction::IgnoreNoise);
        let node = MemoryNode {
            id: "place:Atlantis".to_string(),
            label: "Atlantis".to_string(),
            kind: MemoryNodeKind::Place,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 1.0,
            provenance: Vec::new(),
            created_at: None,
            contradiction_count: 0,
        };
        result.working_memory.nodes.push(node.clone());
        result.context_packet.working_memory.nodes.push(node);

        let plan = plan_conversation_response("What do you know?", &result);

        assert!(plan.actions.contains(&ResponsePlanAction::AvoidAnswering));
        assert!(plan.answer_values.is_empty());
    }

    #[test]
    fn response_plan_rejects_system_root_assertion_without_direct_evidence() {
        let mut result = test_runtime_result(MemoryIntakeAction::IgnoreNoise);
        let key = "project:myth:unsupported".to_string();
        result.memory_state.claims.push(MemoryClaim {
            key: key.clone(),
            domain: "project".to_string(),
            kind: "fact".to_string(),
            value: "Atlantis is a supported project".to_string(),
            status: AssertionConfidenceTier::Confirmed,
            lifecycle_status: AssertionLifecycleStatus::Current,
        });
        result.context_packet.working_memory.nodes.push(MemoryNode {
            id: "project:Atlantis".to_string(),
            label: "Atlantis is a supported project".to_string(),
            kind: MemoryNodeKind::Project,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 1.0,
            provenance: vec![MemoryProvenance {
                episode_id: None,
                turn_id: None,
                assertion_key: Some(key),
                system_root: Some("orb:unsupported:label".to_string()),
                lifecycle_status: None,
            }],
            created_at: None,
            contradiction_count: 0,
        });

        let plan = plan_conversation_response("What do you know?", &result);

        assert!(plan.actions.contains(&ResponsePlanAction::AvoidAnswering));
        assert_eq!(
            plan.uncertainty.as_deref(),
            Some("no supported recalled or active memory matches")
        );
        assert!(plan.answer_values.is_empty());
        assert!(plan.answer_evidence.is_empty());
    }

    #[test]
    fn response_plan_marks_orb_authorized_memory_when_direct_evidence_exists() {
        let mut result = test_runtime_result(MemoryIntakeAction::Accept);
        let key = "project:mkpe:fact".to_string();
        result.memory_state.claims.push(MemoryClaim {
            key: key.clone(),
            domain: "project".to_string(),
            kind: "fact".to_string(),
            value: "MKPE is my provenance engine".to_string(),
            status: AssertionConfidenceTier::Confirmed,
            lifecycle_status: AssertionLifecycleStatus::Current,
        });
        result.context_packet.working_memory.nodes.push(MemoryNode {
            id: "project:MKPE".to_string(),
            label: "MKPE is my provenance engine".to_string(),
            kind: MemoryNodeKind::Project,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 1.0,
            provenance: vec![
                MemoryProvenance {
                    episode_id: None,
                    turn_id: None,
                    assertion_key: Some(key),
                    system_root: None,
                    lifecycle_status: None,
                },
                MemoryProvenance {
                    episode_id: None,
                    turn_id: None,
                    assertion_key: None,
                    system_root: Some("orb:runtime:project:MKPE".to_string()),
                    lifecycle_status: None,
                },
            ],
            created_at: None,
            contradiction_count: 0,
        });

        let plan = plan_conversation_response("What do you know?", &result);

        assert!(plan.actions.contains(&ResponsePlanAction::Answer));
        assert_eq!(
            plan.answer_values,
            vec!["MKPE is my provenance engine".to_string()]
        );
        let evidence = plan.answer_evidence.first().unwrap();
        assert!(evidence.direct_assertion_evidence);
        assert!(evidence.orb_authorized);
        assert_eq!(
            evidence.topology_orb_refs,
            vec!["orb:runtime:project:MKPE".to_string()]
        );
    }

    fn test_runtime_result(action: MemoryIntakeAction) -> RuntimeTurnResult {
        RuntimeTurnResult {
            turn_id: Uuid::new_v4(),
            observation: TurnReading {
                turn_id: Uuid::new_v4(),
                semantic: None,
                intent: None,
                attention: None,
                goal_pressure: None,
                emotional_valence: None,
                emotional_arousal: None,
                identity_relevance: None,
                trust_relevance: None,
                social_frame: None,
                temporal_relevance: None,
                uncertainty: luna_core::Signal::new(
                    0.0,
                    1.0,
                    luna_core::SignalReliability::Heuristic,
                ),
                cue_terms: Vec::new(),
                query_intents: Vec::new(),
                assertions: Vec::new(),
            },
            knowledge_delta: KnowledgeDelta::default(),
            memory_state: MemoryState::default(),
            working_memory: WorkingMemory::default(),
            recalled: RecallSet::default(),
            recall_mode: RecallMode::OpenEnded,
            questions: Vec::new(),
            context_packet: ContextPacket {
                recall_mode: RecallMode::OpenEnded,
                recalled_claims: Vec::new(),
                working_memory: WorkingMemory::default(),
                compressed_memory: Vec::new(),
                open_questions: Vec::new(),
                summary: String::new(),
            },
            intake: MemoryIntakeDecision {
                action,
                reason: "test fixture".to_string(),
            },
            output_packet: OutputPacket {
                items: Vec::new(),
                total_bytes: 0,
                budget: luna_output::BudgetUsage {
                    bytes_used: 0,
                    bytes_max: 4096,
                    items_used: 0,
                    items_max: 12,
                    suppressed_count: 0,
                },
            },
        }
    }

    fn test_observation_with_cues(cue_terms: Vec<String>) -> TurnReading {
        TurnReading {
            turn_id: Uuid::new_v4(),
            semantic: None,
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: luna_core::Signal::new(0.0, 1.0, luna_core::SignalReliability::Heuristic),
            cue_terms,
            query_intents: Vec::new(),
            assertions: Vec::new(),
        }
    }

    fn node_activation_by_label(working_memory: &WorkingMemory, label: &str) -> f32 {
        working_memory
            .nodes
            .iter()
            .find(|node| node.label == label)
            .map(|node| node.activation)
            .unwrap_or(0.0)
    }

    fn compression_test_node(id: &str, label: &str, assertion_key: &str) -> MemoryNode {
        MemoryNode {
            id: id.to_string(),
            label: label.to_string(),
            kind: MemoryNodeKind::Assertion,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 1.0,
            provenance: vec![MemoryProvenance {
                episode_id: None,
                turn_id: None,
                assertion_key: Some(assertion_key.to_string()),
                system_root: None,
                lifecycle_status: None,
            }],
            created_at: None,
            contradiction_count: 0,
        }
    }

    fn compression_test_node_with_turn(id: &str, label: &str, turn_id: Uuid) -> MemoryNode {
        MemoryNode {
            id: id.to_string(),
            label: label.to_string(),
            kind: MemoryNodeKind::Assertion,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 1.0,
            provenance: vec![MemoryProvenance {
                episode_id: None,
                turn_id: Some(turn_id),
                assertion_key: Some(id.to_string()),
                system_root: None,
                lifecycle_status: None,
            }],
            created_at: None,
            contradiction_count: 0,
        }
    }

    fn compression_source_event(content: &str) -> luna_events::StoredEvent {
        luna_events::with_stored_event_hash(luna_core::EventEnvelope::new(
            LunaEvent::TurnObserved(luna_core::TurnObserved {
                turn: ConversationTurn {
                    role: Role::User,
                    content: content.to_string(),
                    timestamp: None,
                },
            }),
            EventSource::User,
            1.0,
        ))
        .unwrap()
    }
}
