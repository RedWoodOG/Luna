use luna_core::{Episode, EpisodeProfile, Signal, TurnReading};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionContribution {
    pub name: String,
    pub raw_similarity: f32,
    pub weight: f32,
    pub contribution: f32,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchBreakdown {
    pub semantic_similarity: f32,
    pub intent_similarity: f32,
    pub assertion_fit: f32,
    pub contradiction_gate: f32,
    pub forgotten_risk_gate: f32,
    pub pre_gate_score: f32,
    pub total: f32,
    pub contributions: Vec<DimensionContribution>,
}

pub fn profile_from_reading(observation: &TurnReading) -> EpisodeProfile {
    EpisodeProfile {
        semantic: observation.semantic.clone(),
        intent: observation.intent.clone(),
        attention: recall_enabled(observation.attention),
        goal_pressure: recall_enabled(observation.goal_pressure),
        emotional_valence: recall_enabled(observation.emotional_valence),
        emotional_arousal: recall_enabled(observation.emotional_arousal),
        identity_relevance: recall_enabled(observation.identity_relevance),
        trust_relevance: recall_enabled(observation.trust_relevance),
        social_frame: recall_enabled(observation.social_frame),
        temporal_relevance: recall_enabled(observation.temporal_relevance),
        reinforcement_count: 0,
        contradiction_count: 0,
        successful_recall_count: 0,
        failed_recall_count: 0,
    }
}

pub fn reinforce_profile(profile: &mut EpisodeProfile, observation: &TurnReading) {
    profile.reinforcement_count += 1;
    profile.semantic = average_optional_vectors(&profile.semantic, &observation.semantic);
    profile.intent = average_optional_vectors(&profile.intent, &observation.intent);
    profile.attention = fuse_pair(profile.attention, observation.attention);
    profile.goal_pressure = fuse_pair(profile.goal_pressure, observation.goal_pressure);
    profile.identity_relevance =
        fuse_pair(profile.identity_relevance, observation.identity_relevance);
    profile.trust_relevance = fuse_pair(profile.trust_relevance, observation.trust_relevance);
    profile.social_frame = fuse_pair(profile.social_frame, observation.social_frame);
    profile.temporal_relevance =
        fuse_pair(profile.temporal_relevance, observation.temporal_relevance);
    profile.emotional_valence = fuse_pair(profile.emotional_valence, observation.emotional_valence);
    profile.emotional_arousal = fuse_pair(profile.emotional_arousal, observation.emotional_arousal);
}

pub fn profile_similarity(current: &TurnReading, episode: &Episode) -> f32 {
    match_breakdown(current, episode).total
}

pub fn match_breakdown(current: &TurnReading, episode: &Episode) -> MatchBreakdown {
    let profile = &episode.profile;
    let semantic = optional_cosine(&current.semantic, &profile.semantic);
    let intent = optional_cosine(&current.intent, &profile.intent);
    let assertion_fit = assertion_intent_fit(current, episode);
    let contradiction_gate =
        (1.0 - (profile.contradiction_count as f32 * 0.18).min(0.72)).clamp(0.0, 1.0);
    let forgotten_risk_gate = (1.0 - episode.forgotten_risk * 0.25).clamp(0.0, 1.0);

    let mut contributions = vec![
        contribution("semantic", semantic, 0.10, semantic > 0.0),
        contribution("intent", intent, 0.13, intent > 0.0),
        contribution("assertion_fit", assertion_fit, 0.22, assertion_fit > 0.0),
        signal_contribution("attention", current.attention, profile.attention, 0.10),
        signal_contribution("goal", current.goal_pressure, profile.goal_pressure, 0.12),
        signal_contribution(
            "identity",
            current.identity_relevance,
            profile.identity_relevance,
            0.14,
        ),
        signal_contribution(
            "trust",
            current.trust_relevance,
            profile.trust_relevance,
            0.08,
        ),
        signal_contribution("social", current.social_frame, profile.social_frame, 0.06),
        signal_contribution(
            "emotional_arousal",
            current.emotional_arousal,
            profile.emotional_arousal,
            0.05,
        ),
        contribution(
            "coherence",
            episode.coherence_score,
            0.10,
            episode.coherence_score > 0.0,
        ),
    ];

    let pre_gate_score = contributions
        .iter()
        .map(|contribution| contribution.contribution)
        .sum::<f32>();
    let total = (pre_gate_score * contradiction_gate * forgotten_risk_gate).clamp(0.0, 1.0);

    contributions.sort_by(|left, right| left.name.cmp(&right.name));

    MatchBreakdown {
        semantic_similarity: semantic,
        intent_similarity: intent,
        assertion_fit,
        contradiction_gate,
        forgotten_risk_gate,
        pre_gate_score,
        total,
        contributions,
    }
}

pub fn coherence_score(profile: &EpisodeProfile, confidence: f32, forgotten_risk: f32) -> f32 {
    let reinforcement = (profile.reinforcement_count as f32 * 0.08).min(0.24);
    let successful = (profile.successful_recall_count as f32 * 0.06).min(0.18);
    let failed = (profile.failed_recall_count as f32 * 0.05).min(0.20);
    let contradiction = (profile.contradiction_count as f32 * 0.16).min(0.48);
    let active_dimensions = active_dimension_count(profile) as f32 / 8.0;
    (confidence * 0.42 + active_dimensions * 0.18 + reinforcement + successful
        - failed
        - contradiction
        - forgotten_risk * 0.18)
        .clamp(0.0, 1.0)
}

pub fn active_dimension_count(profile: &EpisodeProfile) -> usize {
    [
        profile.attention,
        profile.goal_pressure,
        profile.emotional_valence,
        profile.emotional_arousal,
        profile.identity_relevance,
        profile.trust_relevance,
        profile.social_frame,
        profile.temporal_relevance,
    ]
    .iter()
    .filter(|signal| {
        signal
            .map(|signal| signal.can_influence_recall())
            .unwrap_or(false)
    })
    .count()
}

fn recall_enabled(signal: Option<Signal>) -> Option<Signal> {
    signal.filter(|signal| signal.can_influence_recall())
}

fn dimension(current: Option<Signal>, stored: Option<Signal>) -> f32 {
    match (current, stored) {
        (Some(left), Some(right))
            if left.can_influence_recall() && right.can_influence_recall() =>
        {
            scalar_similarity(left.value(), right.value())
                * ((left.confidence() + right.confidence()) / 2.0)
        }
        _ => 0.0,
    }
}

fn signal_contribution(
    name: &str,
    current: Option<Signal>,
    stored: Option<Signal>,
    weight: f32,
) -> DimensionContribution {
    let enabled = matches!((current, stored), (Some(left), Some(right)) if left.can_influence_recall() && right.can_influence_recall());
    let raw_similarity = dimension(current, stored);
    contribution(name, raw_similarity, weight, enabled)
}

fn contribution(
    name: &str,
    raw_similarity: f32,
    weight: f32,
    enabled: bool,
) -> DimensionContribution {
    DimensionContribution {
        name: name.to_string(),
        raw_similarity,
        weight,
        contribution: raw_similarity * weight,
        enabled,
    }
}

fn assertion_intent_fit(current: &TurnReading, episode: &Episode) -> f32 {
    let mut best: f32 = 0.0;
    for intent in &current.query_intents {
        for assertion in &episode.assertions {
            if intent == "identity.profession.query"
                && assertion.domain == "identity"
                && assertion.kind == "profession"
            {
                best = best.max(1.0);
            }
            if intent == "identity.family_structure.query"
                && assertion.domain == "identity"
                && assertion.kind == "family_structure"
            {
                best = best.max(1.0);
            }
        }
    }
    best
}

fn scalar_similarity(left: f32, right: f32) -> f32 {
    (1.0 - (left.clamp(0.0, 1.0) - right.clamp(0.0, 1.0)).abs()).clamp(0.0, 1.0)
}

fn optional_cosine(left: &Option<Vec<f32>>, right: &Option<Vec<f32>>) -> f32 {
    match (left, right) {
        (Some(left), Some(right)) => cosine_like(left, right),
        _ => 0.0,
    }
}

fn cosine_like(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let len = left.len().min(right.len());
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for index in 0..len {
        dot += left[index] * right[index];
        left_norm += left[index] * left[index];
        right_norm += right[index] * right[index];
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        (dot / (left_norm.sqrt() * right_norm.sqrt())).clamp(0.0, 1.0)
    }
}

fn average_optional_vectors(left: &Option<Vec<f32>>, right: &Option<Vec<f32>>) -> Option<Vec<f32>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(average_vectors(left, right)),
        (Some(left), None) => Some(left.clone()),
        (None, Some(right)) => Some(right.clone()),
        (None, None) => None,
    }
}

fn average_vectors(left: &[f32], right: &[f32]) -> Vec<f32> {
    let len = left.len().max(right.len());
    (0..len)
        .map(|index| {
            avg(
                left.get(index).copied().unwrap_or(0.0),
                right.get(index).copied().unwrap_or(0.0),
            )
        })
        .collect()
}

fn fuse_pair(left: Option<Signal>, right: Option<Signal>) -> Option<Signal> {
    match (left, right) {
        (Some(left), Some(right)) => {
            Signal::fuse(&[left, right]).filter(|signal| signal.can_influence_recall())
        }
        (Some(signal), None) | (None, Some(signal)) => recall_enabled(Some(signal)),
        (None, None) => None,
    }
}

fn avg(left: f32, right: f32) -> f32 {
    ((left + right) / 2.0).clamp(0.0, 1.0)
}
