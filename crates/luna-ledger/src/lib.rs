use chrono::{DateTime, Utc};
use luna_core::{LunaError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const RAW_EVENT_HASH_VERSION: &str = "luna.raw_event.v1";

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
    let mut hasher = Sha256::new();
    hasher.update(canonical_raw_event_bytes(id, source, payload));
    let digest = hasher.finalize();
    Ok(format!("{digest:x}"))
}

fn canonical_raw_event_bytes(id: &str, source: &EventSource, payload: &EventPayload) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_canonical_field(&mut bytes, RAW_EVENT_HASH_VERSION.as_bytes());
    push_canonical_field(&mut bytes, id.as_bytes());
    push_canonical_field(&mut bytes, event_source_tag(source).as_bytes());
    match payload {
        EventPayload::Text(text) => {
            push_canonical_field(&mut bytes, b"text");
            push_canonical_field(&mut bytes, text.as_bytes());
        }
    }
    bytes
}

fn event_source_tag(source: &EventSource) -> &'static str {
    match source {
        EventSource::User => "user",
        EventSource::Assistant => "assistant",
        EventSource::System => "system",
    }
}

fn push_canonical_field(bytes: &mut Vec<u8>, field: &[u8]) {
    bytes.extend_from_slice(field.len().to_string().as_bytes());
    bytes.push(b':');
    bytes.extend_from_slice(field);
    bytes.push(b'\n');
}
