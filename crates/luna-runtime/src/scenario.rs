use crate::{
    bridge_runtime_events_to_topology, commit_runtime_events_to_topology_ledger,
    ledger_events_hash, plan_conversation_response, render_conversation_reply,
    topology_commit_from_runtime_ledger_commit, topology_node_ref_for_runtime_ref,
    MemoryIntakeAction, ResponsePlanAction, RuntimeExtractor, RuntimeSession,
};
use chrono::{DateTime, Utc};
use luna_core::{
    AssertionConfidenceTier, AssertionLifecycleStatus, ConversationTurn, MemoryRelationKind, Role,
};
use luna_events::{JsonlEventLog, LunaEvent, StoredEvent};
use luna_replay::ReplayAuditor;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeScenarioFile {
    #[serde(default)]
    pub name: Option<String>,
    pub turns: Vec<RuntimeScenarioTurn>,
    #[serde(default)]
    pub restart_after_turns: Vec<usize>,
    #[serde(default)]
    pub checks: RuntimeScenarioChecks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuntimeScenarioTurn {
    Text(String),
    Timed(RuntimeScenarioTimedTurn),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeScenarioTimedTurn {
    pub content: String,
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
}

impl RuntimeScenarioTurn {
    pub fn content(&self) -> &str {
        match self {
            Self::Text(content) => content,
            Self::Timed(turn) => &turn.content,
        }
    }

    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Text(_) => None,
            Self::Timed(turn) => turn.timestamp,
        }
    }
}

