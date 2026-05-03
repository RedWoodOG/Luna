//! Content-addressed cache for [`crate::llm_observation::LlmObservation`].
//!
//! Cache keys are SHA-256 of the four input fields plus the turn's
//! event-time. Including `turn_timestamp` is load-bearing: the same
//! turn content can carry different temporal meaning at different
//! event-times, and the temporal axis depends on that. A cache that
//! ignored timestamp would silently confuse two real cases that
//! happen to share content.
//!
//! Storage is file-system based, sharded by the first two hex
//! characters of the key so directory listings stay manageable as the
//! cache grows. The default location is `.luna/extraction_cache/` at
//! the repo root, gitignored.

use crate::llm_observation::LlmObservation;
use chrono::{DateTime, SecondsFormat, Utc};
use luna_core::{LunaError, Result};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

/// Stable, content-addressed identifier for a single extraction.
///
/// Constructed by [`CacheKey::compute`]. Two computations that pass the
/// same five inputs produce the same key; changing any one — including
/// `turn_timestamp` — produces a different key. There is no collision-
/// avoidance trick beyond null-byte separators between the four
/// variable-length string fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey([u8; 32]);

impl CacheKey {
    pub fn compute(
        schema_version: u32,
        model_id: &str,
        prompt_hash: &str,
        turn_content: &str,
        turn_timestamp: Option<DateTime<Utc>>,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(schema_version.to_be_bytes());
        hasher.update(b"\0");
        hasher.update(model_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(prompt_hash.as_bytes());
        hasher.update(b"\0");
        hasher.update(turn_content.as_bytes());
        hasher.update(b"\0");
        let timestamp_repr = match turn_timestamp {
            Some(timestamp) => timestamp.to_rfc3339_opts(SecondsFormat::Secs, true),
            None => String::new(),
        };
        hasher.update(timestamp_repr.as_bytes());
        Self(hasher.finalize().into())
    }

    pub fn hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for byte in &self.0 {
            write!(&mut out, "{:02x}", byte).expect("write to String");
        }
        out
    }
}

pub trait ExtractionCache {
    fn get(&self, key: &CacheKey) -> Result<Option<LlmObservation>>;
    fn put(&self, key: &CacheKey, observation: &LlmObservation) -> Result<()>;
}

/// File-system implementation. Layout:
///
/// ```text
/// {root}/
///   ab/
///     ab12cd34...{full hex}.json
/// ```
///
/// `root` is created on the first `put`; `get` returns `None` for
/// missing keys without erroring.
pub struct FileExtractionCache {
    root: PathBuf,
}

impl FileExtractionCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path_for(&self, key: &CacheKey) -> PathBuf {
        let hex = key.hex();
        self.root.join(&hex[..2]).join(format!("{hex}.json"))
    }
}

