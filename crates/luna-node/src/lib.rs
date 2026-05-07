use luna_core::{LunaError, Result};
use luna_ledger::RawEvent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Event,
    Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub source_event_id: String,
    pub source_event_hash: String,
    pub genesis_certificate_id: Option<String>,
}

impl MemoryNode {
    pub fn new(
        id: impl Into<String>,
        kind: NodeKind,
        label: impl Into<String>,
        source_event_id: Option<&str>,
        source_event_hash: Option<&str>,
        genesis_certificate_id: Option<&str>,
    ) -> Result<Self> {
        let id = require_non_empty("node id", id.into())?;
        let label = require_non_empty("node label", label.into())?;
        let source_event_id = require_some_non_empty("source event id", source_event_id)?;
        let source_event_hash = require_some_non_empty("source event hash", source_event_hash)?;
        let genesis_certificate_id = genesis_certificate_id
            .map(|value| require_non_empty("genesis certificate id", value.to_string()))
            .transpose()?;

        Ok(Self {
            id,
            kind,
            label,
            source_event_id,
            source_event_hash,
            genesis_certificate_id,
        })
    }

    pub fn from_event(
        id: impl Into<String>,
        kind: NodeKind,
        label: impl Into<String>,
        event: &RawEvent,
    ) -> Self {
        Self::new(
            id,
            kind,
            label,
            Some(event.id.as_str()),
            Some(event.hash.as_str()),
            None,
        )
        .expect("raw event supplies non-empty node provenance")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NodeRegistry {
    nodes: BTreeMap<String, MemoryNode>,
}

impl NodeRegistry {
    pub fn insert(&mut self, node: MemoryNode) -> Result<()> {
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

fn require_some_non_empty(
    field: &str,
    value: Option<&str>,
) -> Result<String> {
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
