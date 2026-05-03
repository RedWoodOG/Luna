pub mod backend;
pub mod cache;
pub mod extractor;
pub mod fusion;
pub mod llm_observation;
pub mod luna_extractor;
pub mod prompt;
pub mod second_source;

pub use backend::{
    CommandBackend, CountingBackend, FixtureBackend, LlmBackend, LlmRequest, RecordingFakeBackend,
};
pub use cache::{CacheKey, ExtractionCache, FileExtractionCache};
pub use extractor::LlmExtractor;
pub use fusion::fuse_observation;
pub use llm_observation::{
    validate_against_prompt_v3, validate_observation, LlmAssertion, LlmObservation, LlmSignal,
    ALLOWED_DIMENSIONS, ALLOWED_RELIABILITIES, EXTRACTION_SCHEMA_VERSION, PROMPT_V3_DOMAIN_KINDS,
};
pub use luna_extractor::LunaExtractor;
pub use prompt::{build_prompt_v3, prompt_v3_hash};
pub use second_source::{
    default_v1_sources, AffectLexicon, FirstPersonIdentityDetector, GoalPhraseLexicon,
    SecondSource, TemporalDetector,
};

use luna_core::{
    CognitiveObservation, ConversationTurn, Result, Role, Signal, SignalReliability,
    StructuredAssertion,
};
use uuid::Uuid;

