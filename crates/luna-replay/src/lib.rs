use luna_cluster::{
    ClusterEvolutionEvent, ClusterRegistry, CompressionReceipt, CompressionReceiptRegistry,
    ConsolidationEvent, SourceEventRef,
};
use luna_core::{LunaError, Result};
use luna_genesis::{GenesisCertificate, GenesisRegistry};
use luna_inspector::{
    inspect_mutation, InspectionContext, InspectionPass, InspectorRejectReason, MutationRejected,
};
use luna_ledger::{CompressionArtifact, InMemoryLedger, LedgerEvent, RawEvent, TopologyMutation};
use luna_node::NodeRegistry;
use luna_tether::TetherRegistry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub type ReplayEvent = LedgerEvent;
pub type ReplayLedger = InMemoryLedger;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReplayedTopology {
    ledger: InMemoryLedger,
    nodes: NodeRegistry,
    genesis_certificates: GenesisRegistry,
    tethers: TetherRegistry,
    clusters: ClusterRegistry,
    compression_receipts: CompressionReceiptRegistry,
}

impl ReplayedTopology {
    pub fn ledger(&self) -> &InMemoryLedger {
        &self.ledger
    }

    pub fn nodes(&self) -> &NodeRegistry {
        &self.nodes
    }

    pub fn genesis_certificates(&self) -> &GenesisRegistry {
        &self.genesis_certificates
    }

    pub fn tethers(&self) -> &TetherRegistry {
        &self.tethers
    }

    pub fn clusters(&self) -> &ClusterRegistry {
        &self.clusters
    }

    pub fn compression_receipts(&self) -> &CompressionReceiptRegistry {
        &self.compression_receipts
    }

    pub fn record_raw_event(&mut self, event: RawEvent) -> Result<()> {
        self.ledger.append(event)
    }

    pub fn record_compression_artifact(&mut self, artifact: CompressionArtifact) -> Result<()> {
        self.ledger.append_compression_artifact(artifact)
    }

    pub fn commit(
        &mut self,
        mutation: TopologyMutation,
    ) -> std::result::Result<(), MutationRejected> {
        let context = self.inspection_context(&mutation);
        // SAFETY: the context is derived from this topology's live ledger and
        // registries immediately before staging the same mutation.
        let pass = unsafe { inspect_mutation(&mutation, &context)? };
        let mut staged = self.clone();
        staged.apply_mutation(&mutation, &pass).map_err(|err| {
            MutationRejected::new(InspectorRejectReason::ApplyRejected {
                message: err.to_string(),
            })
        })?;
        // SAFETY: the mutation passed the inspector chain above and was applied
        // successfully to a staged topology before entering the ledger.
        unsafe {
            self.ledger
                .append_mutation_unchecked(mutation.clone())
                .expect("append-only in-memory mutation append cannot fail");
        }
        self.nodes = staged.nodes;
        self.genesis_certificates = staged.genesis_certificates;
        self.tethers = staged.tethers;
        Ok(())
    }

    pub fn record_consolidation_event(&mut self, event: ConsolidationEvent) -> Result<()> {
        verify_consolidation_provenance(&event, self)?;
        let mut staged = self.clusters.clone();
        staged.apply_consolidation_event(&event)?;
        self.ledger.append_consolidation_event(event)?;
        self.clusters = staged;
        Ok(())
    }

    pub fn record_compression_receipt(&mut self, receipt: CompressionReceipt) -> Result<()> {
        verify_compression_provenance(&receipt, self)?;
        let mut staged = self.compression_receipts.clone();
        staged.apply_receipt(&receipt)?;
        self.ledger.append_compression_receipt(receipt)?;
        self.compression_receipts = staged;
        Ok(())
    }

    pub fn record_cluster_evolution_event(&mut self, event: ClusterEvolutionEvent) -> Result<()> {
        let mut staged = self.clusters.clone();
        staged.apply_evolution_event(&event)?;
        self.ledger.append_cluster_evolution_event(event)?;
        self.clusters = staged;
        Ok(())
    }

