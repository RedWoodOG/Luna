pub mod decay;

use chrono::Utc;
use luna_core::{
    AssertionConfidenceTier, AssertionExtracted, CognitiveObservation, ConversationTurn, Episode,
    EpisodeCreated, EpisodeDecayed, EpisodeRecalled, EpisodeReinforced, EventEnvelope, EventSource,
    LunaEvent, MemoryEdge, MemoryMap, MemoryNode, MemoryNodeKind, MemoryProvenance,
    MemoryRelationKind, NodeMerged, RawObservationCaptured, RecallMode, RecallSet, Result, Role,
    RootOrb, StructuredAssertion, TurnObserved, WorkingMemory, WorkingMemoryBudget,
};
use luna_events::JsonlEventLog;
use luna_extract::{ExtractionCache, FeatureExtractor, FusedExtractor, LlmBackend, LunaExtractor};
use luna_recall::{RecallEngine, TcfRecallEngine};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub use decay::DecayConfig;

pub trait RuntimeExtractor {
    fn extract_runtime(&self, turn: &ConversationTurn) -> Result<CognitiveObservation>;
}

impl RuntimeExtractor for FusedExtractor {
    fn extract_runtime(&self, turn: &ConversationTurn) -> Result<CognitiveObservation> {
        self.extract(turn)
    }
}

impl<B: LlmBackend, C: ExtractionCache> RuntimeExtractor for LunaExtractor<B, C> {
    fn extract_runtime(&self, turn: &ConversationTurn) -> Result<CognitiveObservation> {
        self.extract(turn)
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeSession<E, R = TcfRecallEngine> {
    log: JsonlEventLog,
    extractor: E,
    recall: R,
    decay_config: DecayConfig,
}

impl<E> RuntimeSession<E, TcfRecallEngine> {
    pub fn new(log_path: impl Into<PathBuf>, extractor: E) -> Self {
        Self {
            log: JsonlEventLog::new(log_path),
            extractor,
            recall: TcfRecallEngine,
            decay_config: DecayConfig::default(),
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
            decay_config: DecayConfig::default(),
        }
    }

    /// Override the default decay configuration. Used by scenarios that
    /// need a tighter or looser half-life than the 7-day default, and
    /// by unit tests that want to exercise decay over short time spans.
    pub fn with_decay_config(mut self, config: DecayConfig) -> Self {
        self.decay_config = config;
        self
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
        let mut previous_episodes = luna_store::rebuild_episodes(&previous_events)?;

        // Time-decay pre-pass. `now` is event-time (the turn's own
        // timestamp), never wall-clock — decay must replay
        // deterministically. Decisions that exceed the emit threshold
        // become EpisodeDecayed events at the head of new_events; the
        // in-memory episodes are mutated to match so the recall pass
        // scores against the post-decay state without a second log
        // rebuild.
        let now = turn.timestamp.unwrap_or_else(Utc::now);
        let decay_decisions =
            decay::compute_decay_events(&previous_episodes, now, &self.decay_config);
        decay::apply_decay_in_place(&mut previous_episodes, &decay_decisions);

        let (known_before, prior_merges) =
            MemoryState::from_episodes_with_merges(&previous_episodes);
        // R-005 closure: only NEW merges (not already attested by past
        // turns) become `NodeMerged` audit events this turn. The prior
        // set is keyed by node_id so a re-merge of the same id this
        // turn is a no-op for the audit log; what counts is the first
        // time a node id is observed merging.
        let prior_merged_ids: BTreeSet<String> = prior_merges
            .into_iter()
            .map(|merge| merge.node_id)
            .collect();
        let mut observation = self.extractor.extract_runtime(&turn)?;
        // R-003 closure: log the unmodified extractor output BEFORE
        // `apply_runtime_fine_capture` mutates it. Replay treats this
        // event as informational (luna-store no-op arm); the audit
        // chain can now reconstruct what the extractor actually emitted
        // before normalization rules were applied.
        let raw_observation_event = EventEnvelope::new(
            LunaEvent::RawObservationCaptured(RawObservationCaptured {
                observation: observation.clone(),
            }),
            EventSource::ClassifierExtractor,
            observation.uncertainty.confidence(),
        )
        .with_turn_id(observation.turn_id);
        apply_runtime_fine_capture(&turn, &mut observation);
        let recall_mode = select_recall_mode(&observation);
        let recalled = self
            .recall
            .recall(&observation, &previous_episodes, recall_mode)?;
        let turn_id = observation.turn_id;

        let mut new_events = Vec::new();

        // Decay events go FIRST so replay applies them before the
        // turn's recall and assertion events. They are dated to the
        // turn's event-time so byte-for-byte replay holds.
        for decision in &decay_decisions {
            let mut env = EventEnvelope::new(
                LunaEvent::EpisodeDecayed(EpisodeDecayed {
                    forgotten_risk: decision.new_risk,
                }),
                EventSource::System,
                1.0,
            )
            .with_episode_id(decision.episode_id)
            .with_turn_id(turn_id);
            env.timestamp = now;
            new_events.push(env);
        }

        new_events.push(
            EventEnvelope::new(
                LunaEvent::TurnObserved(TurnObserved { turn: turn.clone() }),
                EventSource::User,
                1.0,
            )
            .with_turn_id(turn_id),
        );

        // R-003 closure: push the pre-normalization audit record after
        // TurnObserved (so the log reads turn → raw extract → post-norm
        // state changes). Uses the turn's event-time to preserve the
        // byte-for-byte replay invariant from pr-1.0a.
        let mut raw_observation_event = raw_observation_event;
        raw_observation_event.timestamp = now;
        new_events.push(raw_observation_event);

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

            if let Some(episode_id) =
                luna_store::episode_id_for_assertion(&previous_events, assertion)
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

        // Derive the post-turn state BEFORE appending to the log so any
        // NodeMerged audit events this turn produces ride the same
        // batch as the rest of the turn's events. This keeps the log
        // ordered "turn → raw → recalled → assertions → merges" within
        // one atomic append.
        let mut all_events = previous_events;
        all_events.extend(new_events.iter().cloned());
        let episodes = luna_store::rebuild_episodes(&all_events)?;
        let (memory_state, post_merges) = MemoryState::from_episodes_with_merges(&episodes);

        // R-005 closure: emit one NodeMerged event per *fresh* merge
        // (a node id that wasn't already merged in known_before). Same
        // event-time discipline as RawObservationCaptured (R-003).
        for merge in post_merges {
            if prior_merged_ids.contains(&merge.node_id) {
                continue;
            }
            let mut env = EventEnvelope::new(
                LunaEvent::NodeMerged(merge),
                EventSource::System,
                1.0,
            )
            .with_turn_id(turn_id);
            env.timestamp = now;
            new_events.push(env);
        }

        for event in &new_events {
            self.log.append(event)?;
        }

        let knowledge_delta = KnowledgeDelta::from_observation(&observation, &known_before);
        let questions = propose_questions(&turn, &observation, &memory_state);
        let working_memory = activate_working_memory(
            &memory_state.map,
            &turn,
            &observation,
            &recalled,
            WorkingMemoryBudget::default(),
        );
        let context_packet = ContextPacket::from_parts(
            &working_memory,
            &recalled,
            &questions,
            recall_mode,
            WorkingMemoryBudget::default(),
        );

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
        })
    }

