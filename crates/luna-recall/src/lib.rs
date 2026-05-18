use luna_core::{CognitiveObservation, Episode, RecallHit, RecallMode, RecallSet, Result};
use std::time::Instant;

pub trait RecallEngine {
    fn recall(
        &self,
        current: &CognitiveObservation,
        episodes: &[Episode],
        mode: RecallMode,
    ) -> Result<RecallSet>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KeywordRecallEngine;

#[derive(Debug, Clone, Copy, Default)]
pub struct TcfRecallEngine;

impl RecallEngine for KeywordRecallEngine {
    fn recall(
        &self,
        current: &CognitiveObservation,
        episodes: &[Episode],
        _mode: RecallMode,
    ) -> Result<RecallSet> {
        let start = Instant::now();
        let mut hits = episodes
            .iter()
            .filter_map(|episode| {
                let score = keyword_score(current, episode);
                (score >= 0.34).then(|| RecallHit {
                    episode_id: episode.id,
                    score,
                    assertions: episode.assertions.clone(),
                    reason: "keyword_overlap".to_string(),
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

impl RecallEngine for TcfRecallEngine {
    fn recall(
        &self,
        current: &CognitiveObservation,
        episodes: &[Episode],
        _mode: RecallMode,
    ) -> Result<RecallSet> {
        let start = Instant::now();
        let mut hits = episodes
            .iter()
            .filter_map(|episode| {
                let score = luna_tcf::tcf_similarity(current, episode);
                (score >= 0.50).then(|| RecallHit {
                    episode_id: episode.id,
                    score,
                    assertions: episode.assertions.clone(),
                    reason: "state_contour_activation".to_string(),
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

fn keyword_score(current: &CognitiveObservation, episode: &Episode) -> f32 {
    let mut haystack = episode
        .assertions
        .iter()
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

    if haystack.is_empty() || current.cue_terms.is_empty() {
        return 0.0;
    }

    let matches = current
        .cue_terms
        .iter()
        .filter(|term| haystack.iter().any(|candidate| candidate == *term))
        .count();
    matches as f32 / current.cue_terms.len().max(haystack.len()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use luna_core::{ConversationTurn, StructuredAssertion};
    use luna_extract::{FeatureExtractor, FusedExtractor};
    use uuid::Uuid;

    #[test]
    fn tcf_can_recall_profession_query_without_keyword_overlap() {
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
            assertions: vec![StructuredAssertion {
                domain: "identity".to_string(),
                kind: "profession".to_string(),
                value: "mechanical engineer".to_string(),
                ..Default::default()
            }],
            contour: luna_tcf::contour_from_observation(&observed),
            recall_history: Vec::new(),
            confidence: 0.8,
            coherence_score: 0.8,
            forgotten_risk: 0.0,
        };

        let tcf = TcfRecallEngine
            .recall(&query, &[episode.clone()], RecallMode::Factual)
            .unwrap();
        let keyword = KeywordRecallEngine
            .recall(&query, &[episode], RecallMode::Factual)
            .unwrap();

        assert_eq!(tcf.hits.len(), 1);
        assert_eq!(keyword.hits.len(), 0);
    }
}

pub struct SimilarityRecallEngine;
impl SimilarityRecallEngine { pub fn new() -> Self { Self } }

impl RecallEngine for SimilarityRecallEngine {
    fn recall(&self, current: &CognitiveObservation, episodes: &[Episode], _mode: RecallMode) -> Result<RecallSet> {
        Ok(RecallSet::default())
    }
}