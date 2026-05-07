use luna_core::{LunaError, Result};
use luna_genesis::{GenesisCertificate, GenesisRegistry};
use luna_ledger::{InMemoryLedger, RawEvent};
use luna_node::{MemoryNode, NodeRegistry};
use luna_tether::{Tether, TetherRegistry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ReplayEvent {
    RawEventRecorded(RawEvent),
    NodeCreated(MemoryNode),
    GenesisCertificateCreated(GenesisCertificate),
    TetherCreated(Tether),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReplayedTopology {
    pub ledger: InMemoryLedger,
    pub nodes: NodeRegistry,
    pub genesis_certificates: GenesisRegistry,
    pub tethers: TetherRegistry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReplayLedger {
    events: Vec<ReplayEvent>,
}

impl ReplayLedger {
    pub fn append(&mut self, event: ReplayEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[ReplayEvent] {
        &self.events
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
                    topology.ledger.append(raw_event.clone())?;
                }
                ReplayEvent::NodeCreated(node) => {
                    verify_node_provenance(node, &topology.ledger)?;
                    topology.nodes.insert(node.clone())?;
                }
                ReplayEvent::GenesisCertificateCreated(certificate) => {
                    verify_certificate_provenance(certificate, &topology.ledger, &topology.nodes)?;
                    topology.genesis_certificates.insert(certificate.clone())?;
                }
                ReplayEvent::TetherCreated(tether) => {
                    verify_tether_provenance(tether, &topology.ledger, &topology.nodes)?;
                    topology.tethers.insert(tether.clone())?;
                }
            }
        }

        verify_every_node_has_genesis(&topology)?;
        Ok(topology)
    }
}

fn verify_node_provenance(node: &MemoryNode, ledger: &InMemoryLedger) -> Result<()> {
    let event = ledger.get(&node.source_event_id).ok_or_else(|| {
        LunaError::new(format!(
            "node {} references missing source event {}",
            node.id, node.source_event_id
        ))
    })?;
    if event.hash != node.source_event_hash {
        return Err(LunaError::new(format!(
            "node {} source hash does not match event {}",
            node.id, event.id
        )));
    }
    Ok(())
}

fn verify_certificate_provenance(
    certificate: &GenesisCertificate,
    ledger: &InMemoryLedger,
    nodes: &NodeRegistry,
) -> Result<()> {
    let node = nodes.get(&certificate.node_id).ok_or_else(|| {
        LunaError::new(format!(
            "genesis certificate {} references missing node {}",
            certificate.id, certificate.node_id
        ))
    })?;
    let event = ledger.get(&certificate.source_event_id).ok_or_else(|| {
        LunaError::new(format!(
            "genesis certificate {} references missing source event {}",
            certificate.id, certificate.source_event_id
        ))
    })?;
    if certificate.source_event_hash != event.hash {
        return Err(LunaError::new(format!(
            "genesis certificate {} source hash does not match event {}",
            certificate.id, event.id
        )));
    }
    if node.source_event_id != certificate.source_event_id
        || node.source_event_hash != certificate.source_event_hash
    {
        return Err(LunaError::new(format!(
            "genesis certificate {} does not match node {} provenance",
            certificate.id, node.id
        )));
    }
    certificate.verify_hash()
}

fn verify_tether_provenance(
    tether: &Tether,
    ledger: &InMemoryLedger,
    nodes: &NodeRegistry,
) -> Result<()> {
    if nodes.get(&tether.source_node_id).is_none() {
        return Err(LunaError::new(format!(
            "tether {} references missing source node {}",
            tether.id, tether.source_node_id
        )));
    }
    if nodes.get(&tether.target_node_id).is_none() {
        return Err(LunaError::new(format!(
            "tether {} references missing target node {}",
            tether.id, tether.target_node_id
        )));
    }
    let event = ledger.get(&tether.source_event_id).ok_or_else(|| {
        LunaError::new(format!(
            "tether {} references missing source event {}",
            tether.id, tether.source_event_id
        ))
    })?;
    if tether.source_event_hash != event.hash {
        return Err(LunaError::new(format!(
            "tether {} source hash does not match event {}",
            tether.id, event.id
        )));
    }
    Ok(())
}

fn verify_every_node_has_genesis(topology: &ReplayedTopology) -> Result<()> {
    for node in topology.nodes.nodes().values() {
        if topology
            .genesis_certificates
            .certificate_for_node(&node.id)
            .is_none()
        {
            return Err(LunaError::new(format!(
                "node {} is missing a genesis certificate",
                node.id
            )));
        }
    }
    Ok(())
}