impl From<&str> for RuntimeScenarioTurn {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<String> for RuntimeScenarioTurn {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RuntimeScenarioChecks {
    #[serde(default)]
    pub must_contain: Vec<String>,
    #[serde(default)]
    pub must_not_contain: Vec<String>,
    #[serde(default)]
    pub claims: ClaimChecks,
    #[serde(default)]
    pub entities: EntityChecks,
    #[serde(default)]
    pub relations: RelationChecks,
    #[serde(default)]
    pub working_memory: WorkingMemoryChecks,
    #[serde(default)]
    pub recall: RecallChecks,
    #[serde(default)]
    pub unknowns: UnknownChecks,
    #[serde(default)]
    pub intake: IntakeChecks,
    #[serde(default)]
    pub answers: Vec<AnswerCheck>,
    #[serde(default)]
    pub time: TimeChecks,
    #[serde(default)]
    pub topology_bridge: TopologyBridgeChecks,
    #[serde(default)]
    pub runtime_replay_audit: RuntimeReplayAuditChecks,
    #[serde(default)]
    pub manuscript_one_read: ManuscriptOneReadChecks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AnswerCheck {
    pub turn_index: usize,
    #[serde(default)]
    pub must_contain: Vec<String>,
    #[serde(default)]
    pub must_not_contain: Vec<String>,
    #[serde(default)]
    pub must_include_confidence: Vec<AssertionConfidenceTier>,
    #[serde(default)]
    pub must_have_recall_hit: bool,
    #[serde(default)]
    pub must_have_recall_reason: bool,
    #[serde(default)]
    pub max_working_nodes: Option<usize>,
    #[serde(default)]
    pub max_working_edges: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ClaimChecks {
    #[serde(default)]
    pub must_include: Vec<ClaimCheck>,
    #[serde(default)]
    pub must_exclude: Vec<ClaimCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimCheck {
    pub domain: String,
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub confidence_tier: Option<AssertionConfidenceTier>,
    #[serde(default)]
    pub lifecycle_status: Option<AssertionLifecycleStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct EntityChecks {
    #[serde(default)]
    pub must_include: Vec<EntityCheck>,
    #[serde(default)]
    pub must_exclude: Vec<EntityCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityCheck {
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub claims: Vec<ClaimCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RelationChecks {
    #[serde(default)]
    pub must_include: Vec<RelationCheck>,
    #[serde(default)]
    pub must_exclude: Vec<RelationCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationCheck {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_label: Option<String>,
    pub relation: MemoryRelationKind,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub target_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WorkingMemoryChecks {
    #[serde(default)]
    pub max_nodes: Option<usize>,
    #[serde(default)]
    pub max_edges: Option<usize>,
    #[serde(default)]
    pub must_include_labels: Vec<String>,
    #[serde(default)]
    pub must_exclude_labels: Vec<String>,
    #[serde(default)]
    pub activation_reason_must_contain: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RecallChecks {
    #[serde(default)]
    pub must_have_hit: bool,
    #[serde(default)]
    pub must_have_reason: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UnknownChecks {
    #[serde(default)]
    pub must_include: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct IntakeChecks {
    #[serde(default)]
    pub must_include_actions: Vec<MemoryIntakeAction>,
    #[serde(default)]
    pub must_match_actions: Vec<MemoryIntakeAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TimeChecks {
    #[serde(default)]
    pub require_timestamps: bool,
    #[serde(default)]
    pub min_gap_seconds: Option<i64>,
    #[serde(default)]
    pub min_any_gap_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TopologyBridgeChecks {
    #[serde(default)]
    pub nodes: Vec<TopologyNodeCheck>,
    #[serde(default)]
    pub tethers: Vec<TopologyTetherCheck>,
    #[serde(default)]
    pub require_source_event_hashes: bool,
    #[serde(default)]
    pub require_recall_reason_available: bool,
    #[serde(default)]
    pub forbid_root_orb_user_fact_leakage: bool,
    #[serde(default)]
    pub require_durable_commit: bool,
    #[serde(default)]
    pub min_committed_nodes: Option<usize>,
    #[serde(default)]
    pub min_committed_tethers: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReplayAuditChecks {
    #[serde(default)]
    pub require_clean: bool,
    #[serde(default)]
    pub require_source_event_hashes: bool,
    #[serde(default)]
    pub min_stored_events: Option<usize>,
    #[serde(default)]
    pub min_topology_nodes: Option<usize>,
    #[serde(default)]
    pub min_topology_tethers: Option<usize>,
    #[serde(default)]
    pub min_topology_orbs: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManuscriptOneReadChecks {
    #[serde(default)]
    pub require_source_read: bool,
    #[serde(default)]
    pub require_explicit_close: bool,
    #[serde(default)]
    pub retrieval_turns: Vec<usize>,
    #[serde(default)]
    pub forbid_source_after_close: bool,
    #[serde(default)]
    pub forbid_search_or_reread_after_close: bool,
    #[serde(default)]
    pub require_proof_eligible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopologyNodeCheck {
    pub runtime_entity_ref: String,
    #[serde(default)]
    pub topology_ref: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub claim_assertion_keys: Vec<String>,
    #[serde(default)]
    pub require_source_event_hash: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopologyTetherCheck {
    pub source_ref: String,
    pub target_ref: String,
    pub relation: MemoryRelationKind,
    #[serde(default)]
    pub topology_ref: Option<String>,
    #[serde(default)]
    pub require_source_event_hash: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeScenarioTurnSummary {
    pub assertion_count: usize,
    pub working_node_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeScenarioReport {
    pub name: String,
    pub turn_count: usize,
    pub log_path: String,
    pub log_retained: bool,
    pub turn_summaries: Vec<RuntimeScenarioTurnSummary>,
    pub check_count: usize,
    pub proof_eligibility_applicable: bool,
    pub proof_eligible: bool,
    pub proof_eligibility_failures: Vec<String>,
    pub failures: Vec<String>,
}

impl RuntimeScenarioReport {
    pub fn is_pass(&self) -> bool {
        self.failures.is_empty()
    }
}

pub fn run_runtime_scenario<E: RuntimeExtractor>(
    scenario_path: &Path,
    log: &Path,
    extractor: E,
    keep_log: bool,
) -> anyhow::Result<RuntimeScenarioReport> {
    let text = fs::read_to_string(scenario_path)?;
    let scenario: RuntimeScenarioFile = serde_json::from_str(&text)?;
    if log.exists() {
        fs::remove_file(log)?;
    }
    let mut session = RuntimeSession::new(log, &extractor);
    let mut results = Vec::new();
    let mut turn_summaries = Vec::new();

    for (index, turn) in scenario.turns.iter().enumerate() {
        let result = if let Some(timestamp) = turn.timestamp() {
            session.process_turn(ConversationTurn {
                role: Role::User,
                content: turn.content().to_string(),
                timestamp: Some(timestamp),
            })?
        } else {
            session.process_user_turn(turn.content().to_string())?
        };
        turn_summaries.push(RuntimeScenarioTurnSummary {
            assertion_count: result.observation.assertions.len(),
            working_node_count: result.working_memory.nodes.len(),
        });
        results.push(result);
        if scenario.restart_after_turns.contains(&(index + 1)) {
            session = RuntimeSession::new(log, &extractor);
        }
    }

    let state = session.inspect()?;
    let events = JsonlEventLog::new(log).load()?;
    let check_count = scenario_check_count(&scenario);
    let proof_eligibility_failures = evaluate_manuscript_one_read_protocol(
        &scenario.checks.manuscript_one_read,
        &scenario.turns,
        &results,
    );
    let mut failures = evaluate_runtime_scenario_with_events(&scenario, &state, &results, &events);
    if check_count == 0 {
        failures.push("scenario has zero executable memory checks".to_string());
    }

    let report = RuntimeScenarioReport {
        name: scenario.name.unwrap_or_else(|| "unnamed".to_string()),
        turn_count: scenario.turns.len(),
        log_path: log.display().to_string(),
        log_retained: keep_log,
        turn_summaries,
        check_count,
        proof_eligibility_applicable: scenario.checks.manuscript_one_read.require_proof_eligible,
        proof_eligible: scenario.checks.manuscript_one_read.require_proof_eligible
            && proof_eligibility_failures.is_empty()
            && failures.is_empty(),
        proof_eligibility_failures,
        failures,
    };
    if !keep_log && log.exists() {
        fs::remove_file(log)?;
    }
    Ok(report)
}

pub fn evaluate_runtime_scenario(
    scenario: &RuntimeScenarioFile,
    state: &crate::MemoryState,
    results: &[crate::RuntimeTurnResult],
) -> Vec<String> {
    evaluate_runtime_scenario_with_events(scenario, state, results, &[])
}

pub fn evaluate_runtime_scenario_with_events(
    scenario: &RuntimeScenarioFile,
    state: &crate::MemoryState,
    results: &[crate::RuntimeTurnResult],
    events: &[StoredEvent],
) -> Vec<String> {
    let memory_text = scenario_memory_text(state);
    let mut failures = Vec::new();
    let last_result = results.last();

    for needle in &scenario.checks.must_contain {
        if !contains_ci(&memory_text, needle) {
            failures.push(format!("missing required memory: {needle}"));
        }
    }
    for needle in &scenario.checks.must_not_contain {
        if contains_ci(&memory_text, needle) {
            failures.push(format!("forbidden memory present: {needle}"));
        }
    }

    for expected in &scenario.checks.claims.must_include {
        if !state
            .claims
            .iter()
            .any(|claim| claim_matches(claim, expected))
        {
            failures.push(format!(
                "missing required claim: {}",
                render_claim_check(expected)
            ));
        }
    }
    for forbidden in &scenario.checks.claims.must_exclude {
        if state
            .claims
            .iter()
            .any(|claim| claim_matches(claim, forbidden))
        {
            failures.push(format!(
                "forbidden claim present: {}",
                render_claim_check(forbidden)
            ));
        }
    }

    for expected in &scenario.checks.entities.must_include {
        if !state
            .entity_groups
            .iter()
            .any(|group| entity_matches(group, expected))
        {
            failures.push(format!("missing required entity: {}", expected.label));
        }
    }
    for forbidden in &scenario.checks.entities.must_exclude {
        if state
            .entity_groups
            .iter()
            .any(|group| entity_matches(group, forbidden))
        {
            failures.push(format!("forbidden entity present: {}", forbidden.label));
        }
    }

    for expected in &scenario.checks.relations.must_include {
        if !state
            .map
            .edges
            .iter()
            .any(|edge| relation_matches(state, edge, expected))
        {
            failures.push(format!(
                "missing required relation: {}",
                render_relation_check(expected)
            ));
        }
    }
    for forbidden in &scenario.checks.relations.must_exclude {
        if state
            .map
            .edges
            .iter()
            .any(|edge| relation_matches(state, edge, forbidden))
        {
            failures.push(format!(
                "forbidden relation present: {}",
                render_relation_check(forbidden)
            ));
        }
    }

    if !scenario
        .checks
        .working_memory
        .must_include_labels
        .is_empty()
        || !scenario
            .checks
            .working_memory
            .must_exclude_labels
            .is_empty()
        || scenario.checks.working_memory.max_nodes.is_some()
        || scenario.checks.working_memory.max_edges.is_some()
        || scenario.checks.recall.must_have_hit
        || scenario.checks.recall.must_have_reason
    {
        if let Some(result) = last_result {
            evaluate_working_memory(&scenario.checks.working_memory, result, &mut failures);
            if scenario.checks.recall.must_have_hit && result.recalled.hits.is_empty() {
                failures
                    .push("recall.must_have_hit expected at least one recalled hit".to_string());
            }
            if scenario.checks.recall.must_have_reason {
                if result.recalled.hits.is_empty() {
                    failures.push(
                        "recall.must_have_reason expected at least one recalled hit".to_string(),
                    );
                }
                for hit in &result.recalled.hits {
                    if !has_recorded_recall_reason(hit.reason.as_str()) {
                        failures.push(format!("recall hit {} has no reason", hit.episode_id));
                    }
                }
                if !result.recalled.hits.is_empty()
                    && result.working_memory.activation_reason.trim().is_empty()
                {
                    failures.push("working memory activation has no reason".to_string());
                }
            }
        } else {
            failures.push("scenario has no turn result for working-memory checks".to_string());
        }
    }

    if !scenario.checks.unknowns.must_include.is_empty() {
        if let Some(result) = last_result {
            for expected in &scenario.checks.unknowns.must_include {
                if !unknown_present(result, expected) {
                    failures.push(format!(
                        "missing required unknown/open question: {expected}"
                    ));
                }
            }
        } else {
            failures.push("scenario has no turn result for unknown checks".to_string());
        }
    }

    for expected in &scenario.checks.intake.must_include_actions {
        if !results
            .iter()
            .any(|result| result.intake.action == *expected)
        {
            failures.push(format!("missing required intake action: {expected:?}"));
        }
    }
    for (index, expected) in scenario.checks.intake.must_match_actions.iter().enumerate() {
        match results.get(index) {
            Some(result) if result.intake.action == *expected => {}
            Some(result) => failures.push(format!(
                "turn {} intake action was {:?}, expected {expected:?}",
                index + 1,
                result.intake.action
            )),
            None => failures.push(format!(
                "missing turn {} for expected intake action {expected:?}",
                index + 1
            )),
        }
    }
    if !scenario.checks.intake.must_match_actions.is_empty()
        && results.len() != scenario.checks.intake.must_match_actions.len()
    {
        failures.push(format!(
            "intake.must_match_actions covered {} turn(s), but scenario produced {} turn result(s)",
            scenario.checks.intake.must_match_actions.len(),
            results.len()
        ));
    }

    for check in &scenario.checks.answers {
        evaluate_answer_check(check, scenario, results, &mut failures);
    }
    evaluate_time_checks(&scenario.checks.time, scenario, &mut failures);
    evaluate_topology_bridge_checks(
        &scenario.checks.topology_bridge,
        state,
        results,
        events,
        &mut failures,
    );
    evaluate_runtime_replay_audit_checks(
        &scenario.checks.runtime_replay_audit,
        state,
        events,
        &mut failures,
    );
    failures.extend(
        evaluate_manuscript_one_read_protocol(
            &scenario.checks.manuscript_one_read,
            &scenario.turns,
            results,
        )
        .into_iter()
        .map(|failure| format!("manuscript_one_read: {failure}")),
    );

    failures
}

pub fn scenario_check_count(scenario: &RuntimeScenarioFile) -> usize {
    scenario.checks.must_contain.len()
        + scenario.checks.must_not_contain.len()
        + scenario.checks.claims.must_include.len()
        + scenario.checks.claims.must_exclude.len()
        + scenario.checks.entities.must_include.len()
        + scenario.checks.entities.must_exclude.len()
        + scenario.checks.relations.must_include.len()
        + scenario.checks.relations.must_exclude.len()
        + scenario
            .checks
            .working_memory
            .max_nodes
            .map(|_| 1)
            .unwrap_or(0)
        + scenario
            .checks
            .working_memory
            .max_edges
            .map(|_| 1)
            .unwrap_or(0)
        + scenario.checks.working_memory.must_include_labels.len()
        + scenario.checks.working_memory.must_exclude_labels.len()
        + scenario
            .checks
            .working_memory
            .activation_reason_must_contain
            .len()
        + usize::from(scenario.checks.recall.must_have_hit)
        + usize::from(scenario.checks.recall.must_have_reason)
        + scenario.checks.unknowns.must_include.len()
        + scenario.checks.intake.must_include_actions.len()
        + scenario.checks.intake.must_match_actions.len()
        + usize::from(scenario.checks.time.require_timestamps)
        + scenario.checks.time.min_gap_seconds.map(|_| 1).unwrap_or(0)
        + scenario
            .checks
            .time
            .min_any_gap_seconds
            .map(|_| 1)
            .unwrap_or(0)
        + scenario.checks.topology_bridge.nodes.len()
        + scenario.checks.topology_bridge.tethers.len()
        + usize::from(scenario.checks.topology_bridge.require_source_event_hashes)
        + usize::from(
            scenario
                .checks
                .topology_bridge
                .require_recall_reason_available,
        )
        + usize::from(
            scenario
                .checks
                .topology_bridge
                .forbid_root_orb_user_fact_leakage,
        )
        + usize::from(scenario.checks.topology_bridge.require_durable_commit)
        + scenario
            .checks
            .topology_bridge
            .min_committed_nodes
            .map(|_| 1)
            .unwrap_or(0)
        + scenario
            .checks
            .topology_bridge
            .min_committed_tethers
            .map(|_| 1)
            .unwrap_or(0)
        + usize::from(scenario.checks.runtime_replay_audit.require_clean)
        + usize::from(
            scenario
                .checks
                .runtime_replay_audit
                .require_source_event_hashes,
        )
        + scenario
            .checks
            .runtime_replay_audit
            .min_stored_events
            .map(|_| 1)
            .unwrap_or(0)
        + scenario
            .checks
            .runtime_replay_audit
            .min_topology_nodes
            .map(|_| 1)
            .unwrap_or(0)
        + scenario
            .checks
            .runtime_replay_audit
            .min_topology_tethers
            .map(|_| 1)
            .unwrap_or(0)
        + scenario
            .checks
            .runtime_replay_audit
            .min_topology_orbs
            .map(|_| 1)
            .unwrap_or(0)
        + usize::from(scenario.checks.manuscript_one_read.require_source_read)
        + usize::from(scenario.checks.manuscript_one_read.require_explicit_close)
        + scenario.checks.manuscript_one_read.retrieval_turns.len()
        + usize::from(
            scenario
                .checks
                .manuscript_one_read
                .forbid_source_after_close,
        )
        + usize::from(
            scenario
                .checks
                .manuscript_one_read
                .forbid_search_or_reread_after_close,
        )
        + usize::from(scenario.checks.manuscript_one_read.require_proof_eligible)
        + scenario
            .checks
            .answers
            .iter()
            .map(|check| {
                check.must_contain.len()
                    + check.must_not_contain.len()
                    + check.must_include_confidence.len()
                    + usize::from(check.must_have_recall_hit)
                    + usize::from(check.must_have_recall_reason)
                    + check.max_working_nodes.map(|_| 1).unwrap_or(0)
                    + check.max_working_edges.map(|_| 1).unwrap_or(0)
            })
            .sum::<usize>()
}

fn evaluate_topology_bridge_checks(
    checks: &TopologyBridgeChecks,
    state: &crate::MemoryState,
    results: &[crate::RuntimeTurnResult],
    events: &[StoredEvent],
    failures: &mut Vec<String>,
) {
    if checks == &TopologyBridgeChecks::default() {
        return;
    }
    let bridge = if events.is_empty() {
        crate::TopologyBridge::from_memory_state(state)
    } else {
        match bridge_runtime_events_to_topology(events) {
            Ok(bridge) => bridge,
            Err(err) => {
                failures.push(format!(
                    "topology bridge could not replay runtime events: {err}"
                ));
                return;
            }
        }
    };

    for expected in &checks.nodes {
        match bridge
            .node_records
            .iter()
            .find(|record| record.runtime_entity_ref == expected.runtime_entity_ref)
        {
            Some(record) => {
                if let Some(topology_ref) = &expected.topology_ref {
                    if &record.topology_ref != topology_ref {
                        failures.push(format!(
                            "topology node {} had ref {}, expected {topology_ref}",
                            expected.runtime_entity_ref, record.topology_ref
                        ));
                    }
                }
                if let Some(label) = &expected.label {
                    if &record.label != label {
                        failures.push(format!(
                            "topology node {} had label {}, expected {label}",
                            expected.runtime_entity_ref, record.label
                        ));
                    }
                }
                for assertion_key in &expected.claim_assertion_keys {
                    match record
                        .claim_refs
                        .iter()
                        .find(|claim| &claim.assertion_key == assertion_key)
                    {
                        Some(claim)
                            if expected.require_source_event_hash
                                && !claim_has_verified_source_hash(claim) =>
                        {
                            failures.push(format!(
                                "topology node {} claim {assertion_key} has no verified source event hash",
                                expected.runtime_entity_ref
                            ));
                        }
                        Some(_) => {}
                        None => {
                            failures.push(format!(
                                "topology node {} missing claim ref {assertion_key}",
                                expected.runtime_entity_ref
                            ));
                        }
                    }
                }
                if expected.require_source_event_hash && !record_has_source_hashes(record) {
                    failures.push(format!(
                        "topology node {} has no source event hash",
                        expected.runtime_entity_ref
                    ));
                }
            }
            None => failures.push(format!(
                "missing topology node ref for {}",
                expected.runtime_entity_ref
            )),
        }
    }

    for expected in &checks.tethers {
        match bridge.tether_records.iter().find(|record| {
            record.source_ref == expected.source_ref
                && record.target_ref == expected.target_ref
                && record.relation == expected.relation
        }) {
            Some(record) => {
                if let Some(topology_ref) = &expected.topology_ref {
                    if &record.topology_ref != topology_ref {
                        failures.push(format!(
                            "topology tether {}->{:?}->{} had ref {}, expected {topology_ref}",
                            expected.source_ref,
                            expected.relation,
                            expected.target_ref,
                            record.topology_ref
                        ));
                    }
                }
                if expected.require_source_event_hash && !tether_has_verified_source_hash(record) {
                    failures.push(format!(
                        "topology tether {}->{:?}->{} has no source event hash",
                        expected.source_ref, expected.relation, expected.target_ref
                    ));
                }
            }
            None => failures.push(format!(
                "missing topology tether {}->{:?}->{}",
                expected.source_ref, expected.relation, expected.target_ref
            )),
        }
    }

    if checks.require_source_event_hashes && !bridge_has_source_hashes(&bridge) {
        failures.push("topology bridge has no source event refs with hashes".to_string());
    }
    if checks.require_recall_reason_available && !last_result_has_recall_reason(results) {
        failures.push("topology bridge check expected a recorded recall reason".to_string());
    }
    if checks.forbid_root_orb_user_fact_leakage {
        evaluate_root_orb_user_fact_leakage(&bridge, state, failures);
    }
    evaluate_topology_durable_commit_checks(checks, events, failures);
}

fn evaluate_topology_durable_commit_checks(
    checks: &TopologyBridgeChecks,
    events: &[StoredEvent],
    failures: &mut Vec<String>,
) {
    if !checks.require_durable_commit
        && checks.min_committed_nodes.is_none()
        && checks.min_committed_tethers.is_none()
    {
        return;
    }
    let ledger_commit = match commit_runtime_events_to_topology_ledger(events) {
        Ok(commit) => commit,
        Err(err) => {
            failures.push(format!(
                "topology bridge durable ledger commit failed: {err}"
            ));
            return;
        }
    };
    if checks.require_durable_commit {
        if let Ok(report) = crate::audit_runtime_events(events) {
            if !report.is_clean() {
                failures.push(format!(
                    "topology bridge durable commit replay audit quarantined persisted commits: {:?}",
                    report.replay_error
                ));
            }
        }
        for expected in &checks.nodes {
            let node_id = expected
                .topology_ref
                .clone()
                .unwrap_or_else(|| topology_node_ref_for_runtime_ref(&expected.runtime_entity_ref));
            if ledger_commit.topology.nodes().get(&node_id).is_none() {
                failures.push(format!("topology ledger missing committed node {node_id}"));
            }
        }
        for expected in &checks.tethers {
            let tether = ledger_commit.bridge.tether_records.iter().find(|record| {
                record.source_ref == expected.source_ref
                    && record.target_ref == expected.target_ref
                    && record.relation == expected.relation
            });
            match tether {
                Some(tether)
                    if ledger_commit
                        .topology
                        .tethers()
                        .get(&tether.topology_ref)
                        .is_none() =>
                {
                    failures.push(format!(
                        "topology ledger missing committed tether {}",
                        tether.topology_ref
                    ));
                }
                Some(_) => {}
                None => failures.push(format!(
                    "topology ledger could not map expected tether {}->{:?}->{}",
                    expected.source_ref, expected.relation, expected.target_ref
                )),
            }
        }
        match ReplayAuditor::audit_ledger(&ledger_commit.topology) {
            Ok(report) if report.is_clean() => {}
            Ok(report) => failures.push(format!(
                "topology ledger replay audit quarantined runtime commit: live={}, replayed={}, diffs={:?}, error={:?}",
                report.live_snapshot_hash,
                report.replayed_snapshot_hash,
                report.count_diffs,
                report.replay_error
            )),
            Err(err) => failures.push(format!("topology ledger replay audit failed: {err}")),
        }
    }
    if let Some(min_nodes) = checks.min_committed_nodes {
        if ledger_commit.committed_node_ids.len() < min_nodes {
            failures.push(format!(
                "topology ledger committed {} node(s), expected at least {min_nodes}",
                ledger_commit.committed_node_ids.len()
            ));
        }
    }
    if let Some(min_tethers) = checks.min_committed_tethers {
        if ledger_commit.committed_tether_ids.len() < min_tethers {
            failures.push(format!(
                "topology ledger committed {} tether(s), expected at least {min_tethers}",
                ledger_commit.committed_tether_ids.len()
            ));
        }
    }

    let commits = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match &event.payload {
            LunaEvent::TopologyBridgeCommitted(commit) => Some((index, commit)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some((last_commit_index, last_commit)) = commits.last() else {
        failures.push("topology bridge expected a durable commit event".to_string());
        return;
    };
    let expected_commit =
        match commit_runtime_events_to_topology_ledger(&events[..*last_commit_index])
            .and_then(|commit| topology_commit_from_runtime_ledger_commit(&commit))
        {
            Ok(commit) => commit,
            Err(err) => {
                failures.push(format!(
                    "topology bridge durable commit event could not be recomputed: {err}"
                ));
                return;
            }
        };
    if last_commit.node_refs != expected_commit.node_refs
        || last_commit.tether_refs != expected_commit.tether_refs
        || last_commit.source_event_hashes != expected_commit.source_event_hashes
        || last_commit.orb_refs != expected_commit.orb_refs
        || last_commit.accepted_orb_refs != expected_commit.accepted_orb_refs
        || last_commit.rejected_orb_refs != expected_commit.rejected_orb_refs
        || last_commit.ledger_event_count != expected_commit.ledger_event_count
        || last_commit.ledger_event_hash != expected_commit.ledger_event_hash
    {
        failures.push(
            "topology bridge durable commit event does not match recomputed ledger projection"
                .to_string(),
        );
    }
    if checks.require_durable_commit && last_commit.source_event_hashes.is_empty() {
        failures.push("topology bridge durable commit has no source event hashes".to_string());
    }
    if checks.require_durable_commit && last_commit.ledger_events_json.is_empty() {
        failures.push("topology bridge durable commit has no ledger events".to_string());
    }
    if checks.require_durable_commit
        && last_commit.ledger_event_hash != ledger_events_hash(&last_commit.ledger_events_json)
    {
        failures.push("topology bridge durable commit ledger hash is invalid".to_string());
    }
    if let Some(min_nodes) = checks.min_committed_nodes {
        if last_commit.node_refs.len() < min_nodes {
            failures.push(format!(
                "topology bridge durable commit had {} node(s), expected at least {min_nodes}",
                last_commit.node_refs.len()
            ));
        }
    }
    if let Some(min_tethers) = checks.min_committed_tethers {
        if last_commit.tether_refs.len() < min_tethers {
            failures.push(format!(
                "topology bridge durable commit had {} tether(s), expected at least {min_tethers}",
                last_commit.tether_refs.len()
            ));
        }
    }
}

fn evaluate_runtime_replay_audit_checks(
    checks: &RuntimeReplayAuditChecks,
    state: &crate::MemoryState,
    events: &[StoredEvent],
    failures: &mut Vec<String>,
) {
    if checks == &RuntimeReplayAuditChecks::default() {
        return;
    }
    let report = match crate::audit_runtime_events_against_state(state, events) {
        Ok(report) => report,
        Err(err) => {
            failures.push(format!("runtime replay audit could not run: {err}"));
            return;
        }
    };

    if checks.require_clean && !report.is_clean() {
        failures.push(format!(
            "runtime replay audit quarantined persisted log: {:?}",
            report.replay_error
        ));
    }
    if let Some(min_events) = checks.min_stored_events {
        if report.replayed_counts.stored_events < min_events {
            failures.push(format!(
                "runtime replay audit saw {} stored event(s), expected at least {min_events}",
                report.replayed_counts.stored_events
            ));
        }
    }
    if let Some(min_nodes) = checks.min_topology_nodes {
        if report.replayed_counts.topology_nodes < min_nodes {
            failures.push(format!(
                "runtime replay audit saw {} topology node(s), expected at least {min_nodes}",
                report.replayed_counts.topology_nodes
            ));
        }
    }
    if let Some(min_tethers) = checks.min_topology_tethers {
        if report.replayed_counts.topology_tethers < min_tethers {
            failures.push(format!(
                "runtime replay audit saw {} topology tether(s), expected at least {min_tethers}",
                report.replayed_counts.topology_tethers
            ));
        }
    }
    if checks.require_source_event_hashes && report.replayed_counts.topology_source_event_refs == 0
    {
        failures.push(
            "runtime replay audit found no topology source event refs with hashes".to_string(),
        );
    }
    if checks.require_source_event_hashes
        && report.replayed_counts.valid_topology_source_event_refs
            < report.replayed_counts.topology_source_event_refs
    {
        failures.push(format!(
            "runtime replay audit found only {} valid topology source event hash(es) out of {} source event ref(s)",
            report.replayed_counts.valid_topology_source_event_refs,
            report.replayed_counts.topology_source_event_refs
        ));
    }
    if let Some(min_orbs) = checks.min_topology_orbs {
        if report.replayed_counts.topology_orbs < min_orbs {
            failures.push(format!(
                "runtime replay audit saw {} topology orb(s), expected at least {min_orbs}",
                report.replayed_counts.topology_orbs
            ));
        }
    }
}

fn evaluate_manuscript_one_read_protocol(
    checks: &ManuscriptOneReadChecks,
    turns: &[RuntimeScenarioTurn],
    results: &[crate::RuntimeTurnResult],
) -> Vec<String> {
    if checks == &ManuscriptOneReadChecks::default() {
        return Vec::new();
    }

    let mut failures = Vec::new();
    let close_turn = turns
        .iter()
        .position(|turn| contains_manuscript_close(turn.content()))
        .map(|index| index + 1);

    if checks.require_explicit_close && close_turn.is_none() {
        failures.push("missing explicit manuscript close turn".to_string());
    }

    if checks.require_source_read {
        let source_read_seen = turns.iter().enumerate().any(|(index, turn)| {
            let turn_index = index + 1;
            close_turn.map(|close| turn_index <= close).unwrap_or(true)
                && contains_manuscript_source_marker(turn.content())
        });
        if !source_read_seen {
            failures.push("missing source read before the manuscript close marker".to_string());
        }
    }

    for turn_index in &checks.retrieval_turns {
        if *turn_index == 0 || *turn_index > turns.len() {
            failures.push(format!("retrieval turn {turn_index} does not exist"));
            continue;
        }
        if let Some(close) = close_turn {
            if *turn_index <= close {
                failures.push(format!(
                    "retrieval turn {turn_index} must occur after close turn {close}"
                ));
            }
        }
        let content = turns[*turn_index - 1].content();
        if contains_manuscript_source_marker(content) {
            failures.push(format!(
                "retrieval turn {turn_index} contains a manuscript source marker"
            ));
        }
        if contains_manuscript_search_or_reread_request(content) {
            failures.push(format!(
                "retrieval turn {turn_index} requests source search or reread"
            ));
        }
        match results.get(*turn_index - 1) {
            Some(result) => {
                if result
                    .observation
                    .assertions
                    .iter()
                    .any(|assertion| assertion.domain == "manuscript")
                {
                    failures.push(format!(
                        "retrieval turn {turn_index} produced new manuscript assertions"
                    ));
                }
                if result.recalled.hits.is_empty() {
                    failures.push(format!(
                        "retrieval turn {turn_index} did not recall prior manuscript memory"
                    ));
                }
            }
            None => failures.push(format!("retrieval turn {turn_index} has no runtime result")),
        }
    }

    if let Some(close) = close_turn {
        for (index, turn) in turns.iter().enumerate().skip(close) {
            let turn_index = index + 1;
            let content = turn.content();
            if checks.forbid_source_after_close && contains_manuscript_source_marker(content) {
                failures.push(format!(
                    "turn {turn_index} contains manuscript source text after close turn {close}"
                ));
            }
            if checks.forbid_search_or_reread_after_close
                && contains_manuscript_search_or_reread_request(content)
            {
                failures.push(format!(
                    "turn {turn_index} requests source search/reread after close turn {close}"
                ));
            }
        }
    }

    failures
}

fn contains_manuscript_source_marker(text: &str) -> bool {
    contains_ci(text, "MANUSCRIPT:")
}

fn contains_manuscript_close(text: &str) -> bool {
    contains_ci(text, "manuscript is closed") || contains_ci(text, "the manuscript is closed")
}

fn contains_manuscript_search_or_reread_request(text: &str) -> bool {
    [
        "reread",
        "re-read",
        "search the manuscript",
        "search source",
        "search the source",
        "look up in the manuscript",
        "open the manuscript",
        "read the source again",
    ]
    .iter()
    .any(|needle| contains_ci(text, needle))
}

fn record_has_source_hashes(record: &crate::TopologyNodeRecord) -> bool {
    record
        .source_event_refs
        .iter()
        .any(valid_topology_source_ref)
        || record.claim_refs.iter().any(claim_has_verified_source_hash)
}

fn claim_has_verified_source_hash(claim: &crate::TopologyClaimRef) -> bool {
    claim.source_event_refs.iter().any(|source| {
        valid_topology_source_ref(source)
            && source.assertion_key.as_deref() == Some(claim.assertion_key.as_str())
    })
}

fn tether_has_verified_source_hash(record: &crate::TopologyTetherRecord) -> bool {
    let provenance_keys = record
        .provenance
        .iter()
        .filter_map(|provenance| provenance.assertion_key.as_deref())
        .collect::<Vec<_>>();
    !provenance_keys.is_empty()
        && record.source_event_refs.iter().any(|source| {
            valid_topology_source_ref(source)
                && source
                    .assertion_key
                    .as_deref()
                    .is_some_and(|key| provenance_keys.contains(&key))
        })
}

fn valid_topology_source_ref(source: &crate::TopologySourceEventRef) -> bool {
    source.event_hash.len() == 64
        && source
            .event_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn bridge_has_source_hashes(bridge: &crate::TopologyBridge) -> bool {
    bridge.node_records.iter().any(record_has_source_hashes)
        || bridge
            .tether_records
            .iter()
            .any(tether_has_verified_source_hash)
}

fn last_result_has_recall_reason(results: &[crate::RuntimeTurnResult]) -> bool {
    results.last().is_some_and(|result| {
        result
            .recalled
            .hits
            .iter()
            .any(|hit| has_recorded_recall_reason(hit.reason.as_str()))
    })
}

fn evaluate_root_orb_user_fact_leakage(
    bridge: &crate::TopologyBridge,
    state: &crate::MemoryState,
    failures: &mut Vec<String>,
) {
    let user_fact_needles = state
        .claims
        .iter()
        .filter(|claim| claim.lifecycle_status == AssertionLifecycleStatus::Current)
        .flat_map(|claim| [claim.key.as_str(), claim.value.as_str()])
        .filter(|needle| !needle.trim().is_empty())
        .collect::<Vec<_>>();
    let user_fact_tokens = state
        .claims
        .iter()
        .filter(|claim| claim.lifecycle_status == AssertionLifecycleStatus::Current)
        .flat_map(|claim| normalized_terms(&format!("{} {}", claim.key, claim.value)))
        .filter(|token| token.len() > 2 && !root_leakage_stopword(token))
        .collect::<Vec<_>>();

    for orb_ref in &bridge.orb_refs {
        for needle in &user_fact_needles {
            if contains_ci(&orb_ref.system_root, needle) || contains_ci(&orb_ref.source_ref, needle)
            {
                failures.push(format!(
                    "SystemKernel topology ref leaked user fact into {} / {}",
                    orb_ref.system_root, orb_ref.source_ref
                ));
            }
        }
        for token in &user_fact_tokens {
            if contains_ci(&orb_ref.system_root, token) || contains_ci(&orb_ref.source_ref, token) {
                failures.push(format!(
                    "SystemKernel topology ref leaked user fact token into {} / {}",
                    orb_ref.system_root, orb_ref.source_ref
                ));
            }
        }
    }
    for node in &bridge.node_records {
        if node
            .provenance
            .iter()
            .any(|provenance| provenance.system_root.is_some())
            && (node.kind == "self"
                || node.kind == "person"
                || node.kind == "project"
                || node.kind == "character")
        {
            failures.push(format!(
                "SystemKernel provenance attached to user memory node {}",
                node.topology_ref
            ));
        }
    }
    for tether in &bridge.tether_records {
        if tether.source_ref.starts_with("root:") || tether.target_ref.starts_with("root:") {
            failures.push(format!(
                "SystemKernel topology tether leaked into runtime bridge: {}",
                tether.topology_ref
            ));
        }
    }
}

fn root_leakage_stopword(token: &str) -> bool {
    matches!(
        token,
        "identity"
            | "provenance"
            | "project"
            | "person"
            | "location"
            | "source"
            | "event"
            | "memory"
            | "engine"
            | "luna"
            | "root"
    )
}

fn evaluate_answer_check(
    check: &AnswerCheck,
    scenario: &RuntimeScenarioFile,
    results: &[crate::RuntimeTurnResult],
    failures: &mut Vec<String>,
) {
    if check.turn_index == 0 || check.turn_index > results.len() {
        failures.push(format!(
            "answer check references missing turn {}",
            check.turn_index
        ));
        return;
    }
    let turn = scenario.turns[check.turn_index - 1].content();
    let result = &results[check.turn_index - 1];
    let reply = render_conversation_reply(turn, result);
    let plan = plan_conversation_response(turn, result);
    let markdown = crate::render_runtime_markdown(result);
    if check.must_have_recall_hit && result.recalled.hits.is_empty() {
        failures.push(format!(
            "answer turn {} has no recall hit",
            check.turn_index
        ));
    }
    if let Some(max_nodes) = check.max_working_nodes {
        if result.working_memory.nodes.len() > max_nodes {
            failures.push(format!(
                "answer turn {} working memory has {} node(s), expected at most {max_nodes}",
                check.turn_index,
                result.working_memory.nodes.len()
            ));
        }
    }
    if let Some(max_edges) = check.max_working_edges {
        if result.working_memory.edges.len() > max_edges {
            failures.push(format!(
                "answer turn {} working memory has {} edge(s), expected at most {max_edges}",
                check.turn_index,
                result.working_memory.edges.len()
            ));
        }
    }
    if !plan.actions.contains(&ResponsePlanAction::Answer) {
        failures.push(format!(
            "answer turn {} did not choose an answer action",
            check.turn_index
        ));
    }
    for needle in &check.must_contain {
        if !contains_ci(&reply, needle)
            && !plan
                .answer_values
                .iter()
                .any(|value| contains_ci(value, needle))
        {
            failures.push(format!(
                "answer turn {} missing required text: {needle}",
                check.turn_index
            ));
        }
        let matching_evidence = plan
            .answer_evidence
            .iter()
            .filter(|evidence| contains_ci(&evidence.value, needle))
            .collect::<Vec<_>>();
        if !check.must_include_confidence.is_empty()
            && !matching_evidence.iter().any(|evidence| {
                check
                    .must_include_confidence
                    .contains(&evidence.confidence_tier)
            })
        {
            failures.push(format!(
                "answer turn {} required text lacks matching confidence evidence: {needle}",
                check.turn_index
            ));
        }
        if check.must_have_recall_reason
            && !matching_evidence.iter().any(|evidence| {
                evidence
                    .recall_reason
                    .as_deref()
                    .is_some_and(has_recorded_recall_reason)
            })
        {
            failures.push(format!(
                "answer turn {} required text lacks matching recall reason: {needle}",
                check.turn_index
            ));
        }
    }
    for needle in &check.must_not_contain {
        if contains_ci(&reply, needle)
            || plan
                .answer_values
                .iter()
                .any(|value| contains_ci(value, needle))
            || contains_ci(&markdown, needle)
            || contains_ci(&result.context_packet.summary, needle)
            || result
                .context_packet
                .recalled_claims
                .iter()
                .any(|claim| contains_ci(&claim.value, needle))
            || result
                .context_packet
                .working_memory
                .nodes
                .iter()
                .any(|node| contains_ci(&node.label, needle))
        {
            failures.push(format!(
                "answer turn {} contains forbidden text: {needle}",
                check.turn_index
            ));
        }
    }
    for confidence in &check.must_include_confidence {
        if check.must_contain.is_empty()
            && !plan
                .answer_evidence
                .iter()
                .any(|evidence| evidence.confidence_tier == *confidence)
        {
            failures.push(format!(
                "answer turn {} missing confidence tier {confidence:?}",
                check.turn_index
            ));
        }
    }
    if check.must_have_recall_reason
        && check.must_contain.is_empty()
        && !plan.answer_evidence.iter().any(|evidence| {
            evidence
                .recall_reason
                .as_deref()
                .is_some_and(has_recorded_recall_reason)
        })
    {
        failures.push(format!(
            "answer turn {} has no answer evidence recall reason",
            check.turn_index
        ));
    }
}

fn evaluate_time_checks(
    checks: &TimeChecks,
    scenario: &RuntimeScenarioFile,
    failures: &mut Vec<String>,
) {
    if checks.require_timestamps {
        for (index, turn) in scenario.turns.iter().enumerate() {
            if turn.timestamp().is_none() {
                failures.push(format!(
                    "time.require_timestamps missing timestamp on turn {}",
                    index + 1
                ));
            }
        }
    }

    for (index, pair) in scenario.turns.windows(2).enumerate() {
        if let (Some(previous), Some(next)) = (pair[0].timestamp(), pair[1].timestamp()) {
            if next < previous {
                failures.push(format!(
                    "time timestamps move backward from turn {} to {}",
                    index + 1,
                    index + 2
                ));
            }
        }
    }

    if let Some(min_gap_seconds) = checks.min_gap_seconds {
        for (index, pair) in scenario.turns.windows(2).enumerate() {
            match (pair[0].timestamp(), pair[1].timestamp()) {
                (Some(previous), Some(next)) => {
                    let gap_seconds = next.signed_duration_since(previous).num_seconds();
                    if gap_seconds < min_gap_seconds {
                        failures.push(format!(
                            "time.min_gap_seconds turn {} -> {} was {gap_seconds}s, expected at least {min_gap_seconds}s",
                            index + 1,
                            index + 2
                        ));
                    }
                }
                _ => failures.push(format!(
                    "time.min_gap_seconds requires timestamps on turns {} and {}",
                    index + 1,
                    index + 2
                )),
            }
        }
    }

    if let Some(min_any_gap_seconds) = checks.min_any_gap_seconds {
        let max_gap = scenario
            .turns
            .windows(2)
            .filter_map(|pair| match (pair[0].timestamp(), pair[1].timestamp()) {
                (Some(previous), Some(next)) => {
                    Some(next.signed_duration_since(previous).num_seconds())
                }
                _ => None,
            })
            .max();
        match max_gap {
            Some(gap) if gap >= min_any_gap_seconds => {}
            Some(gap) => failures.push(format!(
                "time.min_any_gap_seconds largest gap was {gap}s, expected at least {min_any_gap_seconds}s"
            )),
            None => failures.push(
                "time.min_any_gap_seconds requires at least two timestamped turns".to_string(),
            ),
        }
    }
}

fn scenario_memory_text(state: &crate::MemoryState) -> String {
    state
        .claims
        .iter()
        .filter(|claim| claim.lifecycle_status == AssertionLifecycleStatus::Current)
        .map(|claim| format!("{}:{}={}", claim.domain, claim.kind, claim.value))
        .collect::<Vec<_>>()
        .join("\n")
}

fn evaluate_working_memory(
    checks: &WorkingMemoryChecks,
    result: &crate::RuntimeTurnResult,
    failures: &mut Vec<String>,
) {
    if let Some(max_nodes) = checks.max_nodes {
        if result.working_memory.nodes.len() > max_nodes {
            failures.push(format!(
                "working memory has {} node(s), expected at most {max_nodes}",
                result.working_memory.nodes.len()
            ));
        }
    }
    if let Some(max_edges) = checks.max_edges {
        if result.working_memory.edges.len() > max_edges {
            failures.push(format!(
                "working memory has {} edge(s), expected at most {max_edges}",
                result.working_memory.edges.len()
            ));
        }
    }
    for label in &checks.must_include_labels {
        if !result
            .working_memory
            .nodes
            .iter()
            .any(|node| contains_ci(&node.label, label))
        {
            failures.push(format!("working memory missing label: {label}"));
        }
    }
    for label in &checks.must_exclude_labels {
        if result
            .working_memory
            .nodes
            .iter()
            .any(|node| contains_ci(&node.label, label))
        {
            failures.push(format!("working memory contains forbidden label: {label}"));
        }
    }
    for needle in &checks.activation_reason_must_contain {
        if !contains_ci(&result.working_memory.activation_reason, needle) {
            failures.push(format!(
                "working memory activation reason missing: {needle}"
            ));
        }
    }
}

fn unknown_present(result: &crate::RuntimeTurnResult, expected: &str) -> bool {
    result
        .knowledge_delta
        .unknowns
        .iter()
        .any(|unknown| contains_ci(unknown, expected))
        || result
            .questions
            .iter()
            .any(|question| contains_ci(&question.question, expected))
        || result.context_packet.open_questions.iter().any(|question| {
            contains_ci(&question.question, expected) || contains_ci(&question.reason, expected)
        })
}

fn claim_matches(claim: &crate::MemoryClaim, expected: &ClaimCheck) -> bool {
    claim.domain == expected.domain
        && claim.kind == expected.kind
        && claim.value == expected.value
        && expected
            .confidence_tier
            .map(|tier| claim.status == tier)
            .unwrap_or(true)
        && expected
            .lifecycle_status
            .map(|status| claim.lifecycle_status == status)
            .unwrap_or(true)
}

fn entity_matches(group: &crate::EntityMemoryGroup, expected: &EntityCheck) -> bool {
    expected
        .id
        .as_ref()
        .map(|id| &group.id == id)
        .unwrap_or(true)
        && group.label == expected.label
        && expected
            .kind
            .as_ref()
            .map(|kind| &group.kind == kind)
            .unwrap_or(true)
        && expected.claims.iter().all(|claim_check| {
            group
                .claims
                .iter()
                .any(|claim| claim_matches(claim, claim_check))
        })
}

fn relation_matches(
    state: &crate::MemoryState,
    edge: &luna_core::MemoryEdge,
    expected: &RelationCheck,
) -> bool {
    edge.relation == expected.relation
        && expected
            .source
            .as_ref()
            .map(|id| &edge.source == id)
            .unwrap_or(true)
        && expected
            .target
            .as_ref()
            .map(|id| &edge.target == id)
            .unwrap_or(true)
        && expected
            .source_label
            .as_ref()
            .map(|label| node_label_matches(state, &edge.source, label))
            .unwrap_or(true)
        && expected
            .target_label
            .as_ref()
            .map(|label| node_label_matches(state, &edge.target, label))
            .unwrap_or(true)
}

fn node_label_matches(state: &crate::MemoryState, node_id: &str, label: &str) -> bool {
    state
        .map
        .nodes
        .iter()
        .any(|node| node.id == node_id && node.label == label)
}

fn render_claim_check(check: &ClaimCheck) -> String {
    format!("{}:{}={}", check.domain, check.kind, check.value)
}

fn render_relation_check(check: &RelationCheck) -> String {
    format!(
        "{} -{:?}-> {}",
        check
            .source
            .as_deref()
            .or(check.source_label.as_deref())
            .unwrap_or("*"),
        check.relation,
        check
            .target
            .as_deref()
            .or(check.target_label.as_deref())
            .unwrap_or("*")
    )
}

fn contains_ci(text: &str, needle: &str) -> bool {
    text.to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn normalized_terms(value: &str) -> Vec<String> {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn has_recorded_recall_reason(reason: &str) -> bool {
    let trimmed = reason.trim();
    !trimmed.is_empty() && trimmed != "<unrecorded>"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityMemoryGroup, MemoryClaim, MemoryState};
    use luna_core::{
        MemoryMap, MemoryNode, MemoryNodeKind, MemoryProvenance, RecallHit, RecallReason,
        RecallSet, StructuredAssertion, WorkingMemory,
    };
    use luna_extract::FusedExtractor;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn typed_claim_check_requires_confidence_when_supplied() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["hello".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                claims: ClaimChecks {
                    must_include: vec![ClaimCheck {
                        domain: "identity".to_string(),
                        kind: "profession".to_string(),
                        value: "software developer".to_string(),
                        confidence_tier: Some(AssertionConfidenceTier::Confirmed),
                        lifecycle_status: None,
                    }],
                    must_exclude: Vec::new(),
                },
                ..RuntimeScenarioChecks::default()
            },
        };
        let state = MemoryState {
            claims: vec![MemoryClaim::from_assertion(&StructuredAssertion::inferred(
                "identity",
                "profession",
                "software developer",
            ))],
            entity_groups: Vec::new(),
            open_questions: Vec::new(),
            map: MemoryMap::default(),
        };

        let failures = evaluate_runtime_scenario(&scenario, &state, &[]);

        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("missing required claim"));
    }

    #[test]
    fn typed_entity_check_can_require_nested_claims() {
        let claim = MemoryClaim::from_assertion(
            &StructuredAssertion::new("person", "location", "Chris lives in Iowa")
                .with_source_count(2),
        );
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["hello".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                entities: EntityChecks {
                    must_include: vec![EntityCheck {
                        id: Some("person:Chris".to_string()),
                        label: "Chris".to_string(),
                        kind: Some("person".to_string()),
                        claims: vec![ClaimCheck {
                            domain: "person".to_string(),
                            kind: "location".to_string(),
                            value: "Chris lives in Iowa".to_string(),
                            confidence_tier: Some(AssertionConfidenceTier::Confirmed),
                            lifecycle_status: None,
                        }],
                    }],
                    must_exclude: Vec::new(),
                },
                ..RuntimeScenarioChecks::default()
            },
        };
        let state = MemoryState {
            claims: vec![claim.clone()],
            entity_groups: vec![EntityMemoryGroup {
                id: "person:Chris".to_string(),
                label: "Chris".to_string(),
                kind: "person".to_string(),
                claims: vec![claim],
            }],
            open_questions: Vec::new(),
            map: MemoryMap::default(),
        };

        let failures = evaluate_runtime_scenario(&scenario, &state, &[]);

        assert!(failures.is_empty(), "{failures:?}");
    }

    #[test]
    fn recall_reason_check_requires_at_least_one_hit() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["What do you remember?".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                recall: RecallChecks {
                    must_have_hit: true,
                    must_have_reason: true,
                },
                ..RuntimeScenarioChecks::default()
            },
        };
        let state = MemoryState::default();
        let result = crate::RuntimeTurnResult {
            turn_id: uuid::Uuid::new_v4(),
            observation: test_observation(),
            knowledge_delta: crate::KnowledgeDelta::default(),
            memory_state: MemoryState::default(),
            working_memory: luna_core::WorkingMemory::default(),
            recalled: luna_core::RecallSet::default(),
            recall_mode: luna_core::RecallMode::OpenEnded,
            questions: Vec::new(),
            context_packet: crate::ContextPacket {
                recall_mode: luna_core::RecallMode::OpenEnded,
                recalled_claims: Vec::new(),
                working_memory: luna_core::WorkingMemory::default(),
                compressed_memory: Vec::new(),
                open_questions: Vec::new(),
                summary: String::new(),
            },
            intake: crate::MemoryIntakeDecision {
                action: crate::MemoryIntakeAction::IgnoreNoise,
                reason: "test fixture".to_string(),
            },
            output_packet: luna_output::OutputPacket {
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
        };

        let failures = evaluate_runtime_scenario(&scenario, &state, &[result]);

        assert_eq!(failures.len(), 2);
        assert!(failures[0].contains("must_have_hit"));
        assert!(failures[1].contains("must_have_reason"));
    }

    #[test]
    fn runtime_scenario_rejects_zero_executable_checks() {
        let root = std::env::temp_dir().join(format!("luna_zero_check_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let scenario_path = root.join("zero.json");
        let log_path = root.join("zero.jsonl");
        fs::write(
            &scenario_path,
            r#"{"name":"zero","turns":["hello"],"checks":{"answers":[]}}"#,
        )
        .unwrap();

        let report =
            run_runtime_scenario(&scenario_path, &log_path, FusedExtractor::new(), false).unwrap();

        assert_eq!(report.check_count, 0);
        assert!(!report.is_pass());
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("zero executable memory checks")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recall_reason_check_rejects_legacy_unrecorded_sentinel() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["What do you remember?".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                recall: RecallChecks {
                    must_have_hit: true,
                    must_have_reason: true,
                },
                ..RuntimeScenarioChecks::default()
            },
        };
        let mut result = test_result(crate::MemoryIntakeAction::MarkUnknown);
        result.recalled = RecallSet {
            hits: vec![RecallHit {
                episode_id: uuid::Uuid::new_v4(),
                score: 0.9,
                assertions: Vec::new(),
                reason: serde_json::from_str("\"\"").unwrap(),
            }],
            latency_ms: 0.0,
        };

        let failures = evaluate_runtime_scenario(&scenario, &MemoryState::default(), &[result]);

        assert!(failures
            .iter()
            .any(|failure| failure.contains("has no reason")));
    }

    #[test]
    fn answer_checks_tie_evidence_to_required_text() {
        let mut confirmed =
            StructuredAssertion::new("project", "identity", "MKPE is my provenance engine")
                .with_source_count(2);
        let unconfirmed =
            StructuredAssertion::new("project", "identity", "Atlas Loom is my planning engine");
        confirmed.lifecycle_status = AssertionLifecycleStatus::Current;

        let mut result = test_result(crate::MemoryIntakeAction::MarkUnknown);
        result.memory_state.claims = vec![
            MemoryClaim::from_assertion(&confirmed),
            MemoryClaim::from_assertion(&unconfirmed),
        ];
        result.working_memory = WorkingMemory {
            nodes: vec![
                evidence_node("MKPE is my provenance engine", &confirmed),
                evidence_node("Atlas Loom is my planning engine", &unconfirmed),
            ],
            ..WorkingMemory::default()
        };
        result.recalled = RecallSet {
            hits: vec![RecallHit {
                episode_id: uuid::Uuid::new_v4(),
                score: 0.9,
                assertions: vec![confirmed],
                reason: RecallReason::new("cue_overlap_activation").unwrap(),
            }],
            latency_ms: 0.0,
        };

        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["What do you know?".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                answers: vec![AnswerCheck {
                    turn_index: 1,
                    must_contain: vec!["Atlas Loom is my planning engine".to_string()],
                    must_not_contain: Vec::new(),
                    must_include_confidence: vec![AssertionConfidenceTier::Confirmed],
                    must_have_recall_hit: true,
                    must_have_recall_reason: true,
                    max_working_nodes: Some(5),
                    max_working_edges: Some(10),
                }],
                ..RuntimeScenarioChecks::default()
            },
        };

        let failures = evaluate_runtime_scenario(&scenario, &MemoryState::default(), &[result]);

        assert!(failures
            .iter()
            .any(|failure| failure.contains("required text lacks matching confidence evidence")));
        assert!(failures
            .iter()
            .any(|failure| failure.contains("required text lacks matching recall reason")));
    }

    #[test]
    fn answer_check_without_required_text_rejects_unrecorded_recall_reason() {
        let mut assertion = StructuredAssertion::new("project", "identity", "Atlas Loom is active");
        assertion.lifecycle_status = AssertionLifecycleStatus::Current;
        let mut result = test_result(crate::MemoryIntakeAction::MarkUnknown);
        result.memory_state.claims = vec![MemoryClaim::from_assertion(&assertion)];
        result.working_memory = WorkingMemory {
            nodes: vec![evidence_node("Atlas Loom is active", &assertion)],
            ..WorkingMemory::default()
        };
        result.recalled = RecallSet {
            hits: vec![RecallHit {
                episode_id: uuid::Uuid::new_v4(),
                score: 0.9,
                assertions: vec![assertion],
                reason: serde_json::from_str("\"\"").unwrap(),
            }],
            latency_ms: 0.0,
        };
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["What do you know?".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                answers: vec![AnswerCheck {
                    turn_index: 1,
                    must_have_recall_hit: true,
                    must_have_recall_reason: true,
                    ..AnswerCheck::default()
                }],
                ..RuntimeScenarioChecks::default()
            },
        };

        let failures = evaluate_runtime_scenario(&scenario, &MemoryState::default(), &[result]);

        assert!(failures
            .iter()
            .any(|failure| failure.contains("has no answer evidence recall reason")));
    }

    #[test]
    fn unknown_checks_read_last_turn_result_questions() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["I hate her".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                unknowns: UnknownChecks {
                    must_include: vec!["who".to_string()],
                },
                ..RuntimeScenarioChecks::default()
            },
        };
        let mut result = crate::RuntimeTurnResult {
            turn_id: uuid::Uuid::new_v4(),
            observation: test_observation(),
            knowledge_delta: crate::KnowledgeDelta::default(),
            memory_state: MemoryState::default(),
            working_memory: luna_core::WorkingMemory::default(),
            recalled: luna_core::RecallSet::default(),
            recall_mode: luna_core::RecallMode::OpenEnded,
            questions: vec![crate::QuestionCandidate {
                question: "Who are you referring to?".to_string(),
                reason: "ambiguous pronoun".to_string(),
                priority: 10,
            }],
            context_packet: crate::ContextPacket {
                recall_mode: luna_core::RecallMode::OpenEnded,
                recalled_claims: Vec::new(),
                working_memory: luna_core::WorkingMemory::default(),
                compressed_memory: Vec::new(),
                open_questions: Vec::new(),
                summary: String::new(),
            },
            intake: crate::MemoryIntakeDecision {
                action: crate::MemoryIntakeAction::AskForAnchor,
                reason: "test fixture".to_string(),
            },
            output_packet: luna_output::OutputPacket {
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
        };
        result
            .memory_state
            .open_questions
            .push("stale inspect field".to_string());
        let state = MemoryState::default();

        let failures = evaluate_runtime_scenario(&scenario, &state, &[result]);

        assert!(failures.is_empty(), "{failures:?}");
    }

    #[test]
    fn intake_checks_can_require_turn_order() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["one".into(), "two".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                intake: IntakeChecks {
                    must_include_actions: Vec::new(),
                    must_match_actions: vec![
                        crate::MemoryIntakeAction::StoreWithUncertainty,
                        crate::MemoryIntakeAction::SupersedeOrCorrect,
                    ],
                },
                ..RuntimeScenarioChecks::default()
            },
        };
        let state = MemoryState::default();
        let results = vec![
            test_result(crate::MemoryIntakeAction::SupersedeOrCorrect),
            test_result(crate::MemoryIntakeAction::StoreWithUncertainty),
        ];

        let failures = evaluate_runtime_scenario(&scenario, &state, &results);

        assert_eq!(failures.len(), 2);
        assert!(failures[0].contains("turn 1 intake action"));
    }

    #[test]
    fn intake_match_actions_must_cover_every_turn() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["one".into(), "two".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                intake: IntakeChecks {
                    must_include_actions: Vec::new(),
                    must_match_actions: vec![crate::MemoryIntakeAction::StoreWithUncertainty],
                },
                ..RuntimeScenarioChecks::default()
            },
        };
        let state = MemoryState::default();
        let results = vec![
            test_result(crate::MemoryIntakeAction::StoreWithUncertainty),
            test_result(crate::MemoryIntakeAction::IgnoreNoise),
        ];

        let failures = evaluate_runtime_scenario(&scenario, &state, &results);

        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("covered 1 turn"));
    }

    #[test]
    fn legacy_text_checks_only_render_current_claims() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec!["hello".into()],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                must_not_contain: vec!["Chris lives in Iowa".to_string()],
                ..RuntimeScenarioChecks::default()
            },
        };
        let mut stale = StructuredAssertion::new("person", "location", "Chris lives in Iowa");
        stale.lifecycle_status = AssertionLifecycleStatus::Superseded;
        let state = MemoryState {
            claims: vec![MemoryClaim::from_assertion(&stale)],
            entity_groups: Vec::new(),
            open_questions: Vec::new(),
            map: MemoryMap::default(),
        };

        let failures = evaluate_runtime_scenario(&scenario, &state, &[]);

        assert!(failures.is_empty(), "{failures:?}");
    }

    #[test]
    fn timed_turns_deserialize_from_object_form() {
        let scenario: RuntimeScenarioFile = serde_json::from_str(
            r#"{
                "name": "timed",
                "turns": [
                    {
                        "content": "Chris moved to Iowa.",
                        "timestamp": "2026-01-01T10:00:00Z"
                    }
                ],
                "checks": {
                    "time": {
                        "require_timestamps": true
                    }
                }
            }"#,
        )
        .unwrap();

        assert_eq!(scenario.turns[0].content(), "Chris moved to Iowa.");
        assert_eq!(
            scenario.turns[0].timestamp().unwrap(),
            "2026-01-01T10:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(scenario_check_count(&scenario), 1);
    }

    #[test]
    fn time_checks_require_timestamps_and_minimum_gap() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec![
                RuntimeScenarioTurn::Timed(RuntimeScenarioTimedTurn {
                    content: "Chris moved to Iowa.".to_string(),
                    timestamp: Some("2026-01-01T10:00:00Z".parse().unwrap()),
                }),
                RuntimeScenarioTurn::Timed(RuntimeScenarioTimedTurn {
                    content: "What do you remember?".to_string(),
                    timestamp: Some("2026-01-01T10:00:30Z".parse().unwrap()),
                }),
                RuntimeScenarioTurn::Timed(RuntimeScenarioTimedTurn {
                    content: "Earlier than the prior turn.".to_string(),
                    timestamp: Some("2026-01-01T09:59:00Z".parse().unwrap()),
                }),
                "untimed turn".into(),
            ],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                time: TimeChecks {
                    require_timestamps: true,
                    min_gap_seconds: Some(60),
                    min_any_gap_seconds: None,
                },
                ..RuntimeScenarioChecks::default()
            },
        };

        let failures = evaluate_runtime_scenario(&scenario, &MemoryState::default(), &[]);

        assert!(failures
            .iter()
            .any(|failure| failure.contains("missing timestamp on turn 4")));
        assert!(failures
            .iter()
            .any(|failure| failure.contains("turn 1 -> 2 was 30s")));
        assert!(failures
            .iter()
            .any(|failure| failure.contains("requires timestamps on turns 3 and 4")));
        assert!(failures
            .iter()
            .any(|failure| failure.contains("timestamps move backward from turn 2 to 3")));
    }

    #[test]
    fn manuscript_one_read_protocol_accepts_source_close_and_retrieval() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec![
                "MANUSCRIPT: Mara Vey is the captain of the Tidefall.".into(),
                "The manuscript is closed.".into(),
                "Who is the captain of the Tidefall?".into(),
            ],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                manuscript_one_read: ManuscriptOneReadChecks {
                    require_source_read: true,
                    require_explicit_close: true,
                    retrieval_turns: vec![3],
                    forbid_source_after_close: true,
                    forbid_search_or_reread_after_close: true,
                    require_proof_eligible: true,
                },
                ..RuntimeScenarioChecks::default()
            },
        };
        let mut retrieval = test_result(crate::MemoryIntakeAction::MarkUnknown);
        retrieval.recalled = RecallSet {
            hits: vec![RecallHit {
                episode_id: uuid::Uuid::new_v4(),
                score: 0.9,
                assertions: vec![StructuredAssertion::new(
                    "manuscript",
                    "character_identity",
                    "Mara Vey is the captain of the Tidefall",
                )],
                reason: RecallReason::new("cue_overlap_activation").unwrap(),
            }],
            latency_ms: 0.0,
        };
        let results = vec![
            test_result(crate::MemoryIntakeAction::StoreWithUncertainty),
            test_result(crate::MemoryIntakeAction::StoreWithUncertainty),
            retrieval,
        ];

        let failures = evaluate_runtime_scenario(&scenario, &MemoryState::default(), &results);

        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(scenario_check_count(&scenario), 6);
    }

    #[test]
    fn manuscript_one_read_protocol_rejects_retrieval_time_reread() {
        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec![
                "MANUSCRIPT: Mara Vey is the captain of the Tidefall.".into(),
                "The manuscript is closed.".into(),
                "Please reread the manuscript before answering.".into(),
            ],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                manuscript_one_read: ManuscriptOneReadChecks {
                    require_source_read: true,
                    require_explicit_close: true,
                    retrieval_turns: vec![3],
                    forbid_source_after_close: true,
                    forbid_search_or_reread_after_close: true,
                    require_proof_eligible: true,
                },
                ..RuntimeScenarioChecks::default()
            },
        };
        let mut retrieval = test_result(crate::MemoryIntakeAction::MarkUnknown);
        retrieval
            .observation
            .assertions
            .push(StructuredAssertion::new(
                "manuscript",
                "character_identity",
                "Mara Vey is the captain of the Tidefall",
            ));
        let results = vec![
            test_result(crate::MemoryIntakeAction::StoreWithUncertainty),
            test_result(crate::MemoryIntakeAction::StoreWithUncertainty),
            retrieval,
        ];

        let failures = evaluate_runtime_scenario(&scenario, &MemoryState::default(), &results);

        assert!(failures
            .iter()
            .any(|failure| failure.contains("requests source search or reread")));
        assert!(failures
            .iter()
            .any(|failure| failure.contains("produced new manuscript assertions")));
        assert!(failures
            .iter()
            .any(|failure| failure.contains("did not recall prior manuscript memory")));
    }