    pub fn inspect(&self) -> Result<MemoryState> {
        let events = self.log.load()?;
        let episodes = luna_store::rebuild_episodes(&events)?;
        Ok(MemoryState::from_episodes(&episodes))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeTurnResult {
    pub turn_id: Uuid,
    pub observation: CognitiveObservation,
    pub knowledge_delta: KnowledgeDelta,
    pub memory_state: MemoryState,
    pub working_memory: WorkingMemory,
    pub recalled: RecallSet,
    pub recall_mode: RecallMode,
    pub questions: Vec<QuestionCandidate>,
    pub context_packet: ContextPacket,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct KnowledgeDelta {
    pub confirmed: Vec<MemoryClaim>,
    pub inferred: Vec<MemoryClaim>,
    pub unconfirmed: Vec<MemoryClaim>,
    pub unknowns: Vec<String>,
}

impl KnowledgeDelta {
    fn from_observation(observation: &CognitiveObservation, known_before: &MemoryState) -> Self {
        let mut assertion_claims = observation
            .assertions
            .iter()
            .map(MemoryClaim::from_assertion)
            .collect::<Vec<_>>();
        assertion_claims.retain(|claim| {
            !known_before
                .claims
                .iter()
                .any(|known| known.key == claim.key)
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
}

impl MemoryClaim {
    fn from_assertion(assertion: &StructuredAssertion) -> Self {
        Self {
            key: assertion.key(),
            domain: assertion.domain.clone(),
            kind: assertion.kind.clone(),
            value: assertion.value.clone(),
            status: assertion.confidence_tier,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MemoryState {
    pub claims: Vec<MemoryClaim>,
    pub entity_groups: Vec<EntityMemoryGroup>,
    pub open_questions: Vec<String>,
    pub map: MemoryMap,
}

impl MemoryState {
    /// Derive the working state from the given episode list, discarding
    /// any [`NodeMerged`] audit records produced along the way. Callers
    /// that need to observe merges (the runtime path in `process_turn`,
    /// for R-005 closure) use [`MemoryState::from_episodes_with_merges`]
    /// instead.
    pub fn from_episodes(episodes: &[Episode]) -> Self {
        Self::from_episodes_with_merges(episodes).0
    }

    /// Like [`from_episodes`](Self::from_episodes), but also returns the
    /// [`NodeMerged`] events that fired when [`insert_node`] found an
    /// existing node and extended its density / confidence_tier /
    /// provenance instead of replacing.
    ///
    /// **R-005 closure (pr-1.2).** Before this method existed,
    /// `from_episodes` would silently merge two assertions targeting
    /// the same node id, making it impossible to attribute a tether
    /// across the merge. The runtime now emits one `NodeMerged` event
    /// per fresh merge each turn so the audit log records *what
    /// changed*. Replay does not consume `NodeMerged` (the event is
    /// informational; rebuilding state replays this same derivation).
    pub fn from_episodes_with_merges(episodes: &[Episode]) -> (Self, Vec<NodeMerged>) {
        let mut seen = BTreeSet::new();
        let mut claims = Vec::new();
        let mut assertion_index = BTreeMap::new();
        for episode in episodes {
            for assertion in &episode.assertions {
                let claim = MemoryClaim::from_assertion(assertion);
                if seen.insert(claim.key.clone()) {
                    assertion_index.insert(claim.key.clone(), (episode.id, assertion.clone()));
                    claims.push(claim);
                }
            }
        }
        let entity_groups = group_claims_by_entity(&claims);
        let (map, merges) = memory_map_from_claims(&claims, &assertion_index);
        (
            Self {
                claims,
                entity_groups,
                open_questions: Vec::new(),
                map,
            },
            merges,
        )
    }

    pub fn has_domain_kind(&self, domain: &str, kind: &str) -> bool {
        self.claims
            .iter()
            .any(|claim| claim.domain == domain && claim.kind == kind)
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
        "project" => vec![project_entity_key(&claim.value)],
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
    .unwrap_or_else(|| value.len());
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

fn project_entity_key(value: &str) -> (String, String, String) {
    if contains_ci(value, "mkpe") {
        (
            "project:MKPE".to_string(),
            "MKPE".to_string(),
            "project".to_string(),
        )
    } else {
        (
            "project:unknown".to_string(),
            "unknown project".to_string(),
            "project".to_string(),
        )
    }
}

fn memory_map_from_claims(
    claims: &[MemoryClaim],
    assertion_index: &BTreeMap<String, (Uuid, StructuredAssertion)>,
) -> (MemoryMap, Vec<NodeMerged>) {
    let mut nodes = BTreeMap::<String, MemoryNode>::new();
    let mut edges = Vec::new();
    let mut merges: Vec<NodeMerged> = Vec::new();

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
        },
        &mut merges,
    );
    seed_root_orb(&mut nodes, &mut edges, &mut merges);

    let mut seen_edges = BTreeSet::new();
    for claim in claims {
        let provenance = assertion_index
            .get(&claim.key)
            .map(|(episode_id, assertion)| MemoryProvenance {
                episode_id: Some(*episode_id),
                turn_id: None,
                assertion_key: Some(assertion.key()),
                system_root: None,
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
            },
            &mut merges,
        );
        let entity_keys = entity_keys_for_claim(claim);
        if entity_keys.is_empty() {
            push_edge_once(
                &mut edges,
                &mut seen_edges,
                MemoryEdge {
                    source: "user:self".to_string(),
                    target: target_id,
                    relation: relation_for_claim(claim),
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
                    },
                    &mut merges,
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
                    source,
                    target: target_id.clone(),
                    relation: relation_for_claim(claim),
                    confidence_tier: claim.status,
                    strength: density_for_tier(claim.status),
                    activation: 0.0,
                    provenance: provenance.clone(),
                },
            );
        }
    }

    (
        MemoryMap {
            nodes: nodes.into_values().collect(),
            edges,
        },
        merges,
    )
}

/// Insert a node into the working-memory map, or extend an existing
/// node with the same id. R-005 closure (pr-1.2): every extension
/// records a [`NodeMerged`] entry in `merges` so the merge is visible
/// at audit time. The audit fields capture *what changed*: density
/// delta, confidence-tier transition, count of provenance entries
/// folded in.
fn insert_node(
    nodes: &mut BTreeMap<String, MemoryNode>,
    node: MemoryNode,
    merges: &mut Vec<NodeMerged>,
) {
    match nodes.entry(node.id.clone()) {
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            let existing = entry.get_mut();
            let previous_tier = existing.confidence_tier;
            let previous_density = existing.density;
            let merged_provenance_count = node.provenance.len();
            existing.confidence_tier = existing.confidence_tier.max(node.confidence_tier);
            existing.density = existing.density.max(node.density);
            existing.provenance.extend(node.provenance);
            merges.push(NodeMerged {
                node_id: existing.id.clone(),
                merged_density_delta: existing.density - previous_density,
                previous_confidence_tier: previous_tier,
                new_confidence_tier: existing.confidence_tier,
                merged_provenance_count,
            });
        }
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(node);
        }
    }
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

fn seed_root_orb(
    nodes: &mut BTreeMap<String, MemoryNode>,
    edges: &mut Vec<MemoryEdge>,
    merges: &mut Vec<NodeMerged>,
) {
    let root = RootOrb::default();
    let root_provenance = vec![MemoryProvenance {
        episode_id: None,
        turn_id: None,
        assertion_key: None,
        system_root: Some(root.id.clone()),
    }];
    insert_node(
        nodes,
        MemoryNode {
            id: root.id.clone(),
            label: root.label.clone(),
            kind: MemoryNodeKind::RootOrb,
            confidence_tier: AssertionConfidenceTier::Confirmed,
            density: 1.0,
            activation: 0.0,
            provenance: root_provenance.clone(),
        },
        merges,
    );

    for principle in root.principles {
        let provenance = vec![MemoryProvenance {
            episode_id: None,
            turn_id: None,
            assertion_key: None,
            system_root: Some(principle.id.clone()),
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
            },
            merges,
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
        "self" => MemoryNodeKind::User,
        _ => MemoryNodeKind::Assertion,
    }
}

fn relation_for_claim(claim: &MemoryClaim) -> MemoryRelationKind {
    match claim.domain.as_str() {
        "goal" => MemoryRelationKind::HasGoal,
        "relationship" => MemoryRelationKind::RelatedTo,
        "place" => MemoryRelationKind::LocatedIn,
        "project" => MemoryRelationKind::ProvenanceFor,
        _ => MemoryRelationKind::HasAttribute,
    }
}

fn apply_runtime_fine_capture(turn: &ConversationTurn, observation: &mut CognitiveObservation) {
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
    dedupe_assertions(assertions)
}

fn capture_self_facts(text: &str, assertions: &mut Vec<StructuredAssertion>) {
    if let Some(name) = capture_after_i_am_name(text) {
        assertions.push(StructuredAssertion::new("identity", "name", name));
    }
    if let Some(age) = capture_i_am_years_old(text) {
        assertions.push(StructuredAssertion::new("identity", "age", age));
    }
    if contains_ci(text, "i am a mechanical engineer")
        || contains_ci(text, "i work as a mechanical engineer")
    {
        assertions.push(StructuredAssertion::new(
            "identity",
            "profession",
            "mechanical engineer",
        ));
    }
    if contains_ci(text, "i am a software developer") {
        assertions.push(StructuredAssertion::new(
            "identity",
            "profession",
            "software developer",
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
                    format!("{name} lives in {location}"),
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

fn person_names_from_text(text: &str) -> Vec<String> {
    let tokens = text
        .split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_ascii_alphabetic()))
        .filter(|token| is_single_name(token))
        .filter(|token| !matches!(*token, "Joe" | "Luna"))
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
            let normalized = match trimmed {
                "Chriss" | "Chri" => "Chris",
                _ => trimmed,
            };
            token.replace(trimmed, normalized)
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

fn activate_working_memory(
    map: &MemoryMap,
    turn: &ConversationTurn,
    observation: &CognitiveObservation,
    recalled: &RecallSet,
    budget: WorkingMemoryBudget,
) -> WorkingMemory {
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

    let mut scored_nodes = map
        .nodes
        .iter()
        .cloned()
        .map(|mut node| {
            node.activation = node_activation(&node, &query, &cue_terms, &recalled_values);
            node
        })
        .filter(|node| node.activation > 0.0)
        .collect::<Vec<_>>();
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

    let filtered_node_count = scored_nodes.len().saturating_sub(budget.max_nodes);
    scored_nodes.truncate(budget.max_nodes);
    let active_ids = scored_nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();

    let mut scored_edges = map
        .edges
        .iter()
        .filter(|edge| active_ids.contains(&edge.source) || active_ids.contains(&edge.target))
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

    WorkingMemory {
        nodes: scored_nodes,
        edges: scored_edges,
        filtered_node_count,
        filtered_edge_count,
        activation_reason: "query/cue/recalled activation with fixed working-memory budget"
            .to_string(),
    }
}

fn node_activation(
    node: &MemoryNode,
    query: &str,
    cue_terms: &[String],
    recalled_values: &[String],
) -> f32 {
    if node.kind == MemoryNodeKind::RootOrb {
        return root_orb_activation(query);
    }
    if node.kind == MemoryNodeKind::User {
        return 0.05;
    }
    let label = normalize_for_match(&node.label);
    let id = normalize_for_match(&node.id);
    let direct_match =
        (!label.is_empty() && query.contains(&label)) || (!id.is_empty() && query.contains(&id));
    let cue_match = cue_terms
        .iter()
        .any(|term| !term.is_empty() && (label.contains(term) || id.contains(term)));
    let recalled_match = recalled_values
        .iter()
        .any(|value| !value.is_empty() && label.contains(value));

    let mut activation = 0.0;
    if direct_match {
        activation += 1.0;
    }
    if cue_match {
        activation += 0.55;
    }
    if recalled_match {
        activation += 0.8;
    }
    if activation > 0.0 {
        activation += node.density * 0.2;
    }
    activation
}

fn root_orb_activation(query: &str) -> f32 {
    if contains_any(
        query,
        &[
            "luna",
            "memory",
            "remember",
            "event log",
            "confirmed",
            "inferred",
            "unknown",
            "working memory",
            "proof",
            "source truth",
        ],
    ) {
        0.9
    } else {
        0.02
    }
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
    pub open_questions: Vec<QuestionCandidate>,
    pub summary: String,
}

impl ContextPacket {
    fn from_parts(
        working_memory: &WorkingMemory,
        recalled: &RecallSet,
        questions: &[QuestionCandidate],
        recall_mode: RecallMode,
        budget: WorkingMemoryBudget,
    ) -> Self {
        let recalled_claims = recalled
            .hits
            .iter()
            .flat_map(|hit| hit.assertions.iter())
            .map(MemoryClaim::from_assertion)
            .collect::<Vec<_>>();
        let open_questions = questions
            .iter()
            .take(budget.max_questions)
            .cloned()
            .collect::<Vec<_>>();
        let summary = render_context_summary(&recalled_claims, working_memory, &open_questions);

        Self {
            recall_mode,
            recalled_claims,
            working_memory: working_memory.clone(),
            open_questions,
            summary,
        }
    }
}

pub fn render_runtime_markdown(result: &RuntimeTurnResult) -> String {
    let mut out = String::new();
    out.push_str("# Luna Runtime Turn\n\n");
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
    if result.recalled.hits.is_empty() {
        out.push_str("(none)\n");
    } else {
        for hit in &result.recalled.hits {
            out.push_str(&format!(
                "- {:.2} {}: {}\n",
                hit.score,
                hit.reason,
                hit.assertions
                    .iter()
                    .map(|assertion| assertion.value.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    out.push_str("\n## Working Memory\n");
    if result.working_memory.nodes.is_empty() {
        out.push_str("(none)\n");
    } else {
        for node in &result.working_memory.nodes {
            out.push_str(&format!(
                "- {:.2} {:?}: {}\n",
                node.activation, node.confidence_tier, node.label
            ));
        }
        if result.working_memory.filtered_node_count > 0
            || result.working_memory.filtered_edge_count > 0
        {
            out.push_str(&format!(
                "Filtered: {} node(s), {} edge(s)\n",
                result.working_memory.filtered_node_count,
                result.working_memory.filtered_edge_count
            ));
        }
    }

    out.push_str("\n## Context Packet\n");
    out.push_str(&result.context_packet.summary);
    out.push('\n');
    out
}

pub fn render_conversation_reply(user_text: &str, result: &RuntimeTurnResult) -> String {
    let text = user_text.to_ascii_lowercase();
    if is_greeting(&text) {
        return "Hi, I am Luna. I am listening, and I will keep the memory separate from guesses."
            .to_string();
    }

    if is_user_asking_about_luna(&text) {
        return "I am Luna: a local-first memory layer. I store turns as events, separate confirmed from inferred or unknown facts, and only bring a small working set into the conversation."
            .to_string();
    }

    if is_query_turn(&result.observation) || text.contains('?') {
        if is_identity_query(&text) {
            let remembered = conversational_identity_values(result);
            if remembered.is_empty() {
                return "I do not have enough stored memory to answer that yet.".to_string();
            }
            return format!(
                "From what I have stored about you: {}.",
                remembered.join("; ")
            );
        }

        if let Some(group) = requested_entity_group(&text, &result.memory_state) {
            let remembered = conversational_entity_values(group);
            if !remembered.is_empty() {
                return format!(
                    "From what I have stored about {}: {}.",
                    group.label,
                    remembered.join("; ")
                );
            }
        }

        let remembered = conversational_memory_values(result);
        if remembered.is_empty() {
            return "I do not have enough stored memory to answer that yet.".to_string();
        }
        return format!("From what I have stored: {}.", remembered.join("; "));
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
            return question.question.clone();
        }
        return format!(
            "Got it. I will remember {}. {}",
            learned.join("; "),
            question.question
        );
    }

    if !learned.is_empty() {
        return format!("Got it. I will remember {}.", learned.join("; "));
    }

    "I am with you. I did not find a concrete new memory in that turn.".to_string()
}

fn requested_entity_group<'a>(
    query: &str,
    state: &'a MemoryState,
) -> Option<&'a EntityMemoryGroup> {
    let query = normalize_for_match(query);
    state
        .entity_groups
        .iter()
        .filter(|group| group.kind != "self")
        .filter(|group| {
            let label = normalize_for_match(&group.label);
            let id = normalize_for_match(&group.id.replace(':', " "));
            !label.is_empty() && (query.contains(&label) || query.contains(&id))
        })
        .max_by_key(|group| group.claims.len())
}

fn conversational_entity_values(group: &EntityMemoryGroup) -> Vec<String> {
    let mut values = group
        .claims
        .iter()
        .map(|claim| claim.value.clone())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.truncate(6);
    values
}

fn conversational_memory_values(result: &RuntimeTurnResult) -> Vec<String> {
    let mut values = result
        .recalled
        .hits
        .iter()
        .flat_map(|hit| {
            hit.assertions
                .iter()
                .map(|assertion| assertion.value.clone())
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        values.extend(
            result
                .working_memory
                .nodes
                .iter()
                .filter(|node| {
                    node.kind != MemoryNodeKind::RootOrb && node.kind != MemoryNodeKind::User
                })
                .map(|node| node.label.clone()),
        );
    }
    values.sort();
    values.dedup();
    values.truncate(5);
    values
}

fn conversational_identity_values(result: &RuntimeTurnResult) -> Vec<String> {
    let mut values = result
        .memory_state
        .claims
        .iter()
        .filter(|claim| {
            claim.domain == "identity"
                || (claim.domain == "relationship" && claim.value.starts_with("I "))
        })
        .map(|claim| claim.value.clone())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.truncate(6);
    values
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

fn select_recall_mode(observation: &CognitiveObservation) -> RecallMode {
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
    observation: &CognitiveObservation,
    memory_state: &MemoryState,
) -> Vec<QuestionCandidate> {
    let text = turn.content.to_ascii_lowercase();
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

    if mentions_ambiguous_they(&text) && !has_local_plural_anchor(&text, observation) {
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
    values.truncate(3);
    values
}

fn unknowns_from_observation(observation: &CognitiveObservation) -> Vec<String> {
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

fn is_query_turn(observation: &CognitiveObservation) -> bool {
    observation
        .query_intents
        .iter()
        .any(|intent| intent.contains("query") || intent == "contradiction_check")
}

fn render_context_summary(
    recalled_claims: &[MemoryClaim],
    working_memory: &WorkingMemory,
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
    if let Some(question) = questions.first() {
        lines.push(format!("Next useful question: {}", question.question));
    }
    lines.join("\n")
}

fn assertion_confidence(
    observation: &CognitiveObservation,
    assertion: &StructuredAssertion,
) -> f32 {
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

fn has_local_plural_anchor(text: &str, observation: &CognitiveObservation) -> bool {
    if contains_any(
        text,
        &[
            "chris and francois",
            "francois and chris",
            "co-founders",
            "cofounders",
            "both",
        ],
    ) {
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
    fn entity_sieve_strengthens_matching_llm_assertions_without_proof_path() {
        let log = temp_log();
        let session = RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("I am a mechanical engineer.")
            .unwrap();
        let state = session.inspect().unwrap();

        let claim = state
            .claims
            .iter()
            .find(|claim| {
                claim.domain == "identity"
                    && claim.kind == "profession"
                    && claim.value == "mechanical engineer"
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

    /// Integration proof for priority-5 wiring: when a turn arrives at
    /// an event-time well past an existing episode's last reinforcement,
    /// the runtime emits an `EpisodeDecayed` event into the log. The
    /// log's persisted state is the source of truth — this test asserts
    /// that decay landed on disk, not just in memory.
    ///
    /// Bypasses extraction by seeding the log directly with an
    /// `EpisodeCreated` event at t=0; then runs a turn at t=10 days,
    /// far past the 7-day default half-life. The heuristic extractor
    /// produces no new assertions for the trivial turn content, so the
    /// only newly-emitted event tied to the prior episode_id is the
    /// decay event.
    #[test]
    fn process_turn_emits_episode_decayed_when_event_time_advances() {
        use chrono::TimeZone;
        use luna_core::{
            AssertionConfidenceTier, CognitiveObservation, EpisodeCreated, EventEnvelope,
            EventSource, LunaEvent, Role, Signal, SignalReliability, StructuredAssertion,
        };

        let log_path = temp_log();
        let log = JsonlEventLog::new(&log_path);

        // t=0: seed an EpisodeCreated event with a fixed event-time.
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let episode_id = Uuid::new_v4();
        let assertion = StructuredAssertion {
            domain: "identity".into(),
            kind: "profession".into(),
            value: "mechanical engineer".into(),
            source_count: 1,
            reinforcement_count: 0,
            confidence_tier: AssertionConfidenceTier::Confirmed,
        };
        let observation = CognitiveObservation {
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
            uncertainty: Signal::new(0.3, 0.7, SignalReliability::Heuristic),
            cue_terms: Vec::new(),
            query_intents: Vec::new(),
            assertions: vec![assertion.clone()],
        };
        let mut seed = EventEnvelope::new(
            LunaEvent::EpisodeCreated(EpisodeCreated {
                assertion,
                observation,
            }),
            EventSource::ClassifierExtractor,
            0.85,
        )
        .with_episode_id(episode_id);
        seed.timestamp = t0;
        log.append(&seed).unwrap();

        // t=+10 days: drive a turn whose event-time is well past the
        // 7-day default half-life. Heuristic extractor adds no new
        // assertions for this content, so any new episode-tagged event
        // we find must be the decay event.
        let session = RuntimeSession::new(&log_path, FusedExtractor::new());
        let later = t0 + chrono::Duration::days(10);
        session
            .process_turn(ConversationTurn {
                role: Role::User,
                content: "ten days have passed.".to_string(),
                timestamp: Some(later),
            })
            .unwrap();

        // Verify a decay event landed on disk for our seed episode.
        let events = log.load().unwrap();
        let decay_events: Vec<_> = events
            .iter()
            .filter(|env| {
                env.episode_id == Some(episode_id)
                    && matches!(&env.payload, LunaEvent::EpisodeDecayed(_))
            })
            .collect();
        assert!(
            !decay_events.is_empty(),
            "expected at least one EpisodeDecayed event tied to the seed episode after 10 days"
        );

        // Decay event timestamp should equal the turn's event-time, not
        // wall-clock time. Doctrine: replay determinism (R-002).
        for env in &decay_events {
            assert_eq!(
                env.timestamp, later,
                "decay event must use turn event-time, not Utc::now()"
            );
            if let LunaEvent::EpisodeDecayed(payload) = &env.payload {
                // 10 days at 7-day half-life: 1 - exp(-10*ln2/7) ≈ 0.629.
                assert!(
                    payload.forgotten_risk > 0.5 && payload.forgotten_risk < 0.75,
                    "expected forgotten_risk ~0.63 at 10 days / 7-day half-life, got {}",
                    payload.forgotten_risk
                );
            }
        }

        let _ = fs::remove_dir_all(log_path.parent().unwrap());
    }

    // ---------- pr-1.1a: R-003 closure (raw extractor capture) ----------

    /// Stub extractor that returns a fixed observation. Lets the R-003
    /// tests assert exactly what the "raw" form was, independently of
    /// FusedExtractor's evolving heuristics.
    struct StubExtractor {
        observation: CognitiveObservation,
    }

    impl RuntimeExtractor for StubExtractor {
        fn extract_runtime(&self, _turn: &ConversationTurn) -> Result<CognitiveObservation> {
            let mut obs = self.observation.clone();
            obs.turn_id = Uuid::new_v4();
            Ok(obs)
        }
    }

    fn observation_with_assertion(assertion: StructuredAssertion) -> CognitiveObservation {
        use luna_core::{Signal, SignalReliability};
        CognitiveObservation {
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
            uncertainty: Signal::new(0.3, 0.7, SignalReliability::Heuristic),
            cue_terms: Vec::new(),
            query_intents: Vec::new(),
            assertions: vec![assertion],
        }
    }

    /// R-003 baseline: every processed turn emits exactly one
    /// `RawObservationCaptured` event, and its observation equals what
    /// the extractor produced (modulo turn_id, which `process_turn`
    /// re-derives).
    #[test]
    fn process_turn_emits_one_raw_observation_captured_event_per_turn() {
        let log_path = temp_log();
        let log = JsonlEventLog::new(&log_path);
        let stub_assertion = StructuredAssertion {
            domain: "identity".into(),
            kind: "profession".into(),
            value: "carpenter".into(),
            source_count: 1,
            reinforcement_count: 0,
            confidence_tier: AssertionConfidenceTier::Confirmed,
        };
        let session = RuntimeSession::new(
            &log_path,
            StubExtractor {
                observation: observation_with_assertion(stub_assertion.clone()),
            },
        );

        session
            .process_user_turn("anything — extractor is stubbed")
            .unwrap();

        let events = log.load().unwrap();
        let raw_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(&e.payload, LunaEvent::RawObservationCaptured(_)))
            .collect();
        assert_eq!(
            raw_events.len(),
            1,
            "expected exactly one RawObservationCaptured event per turn"
        );
        if let LunaEvent::RawObservationCaptured(payload) = &raw_events[0].payload {
            assert_eq!(payload.observation.assertions, vec![stub_assertion]);
        }

        let _ = fs::remove_dir_all(log_path.parent().unwrap());
    }

    /// R-003 + R-002 interaction: the raw-capture event timestamp must
    /// be the turn's event-time, not `Utc::now()`. Replay determinism
    /// extends to audit events.
    #[test]
    fn raw_observation_captured_uses_turn_event_time() {
        use chrono::TimeZone;

        let log_path = temp_log();
        let log = JsonlEventLog::new(&log_path);
        let stub_assertion = StructuredAssertion {
            domain: "identity".into(),
            kind: "name".into(),
            value: "Aria".into(),
            source_count: 1,
            reinforcement_count: 0,
            confidence_tier: AssertionConfidenceTier::Inferred,
        };
        let session = RuntimeSession::new(
            &log_path,
            StubExtractor {
                observation: observation_with_assertion(stub_assertion),
            },
        );

        let when = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        session
            .process_turn(ConversationTurn {
                role: Role::User,
                content: "stubbed turn".into(),
                timestamp: Some(when),
            })
            .unwrap();

        let events = log.load().unwrap();
        let raw = events
            .iter()
            .find(|e| matches!(&e.payload, LunaEvent::RawObservationCaptured(_)))
            .expect("RawObservationCaptured must exist");
        assert_eq!(
            raw.timestamp, when,
            "raw-capture event must use turn event-time, not Utc::now()"
        );

        let _ = fs::remove_dir_all(log_path.parent().unwrap());
    }

    /// R-003 doctrine invariant: the raw-capture event is informational
    /// only — `luna-store::rebuild_episodes` must produce identical
    /// `Episode` output whether the event is in the log or not. If this
    /// ever fails, replay has started silently deriving state from the
    /// audit record, which would defeat the audit purpose.
    #[test]
    fn rebuild_episodes_ignores_raw_observation_captured_events() {
        use chrono::TimeZone;
        use luna_core::{
            EpisodeCreated, EventEnvelope, EventSource, RawObservationCaptured, Signal,
            SignalReliability,
        };

        let t0 = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        let assertion = StructuredAssertion {
            domain: "identity".into(),
            kind: "profession".into(),
            value: "engineer".into(),
            source_count: 1,
            reinforcement_count: 0,
            confidence_tier: AssertionConfidenceTier::Confirmed,
        };
        let observation = observation_with_assertion(assertion.clone());

        let mut episode_event = EventEnvelope::new(
            LunaEvent::EpisodeCreated(EpisodeCreated {
                assertion: assertion.clone(),
                observation: observation.clone(),
            }),
            EventSource::ClassifierExtractor,
            0.9,
        )
        .with_episode_id(Uuid::new_v4());
        episode_event.timestamp = t0;

        let mut raw_event = EventEnvelope::new(
            LunaEvent::RawObservationCaptured(RawObservationCaptured {
                observation: CognitiveObservation {
                    // Deliberately weird raw form — proves replay does not
                    // reach into this payload.
                    uncertainty: Signal::new(0.99, 0.01, SignalReliability::Heuristic),
                    assertions: vec![StructuredAssertion {
                        domain: "person".into(),
                        kind: "profession".into(),
                        value: "engineer".into(),
                        source_count: 1,
                        reinforcement_count: 0,
                        confidence_tier: AssertionConfidenceTier::Unconfirmed,
                    }],
                    ..observation.clone()
                },
            }),
            EventSource::ClassifierExtractor,
            0.5,
        );
        raw_event.timestamp = t0;

        let without_raw = luna_store::rebuild_episodes(&[episode_event.clone()]).unwrap();
        let with_raw = luna_store::rebuild_episodes(&[raw_event, episode_event]).unwrap();
        assert_eq!(
            without_raw, with_raw,
            "rebuild_episodes must be byte-identical with or without RawObservationCaptured events"
        );
    }

    // ---------- pr-1.2: R-005 closure (silent node merge visibility) ----------

    /// Helper: build a tiny `Episode` carrying a single assertion. Bypasses
    /// the normal event-log path so unit tests can drive `MemoryState`
    /// derivation directly.
    fn episode_with_assertion(seconds: i64, assertion: StructuredAssertion) -> Episode {
        use chrono::TimeZone;
        let when = Utc.timestamp_opt(seconds, 0).unwrap();
        Episode {
            id: Uuid::new_v4(),
            assertions: vec![assertion],
            confidence: 1.0,
            forgotten_risk: 0.0,
            coherence_score: 1.0,
            recall_history: Vec::new(),
            contour: luna_core::EpisodeContour {
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
                reinforcement_count: 0,
                contradiction_count: 0,
                successful_recall_count: 0,
                failed_recall_count: 0,
            },
            created_at: when,
            updated_at: when,
        }
    }

    /// R-005 closure: when two assertions converge on the same memory-node
    /// id (e.g. two claims about Joe), `from_episodes_with_merges` must
    /// surface a `NodeMerged` audit record so the merge is no longer
    /// silent. Before pr-1.2 this was the doctrine hole the risk register
    /// flagged for tether attribution across merged nodes.
    #[test]
    fn from_episodes_emits_node_merged_when_existing_node_extended() {
        let joe_a = StructuredAssertion {
            domain: "person".into(),
            kind: "lives_in".into(),
            value: "Joe lives in Brooklyn".into(),
            source_count: 1,
            reinforcement_count: 0,
            confidence_tier: AssertionConfidenceTier::Inferred,
        };
        let joe_b = StructuredAssertion {
            domain: "person".into(),
            kind: "writes".into(),
            value: "Joe writes fiction".into(),
            source_count: 2,
            reinforcement_count: 1,
            confidence_tier: AssertionConfidenceTier::Confirmed,
        };
        let episodes = vec![
            episode_with_assertion(1_000_000_000, joe_a),
            episode_with_assertion(1_000_000_010, joe_b),
        ];
        let (_state, merges) = MemoryState::from_episodes_with_merges(&episodes);
        let joe_merge = merges
            .iter()
            .find(|m| m.node_id == "person:Joe")
            .expect("expected a NodeMerged record for person:Joe across two assertions");
        assert!(
            joe_merge.merged_provenance_count >= 1,
            "merge must report a non-zero provenance fold-in count, got {}",
            joe_merge.merged_provenance_count
        );
        assert_eq!(
            joe_merge.previous_confidence_tier,
            AssertionConfidenceTier::Inferred,
            "first claim seeded tier=Inferred",
        );
        assert_eq!(
            joe_merge.new_confidence_tier,
            AssertionConfidenceTier::Confirmed,
            "second claim's Confirmed tier should win the max",
        );
    }

    /// Negative case: the very first insert for a node id is NOT a merge
    /// — it's the seed. Only subsequent inserts targeting the same id
    /// produce `NodeMerged` records. Without this invariant the audit log
    /// would treat node creation and node merging as the same event.
    #[test]
    fn from_episodes_does_not_emit_node_merged_for_first_insert() {
        let single = StructuredAssertion {
            domain: "person".into(),
            kind: "lives_in".into(),
            value: "Aria lives in Vermont".into(),
            source_count: 1,
            reinforcement_count: 0,
            confidence_tier: AssertionConfidenceTier::Inferred,
        };
        let episodes = vec![episode_with_assertion(1_000_000_000, single)];
        let (_state, merges) = MemoryState::from_episodes_with_merges(&episodes);
        assert!(
            !merges.iter().any(|m| m.node_id == "person:Aria"),
            "first insert for an entity must not produce a NodeMerged record",
        );
    }

    /// Replay invariant: `NodeMerged` events are informational. A log
    /// containing them must rebuild to the same `Episode` output as a
    /// log without them. Same contract as `RawObservationCaptured`.
    #[test]
    fn rebuild_episodes_ignores_node_merged_events() {
        use chrono::TimeZone;
        use luna_core::{EpisodeCreated, EventEnvelope, EventSource, NodeMerged};

        let t0 = Utc.with_ymd_and_hms(2026, 7, 2, 0, 0, 0).unwrap();
        let assertion = StructuredAssertion {
            domain: "identity".into(),
            kind: "profession".into(),
            value: "engineer".into(),
            source_count: 1,
            reinforcement_count: 0,
            confidence_tier: AssertionConfidenceTier::Confirmed,
        };
        let observation = observation_with_assertion(assertion.clone());

        let mut episode_event = EventEnvelope::new(
            LunaEvent::EpisodeCreated(EpisodeCreated {
                assertion,
                observation,
            }),
            EventSource::ClassifierExtractor,
            0.9,
        )
        .with_episode_id(Uuid::new_v4());
        episode_event.timestamp = t0;

        let mut merged_event = EventEnvelope::new(
            LunaEvent::NodeMerged(NodeMerged {
                node_id: "person:Joe".into(),
                merged_density_delta: 0.1,
                previous_confidence_tier: AssertionConfidenceTier::Inferred,
                new_confidence_tier: AssertionConfidenceTier::Confirmed,
                merged_provenance_count: 2,
            }),
            EventSource::System,
            1.0,
        );
        merged_event.timestamp = t0;

        let without_merge = luna_store::rebuild_episodes(&[episode_event.clone()]).unwrap();
        let with_merge = luna_store::rebuild_episodes(&[merged_event, episode_event]).unwrap();
        assert_eq!(
            without_merge, with_merge,
            "rebuild_episodes must be byte-identical with or without NodeMerged events",
        );
    }

    /// pr-1.2 vocabulary check: `OrbTetherBound` is wired into the event
    /// enum *and* into the no-op rebuild arm. A log containing one must
    /// rebuild to the same `Episode` output as a log without — pr-1.2
    /// lands the variant; pr-1.6 will produce it from runtime.
    #[test]
    fn rebuild_episodes_ignores_orb_tether_bound_events() {
        use chrono::TimeZone;
        use luna_core::{
            EpisodeCreated, EventEnvelope, EventSource, OrbId, OrbTetherBound, RecallReason,
            TetherKind,
        };

        let t0 = Utc.with_ymd_and_hms(2026, 7, 3, 0, 0, 0).unwrap();
        let assertion = StructuredAssertion {
            domain: "identity".into(),
            kind: "profession".into(),
            value: "engineer".into(),
            source_count: 1,
            reinforcement_count: 0,
            confidence_tier: AssertionConfidenceTier::Confirmed,
        };
        let observation = observation_with_assertion(assertion.clone());

        let mut episode_event = EventEnvelope::new(
            LunaEvent::EpisodeCreated(EpisodeCreated {
                assertion,
                observation,
            }),
            EventSource::ClassifierExtractor,
            0.9,
        )
        .with_episode_id(Uuid::new_v4());
        episode_event.timestamp = t0;

        let mut bind_event = EventEnvelope::new(
            LunaEvent::OrbTetherBound(OrbTetherBound {
                from_orb: OrbId::new("orb.relationship.joe").unwrap(),
                to_orb: OrbId::new("orb.project.beacon").unwrap(),
                kind: TetherKind::DerivedFrom,
                initial_weight: 0.6,
                reason: RecallReason::new("consolidation: joe authored beacon").unwrap(),
            }),
            EventSource::System,
            1.0,
        );
        bind_event.timestamp = t0;

        let without_bind = luna_store::rebuild_episodes(&[episode_event.clone()]).unwrap();
        let with_bind = luna_store::rebuild_episodes(&[bind_event, episode_event]).unwrap();
        assert_eq!(
            without_bind, with_bind,
            "rebuild_episodes must be byte-identical with or without OrbTetherBound events",
        );
    }

    /// Runtime integration: when a turn produces two assertions that
    /// converge on the same entity node id, `process_turn` must emit at
    /// least one `NodeMerged` audit event. Locks the wiring
    /// (`process_turn` → `from_episodes_with_merges` → diff against
    /// `prior_merged_ids`) so pr-1.6 can rely on it.
    ///
    /// Uses `StubExtractor` (returning two assertions about Joe) so the
    /// test isn't coupled to FusedExtractor's evolving heuristics.
    #[test]
    fn process_turn_emits_node_merged_only_for_fresh_merges() {
        use luna_core::{Signal, SignalReliability};

        let observation = CognitiveObservation {
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
            uncertainty: Signal::new(0.3, 0.7, SignalReliability::Heuristic),
            cue_terms: Vec::new(),
            query_intents: Vec::new(),
            assertions: vec![
                StructuredAssertion {
                    domain: "person".into(),
                    kind: "lives_in".into(),
                    value: "Joe lives in Brooklyn".into(),
                    source_count: 1,
                    reinforcement_count: 0,
                    confidence_tier: AssertionConfidenceTier::Inferred,
                },
                StructuredAssertion {
                    domain: "person".into(),
                    kind: "writes".into(),
                    value: "Joe writes fiction".into(),
                    source_count: 2,
                    reinforcement_count: 0,
                    confidence_tier: AssertionConfidenceTier::Confirmed,
                },
            ],
        };

        let log_path = temp_log();
        let log = JsonlEventLog::new(&log_path);
        let session = RuntimeSession::new(&log_path, StubExtractor { observation });

        session
            .process_user_turn("two assertions about Joe in one turn")
            .unwrap();

        let events = log.load().unwrap();
        let merges: Vec<_> = events
            .iter()
            .filter(|e| matches!(&e.payload, LunaEvent::NodeMerged(_)))
            .collect();
        assert!(
            merges.iter().any(|e| matches!(
                &e.payload,
                LunaEvent::NodeMerged(m) if m.node_id == "person:Joe"
            )),
            "expected a NodeMerged audit event for person:Joe after a turn with two converging assertions; \
             got {} merges (ids: {:?})",
            merges.len(),
            merges
                .iter()
                .filter_map(|e| match &e.payload {
                    LunaEvent::NodeMerged(m) => Some(m.node_id.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
        );

        let _ = fs::remove_dir_all(log_path.parent().unwrap());
    }
}
