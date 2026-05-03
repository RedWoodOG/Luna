//! [`LunaExtractor`] — public entry that combines the LLM extractor
//! (PR 0.3) with the deterministic second-source detectors and the
//! fusion step (PR 0.4) to produce a final
//! [`luna_core::CognitiveObservation`] for each turn.
//!
//! Strictly additive in PR 0.4: the existing
//! [`crate::FusedExtractor`] and friends keep working unchanged.
//! `luna-bench` still uses the old path. PR 0.5 (`bench formation`)
//! is what wires `LunaExtractor` into the bench layer.

use crate::{
    backend::LlmBackend,
    cache::ExtractionCache,
    extractor::LlmExtractor,
    fusion::fuse_observation,
    second_source::{default_v1_sources, SecondSource},
};
use luna_core::{CognitiveObservation, ConversationTurn, Result};

pub struct LunaExtractor<B: LlmBackend, C: ExtractionCache> {
    llm: LlmExtractor<B, C>,
    sources: Vec<Box<dyn SecondSource>>,
}

impl<B: LlmBackend, C: ExtractionCache> LunaExtractor<B, C> {
    /// Construct with explicit detector list. Use this from tests
    /// when you want a subset of detectors active.
    pub fn new(llm: LlmExtractor<B, C>, sources: Vec<Box<dyn SecondSource>>) -> Self {
        Self { llm, sources }
    }

    /// Construct with the four canonical v0.1 detectors:
    /// `TemporalDetector`, `AffectLexicon`,
    /// `FirstPersonIdentityDetector`, `GoalPhraseLexicon`.
    pub fn with_default_v1_sources(llm: LlmExtractor<B, C>) -> Self {
        Self::new(llm, default_v1_sources())
    }

    pub fn llm(&self) -> &LlmExtractor<B, C> {
        &self.llm
    }

    /// Extract a turn through the full pipeline:
    /// LLM extract -> validate -> cache write -> detector pass -> fuse.
    /// Errors from the LLM extraction step propagate; the fusion step
    /// itself is total (it cannot fail given a validated
    /// [`crate::LlmObservation`]).
    pub fn extract(&self, turn: &ConversationTurn) -> Result<CognitiveObservation> {
        let llm_obs = self.llm.extract(turn)?;
        Ok(fuse_observation(&llm_obs, &self.sources, turn))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::RecordingFakeBackend,
        cache::FileExtractionCache,
        llm_observation::{LlmAssertion, LlmObservation, LlmSignal},
    };
    use chrono::{TimeZone, Utc};
    use luna_core::SignalReliability;
    use std::{collections::BTreeMap, fs, path::PathBuf};

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("luna_full_extractor_{}", uuid::Uuid::new_v4()))
    }

    fn llm_signal(value: f32) -> LlmSignal {
        LlmSignal {
            value,
            confidence: 0.8,
            reliability: "learned".to_string(),
            evidence: None,
        }
    }

    fn empty_signals() -> BTreeMap<String, Option<LlmSignal>> {
        let mut s = BTreeMap::new();
        s.insert("temporal_relevance".to_string(), None);
        s.insert("emotional_arousal".to_string(), None);
        s.insert("identity_relevance".to_string(), None);
        s.insert("goal_pressure".to_string(), None);
        s
    }

    fn response(observation: &LlmObservation) -> String {
        serde_json::to_string(observation).unwrap()
    }

    #[test]
    fn end_to_end_full_pipeline_fuses_temporal_dimension() {
        let mut signals = empty_signals();
        signals.insert("temporal_relevance".to_string(), Some(llm_signal(0.85)));
        let llm_obs = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "work".to_string(),
                kind: "current_stressor".to_string(),
                value: "client deadline".to_string(),
                confidence: 0.8,
                evidence_span: Some("client deadline".to_string()),
            }],
            signals,
        };

        let root = temp_root();
        let fake = RecordingFakeBackend::new("test-model@v1");
        fake.expect("client deadline", &response(&llm_obs));
        let llm = LlmExtractor::new(fake, FileExtractionCache::new(&root));
        let extractor = LunaExtractor::with_default_v1_sources(llm);

        let mut turn = ConversationTurn::user(
            "Yesterday the client deadline had me tense.",
        );
        turn.timestamp = Some(Utc.with_ymd_and_hms(2026, 5, 3, 10, 0, 0).unwrap());

        let observation = extractor.extract(&turn).unwrap();
        let signal = observation
            .temporal_relevance
            .expect("temporal should fuse end-to-end");
        assert_eq!(signal.source_count(), 2);
        assert_eq!(signal.reliability(), SignalReliability::Learned);
        assert_eq!(observation.assertions.len(), 1);
        assert_eq!(observation.assertions[0].value, "client deadline");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn end_to_end_no_lexical_cue_leaves_dimension_none_even_when_llm_signals() {
        // LLM signals temporal_relevance, but the turn has no temporal
        // cue. The detector emits nothing, so the second-source slot
        // stays empty and the two-source rule denies the dimension.
        let mut signals = empty_signals();
        signals.insert("temporal_relevance".to_string(), Some(llm_signal(0.9)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };

        let root = temp_root();
        let fake = RecordingFakeBackend::new("test-model@v1");
        fake.expect("the sky is blue", &response(&llm_obs));
        let llm = LlmExtractor::new(fake, FileExtractionCache::new(&root));
        let extractor = LunaExtractor::with_default_v1_sources(llm);

        let turn = ConversationTurn::user("the sky is blue");
        let observation = extractor.extract(&turn).unwrap();
        assert!(
            observation.temporal_relevance.is_none(),
            "no temporal cue in turn -> no second source -> two-source rule denies"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn end_to_end_propagates_validation_errors_without_fusing() {
        // LLM returns a validation-failing observation (unknown
        // domain). Pipeline must error out before fusion runs.
        let bad_obs = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "vibe".to_string(),
                kind: "profession".to_string(),
                value: "engineer".to_string(),
                confidence: 0.5,
                evidence_span: None,
            }],
            signals: empty_signals(),
        };

        let root = temp_root();
        let fake = RecordingFakeBackend::new("test-model@v1");
        fake.expect("engineer", &response(&bad_obs));
        let llm = LlmExtractor::new(fake, FileExtractionCache::new(&root));
        let extractor = LunaExtractor::with_default_v1_sources(llm);

        let turn = ConversationTurn::user("I work as an engineer.");
        let result = extractor.extract(&turn);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn extractor_with_subset_sources_isolates_per_dimension_fusion() {
        // Construct with only TemporalDetector. Even though the turn
        // and LLM both signal identity, identity_relevance must stay
        // None because no identity detector is in the list.
        let mut signals = empty_signals();
        signals.insert("temporal_relevance".to_string(), Some(llm_signal(0.9)));
        signals.insert("identity_relevance".to_string(), Some(llm_signal(0.9)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };

        let root = temp_root();
        let fake = RecordingFakeBackend::new("test-model@v1");
        fake.expect("yesterday", &response(&llm_obs));
        let llm = LlmExtractor::new(fake, FileExtractionCache::new(&root));
        let extractor = LunaExtractor::new(
            llm,
            vec![Box::new(crate::second_source::TemporalDetector::new())],
        );

        let turn =
            ConversationTurn::user("Yesterday I am a mechanical engineer.");
        let observation = extractor.extract(&turn).unwrap();
        assert!(observation.temporal_relevance.is_some());
        assert!(observation.identity_relevance.is_none());

        let _ = fs::remove_dir_all(&root);
    }
}