    #[test]
    fn durable_topology_check_rejects_mismatched_commit_event() {
        let log = std::env::temp_dir()
            .join(format!("luna_scenario_{}", Uuid::new_v4()))
            .join("events.jsonl");
        let session = RuntimeSession::new(&log, FusedExtractor::new());
        let first = session
            .process_user_turn("MKPE is my provenance engine.")
            .unwrap();
        let second = session
            .process_user_turn("What did I say about MKPE?")
            .unwrap();
        let state = session.inspect().unwrap();
        let mut events = JsonlEventLog::new(&log).load().unwrap();
        let last_commit = events
            .iter_mut()
            .rev()
            .find_map(|event| match &mut event.payload {
                LunaEvent::TopologyBridgeCommitted(commit) => Some(commit),
                _ => None,
            })
            .expect("runtime turn should append topology bridge commit");
        last_commit.source_event_hashes.clear();

        let scenario = RuntimeScenarioFile {
            name: None,
            turns: vec![
                "MKPE is my provenance engine.".into(),
                "What did I say about MKPE?".into(),
            ],
            restart_after_turns: Vec::new(),
            checks: RuntimeScenarioChecks {
                topology_bridge: TopologyBridgeChecks {
                    nodes: vec![TopologyNodeCheck {
                        runtime_entity_ref: "project:MKPE".to_string(),
                        topology_ref: Some("node:project:MKPE".to_string()),
                        label: Some("MKPE".to_string()),
                        claim_assertion_keys: Vec::new(),
                        require_source_event_hash: false,
                    }],
                    require_durable_commit: true,
                    min_committed_nodes: Some(1),
                    ..TopologyBridgeChecks::default()
                },
                ..RuntimeScenarioChecks::default()
            },
        };

        let failures =
            evaluate_runtime_scenario_with_events(&scenario, &state, &[first, second], &events);

        assert!(failures
            .iter()
            .any(|failure| failure.contains("does not match recomputed ledger projection")));

        let _ = fs::remove_dir_all(log.parent().unwrap());
    }

