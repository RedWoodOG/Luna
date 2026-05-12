use crate::{EntityMemoryGroup, MemoryClaim, MemoryState};
use chrono::{DateTime, Utc};
use luna_core::{
    AssertionConfidenceTier, AssertionLifecycleStatus, LunaError, MemoryNodeKind, MemoryProvenance,
    MemoryRelationKind, Result, TopologyBridgeCommitted,
};
use luna_events::{stable_stored_event_hash, LunaEvent, StoredEvent};
use luna_ledger::{
    EventPayload, EventSource as LedgerEventSource, NodeCreated, NodeKind, RawEvent, RawEventDraft,
    TetherCreated, TetherKind, TopologyMutation,
};
use luna_cluster::{
    form_memory_cluster_at, ConsolidationDecision, ClusterFormationPolicy, ClusterFormationRequest,
    SourceEventRef,
};
use luna_replay::ReplayedTopology;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TopologyBridge {
    pub node_records: Vec<TopologyNodeRecord>,
    pub tether_records: Vec<TopologyTetherRecord>,
    pub orb_refs: Vec<TopologyOrbRef>,
}

impl TopologyBridge {
    pub fn from_memory_state(memory: &MemoryState) -> Self {
        bridge_memory_to_topology(memory)
    }

    pub fn from_runtime_events(events: &[StoredEvent]) -> Result<Self> {
        bridge_runtime_events_to_topology(events)
    }