    fn inspection_context(&self, mutation: &TopologyMutation) -> InspectionContext {
        match mutation {
            TopologyMutation::NodeCreated(event) => InspectionContext {
                source_event_hash: self
                    .ledger
                    .get(&event.source_event_id)
                    .map(|event| event.hash.clone()),
                node_exists: self.nodes.get(&event.node_id).is_some(),
                ..InspectionContext::default()
            },
            TopologyMutation::GenesisAttached(event) => {
                let node = self.nodes.get(&event.node_id);
                InspectionContext {
                    source_event_hash: self
                        .ledger
                        .get(&event.source_event_id)
                        .map(|event| event.hash.clone()),
                    node_exists: node.is_some(),
                    node_source_event_id: node.map(|node| node.source_event_id().to_string()),
                    node_source_event_hash: node.map(|node| node.source_event_hash().to_string()),
                    node_has_genesis: self
                        .genesis_certificates
                        .certificate_for_node(&event.node_id)
                        .is_some(),
                    certificate_exists: self
                        .genesis_certificates
                        .get(&event.certificate_id)
                        .is_some(),
                    ..InspectionContext::default()
                }
            }
            TopologyMutation::TetherCreated(event) => InspectionContext {
                source_event_hash: self
                    .ledger
                    .get(&event.source_event_id)
                    .map(|event| event.hash.clone()),
                source_endpoint_exists: self.nodes.get(&event.source_node_id).is_some(),
                target_endpoint_exists: self.nodes.get(&event.target_node_id).is_some(),
                tether_exists: self.tethers.get(&event.tether_id).is_some(),
                ..InspectionContext::default()
            },
        }
    }

    fn apply_mutation(&mut self, mutation: &TopologyMutation, pass: &InspectionPass) -> Result<()> {
        match mutation {
            TopologyMutation::NodeCreated(event) => {
                verify_node_provenance(
                    event.source_event_id.as_str(),
                    event.source_event_hash.as_str(),
                    &self.ledger,
                )?;
                self.nodes.apply_created(event, pass)
            }
            TopologyMutation::GenesisAttached(event) => {
                let node = self.nodes.get(&event.node_id).ok_or_else(|| {
                    LunaError::new(format!(
                        "genesis certificate {} references missing node {}",
                        event.certificate_id, event.node_id
                    ))
                })?;
                let raw_event = self.ledger.get(&event.source_event_id).ok_or_else(|| {
                    LunaError::new(format!(
                        "genesis certificate {} references missing source event {}",
                        event.certificate_id, event.source_event_id
                    ))
                })?;
                self.genesis_certificates
                    .apply_attached(event, node, raw_event, pass)
            }
            TopologyMutation::TetherCreated(event) => {
                verify_tether_provenance(event, &self.ledger, &self.nodes)?;
                self.tethers.apply_created(event, pass)
            }
        }
    }
}

pub const REPLAY_AUDIT_HASH_VERSION: &str = "luna.replay_audit.snapshot.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayAuditStatus {
    Clean,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReplaySnapshotCounts {
    pub ledger_events: usize,
    pub raw_events: usize,
    pub nodes: usize,
    pub genesis_certificates: usize,
    pub tethers: usize,
    pub accepted_orbs: usize,
    pub retired_orbs: usize,
    pub rejected_consolidation_events: usize,
    pub accepted_orb_evolution_events: usize,
    pub accepted_compression_receipts: usize,
    pub rejected_compression_receipts: usize,
}

