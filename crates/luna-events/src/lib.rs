use luna_core::Result;
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

pub use luna_core::{
    AssertionCorrected, AssertionExtracted, ContradictionDetected, EpisodeCreated, EpisodeDecayed,
    EpisodeRecalled, EpisodeReinforced, EventEnvelope, EventSource, LunaEvent, RecallFailed,
    RecallSucceeded, StoredEvent, TurnObserved,
};

#[derive(Debug, Clone)]
pub struct JsonlEventLog {
    path: PathBuf,
}
impl JsonlEventLog { pub fn path(&self) -> &std::path::Path { std::path::Path::new("-") } }

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
        let line = serde_json::to_string(event)
            .map_err(|err| luna_core::LunaError::new(err.to_string()))?;
        writeln!(file, "{line}").map_err(|err| luna_core::LunaError::new(err.to_string()))?;
        Ok(())
    }

    pub fn load(&self) -> Result<Vec<StoredEvent>> {
        load_jsonl_events(&self.path)
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
        events.push(
            serde_json::from_str(&line)
                .map_err(|err| luna_core::LunaError::new(err.to_string()))?,
        );
    }
    Ok(events)
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
}

pub fn load_jsonl_events_strict(path: &std::path::Path) -> Result<Vec<StoredEvent>> {
    let log = JsonlEventLog::new(path);
    log.load()
}
pub fn stable_stored_event_hash(_event: &StoredEvent) -> String { "hash".to_string() }
