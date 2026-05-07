use luna_core::{LunaError, Result};
use luna_genesis::{GenesisCertificate, GenesisRegistry};
use luna_inspector::{inspect_mutation, MutationRejected};
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
        inspect_mutation(
            &mutation,
            &self.ledger,
            &self.nodes,
            &self.genesis_certificates,
            &self.tethers,
        )?;
        self.ledger
            .append_mutation(mutation.clone())
            .expect("append-only in-memory mutation append cannot fail");
        self.apply_mutation(&mutation)
            .expect("inspector-approved topology mutation must apply");
        Ok(())
    }

    fn apply_mutation(&mut self, mutation: &TopologyMutation) -> Result<()> {
        match mutation {
            TopologyMutation::NodeCreated(event) => {
                verify_node_provenance(
                    event.source_event_id.as_str(),
                    event.source_event_hash.as_str(),
                    &self.ledger,
                )?;
                self.nodes.apply_created(event)
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
                    .apply_attached(event, node, raw_event)
            }
            TopologyMutation::TetherCreated(event) => {
                verify_tether_provenance(event, &self.ledger, &self.nodes)?;
                self.tethers.apply_created(event)
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

        verify_every_node_has_genesis(&topology)?;
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

fn verify_every_node_has_genesis(topology: &ReplayedTopology) -> Result<()> {
    for node in topology.nodes.nodes().values() {
        if topology
            .genesis_certificates
            .certificate_for_node(node.id())
            .is_none()
        {
            return Err(LunaError::new(format!(
                "node {} is missing a genesis certificate",
                node.id()
            )));
        }
    }
    Ok(())
}