impl ReplaySnapshotCounts {
    fn from_topology(topology: &ReplayedTopology) -> Self {
        Self {
            ledger_events: topology.ledger().events().len(),
            raw_events: topology.ledger().raw_events().len(),
            nodes: topology.nodes().nodes().len(),
            genesis_certificates: topology.genesis_certificates().certificates().len(),
            tethers: topology.tethers().tethers().len(),
            accepted_orbs: topology.clusters().clusters().len(),
            retired_orbs: topology.clusters().retired_clusters().len(),
            rejected_consolidation_events: topology.clusters().rejected_event_ids().len(),
            accepted_orb_evolution_events: topology.clusters().evolution_event_ids().len(),
            accepted_compression_receipts: topology.compression_receipts().accepted().len(),
            rejected_compression_receipts: topology
                .compression_receipts()
                .rejected_event_ids()
                .len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayCountDiff {
    pub field: String,
    pub live: usize,
    pub replayed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrbLineageInspection {
    pub orb_id: String,
    pub accepted_event_id: String,
    pub source_node_ids: Vec<String>,
    pub source_tether_ids: Vec<String>,
    pub source_event_refs: Vec<SourceEventRef>,
    pub lineage_event_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayAuditReport {
    pub status: ReplayAuditStatus,
    pub quarantine_required: bool,
    pub live_snapshot_hash: String,
    pub replayed_snapshot_hash: String,
    #[serde(default)]
    pub replay_error: Option<String>,
    pub live_counts: ReplaySnapshotCounts,
    pub replayed_counts: ReplaySnapshotCounts,
    pub count_diffs: Vec<ReplayCountDiff>,
    pub live_orb_lineage: Vec<OrbLineageInspection>,
    pub replayed_orb_lineage: Vec<OrbLineageInspection>,
}

impl ReplayAuditReport {
    pub fn is_clean(&self) -> bool {
        self.status == ReplayAuditStatus::Clean
    }

    pub fn is_quarantined(&self) -> bool {
        self.status == ReplayAuditStatus::Quarantined
    }
}

pub struct TopologyReplay;

impl TopologyReplay {
    pub fn replay_ledger(ledger: &ReplayLedger) -> Result<ReplayedTopology> {
        Self::replay(ledger.events())
    }

    pub fn replay(events: &[ReplayEvent]) -> Result<ReplayedTopology> {
        let mut topology = ReplayedTopology::default();

        for event in events {
            match event {
                ReplayEvent::RawEventRecorded(raw_event) => {
                    topology.record_raw_event(raw_event.clone())?;
                }
                ReplayEvent::TopologyMutation(mutation) => {
                    topology
                        .commit(mutation.clone())
                        .map_err(|err| LunaError::new(err.to_string()))?;
                }
                ReplayEvent::CompressionArtifactRecorded(artifact) => {
                    topology.record_compression_artifact(artifact.clone())?;
                }
                ReplayEvent::ConsolidationEvent(event) => {
                    topology.record_consolidation_event(event.clone())?;
                }
                ReplayEvent::CompressionReceipt(receipt) => {
                    topology.record_compression_receipt(receipt.clone())?;
                }
                ReplayEvent::ClusterEvolutionEvent(event) => {
                    topology.record_cluster_evolution_event(event.clone())?;
                }
            }
        }
        Ok(topology)
    }
}

pub struct ReplayAuditor;

impl ReplayAuditor {
    pub fn audit_ledger(live: &ReplayedTopology) -> Result<ReplayAuditReport> {
        Self::audit_against_ledger(live, live.ledger())
    }

    pub fn audit_against_ledger(
        live: &ReplayedTopology,
        replay_ledger: &ReplayLedger,
    ) -> Result<ReplayAuditReport> {
        let live_snapshot_hash = snapshot_hash(live)?;
        let replayed = match TopologyReplay::replay_ledger(replay_ledger) {
            Ok(replayed) => replayed,
            Err(err) => {
                return Ok(ReplayAuditReport {
                    status: ReplayAuditStatus::Quarantined,
                    quarantine_required: true,
                    live_snapshot_hash,
                    replayed_snapshot_hash: String::new(),
                    replay_error: Some(err.to_string()),
                    live_counts: ReplaySnapshotCounts::from_topology(live),
                    replayed_counts: ReplaySnapshotCounts::default(),
                    count_diffs: Vec::new(),

                    live_orb_lineage: orb_lineage(live.clusters()),
                    replayed_orb_lineage: Vec::new(),
                })
            }
        };
        let replayed_snapshot_hash = snapshot_hash(&replayed)?;
        let live_counts = ReplaySnapshotCounts::from_topology(live);
        let replayed_counts = ReplaySnapshotCounts::from_topology(&replayed);
        let count_diffs = count_diffs(&live_counts, &replayed_counts);
        let quarantine_required = live_snapshot_hash != replayed_snapshot_hash;
        Ok(ReplayAuditReport {
            status: if quarantine_required {
                ReplayAuditStatus::Quarantined
            } else {
                ReplayAuditStatus::Clean
            },
            quarantine_required,
            live_snapshot_hash,
            replayed_snapshot_hash,
            replay_error: None,
            live_counts,
            replayed_counts,
            count_diffs,
            live_orb_lineage: orb_lineage(live.clusters()),
            replayed_orb_lineage: orb_lineage(replayed.clusters()),
        })
    }
}

fn snapshot_hash(topology: &ReplayedTopology) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(REPLAY_AUDIT_HASH_VERSION.as_bytes());
    hasher.update([0]);
    let bytes = serde_json::to_vec(topology).map_err(|err| LunaError::new(err.to_string()))?;
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn count_diffs(
    live: &ReplaySnapshotCounts,
    replayed: &ReplaySnapshotCounts,
) -> Vec<ReplayCountDiff> {
    let pairs = [
        ("ledger_events", live.ledger_events, replayed.ledger_events),
        ("raw_events", live.raw_events, replayed.raw_events),
        ("nodes", live.nodes, replayed.nodes),
        (
            "genesis_certificates",
            live.genesis_certificates,
            replayed.genesis_certificates,
        ),
        ("tethers", live.tethers, replayed.tethers),
        ("accepted_orbs", live.accepted_orbs, replayed.accepted_orbs),
        ("retired_orbs", live.retired_orbs, replayed.retired_orbs),
        (
            "rejected_consolidation_events",
            live.rejected_consolidation_events,
            replayed.rejected_consolidation_events,
        ),
        (
            "accepted_orb_evolution_events",
            live.accepted_orb_evolution_events,
            replayed.accepted_orb_evolution_events,
        ),
        (
            "accepted_compression_receipts",
            live.accepted_compression_receipts,
            replayed.accepted_compression_receipts,
        ),
        (
            "rejected_compression_receipts",
            live.rejected_compression_receipts,
            replayed.rejected_compression_receipts,
        ),
    ];
    pairs
        .into_iter()
        .filter(|(_, live, replayed)| live != replayed)
        .map(|(field, live, replayed)| ReplayCountDiff {
            field: field.to_string(),
            live,
            replayed,
        })
        .collect()
}

fn orb_lineage(registry: &ClusterRegistry) -> Vec<OrbLineageInspection> {
    registry
        .clusters()
        .values()
        .map(|orb| OrbLineageInspection {
            orb_id: orb.orb_id.clone(),
            accepted_event_id: orb.accepted_event_id.clone(),
            source_node_ids: orb.source_node_ids.clone(),
            source_tether_ids: orb.source_tether_ids.clone(),
            source_event_refs: orb.source_event_refs.clone(),
            lineage_event_ids: orb.lineage_event_ids.clone(),
        })
        .collect()
}

fn verify_compression_provenance(
    receipt: &CompressionReceipt,
    topology: &ReplayedTopology,
) -> Result<()> {
    for reference in receipt
        .input_event_refs
        .iter()
        .chain(receipt.retained_event_refs.iter())
    {
        let raw_event = topology.ledger.get(&reference.event_id).ok_or_else(|| {
            LunaError::new(format!(
                "compression {} references missing raw event {}",
                receipt.compression_id, reference.event_id
            ))
        })?;
        if raw_event.hash != reference.event_hash {
            return Err(LunaError::new(format!(
                "compression {} raw event {} hash mismatch",
                receipt.compression_id, reference.event_id
            )));
        }
    }
    Ok(())
}

fn verify_consolidation_provenance(
    event: &ConsolidationEvent,
    topology: &ReplayedTopology,
) -> Result<()> {
    let mut used_source_event_refs = BTreeSet::<(String, String)>::new();
    for node_id in &event.source_node_ids {
        let node = topology.nodes.get(node_id).ok_or_else(|| {
            LunaError::new(format!(
                "orb {} references missing source node {}",
                event.orb_id, node_id
            ))
        })?;
        if !event.source_event_refs.iter().any(|reference| {
            let matches = reference.event_id == node.source_event_id()
                && reference.event_hash == node.source_event_hash();
            if matches {
                used_source_event_refs
                    .insert((reference.event_id.clone(), reference.event_hash.clone()));
            }
            matches
        }) {
            return Err(LunaError::new(format!(
                "orb {} source node {} is not backed by listed source events",
                event.orb_id, node_id
            )));
        }
    }
    for tether_id in &event.source_tether_ids {
        let tether = topology.tethers.get(tether_id).ok_or_else(|| {
            LunaError::new(format!(
                "orb {} references missing source tether {}",
                event.orb_id, tether_id
            ))
        })?;
        if !event
            .source_node_ids
            .contains(&tether.source_node_id().to_string())
            || !event
                .source_node_ids
                .contains(&tether.target_node_id().to_string())
        {
            return Err(LunaError::new(format!(
                "orb {} source tether {} does not connect listed source nodes",
                event.orb_id, tether_id
            )));
        }
        if !event.source_event_refs.iter().any(|reference| {
            let matches = reference.event_id == tether.source_event_id()
                && reference.event_hash == tether.source_event_hash();
            if matches {
                used_source_event_refs
                    .insert((reference.event_id.clone(), reference.event_hash.clone()));
            }
            matches
        }) {
            return Err(LunaError::new(format!(
                "orb {} source tether {} is not backed by listed source events",
                event.orb_id, tether_id
            )));
        }
    }
    for reference in &event.source_event_refs {
        if !used_source_event_refs
            .contains(&(reference.event_id.clone(), reference.event_hash.clone()))
        {
            return Err(LunaError::new(format!(
                "orb {} source event {} does not back any listed source node or tether",
                event.orb_id, reference.event_id
            )));
        }
        let raw_event = topology.ledger.get(&reference.event_id).ok_or_else(|| {
            LunaError::new(format!(
                "orb {} references missing source event {}",
                event.orb_id, reference.event_id
            ))
        })?;
        if raw_event.hash != reference.event_hash {
            return Err(LunaError::new(format!(
                "orb {} source event {} hash mismatch",
                event.orb_id, reference.event_id
            )));
        }
    }
    Ok(())
}

fn verify_node_provenance(
    source_event_id: &str,
    source_event_hash: &str,
    ledger: &InMemoryLedger,
) -> Result<()> {
    let event = ledger.get(source_event_id).ok_or_else(|| {
        LunaError::new(format!(
            "node references missing source event {}",
            source_event_id
        ))
    })?;
    if event.hash != source_event_hash {
        return Err(LunaError::new(format!(
            "node source hash does not match event {}",
            event.id
        )));
    }
    Ok(())
}

#[allow(dead_code)]
fn verify_certificate_provenance(
    certificate: &GenesisCertificate,
    ledger: &InMemoryLedger,
    nodes: &NodeRegistry,
) -> Result<()> {
    let node = nodes.get(certificate.node_id()).ok_or_else(|| {
        LunaError::new(format!(
            "genesis certificate {} references missing node {}",
            certificate.id(),
            certificate.node_id()
        ))
    })?;
    let event = ledger.get(certificate.source_event_id()).ok_or_else(|| {
        LunaError::new(format!(
            "genesis certificate {} references missing source event {}",
            certificate.id(),
            certificate.source_event_id()
        ))
    })?;
    if certificate.source_event_hash() != event.hash {
        return Err(LunaError::new(format!(
            "genesis certificate {} source hash does not match event {}",
            certificate.id(),
            event.id
        )));
    }
    if node.source_event_id() != certificate.source_event_id()
        || node.source_event_hash() != certificate.source_event_hash()
    {
        return Err(LunaError::new(format!(
            "genesis certificate {} does not match node {} provenance",
            certificate.id(),
            node.id()
        )));
    }
    certificate.verify_hash()
}

fn verify_tether_provenance(
    tether: &luna_ledger::TetherCreated,
    ledger: &InMemoryLedger,
    nodes: &NodeRegistry,
) -> Result<()> {
    if nodes.get(&tether.source_node_id).is_none() {
        return Err(LunaError::new(format!(
            "tether {} references missing source node {}",
            tether.tether_id, tether.source_node_id
        )));
    }
    if nodes.get(&tether.target_node_id).is_none() {
        return Err(LunaError::new(format!(
            "tether {} references missing target node {}",
            tether.tether_id, tether.target_node_id
        )));
    }
    let event = ledger.get(&tether.source_event_id).ok_or_else(|| {
        LunaError::new(format!(
            "tether {} references missing source event {}",
            tether.tether_id, tether.source_event_id
        ))
    })?;
    if tether.source_event_hash != event.hash {
        return Err(LunaError::new(format!(
            "tether {} source hash does not match event {}",
            tether.tether_id, event.id
        )));
    }
    Ok(())
}
