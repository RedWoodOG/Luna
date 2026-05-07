use luna_core::{LunaError, Result};
use luna_ledger::RawEvent;
use luna_node::MemoryNode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TetherKind {
    SupportedBy,
    EvidenceFor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tether {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: TetherKind,
    pub reverse_kind: TetherKind,
    pub source_event_id: String,
    pub source_event_hash: String,
}

impl Tether {
    pub fn new(
        id: impl Into<String>,
        source: &MemoryNode,
        target: &MemoryNode,
        kind: Option<TetherKind>,
        reverse_kind: TetherKind,
        event: &RawEvent,
    ) -> Result<Self> {
        let kind = kind.ok_or_else(|| LunaError::new("tether direction is required"))?;
        let tether = Self {
            id: require_non_empty("tether id", id.into())?,
            source_node_id: source.id.clone(),
            target_node_id: target.id.clone(),
            kind,
            reverse_kind,
            source_event_id: event.id.clone(),
            source_event_hash: event.hash.clone(),
        };
        tether.validate()?;
        Ok(tether)
    }

    pub fn reverse(&self, id: impl Into<String>) -> Result<Self> {
        let tether = Self {
            id: require_non_empty("tether id", id.into())?,
            source_node_id: self.target_node_id.clone(),
            target_node_id: self.source_node_id.clone(),
            kind: self.reverse_kind,
            reverse_kind: self.kind,
            source_event_id: self.source_event_id.clone(),
            source_event_hash: self.source_event_hash.clone(),
        };
        tether.validate()?;
        Ok(tether)
    }

    pub fn validate(&self) -> Result<()> {
        require_non_empty("tether id", self.id.clone())?;
        require_non_empty("source node id", self.source_node_id.clone())?;
        require_non_empty("target node id", self.target_node_id.clone())?;
        require_non_empty("source event id", self.source_event_id.clone())?;
        require_non_empty("source event hash", self.source_event_hash.clone())?;
        if self.kind == self.reverse_kind {
            return Err(LunaError::new(
                "reverse tether meaning must be distinct from forward meaning",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TetherRegistry {
    tethers: BTreeMap<String, Tether>,
}

impl TetherRegistry {
    pub fn insert(&mut self, tether: Tether) -> Result<()> {
        tether.validate()?;
        if self.tethers.contains_key(&tether.id) {
            return Err(LunaError::new(format!(
                "tether {} already exists",
                tether.id
            )));
        }
        self.tethers.insert(tether.id.clone(), tether);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Tether> {
        self.tethers.get(id)
    }

    pub fn tethers(&self) -> &BTreeMap<String, Tether> {
        &self.tethers
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
