use luna_core::{LunaError, Result};
use luna_genesis::{GenesisCertificate, GenesisRegistry};
use luna_inspector::{
    inspect_mutation, InspectionContext, InspectionPass, InspectorRejectReason, MutationRejected,
};
use luna_ledger::{InMemoryLedger, LedgerEvent, RawEvent, TopologyMutation};
use luna_node::NodeRegistry;
use luna_tether::TetherRegistry;
use serde::{Deserialize, Serialize};

pub type ReplayEvent = LedgerEvent;
pub type ReplayLedger = InMemoryLedger;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReplayedTopology {
    pub ledger: InMemoryLedger,
    pub nodes: NodeRegistry,
    pub genesis_certificates: GenesisRegistry,
    pub tethers: TetherRegistry,
}

impl ReplayedTopology {
    pub fn record_raw_event(&mut self, event: RawEvent) -> Result<()> {
        self.ledger.append(event)
    }

    pub fn commit(
        &mut self,
        mutation: TopologyMutation,
    ) -> std::result::Result<(), MutationRejected> {
        let context = self.inspection_context(&mutation);
        let pass = inspect_mutation(&mutation, &context)?;
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
            }
        }
        Ok(topology)
    }
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
