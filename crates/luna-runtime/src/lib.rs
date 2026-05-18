// Luna Runtime — minimal compilable skeleton
// Full implementations in lattice.rs, bonds.rs, scenario.rs, topology_bridge.rs
// are stubbed pending type harmonization.

use chrono::{DateTime, Utc};
use luna_core::*;
use luna_events::{stable_stored_event_hash, JsonlEventLog};
use luna_extract::LunaExtractor;
use luna_recall::SimilarityRecallEngine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod scenario;
pub mod topology_bridge;
pub mod lattice;
pub mod bonds;

pub type LunaError = anyhow::Error;
pub type Result<T> = std::result::Result<T, LunaError>;

#[derive(Debug, Clone)]
pub struct RuntimeSession {
    log: JsonlEventLog,
}

impl RuntimeSession {
    pub fn new(log_path: impl Into<PathBuf>) -> Self {
        Self { log: JsonlEventLog::new(log_path) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTurnResult {
    pub turn_id: Option<uuid::Uuid>,
    pub recalled: RecallSet,
    pub intake: MemoryIntakeDecision,
    pub output_packet: OutputPacket,
    pub turn_receipt: RuntimeTurnReceipt,
}