    pub fn to_commit_event(&self) -> TopologyBridgeCommitted {
        topology_commit_from_bridge(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyNodeRecord {
    pub topology_ref: String,
    pub runtime_entity_ref: String,
    pub label: String,
    pub kind: String,
    pub claim_refs: Vec<TopologyClaimRef>,
    pub provenance: Vec<MemoryProvenance>,
    #[serde(default)]
    pub source_event_refs: Vec<TopologySourceEventRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyClaimRef {
    pub topology_ref: String,
    pub assertion_key: String,
    pub confidence_tier: AssertionConfidenceTier,
    pub lifecycle_status: AssertionLifecycleStatus,
    pub provenance: Vec<MemoryProvenance>,
    #[serde(default)]
    pub source_event_refs: Vec<TopologySourceEventRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyTetherRecord {
    pub topology_ref: String,
    pub source_ref: String,
    pub target_ref: String,
    pub relation: MemoryRelationKind,
    pub confidence_tier: AssertionConfidenceTier,
    pub strength: f32,
    pub provenance: Vec<MemoryProvenance>,
    #[serde(default)]
    pub source_event_refs: Vec<TopologySourceEventRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TopologyOrbRef {
    pub system_root: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TopologySourceEventRef {
    pub event_id: Uuid,
    pub event_hash: String,
    #[serde(default)]
    pub episode_id: Option<Uuid>,
    #[serde(default)]
    pub turn_id: Option<Uuid>,
    #[serde(default)]
    pub assertion_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeTopologyLedgerCommit {
    pub bridge: TopologyBridge,
    pub topology: ReplayedTopology,
    pub committed_node_ids: Vec<String>,
    pub committed_tether_ids: Vec<String>,
    pub accepted_orb_ids: Vec<String>,
    pub rejected_orb_ids: Vec<String>,
    pub ledger_event_count: usize,
    pub ledger_event_hash: String,
    pub ledger_events_json: Vec<String>,
}

pub fn bridge_memory_to_topology(memory: &MemoryState) -> TopologyBridge {
    bridge_memory_to_topology_with_sources(memory, &SourceEventIndex::default())
}

pub fn bridge_runtime_events_to_topology(events: &[StoredEvent]) -> Result<TopologyBridge> {
    let episodes = luna_store::rebuild_episodes(events)?;
    let memory = MemoryState::from_episodes(&episodes);
    let source_index = SourceEventIndex::from_events(events);
    Ok(bridge_memory_to_topology_with_sources(
        &memory,
        &source_index,
    ))
}

pub fn topology_commit_from_bridge(bridge: &TopologyBridge) -> TopologyBridgeCommitted {
    let mut source_event_hashes = bridge
        .node_records
        .iter()
        .flat_map(|record| {
            record.source_event_refs.iter().chain(
                record
                    .claim_refs
                    .iter()
                    .flat_map(|claim| claim.source_event_refs.iter()),
            )
        })
        .chain(
            bridge
                .tether_records
                .iter()
                .flat_map(|record| record.source_event_refs.iter()),
        )
        .map(|source| source.event_hash.clone())
        .filter(|hash| !hash.trim().is_empty())
        .collect::<Vec<_>>();
    source_event_hashes.sort();
    source_event_hashes.dedup();

    TopologyBridgeCommitted {
        node_refs: bridge
            .node_records
            .iter()
            .map(|record| record.topology_ref.clone())
            .collect(),
        tether_refs: bridge
            .tether_records
            .iter()
            .map(|record| record.topology_ref.clone())
            .collect(),
        source_event_hashes,
        orb_refs: bridge
            .orb_refs
            .iter()
            .map(|orb| format!("{}<-{}", orb.system_root, orb.source_ref))
            .collect(),
        accepted_orb_refs: Vec::new(),
        rejected_orb_refs: Vec::new(),
        ledger_event_count: 0,
        ledger_event_hash: String::new(),
        ledger_events_json: Vec::new(),
    }
}

pub fn topology_commit_from_runtime_ledger_commit(
    commit: &RuntimeTopologyLedgerCommit,
) -> Result<TopologyBridgeCommitted> {
    let mut persisted = topology_commit_from_bridge(&commit.bridge);
    persisted.accepted_orb_refs = commit.accepted_orb_ids.clone();
    persisted.rejected_orb_refs = commit.rejected_orb_ids.clone();
    persisted.ledger_event_count = commit.ledger_event_count;
    persisted.ledger_event_hash = commit.ledger_event_hash.clone();
    persisted.ledger_events_json = commit.ledger_events_json.clone();
    Ok(persisted)
}

pub fn commit_runtime_events_to_topology_ledger(
    events: &[StoredEvent],
) -> Result<RuntimeTopologyLedgerCommit> {
    let bridge = bridge_runtime_events_to_topology(events)?;
    let raw_events = raw_events_for_runtime_events(events)?;
    let mut topology = ReplayedTopology::default();
    for raw_event in raw_events.values() {
        topology.record_raw_event(raw_event.clone())?;
    }

    let mut committed_node_ids = Vec::new();
    let mut node_sources = BTreeMap::<String, TopologySourceEventRef>::new();
    for record in &bridge.node_records {
        if let Some(source) = source_ref_for_node_record(record) {
            node_sources.insert(record.topology_ref.clone(), source.clone());
        }
    }
    for tether in &bridge.tether_records {
        if let Some(source) = tether.source_event_refs.first() {
            node_sources
                .entry(topology_node_ref_for_runtime_ref(&tether.source_ref))
                .or_insert_with(|| source.clone());
            node_sources
                .entry(topology_node_ref_for_runtime_ref(&tether.target_ref))
                .or_insert_with(|| source.clone());
        }
    }

    for (node_id, source) in &node_sources {
        let raw_event = raw_events.get(&source.event_id).ok_or_else(|| {
            LunaError::new(format!(
                "topology node {node_id} references missing runtime source event {}",
                source.event_id
            ))
        })?;
        let label = bridge
            .node_records
            .iter()
            .find(|record| record.topology_ref == *node_id)
            .map(|record| record.label.clone())
            .unwrap_or_else(|| node_id.trim_start_matches("node:").to_string());
        topology
            .commit(TopologyMutation::NodeCreated(NodeCreated::new(
                node_id.clone(),
                NodeKind::Evidence,
                label,
                raw_event.id.clone(),
                raw_event.hash.clone(),
            )))
            .map_err(|err| LunaError::new(err.to_string()))?;
        committed_node_ids.push(node_id.clone());
    }

    let mut committed_tether_ids = Vec::new();
    for record in &bridge.tether_records {
        let source = record.source_event_refs.first().ok_or_else(|| {
            LunaError::new(format!(
                "topology tether {} has no runtime source event ref",
                record.topology_ref
            ))
        })?;
        let raw_event = raw_events.get(&source.event_id).ok_or_else(|| {
            LunaError::new(format!(
                "topology tether {} references missing runtime source event {}",
                record.topology_ref, source.event_id
            ))
        })?;
        topology
            .commit(TopologyMutation::TetherCreated(TetherCreated::new(
                record.topology_ref.clone(),
                topology_node_ref_for_runtime_ref(&record.source_ref),
                topology_node_ref_for_runtime_ref(&record.target_ref),
                Some(TetherKind::EvidenceFor),
                TetherKind::SupportedBy,
                raw_event.id.clone(),
                raw_event.hash.clone(),
            )))
            .map_err(|err| LunaError::new(err.to_string()))?;
        committed_tether_ids.push(record.topology_ref.clone());
    }

    let recorded_at = runtime_orb_recorded_at(events);
    let (accepted_orb_ids, rejected_orb_ids) =
        commit_runtime_dense_orbs(&bridge, &raw_events, &mut topology, recorded_at)?;
    let ledger_events_json = serialized_ledger_events(&topology)?;
    let ledger_event_count = ledger_events_json.len();
    let ledger_event_hash = ledger_events_hash(&ledger_events_json);

    Ok(RuntimeTopologyLedgerCommit {
        bridge,
        topology,
        committed_node_ids,
        committed_tether_ids,
        accepted_orb_ids,
        rejected_orb_ids,
        ledger_event_count,
        ledger_event_hash,
        ledger_events_json,
    })
}

pub fn topology_node_ref_for_runtime_ref(runtime_ref: &str) -> String {
    format!("node:{runtime_ref}")
}

pub fn ledger_events_from_persisted_json(
    events_json: &[String],
) -> Result<Vec<luna_ledger::LedgerEvent>> {
    events_json
        .iter()
        .map(|event_json| {
            serde_json::from_str(event_json).map_err(|err| LunaError::new(err.to_string()))
        })
        .collect()
}

fn commit_runtime_dense_orbs(
    bridge: &TopologyBridge,
    raw_events: &BTreeMap<Uuid, RawEvent>,
    topology: &mut ReplayedTopology,
    recorded_at: DateTime<Utc>,
) -> Result<(Vec<String>, Vec<String>)> {
    let policy = ClusterFormationPolicy::default();
    let mut accepted_orb_ids = Vec::new();
    let mut rejected_orb_ids = Vec::new();
    for record in &bridge.node_records {
        let request = dense_orb_request_for_record(record, bridge, raw_events);
        let event = form_memory_cluster_at(request, &policy, recorded_at);
        let orb_id = event.orb_id.clone();
        let decision = event.decision;
        topology.record_consolidation_event(event)?;
        match decision {
            ConsolidationDecision::Accepted => accepted_orb_ids.push(orb_id),
            ConsolidationDecision::Rejected => rejected_orb_ids.push(orb_id),
        }
    }
    accepted_orb_ids.sort();
    rejected_orb_ids.sort();
    Ok((accepted_orb_ids, rejected_orb_ids))
}

fn dense_orb_request_for_record(
    record: &TopologyNodeRecord,
    bridge: &TopologyBridge,
    raw_events: &BTreeMap<Uuid, RawEvent>,
) -> ClusterFormationRequest {
    let mut source_node_ids = BTreeSet::from([record.topology_ref.clone()]);
    let mut source_tether_ids = BTreeSet::new();
    let mut source_event_refs = BTreeMap::new();
    if let Some(source) = source_ref_for_node_record(record) {
        insert_orb_source_ref(&mut source_event_refs, source, raw_events);
    }
    for tether in bridge
        .tether_records
        .iter()
        .filter(|tether| tether.source_ref == record.runtime_entity_ref)
    {
        let source_node_id = topology_node_ref_for_runtime_ref(&tether.source_ref);
        let target_node_id = topology_node_ref_for_runtime_ref(&tether.target_ref);
        source_node_ids.insert(source_node_id.clone());
        source_node_ids.insert(target_node_id.clone());
        insert_source_ref_for_topology_node(
            &source_node_id,
            bridge,
            &mut source_event_refs,
            raw_events,
        );
        insert_source_ref_for_topology_node(
            &target_node_id,
            bridge,
            &mut source_event_refs,
            raw_events,
        );
        source_tether_ids.insert(tether.topology_ref.clone());
        if let Some(source) = tether.source_event_refs.first() {
            insert_orb_source_ref(&mut source_event_refs, source, raw_events);
        }
    }

    ClusterFormationRequest::new(
        runtime_dense_orb_id(&record.runtime_entity_ref),
        source_node_ids.into_iter().collect(),
        source_tether_ids.into_iter().collect(),
        source_event_refs.into_values().collect(),
        "runtime-entity-cluster-v1",
        0.91,
    )
}

fn insert_source_ref_for_topology_node(
    topology_node_id: &str,
    bridge: &TopologyBridge,
    source_event_refs: &mut BTreeMap<(String, String), SourceEventRef>,
    raw_events: &BTreeMap<Uuid, RawEvent>,
) {
    if let Some(record) = bridge
        .node_records
        .iter()
        .find(|record| record.topology_ref == topology_node_id)
    {
        if let Some(source) = source_ref_for_node_record(record) {
            insert_orb_source_ref(source_event_refs, source, raw_events);
        }
    }
}

fn insert_orb_source_ref(
    source_event_refs: &mut BTreeMap<(String, String), SourceEventRef>,
    source: &TopologySourceEventRef,
    raw_events: &BTreeMap<Uuid, RawEvent>,
) {
    let source_ref = orb_source_ref(source, raw_events);
    source_event_refs
        .entry((source_ref.event_id.clone(), source_ref.event_hash.clone()))
        .or_insert(source_ref);
}

fn orb_source_ref(
    source: &TopologySourceEventRef,
    raw_events: &BTreeMap<Uuid, RawEvent>,
) -> SourceEventRef {
    let event_hash = raw_events
        .get(&source.event_id)
        .map(|raw_event| raw_event.hash.clone())
        .unwrap_or_else(|| source.event_hash.clone());
    SourceEventRef::new(source.event_id.to_string(), event_hash)
}

fn runtime_dense_orb_id(runtime_entity_ref: &str) -> String {
    let mut normalized = runtime_entity_ref
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == ':' || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while normalized.contains("__") {
        normalized = normalized.replace("__", "_");
    }
    format!("runtime:{normalized}")
}

fn runtime_orb_recorded_at(events: &[StoredEvent]) -> DateTime<Utc> {
    events
        .iter()
        .map(|event| event.timestamp)
        .max()
        .unwrap_or_else(Utc::now)
}

fn serialized_ledger_events(topology: &ReplayedTopology) -> Result<Vec<String>> {
    topology
        .ledger()
        .events()
        .iter()
        .map(|event| serde_json::to_string(event).map_err(|err| LunaError::new(err.to_string())))
        .collect()
}

pub fn ledger_events_hash(events_json: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"luna.runtime.topology-ledger.v1");
    for event_json in events_json {
        let canonical_event_json = canonical_ledger_event_json(event_json);
        hasher.update([0]);
        hasher.update(canonical_event_json.len().to_string().as_bytes());
        hasher.update([0]);
        hasher.update(canonical_event_json.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn canonical_ledger_event_json(event_json: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(event_json) else {
        return event_json.to_string();
    };
    if value.get("type").and_then(|event_type| event_type.as_str()) == Some("raw_event_recorded") {
        if let Some(data) = value.get_mut("data").and_then(|data| data.as_object_mut()) {
            data.remove("recorded_at");
        }
    }
    serde_json::to_string(&value).unwrap_or_else(|_| event_json.to_string())
}

fn bridge_memory_to_topology_with_sources(
    memory: &MemoryState,
    source_index: &SourceEventIndex,
) -> TopologyBridge {
    let node_index = memory
        .map
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();

    let mut node_records = memory
        .entity_groups
        .iter()
        .map(|group| node_record_for_group(group, &node_index, source_index))
        .collect::<Vec<_>>();
    node_records.sort_by(|left, right| left.topology_ref.cmp(&right.topology_ref));

    let node_refs = node_records
        .iter()
        .map(|record| record.runtime_entity_ref.clone())
        .collect::<BTreeSet<_>>();

    let mut tether_records = memory
        .map
        .edges
        .iter()
        .filter(|edge| {
            !is_system_provenance(&edge.provenance)
                && node_refs.contains(&edge.source)
                && !is_system_node(&edge.target, &node_index)
        })
        .map(|edge| TopologyTetherRecord {
            topology_ref: format!("tether:{}>{:?}>{}", edge.source, edge.relation, edge.target),
            source_ref: edge.source.clone(),
            target_ref: edge.target.clone(),
            relation: edge.relation,
            confidence_tier: edge.confidence_tier,
            strength: edge.strength,
            provenance: edge.provenance.clone(),
            source_event_refs: source_index.refs_for_provenance(&edge.provenance),
        })
        .collect::<Vec<_>>();
    tether_records.sort_by(|left, right| left.topology_ref.cmp(&right.topology_ref));

    let mut orb_refs = BTreeSet::new();
    for node in &memory.map.nodes {
        collect_orb_refs(&mut orb_refs, &node.id, &node.provenance);
    }
    for edge in &memory.map.edges {
        collect_orb_refs(&mut orb_refs, &edge.source, &edge.provenance);
    }

    TopologyBridge {
        node_records,
        tether_records,
        orb_refs: orb_refs.into_iter().collect(),
    }
}

fn node_record_for_group(
    group: &EntityMemoryGroup,
    node_index: &BTreeMap<&str, &luna_core::MemoryNode>,
    source_index: &SourceEventIndex,
) -> TopologyNodeRecord {
    let runtime_entity_ref = runtime_entity_ref(group);
    let mut provenance = node_index
        .get(runtime_entity_ref.as_str())
        .map(|node| node.provenance.clone())
        .unwrap_or_default();
    let claim_refs = group
        .claims
        .iter()
        .map(|claim| {
            let claim_provenance = claim_provenance(claim, node_index);
            provenance.extend(claim_provenance.clone());
            TopologyClaimRef {
                topology_ref: format!("claim:{}", claim.key),
                assertion_key: claim.key.clone(),
                confidence_tier: claim.status,
                lifecycle_status: claim.lifecycle_status,
                provenance: claim_provenance,
                source_event_refs: source_index.refs_for_assertion(&claim.key),
            }
        })
        .collect::<Vec<_>>();
    let source_event_refs = source_index.refs_for_provenance(&provenance);

    TopologyNodeRecord {
        topology_ref: format!("node:{runtime_entity_ref}"),
        runtime_entity_ref,
        label: group.label.clone(),
        kind: group.kind.clone(),
        claim_refs,
        provenance: dedupe_provenance(provenance),
        source_event_refs,
    }
}

fn runtime_entity_ref(group: &EntityMemoryGroup) -> String {
    if group.id == "self" {
        "user:self".to_string()
    } else {
        group.id.clone()
    }
}

fn claim_provenance(
    claim: &MemoryClaim,
    node_index: &BTreeMap<&str, &luna_core::MemoryNode>,
) -> Vec<MemoryProvenance> {
    node_index
        .get(claim_node_ref(claim).as_str())
        .map(|node| {
            node.provenance
                .iter()
                .filter(|provenance| provenance.system_root.is_none())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn claim_node_ref(claim: &MemoryClaim) -> String {
    format!(
        "{}:{}:{}",
        claim.domain,
        claim.kind,
        claim.value.replace(' ', "_")
    )
}

fn is_system_node(node_id: &str, node_index: &BTreeMap<&str, &luna_core::MemoryNode>) -> bool {
    node_index
        .get(node_id)
        .map(|node| node.kind == MemoryNodeKind::SystemKernel || is_system_provenance(&node.provenance))
        .unwrap_or(false)
}

fn is_system_provenance(provenance: &[MemoryProvenance]) -> bool {
    !provenance.is_empty()
        && provenance
            .iter()
            .all(|provenance| provenance.system_root.is_some())
}

fn collect_orb_refs(
    orb_refs: &mut BTreeSet<TopologyOrbRef>,
    source_ref: &str,
    provenance: &[MemoryProvenance],
) {
    for system_root in provenance
        .iter()
        .filter_map(|provenance| provenance.system_root.as_ref())
    {
        orb_refs.insert(TopologyOrbRef {
            system_root: system_root.clone(),
            source_ref: source_ref.to_string(),
        });
    }
}

fn dedupe_provenance(provenance: Vec<MemoryProvenance>) -> Vec<MemoryProvenance> {
    let mut seen = BTreeSet::new();
    provenance
        .into_iter()
        .filter(|provenance| {
            seen.insert(format!(
                "{:?}|{:?}|{:?}|{:?}",
                provenance.episode_id,
                provenance.turn_id,
                provenance.assertion_key,
                provenance.system_root
            ))
        })
        .collect()
}

fn raw_events_for_runtime_events(events: &[StoredEvent]) -> Result<BTreeMap<Uuid, RawEvent>> {
    let mut raw_events = BTreeMap::new();
    for event in events {
        if matches!(&event.payload, LunaEvent::TopologyBridgeCommitted(_)) {
            continue;
        }
        let raw = RawEvent::from_draft(RawEventDraft::new(
            event.event_id.to_string(),
            ledger_event_source(event.source),
            EventPayload::Text(runtime_event_payload(event)?),
        ));
        raw_events.insert(event.event_id, raw);
    }
    Ok(raw_events)
}

fn runtime_event_payload(event: &StoredEvent) -> Result<String> {
    let mut canonical = event.clone();
    if canonical.event_hash.is_none() {
        canonical.event_hash = Some(stable_stored_event_hash(&canonical)?);
    }
    serde_json::to_string(&canonical).map_err(|err| LunaError::new(err.to_string()))
}

fn ledger_event_source(source: luna_core::EventSource) -> LedgerEventSource {
    match source {
        luna_core::EventSource::User => LedgerEventSource::User,
        luna_core::EventSource::Assistant => LedgerEventSource::Assistant,
        luna_core::EventSource::System
        | luna_core::EventSource::HeuristicExtractor
        | luna_core::EventSource::EmbeddingExtractor
        | luna_core::EventSource::ClassifierExtractor
        | luna_core::EventSource::RecallEngine
        | luna_core::EventSource::BenchmarkOracle => LedgerEventSource::System,
    }
}

fn source_ref_for_node_record(record: &TopologyNodeRecord) -> Option<&TopologySourceEventRef> {
    record.source_event_refs.first().or_else(|| {
        record
            .claim_refs
            .iter()
            .flat_map(|claim| claim.source_event_refs.iter())
            .next()
    })
}

#[derive(Debug, Clone, Default)]
struct SourceEventIndex {
    by_assertion_key: BTreeMap<String, Vec<TopologySourceEventRef>>,
}

impl SourceEventIndex {
    fn from_events(events: &[StoredEvent]) -> Self {
        let mut by_assertion_key = BTreeMap::<String, Vec<TopologySourceEventRef>>::new();
        for event in events {
            for assertion_key in assertion_keys_for_event(event) {
                by_assertion_key
                    .entry(assertion_key.clone())
                    .or_default()
                    .push(TopologySourceEventRef {
                        event_id: event.event_id,
                        event_hash: event
                            .event_hash
                            .clone()
                            .unwrap_or_else(|| stable_stored_event_hash(event).unwrap_or_default()),
                        episode_id: event.episode_id,
                        turn_id: event.turn_id,
                        assertion_key: Some(assertion_key),
                    });
            }
        }
        for refs in by_assertion_key.values_mut() {
            refs.sort();
            refs.dedup();
        }
        Self { by_assertion_key }
    }

    fn refs_for_assertion(&self, assertion_key: &str) -> Vec<TopologySourceEventRef> {
        self.by_assertion_key
            .get(assertion_key)
            .cloned()
            .unwrap_or_default()
    }

    fn refs_for_provenance(&self, provenance: &[MemoryProvenance]) -> Vec<TopologySourceEventRef> {
        let mut refs = provenance
            .iter()
            .filter_map(|provenance| provenance.assertion_key.as_deref())
            .flat_map(|assertion_key| self.refs_for_assertion(assertion_key))
            .collect::<Vec<_>>();
        refs.sort();
        refs.dedup();
        refs
    }
}

fn assertion_keys_for_event(event: &StoredEvent) -> Vec<String> {
    match &event.payload {
        LunaEvent::AssertionExtracted(payload) => vec![payload.assertion.key()],
        LunaEvent::EpisodeCreated(payload) => vec![payload.assertion.key()],
        LunaEvent::EpisodeReinforced(payload) => vec![payload.assertion.key()],
        LunaEvent::AssertionCorrected(payload) => vec![payload.new_assertion.key()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luna_core::{MemoryEdge, MemoryMap, MemoryNode};
    use luna_events::JsonlEventLog;
    use luna_extract::FusedExtractor;
    use uuid::Uuid;

    fn provenance(assertion_key: &str) -> Vec<MemoryProvenance> {
        vec![MemoryProvenance {
            episode_id: Some(Uuid::new_v4()),
            turn_id: Some(Uuid::new_v4()),
            assertion_key: Some(assertion_key.to_string()),
            system_root: None,
            lifecycle_status: None,
        }]
    }

    fn claim(domain: &str, kind: &str, value: &str) -> MemoryClaim {
        let assertion_key = format!("{domain}:{kind}={}", value.replace(' ', "_"));
        MemoryClaim {
            key: assertion_key,
            domain: domain.to_string(),
            kind: kind.to_string(),
            value: value.to_string(),
            status: AssertionConfidenceTier::Confirmed,
            lifecycle_status: AssertionLifecycleStatus::Current,
        }
    }

    #[test]
    fn bridges_runtime_entity_claims_into_topology_records_with_provenance() {
        let location = claim("person", "location", "Chris lives in Iowa");
        let location_provenance = provenance(&location.key);
        let memory = MemoryState {
            claims: vec![location.clone()],
            entity_groups: vec![EntityMemoryGroup {
                id: "person:Chris".to_string(),
                label: "Chris".to_string(),
                kind: "person".to_string(),
                claims: vec![location.clone()],
            }],
            open_questions: Vec::new(),
            map: MemoryMap {
                nodes: vec![
                    MemoryNode {
                        id: "person:Chris".to_string(),
                        label: "Chris".to_string(),
                        kind: MemoryNodeKind::Person,
                        confidence_tier: AssertionConfidenceTier::Confirmed,
                        density: 1.0,
                        activation: 0.0,
                        created_at: None,
                        contradiction_count: 0,
                        provenance: location_provenance.clone(),
                    },
                    MemoryNode {
                        id: "person:location:Chris_lives_in_Iowa".to_string(),
                        label: "Chris lives in Iowa".to_string(),
                        kind: MemoryNodeKind::Person,
                        confidence_tier: AssertionConfidenceTier::Confirmed,
                        density: 1.0,
                        activation: 0.0,
                        created_at: None,
                        contradiction_count: 0,
                        provenance: location_provenance.clone(),
                    },
                    MemoryNode {
                        id: "place:Iowa".to_string(),
                        label: "Iowa".to_string(),
                        kind: MemoryNodeKind::Place,
                        confidence_tier: AssertionConfidenceTier::Confirmed,
                        density: 1.0,
                        activation: 0.0,
                        created_at: None,
                        contradiction_count: 0,
                        provenance: location_provenance.clone(),
                    },
                ],
                edges: vec![MemoryEdge {
                    source: "person:Chris".to_string(),
                    target: "place:Iowa".to_string(),
                    relation: MemoryRelationKind::LocatedIn,
                    confidence_tier: AssertionConfidenceTier::Confirmed,
                    strength: 1.0,
                    activation: 0.0,
                    provenance: location_provenance.clone(),
                }],
            },
        };

        let bridge = bridge_memory_to_topology(&memory);

        let chris = bridge
            .node_records
            .iter()
            .find(|record| record.runtime_entity_ref == "person:Chris")
            .expect("Chris should be bridged as a topology node");
        assert_eq!(chris.claim_refs.len(), 1);
        assert_eq!(chris.claim_refs[0].assertion_key, location.key);
        assert!(chris.claim_refs[0]
            .provenance
            .iter()
            .any(|provenance| provenance.episode_id.is_some()));
        assert!(bridge.tether_records.iter().any(|tether| {
            tether.source_ref == "person:Chris"
                && tether.target_ref == "place:Iowa"
                && tether.relation == MemoryRelationKind::LocatedIn
                && tether
                    .provenance
                    .iter()
                    .any(|provenance| provenance.assertion_key == Some(location.key.clone()))
        }));
    }

    #[test]
    fn keeps_root_orb_receipts_as_system_refs_without_user_claim_leakage() {
        let profession = claim("identity", "profession", "mechanical engineer");
        let profession_provenance = provenance(&profession.key);
        let memory = MemoryState {
            claims: vec![profession.clone()],
            entity_groups: vec![EntityMemoryGroup {
                id: "self".to_string(),
                label: "self".to_string(),
                kind: "self".to_string(),
                claims: vec![profession.clone()],
            }],
            open_questions: Vec::new(),
            map: MemoryMap {
                nodes: vec![
                    MemoryNode {
                        id: "root:luna".to_string(),
                        label: "Luna Root Orb".to_string(),
                        kind: MemoryNodeKind::SystemKernel,
                        confidence_tier: AssertionConfidenceTier::Confirmed,
                        density: 1.0,
                        activation: 0.0,
                        created_at: None,
                        contradiction_count: 0,
                        provenance: vec![MemoryProvenance {
                            episode_id: None,
                            turn_id: None,
                            assertion_key: None,
                            system_root: Some("root:luna".to_string()),
                            lifecycle_status: None,
                        }],
                    },
                    MemoryNode {
                        id: "user:self".to_string(),
                        label: "self".to_string(),
                        kind: MemoryNodeKind::User,
                        confidence_tier: AssertionConfidenceTier::Confirmed,
                        density: 1.0,
                        activation: 0.0,
                        created_at: None,
                        contradiction_count: 0,
                        provenance: profession_provenance.clone(),
                    },
                    MemoryNode {
                        id: "identity:profession:mechanical_engineer".to_string(),
                        label: "mechanical engineer".to_string(),
                        kind: MemoryNodeKind::Attribute,
                        confidence_tier: AssertionConfidenceTier::Confirmed,
                        density: 1.0,
                        activation: 0.0,
                        created_at: None,
                        contradiction_count: 0,
                        provenance: profession_provenance.clone(),
                    },
                ],
                edges: vec![MemoryEdge {
                    source: "root:luna".to_string(),
                    target: "root:luna:identity".to_string(),
                    relation: MemoryRelationKind::DefinesRule,
                    confidence_tier: AssertionConfidenceTier::Confirmed,
                    strength: 1.0,
                    activation: 0.0,
                    provenance: vec![MemoryProvenance {
                        episode_id: None,
                        turn_id: None,
                        assertion_key: None,
                        system_root: Some("root:luna:identity".to_string()),
                        lifecycle_status: None,
                    }],
                }],
            },
        };

        let bridge = bridge_memory_to_topology(&memory);

        assert!(bridge
            .orb_refs
            .iter()
            .any(|orb_ref| orb_ref.system_root == "root:luna"));
        assert!(bridge.orb_refs.iter().all(|orb_ref| {
            !orb_ref.system_root.contains("mechanical")
                && !orb_ref.source_ref.contains("mechanical")
                && !orb_ref.system_root.contains(&profession.key)
                && !orb_ref.source_ref.contains(&profession.key)
        }));
        assert!(bridge
            .tether_records
            .iter()
            .all(|tether| tether.source_ref != "root:luna"));
    }

    #[test]
    fn bridges_runtime_event_log_with_source_hashes() {
        let root = std::env::temp_dir().join(format!("luna_bridge_{}", Uuid::new_v4()));
        let log = root.join("events.jsonl");
        let session = crate::RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("MKPE is my provenance engine.")
            .unwrap();
        let events = JsonlEventLog::new(&log).load().unwrap();
        let bridge = bridge_runtime_events_to_topology(&events).unwrap();

        let mkpe = bridge
            .node_records
            .iter()
            .find(|record| record.runtime_entity_ref == "project:MKPE")
            .expect("runtime events should bridge MKPE into a topology node");
        assert!(mkpe.claim_refs.iter().any(|claim| {
            claim.assertion_key == "project:identity=MKPE_is_my_provenance_engine"
                && claim
                    .source_event_refs
                    .iter()
                    .any(|source| !source.event_hash.trim().is_empty())
        }));
        assert!(bridge.tether_records.iter().any(|tether| {
            tether.source_ref == "project:MKPE"
                && tether.target_ref == "project:identity:MKPE_is_my_provenance_engine"
                && tether.relation == MemoryRelationKind::ProvenanceFor
                && tether
                    .source_event_refs
                    .iter()
                    .any(|source| !source.event_hash.trim().is_empty())
        }));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn commits_runtime_bridge_records_into_replayable_topology_ledger() {
        let root = std::env::temp_dir().join(format!("luna_bridge_{}", Uuid::new_v4()));
        let log = root.join("events.jsonl");
        let session = crate::RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("MKPE is my provenance engine.")
            .unwrap();
        let events = JsonlEventLog::new(&log).load().unwrap();
        let commit = commit_runtime_events_to_topology_ledger(&events).unwrap();
        let replayed =
            luna_replay::TopologyReplay::replay_ledger(commit.topology.ledger()).unwrap();

        assert!(commit.topology.nodes().get("node:project:MKPE").is_some());
        assert!(commit.topology.tethers().tethers().values().any(|tether| {
            tether.source_node_id() == "node:project:MKPE"
                && tether.target_node_id() == "node:project:identity:MKPE_is_my_provenance_engine"
        }));
        assert!(commit
            .accepted_orb_ids
            .iter()
            .any(|orb_id| orb_id == "runtime:project:MKPE"));
        assert!(commit.ledger_event_count > 0);
        assert_eq!(commit.ledger_event_count, commit.ledger_events_json.len());
        assert_eq!(
            commit.ledger_event_hash,
            ledger_events_hash(&commit.ledger_events_json)
        );
        assert_eq!(replayed, commit.topology);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn topology_commits_do_not_embed_prior_topology_commit_snapshots_as_raw_events() {
        let root = std::env::temp_dir().join(format!("luna_bridge_{}", Uuid::new_v4()));
        let log = root.join("events.jsonl");
        let session = crate::RuntimeSession::new(&log, FusedExtractor::new());

        session
            .process_user_turn("MKPE is my provenance engine.")
            .unwrap();
        session
            .process_user_turn("Atlas Loom is my project workspace.")
            .unwrap();
        let events = JsonlEventLog::new(&log).load().unwrap();
        let commit = commit_runtime_events_to_topology_ledger(&events).unwrap();

        assert!(commit.ledger_events_json.iter().all(|event_json| {
            !event_json.contains("TopologyBridgeCommitted")
                && !event_json.contains("topology_bridge_committed")
                && !event_json.contains("ledger_events_json")
        }));

        let _ = std::fs::remove_dir_all(root);
    }
}
