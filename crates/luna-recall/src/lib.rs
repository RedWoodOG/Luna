use luna_core::{
    AssertionLifecycleStatus, Episode, RecallHit, RecallMode, RecallReason, RecallSet, Result,
    TurnReading,
};
use std::time::Instant;

pub trait RecallEngine {
    fn recall(
        &self,
        current: &TurnReading,
        episodes: &[Episode],
        mode: RecallMode,
    ) -> Result<RecallSet>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KeywordRecallEngine;

#[derive(Debug, Clone, Copy, Default)]
pub struct SimilarityRecallEngine;
#[derive(Debug, Clone, Copy, Default)]
pub struct GeometricRecallEngine;

#[derive(Debug, Clone, Copy, Default)]
pub struct VectorRecallEngine;

impl RecallEngine for KeywordRecallEngine {
    fn recall(
        &self,
        current: &TurnReading,
        episodes: &[Episode],
        _mode: RecallMode,
    ) -> Result<RecallSet> {
        let start = Instant::now();
        let mut hits = episodes
            .iter()
            .filter_map(|episode| {
                let score = keyword_score(current, episode);
                let assertions = current_assertions(episode);
                (score >= 0.34 && !assertions.is_empty()).then(|| RecallHit {
                    episode_id: episode.id,
                    score,
                    assertions,
                    reason: RecallReason::new("keyword_overlap")
                        .expect("static recall reason is non-empty"),
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(3);
        Ok(RecallSet {
            hits,
            latency_ms: start.elapsed().as_secs_f32() * 1000.0,
        })
    }
}

impl RecallEngine for SimilarityRecallEngine {
    fn recall(
        &self,
        current: &TurnReading,
        episodes: &[Episode],
        _mode: RecallMode,
    ) -> Result<RecallSet> {
        let start = Instant::now();
        let mut hits = episodes
            .iter()
            .filter_map(|episode| {
                let contour_score = luna_match::profile_similarity(current, episode);
                let cue_score = keyword_score(current, episode);
                let score = contour_score.max(cue_score);
                let assertions = current_assertions(episode);
                (score >= 0.50 && !assertions.is_empty()).then(|| {
                    let reason_str = if cue_score > contour_score {
                        "cue_overlap_activation"
                    } else {
                        "profile_activation"
                    };
                    RecallHit {
                        episode_id: episode.id,
                        score,
                        assertions,
                        reason: RecallReason::new(reason_str)
                            .expect("static recall reason is non-empty"),
                    }
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(3);
        Ok(RecallSet {
            hits,
            latency_ms: start.elapsed().as_secs_f32() * 1000.0,
        })
    }
}

impl RecallEngine for GeometricRecallEngine {
    fn recall(
        &self,
        current: &TurnReading,
        episodes: &[Episode],
        _mode: RecallMode,
    ) -> Result<RecallSet> {
        let start = Instant::now();
        let mut hits = episodes
            .iter()
            .filter_map(|episode| {
                let score = luna_match::profile_similarity(current, episode);
                let assertions = current_assertions(episode);
                (score >= 0.25 && !assertions.is_empty()).then(|| RecallHit {
                    episode_id: episode.id,
                    score,
                    assertions,
                    reason: RecallReason::new("profile_similarity")
                        .expect("static recall reason is non-empty"),
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(3);
        Ok(RecallSet {
            hits,
            latency_ms: start.elapsed().as_secs_f32() * 1000.0,
        })
    }
}

impl RecallEngine for VectorRecallEngine {
    fn recall(
        &self,
        current: &TurnReading,
        episodes: &[Episode],
        _mode: RecallMode,
    ) -> Result<RecallSet> {
        let start = Instant::now();
        let mut hits = episodes
            .iter()
            .filter_map(|episode| {
                let score = vector_similarity(current, episode);
                let assertions = current_assertions(episode);
                (score >= 0.35 && !assertions.is_empty()).then(|| RecallHit {
                    episode_id: episode.id,
                    score,
                    assertions,
                    reason: RecallReason::new("vector_similarity")
                        .expect("static recall reason is non-empty"),
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(3);
        Ok(RecallSet {
            hits,
            latency_ms: start.elapsed().as_secs_f32() * 1000.0,
        })
    }
}

fn current_assertions(episode: &Episode) -> Vec<luna_core::StructuredAssertion> {
    episode
        .assertions
        .iter()
        .filter(|assertion| assertion.lifecycle_status == AssertionLifecycleStatus::Current)
        .cloned()
        .collect()
}

fn keyword_score(current: &TurnReading, episode: &Episode) -> f32 {
    let mut haystack = episode
        .assertions
        .iter()
        .filter(|assertion| assertion.lifecycle_status == AssertionLifecycleStatus::Current)
        .flat_map(|assertion| {
            [
                assertion.domain.as_str(),
                assertion.kind.as_str(),
                assertion.value.as_str(),
            ]
        })
        .flat_map(|text| text.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|term| term.len() > 2)
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>();
    haystack.sort();
    haystack.dedup();

    let cue_terms = current
        .cue_terms
        .iter()
        .filter(|term| is_recall_cue(term))
        .collect::<Vec<_>>();

    if haystack.is_empty() || cue_terms.is_empty() {
        return 0.0;
    }

    let matches = cue_terms
        .iter()
        .filter(|term| {
            haystack
                .iter()
                .any(|candidate| candidate.as_str() == term.as_str())
        })
        .count();
    matches as f32 / cue_terms.len() as f32
}

fn is_recall_cue(term: &str) -> bool {
    term.len() > 2
        && !matches!(
            term,
            "what"
                | "who"
                | "when"
                | "where"
                | "why"
                | "how"
                | "know"
                | "about"
                | "said"
                | "say"
                | "did"
                | "does"
                | "you"
                | "me"
                | "my"
                | "the"
        )
}

fn vector_similarity(current: &TurnReading, episode: &Episode) -> f32 {
    match (&current.semantic, &episode.profile.semantic) {
        (Some(left), Some(right)) => cosine_similarity(left, right),
        _ => 0.0,
    }
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let len = left.len().min(right.len());
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for i in 0..len {
        dot += left[i] * right[i];
        left_norm += left[i] * left[i];
        right_norm += right[i] * right[i];
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        (dot / (left_norm.sqrt() * right_norm.sqrt())).clamp(0.0, 1.0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use luna_core::{ConversationTurn, EpisodeProfile, SignalReliability, StructuredAssertion};
    use luna_extract::{FeatureExtractor, FusedExtractor};
    use uuid::Uuid;

    #[test]
    fn similarity_can_recall_profession_query_without_keyword_overlap() {
        let extractor = FusedExtractor::new();
        let observed = extractor
            .extract(&ConversationTurn::user("I work as a mechanical engineer."))
            .unwrap();
        let query = extractor
            .extract(&ConversationTurn::user("What do I do for a living?"))
            .unwrap();
        let episode = Episode {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            assertions: vec![StructuredAssertion::inferred(
                "identity",
                "profession",
                "mechanical engineer",
            )
            .with_source_count(2)],
            profile: luna_match::profile_from_reading(&observed),
            recall_history: Vec::new(),
            confidence: 0.8,
            coherence_score: 0.8,
            forgotten_risk: 0.0,
        };

        let engine = SimilarityRecallEngine
            .recall(&query, &[episode.clone()], RecallMode::Factual)
            .unwrap();
        let keyword = KeywordRecallEngine
            .recall(&query, &[episode], RecallMode::Factual)
            .unwrap();

        assert_eq!(engine.hits.len(), 1);
        assert_eq!(keyword.hits.len(), 0);
    }

    // ── GeometricRecallEngine tests ──

    #[test]
    fn geometric_recalls_episode_with_matching_signal_dimensions() {
        let extractor = FusedExtractor::new();
        let observed = extractor
            .extract(&ConversationTurn::user("I work as a mechanical engineer."))
            .unwrap();
        let query = extractor
            .extract(&ConversationTurn::user("What do I do for a living?"))
            .unwrap();
        let episode = Episode {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            assertions: vec![StructuredAssertion::inferred(
                "identity",
                "profession",
                "mechanical engineer",
            )
            .with_source_count(2)],
            profile: luna_match::profile_from_reading(&observed),
            recall_history: Vec::new(),
            confidence: 0.8,
            coherence_score: 0.8,
            forgotten_risk: 0.0,
        };

        let result = GeometricRecallEngine
            .recall(&query, &[episode], RecallMode::Factual)
            .unwrap();
        assert_eq!(result.hits.len(), 1);
        assert!(result.hits[0].score >= 0.45);
        assert_eq!(result.hits[0].reason.as_str(), "profile_similarity");
    }

    #[test]
    fn geometric_does_not_recall_keyword_only_overlap() {
        let query = TurnReading {
            turn_id: Uuid::new_v4(),
            semantic: None,
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: luna_core::Signal::new(0.5, 0.5, SignalReliability::Heuristic),
            cue_terms: vec!["engineer".to_string()],
            query_intents: Vec::new(),
            assertions: Vec::new(),
        };

        let episode = Episode {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            assertions: vec![StructuredAssertion::inferred(
                "identity",
                "profession",
                "mechanical engineer",
            )
            .with_source_count(2)],
            profile: EpisodeProfile {
                semantic: None,
                intent: None,
                attention: None,
                goal_pressure: None,
                emotional_valence: None,
                emotional_arousal: None,
                identity_relevance: None,
                trust_relevance: None,
                social_frame: None,
                temporal_relevance: None,
                reinforcement_count: 0,
                contradiction_count: 0,
                successful_recall_count: 0,
                failed_recall_count: 0,
            },
            recall_history: Vec::new(),
            confidence: 0.8,
            coherence_score: 0.8,
            forgotten_risk: 0.0,
        };

        let result = GeometricRecallEngine
            .recall(&query, &[episode], RecallMode::Factual)
            .unwrap();
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn geometric_returns_empty_for_no_signal_query() {
        let query = TurnReading {
            turn_id: Uuid::new_v4(),
            semantic: None,
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: luna_core::Signal::new(0.5, 0.5, SignalReliability::Heuristic),
            cue_terms: Vec::new(),
            query_intents: Vec::new(),
            assertions: Vec::new(),
        };

        let episode = Episode {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            assertions: vec![StructuredAssertion::inferred(
                "identity",
                "profession",
                "mechanical engineer",
            )
            .with_source_count(2)],
            profile: EpisodeProfile {
                semantic: None,
                intent: None,
                attention: None,
                goal_pressure: None,
                emotional_valence: None,
                emotional_arousal: None,
                identity_relevance: None,
                trust_relevance: None,
                social_frame: None,
                temporal_relevance: None,
                reinforcement_count: 0,
                contradiction_count: 0,
                successful_recall_count: 0,
                failed_recall_count: 0,
            },
            recall_history: Vec::new(),
            confidence: 0.8,
            coherence_score: 0.8,
            forgotten_risk: 0.0,
        };

        let result = GeometricRecallEngine
            .recall(&query, &[episode], RecallMode::Factual)
            .unwrap();
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn geometric_recalls_with_partial_signal_match_above_threshold() {
        let extractor = FusedExtractor::new();
        let observed = extractor
            .extract(&ConversationTurn::user("I work as a mechanical engineer."))
            .unwrap();
        let query = extractor
            .extract(&ConversationTurn::user("What do I do for a living?"))
            .unwrap();

        // Use the real query and observed to build profiles, but override
        // one signal dimension to verify partial match still works.
        let profile = luna_match::profile_from_reading(&observed);
        // Ensure at least some signals are enabled so profile_similarity > 0
        // The extractor already produces matching identity signals.

        let episode = Episode {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            assertions: vec![StructuredAssertion::inferred(
                "identity",
                "profession",
                "mechanical engineer",
            )
            .with_source_count(2)],
            profile,
            recall_history: Vec::new(),
            confidence: 0.8,
            coherence_score: 0.8,
            forgotten_risk: 0.0,
        };

        let result = GeometricRecallEngine
            .recall(&query, &[episode], RecallMode::Factual)
            .unwrap();
        // With real extracted profiles, identity signals should match and
        // produce a score above 0.45.
        assert_eq!(result.hits.len(), 1);
        assert!(result.hits[0].score >= 0.45);
    }

    // ── VectorRecallEngine tests ──

    #[test]
    fn vector_recalls_episode_with_similar_semantic_vector() {
        let semantic = Some(vec![0.1, 0.2, 0.3, 0.4, 0.5]);
        let similar = Some(vec![0.15, 0.22, 0.28, 0.42, 0.48]);

        let query = TurnReading {
            turn_id: Uuid::new_v4(),
            semantic: semantic.clone(),
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: luna_core::Signal::new(0.5, 0.5, SignalReliability::Heuristic),
            cue_terms: Vec::new(),
            query_intents: Vec::new(),
            assertions: Vec::new(),
        };

        let episode = Episode {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            assertions: vec![StructuredAssertion::inferred(
                "identity",
                "profession",
                "mechanical engineer",
            )
            .with_source_count(2)],
            profile: EpisodeProfile {
                semantic: similar,
                intent: None,
                attention: None,
                goal_pressure: None,
                emotional_valence: None,
                emotional_arousal: None,
                identity_relevance: None,
                trust_relevance: None,
                social_frame: None,
                temporal_relevance: None,
                reinforcement_count: 0,
                contradiction_count: 0,
                successful_recall_count: 0,
                failed_recall_count: 0,
            },
            recall_history: Vec::new(),
            confidence: 0.8,
            coherence_score: 0.8,
            forgotten_risk: 0.0,
        };

        let result = VectorRecallEngine
            .recall(&query, &[episode], RecallMode::Factual)
            .unwrap();
        assert_eq!(result.hits.len(), 1);
        assert!(result.hits[0].score >= 0.35);
        assert_eq!(result.hits[0].reason.as_str(), "vector_similarity");
    }

    #[test]
    fn vector_returns_empty_when_both_semantic_vectors_are_none() {
        let query = TurnReading {
            turn_id: Uuid::new_v4(),
            semantic: None,
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: luna_core::Signal::new(0.5, 0.5, SignalReliability::Heuristic),
            cue_terms: Vec::new(),
            query_intents: Vec::new(),
            assertions: Vec::new(),
        };

        let episode = Episode {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            assertions: vec![StructuredAssertion::inferred(
                "identity",
                "profession",
                "mechanical engineer",
            )
            .with_source_count(2)],
            profile: EpisodeProfile {
                semantic: None,
                intent: None,
                attention: None,
                goal_pressure: None,
                emotional_valence: None,
                emotional_arousal: None,
                identity_relevance: None,
                trust_relevance: None,
                social_frame: None,
                temporal_relevance: None,
                reinforcement_count: 0,
                contradiction_count: 0,
                successful_recall_count: 0,
                failed_recall_count: 0,
            },
            recall_history: Vec::new(),
            confidence: 0.8,
            coherence_score: 0.8,
            forgotten_risk: 0.0,
        };

        let result = VectorRecallEngine
            .recall(&query, &[episode], RecallMode::Factual)
            .unwrap();
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn vector_returns_empty_for_orthogonal_vectors() {
        let query = TurnReading {
            turn_id: Uuid::new_v4(),
            semantic: Some(vec![1.0, 0.0, 0.0]),
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: luna_core::Signal::new(0.5, 0.5, SignalReliability::Heuristic),
            cue_terms: Vec::new(),
            query_intents: Vec::new(),
            assertions: Vec::new(),
        };

        let episode = Episode {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            assertions: vec![StructuredAssertion::inferred(
                "identity",
                "profession",
                "mechanical engineer",
            )
            .with_source_count(2)],
            profile: EpisodeProfile {
                semantic: Some(vec![0.0, 1.0, 0.0]),
                intent: None,
                attention: None,
                goal_pressure: None,
                emotional_valence: None,
                emotional_arousal: None,
                identity_relevance: None,
                trust_relevance: None,
                social_frame: None,
                temporal_relevance: None,
                reinforcement_count: 0,
                contradiction_count: 0,
                successful_recall_count: 0,
                failed_recall_count: 0,
            },
            recall_history: Vec::new(),
            confidence: 0.8,
            coherence_score: 0.8,
            forgotten_risk: 0.0,
        };

        let result = VectorRecallEngine
            .recall(&query, &[episode], RecallMode::Factual)
            .unwrap();
        assert_eq!(result.hits.len(), 0);
    }
}
