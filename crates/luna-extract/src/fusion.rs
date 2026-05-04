//! Fusion: collapse one [`crate::LlmObservation`] plus the outputs of
//! deterministic [`crate::SecondSource`] detectors into the final
//! [`luna_core::CognitiveObservation`] that recall reads.
//!
//! Two-source rule enforcement happens here. For each of the four
//! contour signal dimensions plumbed for v0.1, all available signals
//! are collected (LLM signal if present, plus every detector that
//! emitted for the dimension) and passed to [`Signal::fuse`]. The
//! fused signal is set on the contour only if it satisfies
//! [`Signal::can_influence_recall`] (`source_count >= 2`); otherwise
//! the dimension is `None`. This is what makes a single source
//! incapable of moving recall.
//!
//! Non-four-axis fields (`attention`, `emotional_valence`,
//! `trust_relevance`, `social_frame`) intentionally stay `None` for
//! v0.1 — no source is plumbed for them and the proof program does
//! not exercise them. They are reachable for Stage 2+ if the
//! ablation grid asks for them.
//!
//! Fields that aren't gated by the two-source rule (`semantic`,
//! `intent`, `cue_terms`, `query_intents`, `assertions`,
//! `uncertainty`) are populated deterministically:
//! - `assertions` map directly from validated `LlmObservation.assertions`.
//! - `cue_terms` and `query_intents` come from the lexical pass that
//!   has lived in this crate since the pre-LLM extractor.
//! - `semantic` and `intent` are hashed bag-of-terms vectors.
//! - `uncertainty` is heuristic: lower when the LLM produced
//!   assertions, higher when it did not.

use crate::{
    cue_terms, hashed_vector,
    llm_observation::{LlmAssertion, LlmObservation},
    normalize, query_intents,
    second_source::SecondSource,
};
use luna_core::{
    CognitiveObservation, ConversationTurn, Signal, SignalReliability, StructuredAssertion,
};
use std::collections::HashMap;
use uuid::Uuid;

/// Fuse one LLM observation plus the four detector outputs into a
/// final [`CognitiveObservation`]. See module docs for the rules.
pub fn fuse_observation(
    llm_obs: &LlmObservation,
    detectors: &[Box<dyn SecondSource>],
    turn: &ConversationTurn,
) -> CognitiveObservation {
    let mut by_dim: HashMap<String, Vec<Signal>> = HashMap::new();

    // LLM signals are always tagged Learned at fusion time. The
    // `reliability` string in the JSON is data we validated, but the
    // structural truth — "this came from the LLM source" — is what
    // the fused contour records.
    for (dim, llm_signal_opt) in &llm_obs.signals {
        if let Some(llm_sig) = llm_signal_opt {
            by_dim.entry(dim.clone()).or_default().push(Signal::new(
                llm_sig.value,
                llm_sig.confidence,
                SignalReliability::Learned,
            ));
        }
    }

    // Detector signals. Detectors return Heuristic-tagged Signals
    // pre-built; we just merge into the per-dimension bucket.
    for detector in detectors {
        for (dim, signal) in detector.detect(turn) {
            by_dim.entry(dim).or_default().push(signal);
        }
    }

    let temporal_relevance = fuse_dim(&by_dim, "temporal_relevance");
    let emotional_arousal = fuse_dim(&by_dim, "emotional_arousal");
    let identity_relevance = fuse_dim(&by_dim, "identity_relevance");
    let goal_pressure = fuse_dim(&by_dim, "goal_pressure");

    // Deterministic structural pass. None of these fields are
    // signal-typed; the two-source rule does not apply.
    let normalized = normalize(&turn.content);
    let cue_terms_v = cue_terms(&normalized);
    let query_intents_v = query_intents(&normalized);
    let semantic = hashed_vector(&cue_terms_v, 16);
    let intent = hashed_vector(&query_intents_v, 8);

    let assertions: Vec<StructuredAssertion> = llm_obs
        .assertions
        .iter()
        .map(|assertion| {
            to_structured(
                assertion,
                turn,
                temporal_relevance,
                emotional_arousal,
                identity_relevance,
                goal_pressure,
            )
        })
        .collect();

    // Uncertainty mirrors the existing extractor's heuristic: if the
    // turn produced assertions, the system has more to work with;
    // otherwise it is more uncertain. This is a Signal (not Option)
    // because CognitiveObservation requires it.
    let uncertainty = if assertions.is_empty() {
        Signal::new(0.44, 0.6, SignalReliability::Heuristic)
    } else {
        Signal::new(0.16, 0.7, SignalReliability::Heuristic)
    };

    CognitiveObservation {
        turn_id: Uuid::new_v4(),
        semantic: Some(semantic),
        intent: Some(intent),
        attention: None,
        goal_pressure,
        emotional_valence: None,
        emotional_arousal,
        identity_relevance,
        trust_relevance: None,
        social_frame: None,
        temporal_relevance,
        uncertainty,
        cue_terms: cue_terms_v,
        query_intents: query_intents_v,
        assertions,
    }
}

