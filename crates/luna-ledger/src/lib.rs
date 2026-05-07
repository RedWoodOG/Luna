use chrono::{DateTime, Utc};
use luna_core::{LunaError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum EventPayload {
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEventDraft {
    pub id: String,
    pub source: EventSource,
    pub payload: EventPayload,
}

impl RawEventDraft {
    pub fn new(id: impl Into<String>, source: EventSource, payload: EventPayload) -> Self {
        Self {
            id: id.into(),
            source,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEvent {
    pub id: String,
    pub recorded_at: DateTime<Utc>,
    pub source: EventSource,
    pub payload: EventPayload,
    pub hash: String,
}

impl RawEvent {
    pub fn from_draft(draft: RawEventDraft) -> Self {
        let hash = stable_event_hash(&draft.id, &draft.source, &draft.payload)
            .expect("serializing supported raw event payload should not fail");
        Self {
            id: draft.id,
            recorded_at: Utc::now(),
            source: draft.source,
            payload: draft.payload,
            hash,
        }
    }

    pub fn verify_hash(&self) -> Result<()> {
        let recomputed = stable_event_hash(&self.id, &self.source, &self.payload)?;
        if recomputed == self.hash {
            Ok(())
        } else {
            Err(LunaError::new(format!(
                "raw event {} hash mismatch: stored {}, recomputed {}",
                self.id, self.hash, recomputed
            )))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InMemoryLedger {
    events: BTreeMap<String, RawEvent>,
}

impl InMemoryLedger {
    pub fn append(&mut self, event: RawEvent) -> Result<()> {
        event.verify_hash()?;
        if self.events.contains_key(&event.id) {
            return Err(LunaError::new(format!(
                "raw event {} already exists in append-only ledger",
                event.id
            )));
        }
        self.events.insert(event.id.clone(), event);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&RawEvent> {
        self.events.get(id)
    }

    pub fn events(&self) -> &BTreeMap<String, RawEvent> {
        &self.events
    }
}

fn stable_event_hash(id: &str, source: &EventSource, payload: &EventPayload) -> Result<String> {
    let canonical = serde_json::to_vec(&(id, source, payload))
        .map_err(|err| LunaError::new(err.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(canonical);
    let digest = hasher.finalize();
    Ok(format!("{digest:x}"))
}