impl ExtractionCache for FileExtractionCache {
    fn get(&self, key: &CacheKey) -> Result<Option<LlmObservation>> {
        let path = self.path_for(key);
        if !path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|err| LunaError::new(err.to_string()))?;
        let observation =
            serde_json::from_str(&text).map_err(|err| LunaError::new(err.to_string()))?;
        Ok(Some(observation))
    }

    fn put(&self, key: &CacheKey, observation: &LlmObservation) -> Result<()> {
        let path = self.path_for(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| LunaError::new(err.to_string()))?;
        }
        let mut text = serde_json::to_string_pretty(observation)
            .map_err(|err| LunaError::new(err.to_string()))?;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        fs::write(&path, text).map_err(|err| LunaError::new(err.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_observation::{LlmAssertion, LlmObservation, LlmSignal, EXTRACTION_SCHEMA_VERSION};
    use chrono::TimeZone;
    use std::collections::BTreeMap;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("luna_cache_{}", uuid::Uuid::new_v4()))
    }

    fn sample_observation() -> LlmObservation {
        let mut signals = BTreeMap::new();
        signals.insert(
            "temporal_relevance".to_string(),
            Some(LlmSignal {
                value: 0.86,
                confidence: 0.78,
                reliability: "learned".to_string(),
                evidence: Some("recently".to_string()),
            }),
        );
        signals.insert("goal_pressure".to_string(), None);
        LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "work".to_string(),
                kind: "current_stressor".to_string(),
                value: "client deadline".to_string(),
                confidence: 0.74,
                evidence_span: Some("the client deadline has been weighing on me".to_string()),
            }],
            signals,
        }
    }

    fn baseline_key() -> CacheKey {
        CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            "stub-model",
            "stub-prompt-hash",
            "I work as a mechanical engineer.",
            Some(Utc.with_ymd_and_hms(2026, 5, 3, 10, 0, 0).unwrap()),
        )
    }

    #[test]
    fn key_is_deterministic_for_identical_inputs() {
        assert_eq!(baseline_key(), baseline_key());
    }

    #[test]
    fn key_changes_with_schema_version() {
        let other = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION + 1,
            "stub-model",
            "stub-prompt-hash",
            "I work as a mechanical engineer.",
            Some(Utc.with_ymd_and_hms(2026, 5, 3, 10, 0, 0).unwrap()),
        );
        assert_ne!(baseline_key(), other);
    }

    #[test]
    fn key_changes_with_model_id() {
        let other = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            "different-model",
            "stub-prompt-hash",
            "I work as a mechanical engineer.",
            Some(Utc.with_ymd_and_hms(2026, 5, 3, 10, 0, 0).unwrap()),
        );
        assert_ne!(baseline_key(), other);
    }

    #[test]
    fn key_changes_with_prompt_hash() {
        let other = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            "stub-model",
            "different-prompt",
            "I work as a mechanical engineer.",
            Some(Utc.with_ymd_and_hms(2026, 5, 3, 10, 0, 0).unwrap()),
        );
        assert_ne!(baseline_key(), other);
    }

    #[test]
    fn key_changes_with_turn_content() {
        let other = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            "stub-model",
            "stub-prompt-hash",
            "I work as a software engineer.",
            Some(Utc.with_ymd_and_hms(2026, 5, 3, 10, 0, 0).unwrap()),
        );
        assert_ne!(baseline_key(), other);
    }

    #[test]
    fn key_changes_with_turn_timestamp() {
        // The amendment that distinguishes PR 0.2 from a naive cache:
        // identical turn content with different event-time produces a
        // different key, because temporal extraction reads timestamp as
        // input rather than metadata.
        let other = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            "stub-model",
            "stub-prompt-hash",
            "I work as a mechanical engineer.",
            Some(Utc.with_ymd_and_hms(2026, 5, 3, 11, 0, 0).unwrap()),
        );
        assert_ne!(baseline_key(), other);
    }

    #[test]
    fn key_distinguishes_some_timestamp_from_none() {
        let with_ts = baseline_key();
        let without_ts = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            "stub-model",
            "stub-prompt-hash",
            "I work as a mechanical engineer.",
            None,
        );
        assert_ne!(with_ts, without_ts);
    }

    #[test]
    fn hex_is_64_lowercase_chars() {
        let hex = baseline_key().hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn file_cache_round_trips() {
        let root = temp_root();
        let cache = FileExtractionCache::new(&root);
        let key = baseline_key();
        let observation = sample_observation();
        cache.put(&key, &observation).unwrap();
        let loaded = cache.get(&key).unwrap();
        assert_eq!(loaded, Some(observation));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn file_cache_get_missing_returns_none() {
        let root = temp_root();
        let cache = FileExtractionCache::new(&root);
        let result = cache.get(&baseline_key()).unwrap();
        assert!(result.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn file_cache_uses_two_char_shard_layout() {
        let root = temp_root();
        let cache = FileExtractionCache::new(&root);
        let key = baseline_key();
        let path = cache.path_for(&key);
        let hex = key.hex();
        assert!(path.starts_with(&root));
        let parent = path.parent().unwrap().file_name().unwrap();
        assert_eq!(parent.to_string_lossy(), hex[..2]);
        let file = path.file_name().unwrap();
        assert_eq!(file.to_string_lossy(), format!("{hex}.json"));
    }

    #[test]
    fn file_cache_writes_canonical_bytes_for_same_observation() {
        // Repeated put of the same observation must produce byte-identical
        // file contents. This is the property the cache relies on for
        // determinism gate 5 in the formation report.
        let root = temp_root();
        let cache = FileExtractionCache::new(&root);
        let key = baseline_key();
        let observation = sample_observation();

        cache.put(&key, &observation).unwrap();
        let first = fs::read(cache.path_for(&key)).unwrap();
        cache.put(&key, &observation).unwrap();
        let second = fs::read(cache.path_for(&key)).unwrap();
        assert_eq!(first, second);

        // Construct an equivalent observation with a different signal-
        // insertion order; bytes must still be identical because BTreeMap
        // sorts on serialize.
        let mut signals_other_order = BTreeMap::new();
        signals_other_order.insert("goal_pressure".to_string(), None);
        signals_other_order.insert(
            "temporal_relevance".to_string(),
            observation
                .signals
                .get("temporal_relevance")
                .cloned()
                .unwrap(),
        );
        let reordered = LlmObservation {
            assertions: observation.assertions.clone(),
            signals: signals_other_order,
        };
        cache.put(&key, &reordered).unwrap();
        let third = fs::read(cache.path_for(&key)).unwrap();
        assert_eq!(first, third);

        let _ = fs::remove_dir_all(&root);
    }
}