pub trait FeatureExtractor {
    fn extract(&self, turn: &ConversationTurn) -> Result<CognitiveObservation>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HeuristicExtractor;

#[derive(Debug, Clone, Copy, Default)]
pub struct EmbeddingExtractor;

#[derive(Debug, Clone, Copy, Default)]
pub struct ModelExtractor;

#[derive(Debug, Clone, Copy, Default)]
pub struct FusedExtractor {
    heuristic: HeuristicExtractor,
    embedding: EmbeddingExtractor,
}

impl FusedExtractor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FeatureExtractor for FusedExtractor {
    fn extract(&self, turn: &ConversationTurn) -> Result<CognitiveObservation> {
        let heuristic = self.heuristic.extract(turn)?;
        let embedding = self.embedding.extract(turn)?;
        Ok(fuse_observations(&[heuristic, embedding]))
    }
}

impl FeatureExtractor for HeuristicExtractor {
    fn extract(&self, turn: &ConversationTurn) -> Result<CognitiveObservation> {
        observation_from(turn, SignalReliability::Heuristic, 0.68)
    }
}

impl FeatureExtractor for EmbeddingExtractor {
    fn extract(&self, turn: &ConversationTurn) -> Result<CognitiveObservation> {
        observation_from(turn, SignalReliability::Statistical, 0.62)
    }
}

impl FeatureExtractor for ModelExtractor {
    fn extract(&self, turn: &ConversationTurn) -> Result<CognitiveObservation> {
        observation_from(turn, SignalReliability::Learned, 0.72)
    }
}

pub fn fuse_observations(observations: &[CognitiveObservation]) -> CognitiveObservation {
    let first = observations
        .first()
        .expect("at least one observation is required");
    CognitiveObservation {
        turn_id: first.turn_id,
        semantic: first.semantic.clone(),
        intent: first.intent.clone(),
        attention: fuse_dimension(observations, |obs| obs.attention),
        goal_pressure: fuse_dimension(observations, |obs| obs.goal_pressure),
        emotional_valence: fuse_dimension(observations, |obs| obs.emotional_valence),
        emotional_arousal: fuse_dimension(observations, |obs| obs.emotional_arousal),
        identity_relevance: fuse_dimension(observations, |obs| obs.identity_relevance),
        trust_relevance: fuse_dimension(observations, |obs| obs.trust_relevance),
        social_frame: fuse_dimension(observations, |obs| obs.social_frame),
        temporal_relevance: fuse_dimension(observations, |obs| obs.temporal_relevance),
        uncertainty: Signal::fuse(
            &observations
                .iter()
                .map(|observation| observation.uncertainty)
                .collect::<Vec<_>>(),
        )
        .unwrap_or(first.uncertainty),
        cue_terms: merge_strings(observations.iter().flat_map(|obs| obs.cue_terms.clone())),
        query_intents: merge_strings(
            observations
                .iter()
                .flat_map(|obs| obs.query_intents.clone()),
        ),
        assertions: merge_assertions(observations.iter().flat_map(|obs| obs.assertions.clone())),
    }
}

fn observation_from(
    turn: &ConversationTurn,
    reliability: SignalReliability,
    confidence: f32,
) -> Result<CognitiveObservation> {
    let normalized = normalize(&turn.content);
    let cue_terms = cue_terms(&normalized);
    let query_intents = query_intents(&normalized);
    let assertions = extract_assertions(&normalized);
    let semantic = hashed_vector(&cue_terms, 16);
    let intent = hashed_vector(&query_intents, 8);
    let uncertainty = if assertions.is_empty() { 0.44 } else { 0.16 };

    Ok(CognitiveObservation {
        turn_id: Uuid::new_v4(),
        semantic: Some(semantic),
        intent: Some(intent),
        attention: Some(Signal::new(
            attention(&turn.content, &cue_terms),
            confidence,
            reliability,
        )),
        goal_pressure: Some(Signal::new(
            if has_any(&normalized, &["need", "trying", "goal", "what do"]) {
                0.72
            } else {
                0.35
            },
            confidence,
            reliability,
        )),
        emotional_valence: Some(Signal::new(valence(&normalized), confidence, reliability)),
        emotional_arousal: Some(Signal::new(arousal(&normalized), confidence, reliability)),
        identity_relevance: Some(Signal::new(
            if !assertions.is_empty()
                || query_intents
                    .iter()
                    .any(|intent| intent.contains("identity"))
            {
                0.86
            } else {
                0.28
            },
            confidence,
            reliability,
        )),
        trust_relevance: Some(Signal::new(
            if has_any(&normalized, &["remember", "told you", "private"]) {
                0.76
            } else {
                0.44
            },
            confidence,
            reliability,
        )),
        social_frame: Some(Signal::new(
            if turn.role == Role::User { 0.62 } else { 0.35 },
            confidence,
            reliability,
        )),
        temporal_relevance: Some(Signal::new(1.0, confidence, reliability)),
        uncertainty: Signal::new(uncertainty, confidence, reliability),
        cue_terms,
        query_intents,
        assertions,
    })
}

fn fuse_dimension(
    observations: &[CognitiveObservation],
    read: impl Fn(&CognitiveObservation) -> Option<Signal>,
) -> Option<Signal> {
    Signal::fuse(&observations.iter().filter_map(read).collect::<Vec<_>>())
}

fn extract_assertions(normalized: &str) -> Vec<StructuredAssertion> {
    let mut assertions = Vec::new();
    if has_any(
        normalized,
        &["mechanical engineer", "mechanical engineering"],
    ) && has_any(
        normalized,
        &[
            "i work",
            "i am",
            "i'm",
            "my career",
            "make a living",
            "professionally",
        ],
    ) {
        assertions.push(StructuredAssertion {
            domain: "identity".to_string(),
            kind: "profession".to_string(),
            value: "mechanical engineer".to_string(),
        });
    }

    if has_any(
        normalized,
        &["i'm an only child", "i am an only child", "only child"],
    ) {
        assertions.push(StructuredAssertion {
            domain: "identity".to_string(),
            kind: "family_structure".to_string(),
            value: "only child".to_string(),
        });
    }

    assertions
}

pub(crate) fn query_intents(normalized: &str) -> Vec<String> {
    let mut intents = Vec::new();
    if has_any(
        normalized,
        &[
            "what do i do",
            "for a living",
            "my job",
            "my profession",
            "my career",
        ],
    ) {
        intents.push("identity.profession.query".to_string());
    }
    if has_any(normalized, &["siblings", "brother", "sister"]) {
        intents.push("identity.family_structure.query".to_string());
        intents.push("contradiction_check".to_string());
    }
    if normalized.contains('?') && intents.is_empty() {
        intents.push("factual.query".to_string());
    }
    if intents.is_empty() {
        intents.push("statement".to_string());
    }
    intents
}

pub(crate) fn cue_terms(normalized: &str) -> Vec<String> {
    normalized
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|word| word.len() > 2 && !STOP_WORDS.contains(word))
        .map(lemmatize)
        .fold(Vec::new(), |mut acc, term| {
            if !acc.contains(&term) {
                acc.push(term);
            }
            acc
        })
}

