//! Cache-first extraction flow.
//!
//! Wires [`LlmBackend`], [`ExtractionCache`], the prompt template, and
//! validation together.
//!
//! ```text
//!   ConversationTurn
//!      |
//!      | build prompt + cache key
//!      v
//!   cache.get(key)? --hit--> return LlmObservation
//!      |
//!      | miss
//!      v
//!   backend.complete(request)
//!      |
//!      | parse JSON
//!      v
//!   validate_against_prompt_v3
//!      |
//!      | aggregate violations -> error if any
//!      v
//!   cache.put(key, observation)  (only on success)
//!      |
//!      v
//!   return LlmObservation
//! ```
//!
//! Invalid JSON, parse errors, and validation failures all propagate
//! to the caller and never write to the cache. PR 0.5's `bench
//! formation` decides retry / loud-fail policy on top of these errors.

use crate::{
    backend::{LlmBackend, LlmRequest},
    cache::{CacheKey, ExtractionCache},
    llm_observation::{validate_against_prompt_v3, LlmObservation, EXTRACTION_SCHEMA_VERSION},
    prompt::{build_prompt_v3, prompt_v3_hash},
};
use luna_core::{ConversationTurn, LunaError, Result};

pub struct LlmExtractor<B: LlmBackend, C: ExtractionCache> {
    backend: B,
    cache: C,
}