/// Per-dimension fusion. Returns `Some` only when fused source_count
/// is at least 2; otherwise `None`. Wraps [`Signal::fuse`] so callers
/// don't need to remember the `can_influence_recall` filter.
fn fuse_dim(by_dim: &HashMap<String, Vec<Signal>>, name: &str) -> Option<Signal> {
    let signals = by_dim.get(name)?;
    Signal::fuse(signals).filter(|signal| signal.can_influence_recall())
}

fn to_structured(
    assertion: &LlmAssertion,
    turn: &ConversationTurn,
    temporal_relevance: Option<Signal>,
    emotional_arousal: Option<Signal>,
    identity_relevance: Option<Signal>,
    goal_pressure: Option<Signal>,
) -> StructuredAssertion {
    let source_count = assertion_support_source_count(
        assertion,
        temporal_relevance,
        emotional_arousal,
        identity_relevance,
        goal_pressure,
    );
    let value = if assertion.domain == "person" {
        anchor_person_value(&assertion.kind, &assertion.value, &turn.content)
    } else {
        assertion.value.clone()
    };
    StructuredAssertion::inferred(&assertion.domain, &assertion.kind, value)
        .with_source_count(source_count)
}

fn anchor_person_value(kind: &str, value: &str, turn_content: &str) -> String {
    if person_value_has_name(value) {
        return value.to_string();
    }

    if let Some(anchored) = anchor_they_both_value(value, turn_content) {
        return anchored;
    }

    if let Some(anchored) = anchor_group_role_value(kind, value, turn_content) {
        return anchored;
    }

    let lower_turn = turn_content.to_ascii_lowercase();
    let lower_value = value.to_ascii_lowercase();
    let Some(index) = lower_turn.find(&lower_value) else {
        return value.to_string();
    };
    let prefix = &turn_content[..index];
    let name = nearest_name(prefix);
    match name {
        Some(name) => format_person_value(kind, &name, value),
        None => value.to_string(),
    }
}

fn person_value_has_name(value: &str) -> bool {
    value
        .split_whitespace()
        .next()
        .map(is_person_name_token)
        .unwrap_or(false)
}

fn anchor_they_both_value(value: &str, turn_content: &str) -> Option<String> {
    let lower_turn = turn_content.to_ascii_lowercase();
    let lower_value = value.to_ascii_lowercase();
    let index = lower_turn.find(&lower_value)?;
    let prefix = &turn_content[..index];
    let current_clause_start = prefix
        .rfind(['.', '!', '?', ';', ','])
        .map(|idx| idx + 1)
        .unwrap_or(0);
    if !prefix[current_clause_start..]
        .to_ascii_lowercase()
        .contains("they both")
    {
        return None;
    }
    let names = names_before_pronoun(prefix);
    if names.len() >= 2 {
        Some(format!("{} both {value}", names.join(" and ")))
    } else {
        None
    }
}

