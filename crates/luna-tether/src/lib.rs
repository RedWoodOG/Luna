use luna_core::{LunaError, Result};
use luna_ledger::TetherCreated;
pub use luna_ledger::TetherKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tether {
    id: String,
    source_node_id: String,
    target_node_id: String,
    kind: TetherKind,
    reverse_kind: TetherKind,
    source_event_id: String,
    source_event_hash: String,
}

impl Tether {
    pub fn from_created(event: &TetherCreated) -> Result<Self> {
        let kind = event
            .kind
            .ok_or_else(|| LunaError::new("tether direction is required"))?;
        let tether = Self {
            id: require_non_empty("tether id", event.tether_id.clone())?,
            source_node_id: require_non_empty("source node id", event.source_node_id.clone())?,
            target_node_id: require_non_empty("target node id", event.target_node_id.clone())?,
            kind,
            reverse_kind: event.reverse_kind,
            source_event_id: require_non_empty("source event id", event.source_event_id.clone())?,
            source_event_hash: require_non_empty(
                "source event hash",
                event.source_event_hash.clone(),
            )?,
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

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn source_node_id(&self) -> &str {
        &self.source_node_id
    }

    pub fn target_node_id(&self) -> &str {
        &self.target_node_id
    }

    pub fn kind(&self) -> TetherKind {
        self.kind
    }

    pub fn reverse_kind(&self) -> TetherKind {
        self.reverse_kind
    }

    pub fn source_event_id(&self) -> &str {
        &self.source_event_id
    }

    pub fn source_event_hash(&self) -> &str {
        &self.source_event_hash
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TetherRegistry {
    tethers: BTreeMap<String, Tether>,
}

impl TetherRegistry {
    pub fn apply_created(&mut self, event: &TetherCreated) -> Result<()> {
        let tether = Tether::from_created(event)?;
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