impl<B: LlmBackend, C: ExtractionCache> LlmExtractor<B, C> {
    pub fn new(backend: B, cache: C) -> Self {
        Self { backend, cache }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn cache(&self) -> &C {
        &self.cache
    }

    /// Extract a single turn. See module docs for the flow.
    pub fn extract(&self, turn: &ConversationTurn) -> Result<LlmObservation> {
        let key = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            self.backend.model_id(),
            prompt_v3_hash(),
            &turn.content,
            turn.timestamp,
        );

        if let Some(cached) = self.cache.get(&key)? {
            return Ok(cached);
        }

        let request = LlmRequest {
            prompt: build_prompt_v3(turn),
        };
        let raw = self.backend.complete(&request)?;
        let observation: LlmObservation = serde_json::from_str(&raw)
            .map_err(|err| LunaError::new(format!("backend output is not valid JSON: {err}")))?;

        let violations = validate_against_prompt_v3(&observation);
        if !violations.is_empty() {
            return Err(LunaError::new(format!(
                "extraction validation failed ({}): {}",
                violations.len(),
                violations.join("; ")
            )));
        }

        self.cache.put(&key, &observation)?;
        Ok(observation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::RecordingFakeBackend,
        cache::FileExtractionCache,
        llm_observation::{LlmAssertion, LlmSignal},
    };
    use chrono::{TimeZone, Utc};
    use std::{collections::BTreeMap, fs, path::PathBuf};

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("luna_extractor_{}", uuid::Uuid::new_v4()))
    }

    fn well_formed_response() -> String {
        // Hand-crafted to satisfy validate_against_prompt_v3: valid
        // domain/kind, all confidences in [0,1], reliability "learned".
        let observation = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "identity".to_string(),
                kind: "profession".to_string(),
                value: "mechanical engineer".to_string(),
                confidence: 0.92,
                evidence_span: Some("mechanical engineer".to_string()),
            }],
            signals: BTreeMap::from([
                (
                    "identity_relevance".to_string(),
                    Some(LlmSignal {
                        value: 0.88,
                        confidence: 0.8,
                        reliability: "learned".to_string(),
                        evidence: Some("I work as".to_string()),
                    }),
                ),
                ("temporal_relevance".to_string(), None),
                ("emotional_arousal".to_string(), None),
                ("goal_pressure".to_string(), None),
            ]),
        };
        serde_json::to_string(&observation).unwrap()
    }

    fn invalid_domain_response() -> String {
        // domain "vibe" is not in the prompt-v1 allowlist; passes
        // serde parsing, fails validate_against_prompt_v3.
        let observation = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "vibe".to_string(),
                kind: "profession".to_string(),
                value: "mechanical engineer".to_string(),
                confidence: 0.5,
                evidence_span: None,
            }],
            signals: BTreeMap::from([
                ("identity_relevance".to_string(), None),
                ("temporal_relevance".to_string(), None),
                ("emotional_arousal".to_string(), None),
                ("goal_pressure".to_string(), None),
            ]),
        };
        serde_json::to_string(&observation).unwrap()
    }

    #[test]
    fn extract_cache_miss_calls_backend_and_writes_cache() {
        let root = temp_root();
        let fake = RecordingFakeBackend::new("test-model@v1");
        fake.expect("mechanical engineer", &well_formed_response());
        let extractor = LlmExtractor::new(fake.clone(), FileExtractionCache::new(&root));

        let turn = ConversationTurn::user("I work as a mechanical engineer.");
        let observation = extractor.extract(&turn).unwrap();
        assert_eq!(observation.assertions[0].value, "mechanical engineer");
        assert_eq!(fake.call_count(), 1);

        let key = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            "test-model@v1",
            prompt_v3_hash(),
            &turn.content,
            turn.timestamp,
        );
        let cache_path = extractor.cache().path_for(&key);
        assert!(cache_path.exists(), "cache file should exist after miss");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn extract_cache_hit_does_not_call_backend() {
        let root = temp_root();
        let fake = RecordingFakeBackend::new("test-model@v1");
        fake.expect("mechanical engineer", &well_formed_response());
        let extractor = LlmExtractor::new(fake.clone(), FileExtractionCache::new(&root));

        let turn = ConversationTurn::user("I work as a mechanical engineer.");
        let _ = extractor.extract(&turn).unwrap();
        assert_eq!(fake.call_count(), 1);
        // Second extraction must hit cache, not backend.
        let _ = extractor.extract(&turn).unwrap();
        assert_eq!(fake.call_count(), 1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn different_timestamp_causes_cache_miss() {
        let root = temp_root();
        let fake = RecordingFakeBackend::new("test-model@v1");
        fake.expect("mechanical engineer", &well_formed_response());
        let extractor = LlmExtractor::new(fake.clone(), FileExtractionCache::new(&root));

        let mut turn = ConversationTurn::user("I work as a mechanical engineer.");
        turn.timestamp = Some(Utc.with_ymd_and_hms(2026, 5, 3, 10, 0, 0).unwrap());
        let _ = extractor.extract(&turn).unwrap();
        assert_eq!(fake.call_count(), 1);

        // Same content, different timestamp: cache key changes, so the
        // backend gets called again.
        turn.timestamp = Some(Utc.with_ymd_and_hms(2026, 5, 3, 11, 0, 0).unwrap());
        let _ = extractor.extract(&turn).unwrap();
        assert_eq!(fake.call_count(), 2);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_json_does_not_write_cache() {
        let root = temp_root();
        let fake = RecordingFakeBackend::new("test-model@v1");
        fake.expect("mechanical engineer", "not valid json {");
        let extractor = LlmExtractor::new(fake.clone(), FileExtractionCache::new(&root));

        let turn = ConversationTurn::user("I work as a mechanical engineer.");
        let result = extractor.extract(&turn);
        assert!(result.is_err());

        let key = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            "test-model@v1",
            prompt_v3_hash(),
            &turn.content,
            turn.timestamp,
        );
        assert!(
            !extractor.cache().path_for(&key).exists(),
            "cache must not be populated when backend output fails to parse"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn validation_failure_does_not_write_cache() {
        let root = temp_root();
        let fake = RecordingFakeBackend::new("test-model@v1");
        fake.expect("mechanical engineer", &invalid_domain_response());
        let extractor = LlmExtractor::new(fake.clone(), FileExtractionCache::new(&root));

        let turn = ConversationTurn::user("I work as a mechanical engineer.");
        let result = extractor.extract(&turn);
        assert!(result.is_err(), "expected validation error");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("validation failed"));

        let key = CacheKey::compute(
            EXTRACTION_SCHEMA_VERSION,
            "test-model@v1",
            prompt_v3_hash(),
            &turn.content,
            turn.timestamp,
        );
        assert!(
            !extractor.cache().path_for(&key).exists(),
            "cache must not be populated when validation fails"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn different_model_id_causes_cache_miss_even_with_same_content() {
        let root = temp_root();
        let fake_a = RecordingFakeBackend::new("model-A");
        let fake_b = RecordingFakeBackend::new("model-B");
        fake_a.expect("mechanical engineer", &well_formed_response());
        fake_b.expect("mechanical engineer", &well_formed_response());

        let extractor_a = LlmExtractor::new(fake_a.clone(), FileExtractionCache::new(&root));
        let extractor_b = LlmExtractor::new(fake_b.clone(), FileExtractionCache::new(&root));

        let turn = ConversationTurn::user("I work as a mechanical engineer.");
        let _ = extractor_a.extract(&turn).unwrap();
        let _ = extractor_b.extract(&turn).unwrap();

        // Each backend got its own miss; neither short-circuited the
        // other's cache entry.
        assert_eq!(fake_a.call_count(), 1);
        assert_eq!(fake_b.call_count(), 1);

        let _ = fs::remove_dir_all(&root);
    }
}