fn anchor_group_role_value(kind: &str, value: &str, turn_content: &str) -> Option<String> {
    if kind != "role" {
        return None;
    }
    let lower_turn = turn_content.to_ascii_lowercase();
    let lower_value = value.to_ascii_lowercase();
    let index = lower_turn.find(&lower_value)?;
    let prefix = &turn_content[..index];
    let current_sentence_start = prefix
        .rfind(['.', '!', '?', ';'])
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let names = names_in_text(&prefix[current_sentence_start..]);
    if names.len() >= 2 && value.contains("co-founder") {
        Some(format!("{} are {value}", names.join(" and ")))
    } else {
        None
    }
}

fn format_person_value(kind: &str, name: &str, value: &str) -> String {
    match kind {
        "age" => format!("{name} is {value}"),
        "interest" | "relationship_status" | "trait" if !value.starts_with("is ") => {
            let article = if value.starts_with("a ") || value.starts_with("an ") {
                ""
            } else if kind == "interest" {
                "a "
            } else {
                ""
            };
            format!("{name} is {article}{value}")
        }
        _ => format!("{name} {value}"),
    }
}

fn names_before_pronoun(prefix: &str) -> Vec<String> {
    let Some(pronoun_index) = prefix.to_ascii_lowercase().rfind("they both") else {
        return Vec::new();
    };
    let before_pronoun = prefix[..pronoun_index].trim_end();
    let previous_text = before_pronoun
        .trim_end_matches(['.', '!', '?', ';'])
        .trim_end();
    let sentence_start = previous_text
        .rfind(['.', '!', '?', ';'])
        .map(|idx| idx + 1)
        .unwrap_or(0);
    names_in_text(&previous_text[sentence_start..])
}

fn nearest_name(prefix: &str) -> Option<String> {
    let start = prefix
        .rfind(['.', '!', '?', ';', ','])
        .map(|idx| idx + 1)
        .unwrap_or(0);
    names_in_text(&prefix[start..]).pop()
}

fn names_in_text(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_ascii_alphabetic()))
        .filter(|token| is_person_name_token(token))
        .map(ToString::to_string)
        .collect()
}

fn is_person_name_token(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && chars.all(|c| c.is_ascii_lowercase())
        && !matches!(token, "I" | "They" | "The" | "A" | "An" | "My")
}