    #[test]
    fn scenario_keep_log_preserves_fresh_log_without_appending_old_evidence() {
        let root = std::env::temp_dir().join(format!("luna_scenario_{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let scenario_path = root.join("scenario.json");
        let log = root.join("events.jsonl");
        JsonlEventLog::new(&log)
            .append(&luna_core::EventEnvelope::new(
                LunaEvent::TurnObserved(luna_core::TurnObserved {
                    turn: luna_core::ConversationTurn {
                        role: luna_core::Role::User,
                        content: "STALE SHOULD NOT SURVIVE".to_string(),
                        timestamp: None,
                    },
                }),
                luna_core::EventSource::User,
                1.0,
            ))
            .unwrap();
        fs::write(
            &scenario_path,
            r#"{
                "name": "fresh_keep_log",
                "turns": ["MKPE is my provenance engine."],
                "checks": {
                    "claims": {
                        "must_include": [
                            {"domain": "project", "kind": "identity", "value": "MKPE is my provenance engine"}
                        ]
                    }
                }
            }"#,
        )
        .unwrap();

        let report = run_runtime_scenario(&scenario_path, &log, FusedExtractor::new(), true)
            .expect("scenario should pass");
        let events = JsonlEventLog::new(&log).load().unwrap();
        let stale_survived = events.iter().any(|event| match &event.payload {
            LunaEvent::TurnObserved(observed) => observed.turn.content.contains("STALE"),
            _ => false,
        });

        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!stale_survived);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scenario_json_rejects_unknown_check_fields() {
        let error = serde_json::from_str::<RuntimeScenarioFile>(
            r#"{
                "turns": ["What do you remember?"],
                "checks": {
                    "recall": {
                        "must_have_reason_typo": true
                    }
                }
            }"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn scenario_json_rejects_unknown_turn_fields() {
        let error = serde_json::from_str::<RuntimeScenarioFile>(
            r#"{
                "turns": [
                    {
                        "content": "Chris lives in Iowa.",
                        "timestamp": "2026-05-10T12:00:00Z",
                        "timestamp_typo": "2026-05-10T12:00:00Z"
                    }
                ],
                "checks": {
                    "must_contain": ["Chris"]
                }
            }"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("did not match any variant"));
    }

    #[test]
    fn malformed_scenario_does_not_delete_existing_log() {
        let root = std::env::temp_dir().join(format!("luna_scenario_{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let scenario_path = root.join("bad.json");
        let log = root.join("events.jsonl");
        fs::write(&scenario_path, "{ not json").unwrap();
        fs::write(&log, "do not delete").unwrap();

        let error =
            run_runtime_scenario(&scenario_path, &log, FusedExtractor::new(), false).unwrap_err();

        assert!(error.to_string().contains("key must be a string"));
        assert_eq!(fs::read_to_string(&log).unwrap(), "do not delete");

        let _ = fs::remove_dir_all(root);
    }

    fn test_observation() -> luna_core::TurnReading {
        luna_core::TurnReading {
            turn_id: uuid::Uuid::new_v4(),
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
            cue_terms: Vec::new(),
            query_intents: Vec::new(),
            assertions: Vec::new(),
        }
    }

    fn evidence_node(label: &str, assertion: &StructuredAssertion) -> MemoryNode {
        MemoryNode {
            id: label.replace(' ', "_"),
            label: label.to_string(),
            kind: MemoryNodeKind::Assertion,
            confidence_tier: assertion.confidence_tier,
            density: 1.0,
            activation: 1.0,
            created_at: None,
            contradiction_count: 0,
            provenance: vec![MemoryProvenance::from_assertion(assertion.key())],
        }
    }

    fn test_result(action: crate::MemoryIntakeAction) -> crate::RuntimeTurnResult {
        crate::RuntimeTurnResult {
            turn_id: uuid::Uuid::new_v4(),
            observation: test_observation(),
            knowledge_delta: crate::KnowledgeDelta::default(),
            memory_state: MemoryState::default(),
            working_memory: luna_core::WorkingMemory::default(),
            recalled: luna_core::RecallSet::default(),
            recall_mode: luna_core::RecallMode::OpenEnded,
            questions: Vec::new(),
            context_packet: crate::ContextPacket {
                recall_mode: luna_core::RecallMode::OpenEnded,
                recalled_claims: Vec::new(),
                working_memory: luna_core::WorkingMemory::default(),
                compressed_memory: Vec::new(),
                open_questions: Vec::new(),
                summary: String::new(),
            },
            intake: crate::MemoryIntakeDecision {
                action,
                reason: "test fixture".to_string(),
            },
            output_packet: luna_output::OutputPacket {
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
}
