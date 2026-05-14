use luna_core::{LunaError, Result};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

pub use luna_core::{
    AssertionCorrected, AssertionExtracted, ContradictionDetected, EpisodeCreated, EpisodeDecayed,
    EpisodeRecalled, EpisodeReinforced, EventEnvelope, EventSource, LunaEvent, RecallFailed,
    RecallSucceeded, StoredEvent, SurpriseUpdateReceipt, TurnObserved, UpdateKind,
};

#[derive(Debug, Clone)]
pub struct JsonlEventLog {
    path: PathBuf,
}

impl JsonlEventLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn append(&self, event: &StoredEvent) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| luna_core::LunaError::new(err.to_string()))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|err| luna_core::LunaError::new(err.to_string()))?;
        let event = with_stored_event_hash(event.clone())?;
        let line = serde_json::to_string(&event).map_err(|err| LunaError::new(err.to_string()))?;
        writeln!(file, "{line}").map_err(|err| luna_core::LunaError::new(err.to_string()))?;
        Ok(())
    }

    pub fn load(&self) -> Result<Vec<StoredEvent>> {
        load_jsonl_events(&self.path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn load_jsonl_events(path: &Path) -> Result<Vec<StoredEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path).map_err(|err| luna_core::LunaError::new(err.to_string()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|err| luna_core::LunaError::new(err.to_string()))?;
        if line.trim().is_empty() {
            continue;
        }
        let mut event: StoredEvent =
            serde_json::from_str(&line).map_err(|err| LunaError::new(err.to_string()))?;
        match event.event_hash.as_deref() {
            Some(stored) => {
                let recomputed = stable_stored_event_hash(&event)?;
                if stored != recomputed {
                    return Err(LunaError::new(format!(
                        "stored runtime event {} hash mismatch",
                        event.event_id
                    )));
                }
            }
            None => {
                event.event_hash = Some(stable_stored_event_hash(&event)?);
            }
        }
        events.push(event);
    }
    Ok(events)
}

pub fn load_jsonl_events_strict(path: &Path) -> Result<Vec<StoredEvent>> {
    if !path.exists() {
        return Err(LunaError::new(format!(
            "runtime event log {} does not exist",
            path.display()
        )));
    }
    let file = fs::File::open(path).map_err(|err| luna_core::LunaError::new(err.to_string()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|err| luna_core::LunaError::new(err.to_string()))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: StoredEvent =
            serde_json::from_str(&line).map_err(|err| LunaError::new(err.to_string()))?;
        let Some(stored) = event.event_hash.as_deref() else {
            return Err(LunaError::new(format!(
                "stored runtime event {} is missing event_hash",
                event.event_id
            )));
        };
        let recomputed = stable_stored_event_hash(&event)?;
        if stored != recomputed {
            return Err(LunaError::new(format!(
                "stored runtime event {} hash mismatch",
                event.event_id
            )));
        }
        events.push(event);
    }
    if events.is_empty() {
        return Err(LunaError::new(format!(
            "runtime event log {} has no auditable events",
            path.display()
        )));
    }
    Ok(events)
}

pub fn with_stored_event_hash(mut event: StoredEvent) -> Result<StoredEvent> {
    event.event_hash = None;
    event.event_hash = Some(stable_stored_event_hash(&event)?);
    Ok(event)
}

pub fn stable_stored_event_hash(event: &StoredEvent) -> Result<String> {
    let value = json!({
        "event_id": event.event_id,
        "episode_id": event.episode_id,
        "turn_id": event.turn_id,
        "timestamp": event.timestamp,
        "source": event.source,
        "confidence": event.confidence,
        "payload": event.payload,
    });
    let bytes = serde_json::to_vec(&value).map_err(|err| LunaError::new(err.to_string()))?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use luna_core::{ConversationTurn, Role};
    use std::fs;

    #[test]
    fn replay_is_deterministic_for_100_events() {
        let dir = std::env::temp_dir().join(format!("luna_events_{}", uuid::Uuid::new_v4()));
        let path = dir.join("events.jsonl");
        let log = JsonlEventLog::new(&path);

        for index in 0..100 {
            let event = EventEnvelope::new(
                LunaEvent::TurnObserved(TurnObserved {
                    turn: ConversationTurn {
                        role: Role::User,
                        content: format!("turn {index}"),
                        timestamp: None,
                    },
                }),
                EventSource::User,
                1.0,
            );
            log.append(&event).unwrap();
        }

        let first = log.load().unwrap();
        let second = log.load().unwrap();
        let first_bytes = serde_json::to_vec(&first).unwrap();
        let second_bytes = serde_json::to_vec(&second).unwrap();

        assert_eq!(first_bytes, second_bytes);
        assert_eq!(first.len(), 100);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn append_persists_and_load_verifies_event_hash() {
        let dir = std::env::temp_dir().join(format!("luna_events_{}", uuid::Uuid::new_v4()));
        let path = dir.join("events.jsonl");
        let log = JsonlEventLog::new(&path);
        let event = EventEnvelope::new(
            LunaEvent::TurnObserved(TurnObserved {
                turn: ConversationTurn {
                    role: Role::User,
                    content: "Chris lives in Iowa.".to_string(),
                    timestamp: None,
                },
            }),
            EventSource::User,
            1.0,
        );

        log.append(&event).unwrap();
        let loaded = log.load().unwrap();

        assert!(loaded[0]
            .event_hash
            .as_deref()
            .is_some_and(|hash| hash.len() == 64));

        let mut value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        value["payload"]["data"]["turn"]["content"] =
            serde_json::Value::String("Chris lives in Ohio.".to_string());
        fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();

        let error = log.load().unwrap_err();
        assert!(error.to_string().contains("hash mismatch"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn strict_load_rejects_missing_hash_and_empty_logs() {
        let dir = std::env::temp_dir().join(format!("luna_events_{}", uuid::Uuid::new_v4()));
        let path = dir.join("events.jsonl");
        fs::create_dir_all(&dir).unwrap();

        fs::write(&path, "").unwrap();
        let empty_error = load_jsonl_events_strict(&path).unwrap_err();
        assert!(empty_error.to_string().contains("no auditable events"));

        let event = EventEnvelope::new(
            LunaEvent::TurnObserved(TurnObserved {
                turn: ConversationTurn {
                    role: Role::User,
                    content: "Chris lives in Iowa.".to_string(),
                    timestamp: None,
                },
            }),
            EventSource::User,
            1.0,
        );
        fs::write(&path, serde_json::to_string(&event).unwrap()).unwrap();

        let missing_hash_error = load_jsonl_events_strict(&path).unwrap_err();
        assert!(missing_hash_error
            .to_string()
            .contains("missing event_hash"));

        let _ = fs::remove_dir_all(dir);
    }
}