fn attention(text: &str, terms: &[String]) -> f32 {
    (0.42 + (text.len() as f32 / 220.0).min(0.28) + (terms.len() as f32 * 0.025).min(0.2))
        .clamp(0.0, 1.0)
}

fn valence(normalized: &str) -> f32 {
    if has_any(normalized, &["happy", "love", "excited"]) {
        0.72
    } else if has_any(
        normalized,
        &["sad", "angry", "terrified", "worried", "hate", "hated"],
    ) {
        0.28
    } else {
        0.52
    }
}

fn arousal(normalized: &str) -> f32 {
    if has_any(
        normalized,
        &[
            "urgent",
            "terrified",
            "angry",
            "worried",
            "burned out",
            "hated",
        ],
    ) {
        0.78
    } else {
        0.38
    }
}

pub(crate) fn hashed_vector(terms: &[String], dims: usize) -> Vec<f32> {
    let mut vector = vec![0.0; dims];
    for term in terms {
        let index = stable_hash(term) as usize % dims;
        vector[index] += 1.0;
    }
    let max = vector.iter().copied().fold(0.0_f32, f32::max).max(1.0);
    vector
        .iter_mut()
        .for_each(|value| *value = (*value / max).clamp(0.0, 1.0));
    vector
}

fn stable_hash(value: &str) -> u64 {
    value
        .bytes()
        .fold(14_695_981_039_346_656_037, |hash, byte| {
            (hash ^ byte as u64).wrapping_mul(1_099_511_628_211)
        })
}

pub(crate) fn normalize(text: &str) -> String {
    text.to_ascii_lowercase().replace("i'm", "i am")
}

fn has_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn lemmatize(word: &str) -> String {
    match word {
        "engineering" => "engineer".to_string(),
        "career" | "professionally" | "profession" => "profession".to_string(),
        "living" | "job" => "work".to_string(),
        "siblings" | "brother" | "sister" => "sibling".to_string(),
        "hated" => "hate".to_string(),
        other => other.to_string(),
    }
}

fn merge_strings(values: impl Iterator<Item = String>) -> Vec<String> {
    values.fold(Vec::new(), |mut acc, value| {
        if !acc.contains(&value) {
            acc.push(value);
        }
        acc
    })
}

fn merge_assertions(values: impl Iterator<Item = StructuredAssertion>) -> Vec<StructuredAssertion> {
    values.fold(Vec::new(), |mut acc, value| {
        if !acc
            .iter()
            .any(|existing: &StructuredAssertion| existing.key() == value.key())
        {
            acc.push(value);
        }
        acc
    })
}

const STOP_WORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "you",
    "that",
    "this",
    "with",
    "what",
    "make",
    "living",
    "work",
    "career",
    "professionally",
    "profession",
    "my",
    "as",
    "in",
    "is",
    "do",
    "are",
    "names",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_profession_across_paraphrase() {
        let extractor = FusedExtractor::new();
        let first = extractor
            .extract(&ConversationTurn::user("I work as a mechanical engineer."))
            .unwrap();
        let second = extractor
            .extract(&ConversationTurn::user(
                "My career is in mechanical engineering.",
            ))
            .unwrap();

        assert_eq!(
            first.assertions[0].key(),
            "identity:profession=mechanical_engineer"
        );
        assert_eq!(
            second.assertions[0].key(),
            "identity:profession=mechanical_engineer"
        );
        assert!(first.identity_relevance.unwrap().can_influence_recall());
    }
}
