use luna_core::{LunaError, Result};
use luna_inspector::InspectionPass;
use luna_ledger::NodeCreated;
pub use luna_ledger::NodeKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryNode {
    id: String,
    kind: NodeKind,
    label: String,
    source_event_id: String,
    source_event_hash: String,
}

impl MemoryNode {
    fn new(
        id: impl Into<String>,
        kind: NodeKind,
        label: impl Into<String>,
        source_event_id: Option<&str>,
        source_event_hash: Option<&str>,
    ) -> Result<Self> {
        let id = require_non_empty("node id", id.into())?;
        let label = require_non_empty("node label", label.into())?;
        let source_event_id = require_some_non_empty("source event id", source_event_id)?;
        let source_event_hash = require_some_non_empty("source event hash", source_event_hash)?;

        Ok(Self {
            id,
            kind,
            label,
            source_event_id,
            source_event_hash,
        })
    }

    pub fn from_created(event: &NodeCreated) -> Result<Self> {
        Self::new(
            event.node_id.as_str(),
            event.kind,
            event.label.as_str(),
            Some(event.source_event_id.as_str()),
            Some(event.source_event_hash.as_str()),
        )
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn kind(&self) -> NodeKind {
        self.kind
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn source_event_id(&self) -> &str {
        &self.source_event_id
    }

    pub fn source_event_hash(&self) -> &str {
        &self.source_event_hash
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NodeRegistry {
    nodes: BTreeMap<String, MemoryNode>,
}

impl NodeRegistry {
    pub fn apply_created(&mut self, event: &NodeCreated, _pass: &InspectionPass) -> Result<()> {
        let node = MemoryNode::from_created(event)?;
        if self.nodes.contains_key(&node.id) {
            return Err(LunaError::new(format!("node {} already exists", node.id)));
        }
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&MemoryNode> {
        self.nodes.get(id)
    }

    pub fn nodes(&self) -> &BTreeMap<String, MemoryNode> {
        &self.nodes
    }
}

fn require_some_non_empty(field: &str, value: Option<&str>) -> Result<String> {
    match value {
        Some(value) => require_non_empty(field, value.to_string()),
        None => Err(LunaError::new(format!("{field} is required"))),
    }
}

fn require_non_empty(field: &str, value: String) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(LunaError::new(format!("{field} cannot be empty")))
    } else {
        Ok(trimmed.to_string())
    }
}