fn assertion_support_source_count(
    assertion: &LlmAssertion,
    temporal_relevance: Option<Signal>,
    emotional_arousal: Option<Signal>,
    identity_relevance: Option<Signal>,
    goal_pressure: Option<Signal>,
) -> u8 {
    let supporting_signal = match assertion.domain.as_str() {
        "identity" | "person" => identity_relevance,
        "goal" | "work" => goal_pressure,
        "emotion" | "relationship" => emotional_arousal,
        "temporal" => temporal_relevance,
        _ => None,
    };
    supporting_signal
        .map(|signal| signal.source_count())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        llm_observation::{LlmObservation, LlmSignal},
        second_source::{
            default_v1_sources, AffectLexicon, FirstPersonIdentityDetector, GoalPhraseLexicon,
            TemporalDetector,
        },
    };
    use std::collections::BTreeMap;

    fn empty_signals() -> BTreeMap<String, Option<LlmSignal>> {
        let mut s = BTreeMap::new();
        s.insert("temporal_relevance".to_string(), None);
        s.insert("emotional_arousal".to_string(), None);
        s.insert("identity_relevance".to_string(), None);
        s.insert("goal_pressure".to_string(), None);
        s
    }

    fn llm_signal(value: f32) -> LlmSignal {
        LlmSignal {
            value,
            confidence: 0.8,
            reliability: "learned".to_string(),
            evidence: None,
        }
    }

    #[test]
    fn llm_only_signal_does_not_influence_recall() {
        let mut signals = empty_signals();
        signals.insert("temporal_relevance".to_string(), Some(llm_signal(0.9)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };
        // Turn content has no temporal cue, so detector emits nothing.
        let turn = ConversationTurn::user("The sky is blue.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        assert!(
            observation.temporal_relevance.is_none(),
            "single-source signal must not pass the two-source gate"
        );
    }

    #[test]
    fn detector_only_signal_does_not_influence_recall() {
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals: empty_signals(),
        };
        // Turn content fires the temporal detector but LLM emits nothing.
        let turn = ConversationTurn::user("Yesterday I went to the store.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        assert!(
            observation.temporal_relevance.is_none(),
            "detector-only signal must not pass the two-source gate"
        );
    }

    #[test]
    fn temporal_fuses_when_lexical_cue_and_llm_agree() {
        let mut signals = empty_signals();
        signals.insert("temporal_relevance".to_string(), Some(llm_signal(0.85)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };
        let turn = ConversationTurn::user("Yesterday I worked on the report.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        let signal = observation
            .temporal_relevance
            .expect("temporal_relevance should fuse");
        assert_eq!(signal.source_count(), 2);
        assert_eq!(signal.reliability(), SignalReliability::Learned);
        assert!(signal.can_influence_recall());
    }

    #[test]
    fn affect_fuses_when_lexicon_and_llm_agree() {
        let mut signals = empty_signals();
        signals.insert("emotional_arousal".to_string(), Some(llm_signal(0.85)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };
        let turn = ConversationTurn::user("I was terrified before the demo.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        let signal = observation
            .emotional_arousal
            .expect("emotional_arousal should fuse");
        assert_eq!(signal.source_count(), 2);
        assert_eq!(signal.reliability(), SignalReliability::Learned);
    }

    #[test]
    fn identity_fuses_when_first_person_and_llm_agree() {
        let mut signals = empty_signals();
        signals.insert("identity_relevance".to_string(), Some(llm_signal(0.9)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };
        let turn = ConversationTurn::user("I am a mechanical engineer.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        let signal = observation
            .identity_relevance
            .expect("identity_relevance should fuse");
        assert_eq!(signal.source_count(), 2);
    }

    #[test]
    fn goal_fuses_when_goal_phrase_and_llm_agree() {
        let mut signals = empty_signals();
        signals.insert("goal_pressure".to_string(), Some(llm_signal(0.85)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };
        let turn = ConversationTurn::user("I need to finish by tomorrow.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        let signal = observation
            .goal_pressure
            .expect("goal_pressure should fuse");
        assert_eq!(signal.source_count(), 2);
    }

    #[test]
    fn both_sources_none_keeps_dimension_none() {
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals: empty_signals(),
        };
        let turn = ConversationTurn::user("The sky is blue.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        assert!(observation.temporal_relevance.is_none());
        assert!(observation.emotional_arousal.is_none());
        assert!(observation.identity_relevance.is_none());
        assert!(observation.goal_pressure.is_none());
    }

    #[test]
    fn non_four_axis_fields_stay_none() {
        let mut signals = empty_signals();
        signals.insert("temporal_relevance".to_string(), Some(llm_signal(0.9)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };
        let turn = ConversationTurn::user("Yesterday I worked on the report.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        assert!(observation.attention.is_none());
        assert!(observation.emotional_valence.is_none());
        assert!(observation.trust_relevance.is_none());
        assert!(observation.social_frame.is_none());
    }

    #[test]
    fn assertions_are_converted_from_llm_obs() {
        let llm_obs = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "identity".to_string(),
                kind: "profession".to_string(),
                value: "mechanical engineer".to_string(),
                confidence: 0.9,
                evidence_span: Some("mechanical engineer".to_string()),
            }],
            signals: empty_signals(),
        };
        let turn = ConversationTurn::user("I work as a mechanical engineer.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        assert_eq!(observation.assertions.len(), 1);
        let assertion = &observation.assertions[0];
        assert_eq!(assertion.domain, "identity");
        assert_eq!(assertion.kind, "profession");
        assert_eq!(assertion.value, "mechanical engineer");
        assert_eq!(
            assertion.confidence_tier,
            luna_core::AssertionConfidenceTier::Inferred
        );
    }

    #[test]
    fn assertion_is_confirmed_when_llm_assertion_has_second_source_support() {
        let mut signals = empty_signals();
        signals.insert("identity_relevance".to_string(), Some(llm_signal(0.9)));
        let llm_obs = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "identity".to_string(),
                kind: "profession".to_string(),
                value: "mechanical engineer".to_string(),
                confidence: 0.9,
                evidence_span: Some("mechanical engineer".to_string()),
            }],
            signals,
        };
        let turn = ConversationTurn::user("I work as a mechanical engineer.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        assert_eq!(
            observation.assertions[0].confidence_tier,
            luna_core::AssertionConfidenceTier::Confirmed
        );
    }

    #[test]
    fn person_assertions_are_anchored_to_nearest_name_when_llm_drops_subject() {
        let llm_obs = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "person".to_string(),
                kind: "location".to_string(),
                value: "lives in Iowa".to_string(),
                confidence: 0.9,
                evidence_span: Some("lives in Iowa".to_string()),
            }],
            signals: empty_signals(),
        };
        let turn = ConversationTurn::user("Francois lives in Washington, and Chris lives in Iowa.");
        let observation = fuse_observation(&llm_obs, &[], &turn);

        assert_eq!(observation.assertions[0].value, "Chris lives in Iowa");
    }

    #[test]
    fn plural_person_assertions_are_anchored_to_named_group() {
        let llm_obs = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "person".to_string(),
                kind: "profession".to_string(),
                value: "write programs".to_string(),
                confidence: 0.9,
                evidence_span: Some("write programs".to_string()),
            }],
            signals: empty_signals(),
        };
        let turn = ConversationTurn::user(
            "Chris and Francois are my co-founders. They both write programs.",
        );
        let observation = fuse_observation(&llm_obs, &[], &turn);

        assert_eq!(
            observation.assertions[0].value,
            "Chris and Francois both write programs"
        );
    }

    #[test]
    fn later_single_person_facts_do_not_reuse_plural_pronoun_anchor() {
        let llm_obs = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "person".to_string(),
                kind: "age".to_string(),
                value: "37".to_string(),
                confidence: 0.9,
                evidence_span: Some("37".to_string()),
            }],
            signals: empty_signals(),
        };
        let turn = ConversationTurn::user(
            "Chris and Francois are my co-founders. They both write programs. Chris is 37.",
        );
        let observation = fuse_observation(&llm_obs, &[], &turn);

        assert_eq!(observation.assertions[0].value, "Chris is 37");
    }

    #[test]
    fn group_role_assertions_preserve_both_names() {
        let llm_obs = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "person".to_string(),
                kind: "role".to_string(),
                value: "my co-founders".to_string(),
                confidence: 0.9,
                evidence_span: Some("my co-founders".to_string()),
            }],
            signals: empty_signals(),
        };
        let turn = ConversationTurn::user("Chris and Francois are my co-founders.");
        let observation = fuse_observation(&llm_obs, &[], &turn);

        assert_eq!(
            observation.assertions[0].value,
            "Chris and Francois are my co-founders"
        );
    }

    #[test]
    fn fused_signal_carries_strongest_reliability() {
        // LLM (Learned) + Detector (Heuristic) -> strongest = Learned.
        // Locks the reliability mapping rule.
        let mut signals = empty_signals();
        signals.insert("temporal_relevance".to_string(), Some(llm_signal(0.85)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };
        let turn = ConversationTurn::user("Yesterday it happened.");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        let signal = observation.temporal_relevance.unwrap();
        assert_eq!(signal.reliability(), SignalReliability::Learned);
    }

    #[test]
    fn detectors_never_create_assertions() {
        // A turn rich in detector triggers but with no LLM assertions
        // must still produce zero structured assertions in the
        // CognitiveObservation. Detectors are signal-only.
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals: empty_signals(),
        };
        let turn = ConversationTurn::user(
            "Yesterday I was terrified, I am a mechanical engineer, deadline tomorrow.",
        );
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        assert!(observation.assertions.is_empty());
    }

    #[test]
    fn cue_terms_and_query_intents_populated_deterministically() {
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals: empty_signals(),
        };
        // Content with words outside the existing stop-word list so
        // cue_terms is non-empty. The "?" forces the lexical
        // query_intents helper to emit at least one intent (factual).
        let turn = ConversationTurn::user("Yesterday the manager mentioned the budget?");
        let observation = fuse_observation(&llm_obs, &default_v1_sources(), &turn);
        assert!(
            !observation.cue_terms.is_empty(),
            "cue_terms should be non-empty for content with non-stop-word terms"
        );
        assert!(
            !observation.query_intents.is_empty(),
            "query_intents should be non-empty for a question turn"
        );
    }

    #[test]
    fn uncertainty_is_lower_when_assertions_present() {
        let with_assertion = LlmObservation {
            assertions: vec![LlmAssertion {
                domain: "identity".to_string(),
                kind: "profession".to_string(),
                value: "engineer".to_string(),
                confidence: 0.9,
                evidence_span: None,
            }],
            signals: empty_signals(),
        };
        let without_assertion = LlmObservation {
            assertions: vec![],
            signals: empty_signals(),
        };
        let turn = ConversationTurn::user("hello");
        let with_obs = fuse_observation(&with_assertion, &[], &turn);
        let without_obs = fuse_observation(&without_assertion, &[], &turn);
        assert!(with_obs.uncertainty.value() < without_obs.uncertainty.value());
    }

    #[test]
    fn fusion_uses_only_detectors_passed_in() {
        // Pass an empty detector list. Even with an LLM signal, no
        // dimension should fuse because there's no second source.
        let mut signals = empty_signals();
        signals.insert("temporal_relevance".to_string(), Some(llm_signal(0.9)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };
        let turn = ConversationTurn::user("Yesterday I worked.");
        let observation = fuse_observation(&llm_obs, &[], &turn);
        assert!(
            observation.temporal_relevance.is_none(),
            "no detectors -> no second source -> no fusion, even with strong lexical content"
        );
    }

    #[test]
    fn fusion_works_with_subset_of_detectors() {
        // Pass only the temporal detector. Identity content is
        // present but identity detector is not in the list, so
        // identity_relevance must stay None even with LLM agreement.
        let mut signals = empty_signals();
        signals.insert("temporal_relevance".to_string(), Some(llm_signal(0.9)));
        signals.insert("identity_relevance".to_string(), Some(llm_signal(0.9)));
        let llm_obs = LlmObservation {
            assertions: vec![],
            signals,
        };
        let turn = ConversationTurn::user(
            "Yesterday I am a mechanical engineer.", // contrived: both cues present
        );
        let detectors: Vec<Box<dyn SecondSource>> = vec![Box::new(TemporalDetector::new())];
        let observation = fuse_observation(&llm_obs, &detectors, &turn);
        assert!(observation.temporal_relevance.is_some());
        assert!(observation.identity_relevance.is_none());
    }

    // Suppress warnings for unused imports of detectors that the test
    // module uses indirectly via default_v1_sources.
    #[allow(dead_code)]
    fn _unused_imports_silencer() {
        let _ = AffectLexicon::new();
        let _ = FirstPersonIdentityDetector::new();
        let _ = GoalPhraseLexicon::new();
    }
}
