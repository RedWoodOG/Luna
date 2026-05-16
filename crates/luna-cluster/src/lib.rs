use chrono::{DateTime, SecondsFormat, Utc};
use luna_core::{LunaError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    path::Path,
};

pub mod compression;
pub use compression::{
    compression_output_hash, issue_compression_receipt, validate_compression_receipt,
    CompressionDecision, CompressionPolicy, CompressionReceipt, CompressionReceiptRegistry,
    CompressionRequest,
};

pub const CONSOLIDATION_SCHEMA_VERSION: &str = "luna.consolidation_event.v1";
pub const CLUSTER_EVOLUTION_SCHEMA_VERSION: &str = "luna.cluster_evolution_event.v1";
pub const CLUSTER_FORMED_OPERATION: &str = "cluster_formed";
pub const CLUSTER_SPLIT_OPERATION: &str = "cluster_split";
pub const CLUSTER_MERGED_OPERATION: &str = "cluster_merged";
pub const DEFAULT_MIN_SOURCE_NODES: usize = 2;
pub const DEFAULT_MIN_SOURCE_EVENTS: usize = 1;
pub const DEFAULT_MIN_COHESION_SCORE: f64 = 0.7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEventRef {
    pub event_id: String,
    pub event_hash: String,
}

impl SourceEventRef {
    pub fn new(event_id: impl Into<String>, event_hash: impl Into<String>) -> Self {
        Self {
            event_id: event_id.into(),
            event_hash: event_hash.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsolidationDecision {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsolidationEvent {
    pub schema_version: String,
    pub event_id: String,
    pub operation: String,
    pub orb_id: String,
    pub source_node_ids: Vec<String>,
    pub source_tether_ids: Vec<String>,
    pub source_event_refs: Vec<SourceEventRef>,
    pub cohesion_rule_id: String,
    pub cohesion_score: f64,
    pub decision: ConsolidationDecision,
    pub rejection_reason: Option<String>,
    pub replay_trace_hash: String,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterEvolutionOperation {
    Split,
    Merge,
}

impl ClusterEvolutionOperation {
    fn as_operation_str(self) -> &'static str {
        match self {
            Self::Split => CLUSTER_SPLIT_OPERATION,
            Self::Merge => CLUSTER_MERGED_OPERATION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterEvolutionEvent {
    pub schema_version: String,
    pub event_id: String,
    pub operation: String,
    pub parent_orb_ids: Vec<String>,
    #[serde(default)]
    pub parent_orb_refs: Vec<ParentOrbStateRef>,
    pub child_orb_ids: Vec<String>,
    pub cause: String,
    pub metric_evidence_refs: Vec<MetricEvidenceRef>,
    pub decision: ConsolidationDecision,
    pub rejection_reason: Option<String>,
    pub replay_trace_hash: String,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ParentOrbStateRef {
    pub orb_id: String,
    pub accepted_event_id: String,
    pub lineage_hash: String,
    pub source_event_ancestry_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MetricEvidenceRef {
    pub metric_id: String,
    pub event_id: String,
    pub event_hash: String,
}

impl MetricEvidenceRef {
    pub fn new(
        metric_id: impl Into<String>,
        event_id: impl Into<String>,
        event_hash: impl Into<String>,
    ) -> Self {
        Self {
            metric_id: metric_id.into(),
            event_id: event_id.into(),
            event_hash: event_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClusterEvolutionRequest {
    pub operation: ClusterEvolutionOperation,
    pub parent_orb_ids: Vec<String>,
    pub child_orb_ids: Vec<String>,
    pub cause: String,
    pub metric_evidence_refs: Vec<MetricEvidenceRef>,
}

impl ClusterEvolutionRequest {
    pub fn split(
        parent_orb_id: impl Into<String>,
        child_orb_ids: Vec<String>,
        cause: impl Into<String>,
        metric_evidence_refs: Vec<MetricEvidenceRef>,
    ) -> Self {
        Self {
            operation: ClusterEvolutionOperation::Split,
            parent_orb_ids: vec![parent_orb_id.into()],
            child_orb_ids,
            cause: cause.into(),
            metric_evidence_refs,
        }
    }

    pub fn merge(
        parent_orb_ids: Vec<String>,
        child_orb_id: impl Into<String>,
        cause: impl Into<String>,
        metric_evidence_refs: Vec<MetricEvidenceRef>,
    ) -> Self {
        Self {
            operation: ClusterEvolutionOperation::Merge,
            parent_orb_ids,
            child_orb_ids: vec![child_orb_id.into()],
            cause: cause.into(),
            metric_evidence_refs,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClusterFormationPolicy {
    pub min_source_nodes: usize,
    pub min_source_events: usize,
    pub min_cohesion_score: f64,
}

impl Default for ClusterFormationPolicy {
    fn default() -> Self {
        Self {
            min_source_nodes: DEFAULT_MIN_SOURCE_NODES,
            min_source_events: DEFAULT_MIN_SOURCE_EVENTS,
            min_cohesion_score: DEFAULT_MIN_COHESION_SCORE,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClusterFormationRequest {
    pub orb_id: String,
    pub source_node_ids: Vec<String>,
    pub source_tether_ids: Vec<String>,
    pub source_event_refs: Vec<SourceEventRef>,
    pub cohesion_rule_id: String,
    pub cohesion_score: f64,
}

impl ClusterFormationRequest {
    pub fn new(
        orb_id: impl Into<String>,
        source_node_ids: Vec<String>,
        source_tether_ids: Vec<String>,
        source_event_refs: Vec<SourceEventRef>,
        cohesion_rule_id: impl Into<String>,
        cohesion_score: f64,
    ) -> Self {
        Self {
            orb_id: orb_id.into(),
            source_node_ids,
            source_tether_ids,
            source_event_refs,
            cohesion_rule_id: cohesion_rule_id.into(),
            cohesion_score,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCluster {
    pub orb_id: String,
    pub source_node_ids: Vec<String>,
    pub source_tether_ids: Vec<String>,
    pub source_event_refs: Vec<SourceEventRef>,
    pub accepted_event_id: String,
    pub lineage_event_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ClusterRegistry {
    clusters: BTreeMap<String, MemoryCluster>,
    retired_clusters: BTreeMap<String, MemoryCluster>,
    rejected_event_ids: Vec<String>,
    evolution_event_ids: Vec<String>,
}

impl ClusterRegistry {
    pub fn apply_consolidation_event(&mut self, event: &ConsolidationEvent) -> Result<()> {
        validate_consolidation_event(event)?;
        match event.decision {
            ConsolidationDecision::Accepted => {
                if self.clusters.contains_key(&event.orb_id)
                    || self.retired_clusters.contains_key(&event.orb_id)
                {
                    return Err(LunaError::new(format!(
                        "cluster {} already exists during replay",
                        event.orb_id
                    )));
                }
                self.clusters.insert(
                    event.orb_id.clone(),
                    MemoryCluster {
                        orb_id: event.orb_id.clone(),
                        source_node_ids: sorted_unique(event.source_node_ids.clone()),
                        source_tether_ids: sorted_unique(event.source_tether_ids.clone()),
                        source_event_refs: sorted_unique_event_refs(
                            event.source_event_refs.clone(),
                        ),
                        accepted_event_id: event.event_id.clone(),
                        lineage_event_ids: vec![event.event_id.clone()],
                    },
                );
            }
            ConsolidationDecision::Rejected => {
                self.rejected_event_ids.push(event.event_id.clone());
            }
        }
        Ok(())
    }

    pub fn apply_evolution_event(&mut self, event: &ClusterEvolutionEvent) -> Result<()> {
        validate_cluster_evolution_event(event)?;
        match event.decision {
            ConsolidationDecision::Accepted => {
                self.apply_accepted_evolution_event(event)?;
                self.evolution_event_ids.push(event.event_id.clone());
            }
            ConsolidationDecision::Rejected => {
                self.rejected_event_ids.push(event.event_id.clone());
            }
        }
        Ok(())
    }

    fn apply_accepted_evolution_event(&mut self, event: &ClusterEvolutionEvent) -> Result<()> {
        for child_id in &event.child_orb_ids {
            if self.clusters.contains_key(child_id) || self.retired_clusters.contains_key(child_id)
            {
                return Err(LunaError::new(format!(
                    "child cluster {} already exists during evolution replay",
                    child_id
                )));
            }
        }

        let mut parents = Vec::new();
        for parent_id in &event.parent_orb_ids {
            let parent = self.clusters.get(parent_id).ok_or_else(|| {
                LunaError::new(format!(
                    "cluster evolution references missing active parent cluster {}",
                    parent_id
                ))
            })?;
            let expected_ref = parent_state_ref(parent);
            if !event.parent_orb_refs.contains(&expected_ref) {
                return Err(LunaError::new(format!(
                    "cluster evolution parent state ref mismatch for {}",
                    parent_id
                )));
            }
            parents.push(parent.clone());
        }

        let mut source_node_ids = Vec::new();
        let mut source_tether_ids = Vec::new();
        let mut source_event_refs = Vec::new();
        let mut lineage_event_ids = Vec::new();
        for parent in &parents {
            source_node_ids.extend(parent.source_node_ids.clone());
            source_tether_ids.extend(parent.source_tether_ids.clone());
            source_event_refs.extend(parent.source_event_refs.clone());
            lineage_event_ids.extend(parent.lineage_event_ids.clone());
            lineage_event_ids.push(parent.accepted_event_id.clone());
        }
        lineage_event_ids.push(event.event_id.clone());
        let lineage_event_ids = sorted_unique(lineage_event_ids);

        for parent_id in &event.parent_orb_ids {
            if let Some(parent) = self.clusters.remove(parent_id) {
                self.retired_clusters.insert(parent_id.clone(), parent);
            }
        }

        for child_id in &event.child_orb_ids {
            self.clusters.insert(
                child_id.clone(),
                MemoryCluster {
                    orb_id: child_id.clone(),
                    source_node_ids: sorted_unique(source_node_ids.clone()),
                    source_tether_ids: sorted_unique(source_tether_ids.clone()),
                    source_event_refs: sorted_unique_event_refs(source_event_refs.clone()),
                    accepted_event_id: event.event_id.clone(),
                    lineage_event_ids: lineage_event_ids.clone(),
                },
            );
        }
        Ok(())
    }

    pub fn clusters(&self) -> &BTreeMap<String, MemoryCluster> {
        &self.clusters
    }

    pub fn retired_clusters(&self) -> &BTreeMap<String, MemoryCluster> {
        &self.retired_clusters
    }

    pub fn rejected_event_ids(&self) -> &[String] {
        &self.rejected_event_ids
    }

    pub fn evolution_event_ids(&self) -> &[String] {
        &self.evolution_event_ids
    }
}

pub fn form_memory_cluster(
    request: ClusterFormationRequest,
    policy: &ClusterFormationPolicy,
) -> ConsolidationEvent {
    form_memory_cluster_at(request, policy, Utc::now())
}

pub fn form_memory_cluster_at(
    request: ClusterFormationRequest,
    policy: &ClusterFormationPolicy,
    recorded_at: DateTime<Utc>,
) -> ConsolidationEvent {
    let request = canonical_request(request);
    let rejection_reason = rejection_reason(&request, policy);
    let decision = if rejection_reason.is_some() {
        ConsolidationDecision::Rejected
    } else {
        ConsolidationDecision::Accepted
    };
    let replay_trace_hash = replay_trace_hash(
        &request,
        decision,
        rejection_reason.as_deref(),
        &recorded_at,
    );
    let event_id = consolidation_event_id(&request.orb_id, &replay_trace_hash, decision);

    ConsolidationEvent {
        schema_version: CONSOLIDATION_SCHEMA_VERSION.to_string(),
        event_id,
        operation: CLUSTER_FORMED_OPERATION.to_string(),
        orb_id: request.orb_id,
        source_node_ids: sorted_unique(request.source_node_ids),
        source_tether_ids: sorted_unique(request.source_tether_ids),
        source_event_refs: sorted_unique_event_refs(request.source_event_refs),
        cohesion_rule_id: request.cohesion_rule_id,
        cohesion_score: request.cohesion_score,
        decision,
        rejection_reason,
        replay_trace_hash,
        recorded_at,
    }
}

pub fn replay_consolidation_events(events: &[ConsolidationEvent]) -> Result<ClusterRegistry> {
    let mut registry = ClusterRegistry::default();
    for event in events {
        registry.apply_consolidation_event(event)?;
    }
    Ok(registry)
}

pub fn evolve_cluster(
    request: ClusterEvolutionRequest,
    registry: &ClusterRegistry,
) -> ClusterEvolutionEvent {
    let request = canonical_evolution_request(request);
    let parent_orb_refs = parent_refs_for_request(&request, registry);
    let rejection_reason = evolution_rejection_reason(&request, registry);
    let decision = if rejection_reason.is_some() {
        ConsolidationDecision::Rejected
    } else {
        ConsolidationDecision::Accepted
    };
    let recorded_at = Utc::now();
    let replay_trace_hash = cluster_evolution_trace_hash(
        &request,
        &parent_orb_refs,
        decision,
        rejection_reason.as_deref(),
        &recorded_at,
    );
    let event_id = cluster_evolution_event_id(
        request.operation,
        &request.parent_orb_ids,
        &request.child_orb_ids,
        &replay_trace_hash,
        decision,
    );

    ClusterEvolutionEvent {
        schema_version: CLUSTER_EVOLUTION_SCHEMA_VERSION.to_string(),
        event_id,
        operation: request.operation.as_operation_str().to_string(),
        parent_orb_ids: request.parent_orb_ids,
        parent_orb_refs,
        child_orb_ids: request.child_orb_ids,
        cause: request.cause,
        metric_evidence_refs: request.metric_evidence_refs,
        decision,
        rejection_reason,
        replay_trace_hash,
        recorded_at,
    }
}

pub fn replay_cluster_events(
    consolidation_events: &[ConsolidationEvent],
    evolution_events: &[ClusterEvolutionEvent],
) -> Result<ClusterRegistry> {
    let mut registry = replay_consolidation_events(consolidation_events)?;
    for event in evolution_events {
        registry.apply_evolution_event(event)?;
    }
    Ok(registry)
}

pub fn append_consolidation_event_jsonl(path: &Path, event: &ConsolidationEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| LunaError::new(err.to_string()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| LunaError::new(err.to_string()))?;
    let line = serde_json::to_string(event).map_err(|err| LunaError::new(err.to_string()))?;
    writeln!(file, "{line}").map_err(|err| LunaError::new(err.to_string()))
}

pub fn load_consolidation_events_jsonl(path: &Path) -> Result<Vec<ConsolidationEvent>> {
    let file = std::fs::File::open(path).map_err(|err| LunaError::new(err.to_string()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|err| LunaError::new(err.to_string()))?;
        if line.trim().is_empty() {
            continue;
        }
        let event = serde_json::from_str(&line).map_err(|err| LunaError::new(err.to_string()))?;
        validate_consolidation_event(&event)?;
        events.push(event);
    }
    Ok(events)
}

fn rejection_reason(
    request: &ClusterFormationRequest,
    policy: &ClusterFormationPolicy,
) -> Option<String> {
    if request.cohesion_rule_id.trim().is_empty() {
        return Some("cohesion rule id is required for memory cluster formation".to_string());
    }
    if request.source_node_ids.len() < policy.min_source_nodes {
        return Some("not enough source nodes for memory cluster formation".to_string());
    }
    if request.source_event_refs.len() < policy.min_source_events {
        return Some("not enough source events for memory cluster formation".to_string());
    }
    if !(0.0..=1.0).contains(&request.cohesion_score) {
        return Some("cohesion score must be between 0 and 1".to_string());
    }
    if request.cohesion_score < policy.min_cohesion_score {
        return Some(format!(
            "cohesion score {:.3} below threshold {:.3}",
            request.cohesion_score, policy.min_cohesion_score
        ));
    }
    if !request.source_event_refs.iter().all(valid_source_event_ref) {
        return Some("source event refs must include ids and 64-char lowercase hashes".to_string());
    }
    None
}

pub fn validate_consolidation_event(event: &ConsolidationEvent) -> Result<()> {
    if event.schema_version != CONSOLIDATION_SCHEMA_VERSION {
        return Err(LunaError::new("unexpected consolidation schema version"));
    }
    if event.operation != CLUSTER_FORMED_OPERATION {
        return Err(LunaError::new("unexpected consolidation operation"));
    }
    if event.orb_id.trim().is_empty() || event.event_id.trim().is_empty() {
        return Err(LunaError::new(
            "consolidation event and orb ids are required",
        ));
    }
    if event.cohesion_rule_id.trim().is_empty() {
        return Err(LunaError::new("consolidation cohesion rule id is required"));
    }
    if event.source_node_ids.is_empty() || event.source_event_refs.is_empty() {
        return Err(LunaError::new(
            "accepted or rejected consolidation receipts must preserve sources",
        ));
    }
    if event.source_node_ids.len() != sorted_unique(event.source_node_ids.clone()).len()
        || event.source_tether_ids.len() != sorted_unique(event.source_tether_ids.clone()).len()
        || event.source_event_refs.len()
            != sorted_unique_event_refs(event.source_event_refs.clone()).len()
    {
        return Err(LunaError::new(
            "consolidation receipt sources must be non-empty and unique",
        ));
    }
    if !event.source_event_refs.iter().all(valid_source_event_ref) {
        return Err(LunaError::new("invalid source event ref"));
    }
    if !(0.0..=1.0).contains(&event.cohesion_score) {
        return Err(LunaError::new(
            "consolidation cohesion score must be between 0 and 1",
        ));
    }
    if !is_hex_hash(&event.replay_trace_hash) {
        return Err(LunaError::new("invalid replay trace hash"));
    }
    let expected_replay_trace_hash = replay_trace_hash(
        &ClusterFormationRequest::new(
            event.orb_id.clone(),
            event.source_node_ids.clone(),
            event.source_tether_ids.clone(),
            event.source_event_refs.clone(),
            event.cohesion_rule_id.clone(),
            event.cohesion_score,
        ),
        event.decision,
        event.rejection_reason.as_deref(),
        &event.recorded_at,
    );
    if event.replay_trace_hash != expected_replay_trace_hash {
        return Err(LunaError::new("consolidation replay trace hash mismatch"));
    }
    let expected_event_id =
        consolidation_event_id(&event.orb_id, &event.replay_trace_hash, event.decision);
    if event.event_id != expected_event_id {
        return Err(LunaError::new("consolidation event id mismatch"));
    }
    match event.decision {
        ConsolidationDecision::Accepted if event.rejection_reason.is_some() => Err(LunaError::new(
            "accepted cluster formation cannot carry a rejection reason",
        )),
        ConsolidationDecision::Accepted
            if event.source_node_ids.len() < DEFAULT_MIN_SOURCE_NODES =>
        {
            Err(LunaError::new(
                "accepted cluster formation does not have enough source nodes",
            ))
        }
        ConsolidationDecision::Accepted
            if event.source_event_refs.len() < DEFAULT_MIN_SOURCE_EVENTS =>
        {
            Err(LunaError::new(
                "accepted cluster formation does not have enough source events",
            ))
        }
        ConsolidationDecision::Accepted if event.cohesion_score < DEFAULT_MIN_COHESION_SCORE => {
            Err(LunaError::new(
                "accepted cluster formation cohesion score is below threshold",
            ))
        }
        ConsolidationDecision::Rejected
            if event
                .rejection_reason
                .as_deref()
                .is_none_or(|reason| reason.trim().is_empty()) =>
        {
            Err(LunaError::new(
                "rejected cluster formation requires a rejection reason",
            ))
        }
        _ => Ok(()),
    }
}

pub fn validate_cluster_evolution_event(event: &ClusterEvolutionEvent) -> Result<()> {
    if event.schema_version != CLUSTER_EVOLUTION_SCHEMA_VERSION {
        return Err(LunaError::new(
            "unexpected cluster evolution schema version",
        ));
    }
    let operation = parse_cluster_evolution_operation(&event.operation)?;
    if event.event_id.trim().is_empty() {
        return Err(LunaError::new("cluster evolution event id is required"));
    }
    if event.parent_orb_ids.is_empty() || event.child_orb_ids.is_empty() {
        return Err(LunaError::new(
            "cluster evolution receipts must preserve parent and child cluster ids",
        ));
    }
    if event.parent_orb_ids.len() != sorted_unique(event.parent_orb_ids.clone()).len()
        || event.parent_orb_refs.len()
            != sorted_unique_parent_refs(event.parent_orb_refs.clone()).len()
        || event.child_orb_ids.len() != sorted_unique(event.child_orb_ids.clone()).len()
        || event.metric_evidence_refs.len()
            != sorted_unique_metric_refs(event.metric_evidence_refs.clone()).len()
    {
        return Err(LunaError::new(
            "cluster evolution parent, child, and metric refs must be unique",
        ));
    }
    if event
        .parent_orb_ids
        .iter()
        .any(|parent| event.child_orb_ids.contains(parent))
    {
        return Err(LunaError::new(
            "cluster evolution parent and child ids must not overlap",
        ));
    }
    if event.cause.trim().is_empty() {
        return Err(LunaError::new("cluster evolution cause is required"));
    }
    if event.metric_evidence_refs.is_empty() {
        return Err(LunaError::new(
            "cluster evolution requires at least one metric or sentinel evidence ref",
        ));
    }
    if !event
        .metric_evidence_refs
        .iter()
        .all(valid_metric_evidence_ref)
    {
        return Err(LunaError::new(
            "cluster evolution metric evidence refs must include metric ids, event ids, and 64-char lowercase event hashes",
        ));
    }
    match operation {
        ClusterEvolutionOperation::Split if event.parent_orb_ids.len() != 1 => {
            return Err(LunaError::new(
                "cluster split requires exactly one parent cluster id",
            ));
        }
        ClusterEvolutionOperation::Split if event.child_orb_ids.len() < 2 => {
            return Err(LunaError::new(
                "cluster split requires at least two child cluster ids",
            ));
        }
        ClusterEvolutionOperation::Merge if event.parent_orb_ids.len() < 2 => {
            return Err(LunaError::new(
                "cluster merge requires at least two parent cluster ids",
            ));
        }
        ClusterEvolutionOperation::Merge if event.child_orb_ids.len() != 1 => {
            return Err(LunaError::new(
                "cluster merge requires exactly one child cluster id",
            ));
        }
        _ => {}
    }
    if !is_hex_hash(&event.replay_trace_hash) {
        return Err(LunaError::new(
            "invalid cluster evolution replay trace hash",
        ));
    }
    let request = ClusterEvolutionRequest {
        operation,
        parent_orb_ids: event.parent_orb_ids.clone(),
        child_orb_ids: event.child_orb_ids.clone(),
        cause: event.cause.clone(),
        metric_evidence_refs: event.metric_evidence_refs.clone(),
    };
    let expected_trace = cluster_evolution_trace_hash(
        &request,
        &event.parent_orb_refs,
        event.decision,
        event.rejection_reason.as_deref(),
        &event.recorded_at,
    );
    if event.replay_trace_hash != expected_trace {
        return Err(LunaError::new(
            "cluster evolution replay trace hash mismatch",
        ));
    }
    let expected_event_id = cluster_evolution_event_id(
        operation,
        &event.parent_orb_ids,
        &event.child_orb_ids,
        &event.replay_trace_hash,
        event.decision,
    );
    if event.event_id != expected_event_id {
        return Err(LunaError::new("cluster evolution event id mismatch"));
    }
    match event.decision {
        ConsolidationDecision::Accepted if event.rejection_reason.is_some() => Err(LunaError::new(
            "accepted cluster evolution cannot carry a rejection reason",
        )),
        ConsolidationDecision::Accepted
            if operation == ClusterEvolutionOperation::Split
                && !split_gate_has_pressure_evidence(&request) =>
        {
            Err(LunaError::new(
                "accepted orb split requires contradiction-pressure or splinter-pressure evidence",
            ))
        }
        ConsolidationDecision::Accepted
            if event.parent_orb_refs.len() != event.parent_orb_ids.len() =>
        {
            Err(LunaError::new(
                "accepted cluster evolution must bind every parent state",
            ))
        }
        ConsolidationDecision::Rejected
            if event
                .rejection_reason
                .as_deref()
                .is_none_or(|reason| reason.trim().is_empty()) =>
        {
            Err(LunaError::new(
                "rejected cluster evolution requires a rejection reason",
            ))
        }
        _ => Ok(()),
    }
}

fn parent_refs_for_request(
    request: &ClusterEvolutionRequest,
    registry: &ClusterRegistry,
) -> Vec<ParentOrbStateRef> {
    request
        .parent_orb_ids
        .iter()
        .filter_map(|parent_id| registry.clusters.get(parent_id))
        .map(parent_state_ref)
        .collect()
}

fn parent_state_ref(parent: &MemoryCluster) -> ParentOrbStateRef {
    ParentOrbStateRef {
        orb_id: parent.orb_id.clone(),
        accepted_event_id: parent.accepted_event_id.clone(),
        lineage_hash: string_list_hash("lineage", &parent.lineage_event_ids),
        source_event_ancestry_hash: source_event_refs_hash(&parent.source_event_refs),
    }
}

fn evolution_rejection_reason(
    request: &ClusterEvolutionRequest,
    registry: &ClusterRegistry,
) -> Option<String> {
    if request.cause.trim().is_empty() {
        return Some("cluster evolution cause is required".to_string());
    }
    if request.metric_evidence_refs.is_empty() {
        return Some("cluster evolution requires metric or sentinel evidence refs".to_string());
    }
    if !request
        .metric_evidence_refs
        .iter()
        .all(valid_metric_evidence_ref)
    {
        return Some(
            "metric evidence refs must include metric ids, event ids, and 64-char lowercase hashes"
                .to_string(),
        );
    }
    if request
        .parent_orb_ids
        .iter()
        .any(|parent| request.child_orb_ids.contains(parent))
    {
        return Some("parent and child cluster ids must not overlap".to_string());
    }
    match request.operation {
        ClusterEvolutionOperation::Split if request.parent_orb_ids.len() != 1 => {
            return Some("cluster split requires exactly one parent cluster id".to_string());
        }
        ClusterEvolutionOperation::Split if request.child_orb_ids.len() < 2 => {
            return Some("cluster split requires at least two child cluster ids".to_string());
        }
        ClusterEvolutionOperation::Split if !split_gate_has_pressure_evidence(request) => {
            return Some(
                "cluster split requires contradiction-pressure or splinter-pressure evidence"
                    .to_string(),
            );
        }
        ClusterEvolutionOperation::Merge if request.parent_orb_ids.len() < 2 => {
            return Some("cluster merge requires at least two parent cluster ids".to_string());
        }
        ClusterEvolutionOperation::Merge if request.child_orb_ids.len() != 1 => {
            return Some("cluster merge requires exactly one child cluster id".to_string());
        }
        _ => {}
    }
    for parent_id in &request.parent_orb_ids {
        if !registry.clusters.contains_key(parent_id) {
            return Some(format!("missing active parent cluster {parent_id}"));
        }
    }
    for child_id in &request.child_orb_ids {
        if registry.clusters.contains_key(child_id)
            || registry.retired_clusters.contains_key(child_id)
        {
            return Some(format!("child cluster {child_id} already exists"));
        }
    }
    None
}

fn split_gate_has_pressure_evidence(request: &ClusterEvolutionRequest) -> bool {
    request.metric_evidence_refs.iter().any(|reference| {
        let metric = reference.metric_id.to_ascii_lowercase();
        metric.contains("contradiction_pressure")
            || metric.contains("contradiction-pressure")
            || metric.contains("sentinel:contradiction")
            || metric.contains("splinter_pressure")
            || metric.contains("splinter-pressure")
            || metric.contains("sentinel:splinter")
    })
}

fn parse_cluster_evolution_operation(operation: &str) -> Result<ClusterEvolutionOperation> {
    match operation {
        CLUSTER_SPLIT_OPERATION => Ok(ClusterEvolutionOperation::Split),
        CLUSTER_MERGED_OPERATION => Ok(ClusterEvolutionOperation::Merge),
        _ => Err(LunaError::new("unexpected cluster evolution operation")),
    }
}

fn canonical_request(request: ClusterFormationRequest) -> ClusterFormationRequest {
    ClusterFormationRequest {
        orb_id: request.orb_id,
        source_node_ids: sorted_unique(request.source_node_ids),
        source_tether_ids: sorted_unique(request.source_tether_ids),
        source_event_refs: sorted_unique_event_refs(request.source_event_refs),
        cohesion_rule_id: request.cohesion_rule_id,
        cohesion_score: request.cohesion_score,
    }
}

fn canonical_evolution_request(request: ClusterEvolutionRequest) -> ClusterEvolutionRequest {
    ClusterEvolutionRequest {
        operation: request.operation,
        parent_orb_ids: sorted_unique(request.parent_orb_ids),
        child_orb_ids: sorted_unique(request.child_orb_ids),
        cause: request.cause,
        metric_evidence_refs: sorted_unique_metric_refs(request.metric_evidence_refs),
    }
}

fn valid_source_event_ref(reference: &SourceEventRef) -> bool {
    !reference.event_id.trim().is_empty() && is_hex_hash(&reference.event_hash)
}

pub fn valid_metric_evidence_ref(reference: &MetricEvidenceRef) -> bool {
    !reference.metric_id.trim().is_empty()
        && !reference.event_id.trim().is_empty()
        && is_hex_hash(&reference.event_hash)
}

fn is_hex_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sorted_unique_event_refs(values: Vec<SourceEventRef>) -> Vec<SourceEventRef> {
    let mut refs = BTreeMap::<String, SourceEventRef>::new();
    for value in values {
        if valid_source_event_ref(&value) {
            refs.entry(value.event_id.clone()).or_insert(value);
        }
    }
    refs.into_values().collect()
}

fn sorted_unique_parent_refs(values: Vec<ParentOrbStateRef>) -> Vec<ParentOrbStateRef> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sorted_unique_metric_refs(values: Vec<MetricEvidenceRef>) -> Vec<MetricEvidenceRef> {
    values
        .into_iter()
        .filter(valid_metric_evidence_ref)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn replay_trace_hash(
    request: &ClusterFormationRequest,
    decision: ConsolidationDecision,
    rejection_reason: Option<&str>,
    recorded_at: &DateTime<Utc>,
) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "schema_version", CONSOLIDATION_SCHEMA_VERSION);
    hash_field(&mut hasher, "orb_id", &request.orb_id);
    for node_id in sorted_unique(request.source_node_ids.clone()) {
        hash_field(&mut hasher, "source_node_id", &node_id);
    }
    for tether_id in sorted_unique(request.source_tether_ids.clone()) {
        hash_field(&mut hasher, "source_tether_id", &tether_id);
    }
    for reference in sorted_unique_event_refs(request.source_event_refs.clone()) {
        hash_field(&mut hasher, "source_event_id", &reference.event_id);
        hash_field(&mut hasher, "source_event_hash", &reference.event_hash);
    }
    hash_field(&mut hasher, "cohesion_rule_id", &request.cohesion_rule_id);
    hash_field(
        &mut hasher,
        "cohesion_score",
        &format!("{:016x}", request.cohesion_score.to_bits()),
    );
    hash_field(&mut hasher, "decision", &format!("{decision:?}"));
    if let Some(reason) = rejection_reason {
        hash_field(&mut hasher, "rejection_reason", reason);
    }
    hash_field(
        &mut hasher,
        "recorded_at",
        &canonical_recorded_at(recorded_at),
    );
    format!("{:x}", hasher.finalize())
}

fn consolidation_event_id(
    orb_id: &str,
    replay_trace_hash: &str,
    decision: ConsolidationDecision,
) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "orb_id", orb_id);
    hash_field(&mut hasher, "replay_trace_hash", replay_trace_hash);
    hash_field(&mut hasher, "decision", &format!("{decision:?}"));
    format!("consolidation-{:x}", hasher.finalize())
}

fn cluster_evolution_trace_hash(
    request: &ClusterEvolutionRequest,
    parent_orb_refs: &[ParentOrbStateRef],
    decision: ConsolidationDecision,
    rejection_reason: Option<&str>,
    recorded_at: &DateTime<Utc>,
) -> String {
    let mut hasher = Sha256::new();
    hash_field(
        &mut hasher,
        "schema_version",
        CLUSTER_EVOLUTION_SCHEMA_VERSION,
    );
    hash_field(
        &mut hasher,
        "operation",
        request.operation.as_operation_str(),
    );
    for parent_id in sorted_unique(request.parent_orb_ids.clone()) {
        hash_field(&mut hasher, "parent_orb_id", &parent_id);
    }
    for parent_ref in sorted_unique_parent_refs(parent_orb_refs.to_vec()) {
        hash_field(&mut hasher, "parent_ref_orb_id", &parent_ref.orb_id);
        hash_field(
            &mut hasher,
            "parent_ref_accepted_event_id",
            &parent_ref.accepted_event_id,
        );
        hash_field(
            &mut hasher,
            "parent_ref_lineage_hash",
            &parent_ref.lineage_hash,
        );
        hash_field(
            &mut hasher,
            "parent_ref_source_event_ancestry_hash",
            &parent_ref.source_event_ancestry_hash,
        );
    }
    for child_id in sorted_unique(request.child_orb_ids.clone()) {
        hash_field(&mut hasher, "child_orb_id", &child_id);
    }
    hash_field(&mut hasher, "cause", &request.cause);
    for metric_ref in sorted_unique_metric_refs(request.metric_evidence_refs.clone()) {
        hash_field(&mut hasher, "metric_id", &metric_ref.metric_id);
        hash_field(&mut hasher, "metric_event_id", &metric_ref.event_id);
        hash_field(&mut hasher, "metric_event_hash", &metric_ref.event_hash);
    }
    hash_field(&mut hasher, "decision", &format!("{decision:?}"));
    if let Some(reason) = rejection_reason {
        hash_field(&mut hasher, "rejection_reason", reason);
    }
    hash_field(
        &mut hasher,
        "recorded_at",
        &canonical_recorded_at(recorded_at),
    );
    format!("{:x}", hasher.finalize())
}

fn string_list_hash(label: &str, values: &[String]) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "hash_type", label);
    for value in sorted_unique(values.to_vec()) {
        hash_field(&mut hasher, "value", &value);
    }
    format!("{:x}", hasher.finalize())
}

fn source_event_refs_hash(values: &[SourceEventRef]) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "hash_type", "source_event_refs");
    for reference in sorted_unique_event_refs(values.to_vec()) {
        hash_field(&mut hasher, "event_id", &reference.event_id);
        hash_field(&mut hasher, "event_hash", &reference.event_hash);
    }
    format!("{:x}", hasher.finalize())
}

fn cluster_evolution_event_id(
    operation: ClusterEvolutionOperation,
    parent_orb_ids: &[String],
    child_orb_ids: &[String],
    replay_trace_hash: &str,
    decision: ConsolidationDecision,
) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "operation", operation.as_operation_str());
    for parent_id in sorted_unique(parent_orb_ids.to_vec()) {
        hash_field(&mut hasher, "parent_orb_id", &parent_id);
    }
    for child_id in sorted_unique(child_orb_ids.to_vec()) {
        hash_field(&mut hasher, "child_orb_id", &child_id);
    }
    hash_field(&mut hasher, "replay_trace_hash", replay_trace_hash);
    hash_field(&mut hasher, "decision", &format!("{decision:?}"));
    format!("cluster-evolution-{:x}", hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, label: &str, value: &str) {
    hasher.update(label.as_bytes());
    hasher.update([0]);
    hasher.update(value.len().to_string().as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hasher.update([0xff]);
}

fn canonical_recorded_at(recorded_at: &DateTime<Utc>) -> String {
    recorded_at.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_ref() -> SourceEventRef {
        SourceEventRef::new("event-1", "a".repeat(64))
    }

    fn metric_ref(metric_id: &str) -> MetricEvidenceRef {
        MetricEvidenceRef::new(metric_id, "metric-event-1", "b".repeat(64))
    }

    fn request(score: f64) -> ClusterFormationRequest {
        ClusterFormationRequest::new(
            "orb-1",
            vec!["node-2".to_string(), "node-1".to_string()],
            vec!["tether-1".to_string()],
            vec![source_ref()],
            "shared-character-cohesion",
            score,
        )
    }

    #[test]
    fn accepted_orb_formation_replays_into_registry() {
        let event = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());

        assert_eq!(event.decision, ConsolidationDecision::Accepted);
        assert_eq!(event.rejection_reason, None);
        assert_eq!(event.source_node_ids, vec!["node-1", "node-2"]);
        let replayed = replay_consolidation_events(&[event.clone()]).unwrap();

        assert_eq!(replayed.clusters().len(), 1);
        assert_eq!(
            replayed.clusters()["orb-1"].accepted_event_id,
            event.event_id
        );
    }

    #[test]
    fn below_threshold_orb_formation_logs_rejection_without_orb() {
        let event = form_memory_cluster(request(0.2), &ClusterFormationPolicy::default());

        assert_eq!(event.decision, ConsolidationDecision::Rejected);
        assert!(event
            .rejection_reason
            .as_deref()
            .unwrap()
            .contains("below threshold"));
        let replayed = replay_consolidation_events(&[event.clone()]).unwrap();

        assert!(replayed.clusters().is_empty());
        assert_eq!(replayed.rejected_event_ids(), &[event.event_id]);
    }

    #[test]
    fn blank_cohesion_rule_id_rejects_validation() {
        let mut event = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        event.cohesion_rule_id = String::new();

        let error = validate_consolidation_event(&event).unwrap_err();

        assert!(error.to_string().contains("cohesion rule id"));
    }

    #[test]
    fn recorded_at_tampering_rejects_validation() {
        let mut event = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        event.recorded_at += chrono::TimeDelta::seconds(1);

        let error = validate_consolidation_event(&event).unwrap_err();

        assert!(error.to_string().contains("replay trace hash mismatch"));
    }

    #[test]
    fn consolidation_event_jsonl_round_trips_and_replays() {
        let path = std::env::temp_dir().join(format!(
            "luna-consolidation-{}.jsonl",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let accepted = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        let rejected = form_memory_cluster(request(0.1), &ClusterFormationPolicy::default());

        append_consolidation_event_jsonl(&path, &accepted).unwrap();
        append_consolidation_event_jsonl(&path, &rejected).unwrap();
        let loaded = load_consolidation_events_jsonl(&path).unwrap();
        let replayed = replay_consolidation_events(&loaded).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(replayed.clusters().len(), 1);
        assert_eq!(replayed.rejected_event_ids().len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn accepted_split_retires_parent_and_preserves_lineage() {
        let formation = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        let mut registry = replay_consolidation_events(&[formation.clone()]).unwrap();
        let split = evolve_cluster(
            ClusterEvolutionRequest::split(
                "orb-1",
                vec!["orb-1-a".to_string(), "orb-1-b".to_string()],
                "splinter pressure from falling precision",
                vec![metric_ref("sentinel:splinter_pressure:run-1")],
            ),
            &registry,
        );

        registry.apply_evolution_event(&split).unwrap();

        assert_eq!(split.decision, ConsolidationDecision::Accepted);
        assert!(!registry.clusters().contains_key("orb-1"));
        assert!(registry.retired_clusters().contains_key("orb-1"));
        assert_eq!(registry.clusters().len(), 2);
        assert!(registry.clusters()["orb-1-a"]
            .lineage_event_ids
            .contains(&formation.event_id));
        assert!(registry.clusters()["orb-1-a"]
            .lineage_event_ids
            .contains(&split.event_id));
        assert_eq!(registry.evolution_event_ids(), &[split.event_id]);
    }

    #[test]
    fn retired_orb_id_cannot_be_reaccepted() {
        let formation = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        let mut registry = replay_consolidation_events(&[formation]).unwrap();
        let split = evolve_cluster(
            ClusterEvolutionRequest::split(
                "orb-1",
                vec!["orb-1-a".to_string(), "orb-1-b".to_string()],
                "splinter pressure from falling precision",
                vec![metric_ref("sentinel:splinter_pressure:run-1")],
            ),
            &registry,
        );
        registry.apply_evolution_event(&split).unwrap();
        let replacement = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());

        let error = registry
            .apply_consolidation_event(&replacement)
            .unwrap_err();

        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn accepted_merge_unions_parent_lineage() {
        let first = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        let mut second_request = request(0.9);
        second_request.orb_id = "orb-2".to_string();
        let second = form_memory_cluster(second_request, &ClusterFormationPolicy::default());
        let mut registry = replay_consolidation_events(&[first.clone(), second.clone()]).unwrap();
        let merge = evolve_cluster(
            ClusterEvolutionRequest::merge(
                vec!["orb-1".to_string(), "orb-2".to_string()],
                "orb-merged",
                "compatible source evidence",
                vec![metric_ref("metric:compatibility:0.94")],
            ),
            &registry,
        );
        registry.apply_evolution_event(&merge).unwrap();
        assert_eq!(registry.clusters().len(), 1);
        assert!(registry.retired_clusters().contains_key("orb-1"));
        assert!(registry.retired_clusters().contains_key("orb-2"));
        let merged = &registry.clusters()["orb-merged"];
        assert!(merged.lineage_event_ids.contains(&first.event_id));
        assert!(merged.lineage_event_ids.contains(&second.event_id));
        assert!(merged.lineage_event_ids.contains(&merge.event_id));
    }

    #[test]
    fn contradiction_pressure_evidence_allows_split_gate() {
        let formation = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        let mut registry = replay_consolidation_events(&[formation]).unwrap();
        let split = evolve_cluster(
            ClusterEvolutionRequest::split(
                "orb-1",
                vec!["orb-1-a".to_string(), "orb-1-b".to_string()],
                "contradiction pressure from incompatible single-valued assertions",
                vec![metric_ref("sentinel:contradiction_pressure:run-1")],
            ),
            &registry,
        );

        registry.apply_evolution_event(&split).unwrap();

        assert_eq!(split.decision, ConsolidationDecision::Accepted);
    }

    #[test]
    fn split_without_pressure_evidence_is_rejected_before_retiring_parent() {
        let formation = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        let mut registry = replay_consolidation_events(&[formation]).unwrap();
        let split = evolve_cluster(
            ClusterEvolutionRequest::split(
                "orb-1",
                vec!["orb-1-a".to_string(), "orb-1-b".to_string()],
                "generic maintenance split",
                vec![metric_ref("metric:compatibility:0.94")],
            ),
            &registry,
        );

        registry.apply_evolution_event(&split).unwrap();

        assert_eq!(split.decision, ConsolidationDecision::Rejected);
        assert!(split
            .rejection_reason
            .as_deref()
            .unwrap()
            .contains("contradiction-pressure or splinter-pressure evidence"));
    }

    #[test]
    fn accepted_split_without_pressure_evidence_rejects_at_validation_boundary() {
        let formation = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        let registry = replay_consolidation_events(&[formation]).unwrap();
        let mut split = evolve_cluster(
            ClusterEvolutionRequest::split(
                "orb-1",
                vec!["orb-1-a".to_string(), "orb-1-b".to_string()],
                "generic maintenance split",
                vec![metric_ref("sentinel:splinter_pressure:run-1")],
            ),
            &registry,
        );
        split.metric_evidence_refs = vec![metric_ref("metric:compatibility:0.94")];
        let request = ClusterEvolutionRequest {
            operation: ClusterEvolutionOperation::Split,
            parent_orb_ids: split.parent_orb_ids.clone(),
            child_orb_ids: split.child_orb_ids.clone(),
            cause: split.cause.clone(),
            metric_evidence_refs: split.metric_evidence_refs.clone(),
        };
        split.replay_trace_hash = cluster_evolution_trace_hash(
            &request,
            &split.parent_orb_refs,
            split.decision,
            split.rejection_reason.as_deref(),
            &split.recorded_at,
        );
        split.event_id = cluster_evolution_event_id(
            ClusterEvolutionOperation::Split,
            &split.parent_orb_ids,
            &split.child_orb_ids,
            &split.replay_trace_hash,
            split.decision,
        );

        let error = validate_cluster_evolution_event(&split).unwrap_err();

        assert!(error
            .to_string()
            .contains("accepted orb split requires contradiction-pressure"));
    }

    #[test]
    fn split_of_missing_parent_is_rejected_without_mutating_registry() {
        let formation = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        let mut registry = replay_consolidation_events(&[formation]).unwrap();
        let split = evolve_cluster(
            ClusterEvolutionRequest::split(
                "missing-orb",
                vec!["child-a".to_string(), "child-b".to_string()],
                "splinter pressure",
                vec![metric_ref("sentinel:splinter_pressure:run-2")],
            ),
            &registry,
        );
        let _before = registry.clone();

        registry.apply_evolution_event(&split).unwrap();

        assert_eq!(split.decision, ConsolidationDecision::Rejected);
        assert_eq!(registry.rejected_event_ids(), &[split.event_id]);
    }

    #[test]
    fn forged_evolution_trace_rejects_before_mutation() {
        let formation = form_memory_cluster(request(0.91), &ClusterFormationPolicy::default());
        let mut registry = replay_consolidation_events(&[formation]).unwrap();
        let mut split = evolve_cluster(
            ClusterEvolutionRequest::split(
                "orb-1",
                vec!["orb-1-a".to_string(), "orb-1-b".to_string()],
                "splinter pressure",
                vec![metric_ref("sentinel:splinter_pressure:run-3")],
            ),
            &registry,
        );
        split.cause = "forged cause".to_string();
        let before = registry.clone();

        let error = registry.apply_evolution_event(&split).unwrap_err();

        assert!(error
            .to_string()
            .contains("cluster evolution replay trace hash mismatch"));
        assert_eq!(registry, before);
    }
}
// Placeholder
