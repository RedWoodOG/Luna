use luna_genesis::GenesisRegistry;
use luna_ledger::{InMemoryLedger, TopologyMutation};
use luna_node::NodeRegistry;
use luna_tether::TetherRegistry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationRejected {
    reason: InspectorRejectReason,
}

impl MutationRejected {
    pub fn new(reason: InspectorRejectReason) -> Self {
        Self { reason }
    }

    pub fn reason(&self) -> &InspectorRejectReason {
        &self.reason
    }
}

impl std::fmt::Display for MutationRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.reason)
    }
}

impl std::error::Error for MutationRejected {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InspectorRejectReason {
    SourceEventMissing { event_id: String },
    SourceEventHashMismatch { event_id: String },
    DirectionMissing { tether_id: String },
    DuplicateGenesis { node_id: String },
    EndpointMissing { tether_id: String, node_id: String },
    NodeMissing { node_id: String },
    DuplicateNode { node_id: String },
    DuplicateTether { tether_id: String },
    ReverseMeaningNotDistinct { tether_id: String },
}

pub fn inspect_mutation(
    mutation: &TopologyMutation,
    ledger: &InMemoryLedger,
    nodes: &NodeRegistry,
    genesis: &GenesisRegistry,
    tethers: &TetherRegistry,
) -> Result<(), MutationRejected> {
    match mutation {
        TopologyMutation::NodeCreated(event) => {
            inspect_source_event(&event.source_event_id, &event.source_event_hash, ledger)?;
            if nodes.get(&event.node_id).is_some() {
                return reject(InspectorRejectReason::DuplicateNode {
                    node_id: event.node_id.clone(),
                });
            }
        }
        TopologyMutation::GenesisAttached(event) => {
            inspect_source_event(&event.source_event_id, &event.source_event_hash, ledger)?;
            if nodes.get(&event.node_id).is_none() {
                return reject(InspectorRejectReason::NodeMissing {
                    node_id: event.node_id.clone(),
                });
            }
            if genesis.certificate_for_node(&event.node_id).is_some() {
                return reject(InspectorRejectReason::DuplicateGenesis {
                    node_id: event.node_id.clone(),
                });
            }
        }
        TopologyMutation::TetherCreated(event) => {
            inspect_source_event(&event.source_event_id, &event.source_event_hash, ledger)?;
            if event.kind.is_none() {
                return reject(InspectorRejectReason::DirectionMissing {
                    tether_id: event.tether_id.clone(),
                });
            }
            if event.kind == Some(event.reverse_kind) {
                return reject(InspectorRejectReason::ReverseMeaningNotDistinct {
                    tether_id: event.tether_id.clone(),
                });
            }
            if nodes.get(&event.source_node_id).is_none() {
                return reject(InspectorRejectReason::EndpointMissing {
                    tether_id: event.tether_id.clone(),
                    node_id: event.source_node_id.clone(),
                });
            }
            if nodes.get(&event.target_node_id).is_none() {
                return reject(InspectorRejectReason::EndpointMissing {
                    tether_id: event.tether_id.clone(),
                    node_id: event.target_node_id.clone(),
                });
            }
            if tethers.get(&event.tether_id).is_some() {
                return reject(InspectorRejectReason::DuplicateTether {
                    tether_id: event.tether_id.clone(),
                });
            }
        }
    }

    Ok(())
}

fn inspect_source_event(
    event_id: &str,
    expected_hash: &str,
    ledger: &InMemoryLedger,
) -> Result<(), MutationRejected> {
    let event = ledger.get(event_id).ok_or_else(|| {
        MutationRejected::new(InspectorRejectReason::SourceEventMissing {
            event_id: event_id.to_string(),
        })
    })?;
    if event.hash != expected_hash {
        return reject(InspectorRejectReason::SourceEventHashMismatch {
            event_id: event_id.to_string(),
        });
    }
    Ok(())
}

fn reject<T>(reason: InspectorRejectReason) -> Result<T, MutationRejected> {
    Err(MutationRejected::new(reason))
}
