use luna_ledger::TopologyMutation;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct InspectionPass {
    _private: (),
}

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
    GenesisSourceMismatch { node_id: String },
    DuplicateCertificate { certificate_id: String },
    ApplyRejected { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InspectionContext {
    pub source_event_hash: Option<String>,
    pub node_exists: bool,
    pub node_source_event_id: Option<String>,
    pub node_source_event_hash: Option<String>,
    pub node_has_genesis: bool,
    pub certificate_exists: bool,
    pub source_endpoint_exists: bool,
    pub target_endpoint_exists: bool,
    pub tether_exists: bool,
}

pub fn inspect_mutation(
    mutation: &TopologyMutation,
    context: &InspectionContext,
) -> Result<InspectionPass, MutationRejected> {
    match mutation {
        TopologyMutation::NodeCreated(event) => {
            inspect_source_event(&event.source_event_id, &event.source_event_hash, context)?;
            if context.node_exists {
                return reject(InspectorRejectReason::DuplicateNode {
                    node_id: event.node_id.clone(),
                });
            }
        }
        TopologyMutation::GenesisAttached(event) => {
            inspect_source_event(&event.source_event_id, &event.source_event_hash, context)?;
            if !context.node_exists {
                return reject(InspectorRejectReason::NodeMissing {
                    node_id: event.node_id.clone(),
                });
            }
            if context.certificate_exists {
                return reject(InspectorRejectReason::DuplicateCertificate {
                    certificate_id: event.certificate_id.clone(),
                });
            }
            if context.node_has_genesis {
                return reject(InspectorRejectReason::DuplicateGenesis {
                    node_id: event.node_id.clone(),
                });
            }
            if context.node_source_event_id.as_deref() != Some(event.source_event_id.as_str())
                || context.node_source_event_hash.as_deref()
                    != Some(event.source_event_hash.as_str())
            {
                return reject(InspectorRejectReason::GenesisSourceMismatch {
                    node_id: event.node_id.clone(),
                });
            }
        }
        TopologyMutation::TetherCreated(event) => {
            inspect_source_event(&event.source_event_id, &event.source_event_hash, context)?;
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
            if !context.source_endpoint_exists {
                return reject(InspectorRejectReason::EndpointMissing {
                    tether_id: event.tether_id.clone(),
                    node_id: event.source_node_id.clone(),
                });
            }
            if !context.target_endpoint_exists {
                return reject(InspectorRejectReason::EndpointMissing {
                    tether_id: event.tether_id.clone(),
                    node_id: event.target_node_id.clone(),
                });
            }
            if context.tether_exists {
                return reject(InspectorRejectReason::DuplicateTether {
                    tether_id: event.tether_id.clone(),
                });
            }
        }
    }

    Ok(InspectionPass { _private: () })
}

fn inspect_source_event(
    event_id: &str,
    expected_hash: &str,
    context: &InspectionContext,
) -> Result<(), MutationRejected> {
    let actual_hash = context.source_event_hash.as_deref().ok_or_else(|| {
        MutationRejected::new(InspectorRejectReason::SourceEventMissing {
            event_id: event_id.to_string(),
        })
    })?;
    if actual_hash != expected_hash {
        return reject(InspectorRejectReason::SourceEventHashMismatch {
            event_id: event_id.to_string(),
        });
    }
    Ok(())
}

fn reject<T>(reason: InspectorRejectReason) -> Result<T, MutationRejected> {
    Err(MutationRejected::new(reason))
}
