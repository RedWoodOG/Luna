//! Schema for the JSON observation an LLM extractor proposes for a single
//! conversation turn. Distinct from [`luna_core::CognitiveObservation`],
//! which is the validated, source-fused, recall-ready shape.
//!
//! PR 0.2 owns this schema, validation against it, and the
//! content-addressed cache that stores it. PR 0.3 adds the actual LLM
//! adapter that emits these structures; PR 0.4 adds the deterministic
//! second-source detectors that fuse with these into a final
//! [`luna_core::CognitiveObservation`]. Until then, an `LlmObservation`
//! is **proposed evidence** carrying one source's claim — never trusted
//! to set a contour dimension on its own.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Bumped when the schema or validation rules change in a way that
/// invalidates previously-cached extractions. Lives alongside (not
/// inside) the cache key derivation so callers can compare versions
/// directly without recomputing hashes.
pub const EXTRACTION_SCHEMA_VERSION: u32 = 1;

/// Dimension allowlist for the v0.1 four-axis proof program. Exactly
/// the four dimensions whose two-source plumbing is on the roadmap
/// (Stage 0, PR 0.4). Other [`luna_core::CognitiveObservation`] fields
/// (`attention`, `social_frame`, `trust_relevance`, `emotional_valence`)
/// are intentionally outside this allowlist for proof runs.
pub const ALLOWED_DIMENSIONS: &[&str] = &[
    "temporal_relevance",
    "emotional_arousal",
    "identity_relevance",
    "goal_pressure",
];

/// Reliability allowlist. Mirrors the snake-cased serde representation
/// of [`luna_core::SignalReliability`] so an LLM can name any of the
/// real reliability tiers (typically `learned`).
pub const ALLOWED_RELIABILITIES: &[&str] =
    &["heuristic", "statistical", "learned", "user_confirmed"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmObservation {
    pub assertions: Vec<LlmAssertion>,
    /// Per-dimension signal proposals. Keys are dimension names from
    /// [`ALLOWED_DIMENSIONS`]. A `None` value means the LLM explicitly
    /// reports no signal for that dimension; downstream fusion treats
    /// it as absent. `BTreeMap` so serialized JSON has alphabetically
    /// sorted keys — that gives the cache byte-identical writes for
    /// identical observations regardless of the LLM's emission order.
    pub signals: BTreeMap<String, Option<LlmSignal>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmAssertion {
    pub domain: String,
    pub kind: String,
    pub value: String,
    pub confidence: f32,
    /// The substring of the source turn the LLM cites as evidence.
    /// Optional because the LLM may legitimately be unable to point at
    /// a single span (e.g. when the assertion is the cumulative result
    /// of several phrases).
    #[serde(default)]
    pub evidence_span: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmSignal {
    pub value: f32,
    pub confidence: f32,
    /// Snake-cased [`luna_core::SignalReliability`] variant. Validation
    /// rejects unknown strings rather than silently mapping to a
    /// default — see `ALLOWED_RELIABILITIES`.
    pub reliability: String,
    #[serde(default)]
    pub evidence: Option<String>,
}

/// Returns a list of human-readable violations for the given proposed
/// observation. Empty vec means the schema accepts it.
///
/// Domain and kind are checked only for non-emptiness in PR 0.2; a
/// starter allowlist arrives with PR 0.3 once the LLM prompt commits to
/// a vocabulary it claims to follow.
pub fn validate_observation(observation: &LlmObservation) -> Vec<String> {
    let mut violations = Vec::new();

    for (index, assertion) in observation.assertions.iter().enumerate() {
        if assertion.domain.is_empty() {
            violations.push(format!("assertion[{index}]: domain must not be empty"));
        }
        if assertion.kind.is_empty() {
            violations.push(format!("assertion[{index}]: kind must not be empty"));
        }
        if assertion.value.is_empty() {
            violations.push(format!("assertion[{index}]: value must not be empty"));
        }
        if !is_unit(assertion.confidence) {
            violations.push(format!(
                "assertion[{index}]: confidence {} not in [0,1]",
                assertion.confidence
            ));
        }
    }

    for (name, signal_opt) in &observation.signals {
        if !ALLOWED_DIMENSIONS.contains(&name.as_str()) {
            violations.push(format!(
                "signal '{name}': dimension not in allowlist {ALLOWED_DIMENSIONS:?}"
            ));
            continue;
        }
        let Some(signal) = signal_opt else {
            continue;
        };
        if !is_unit(signal.value) {
            violations.push(format!(
                "signal '{name}': value {} not in [0,1]",
                signal.value
            ));
        }
        if !is_unit(signal.confidence) {
            violations.push(format!(
                "signal '{name}': confidence {} not in [0,1]",
                signal.confidence
            ));
        }
        if !ALLOWED_RELIABILITIES.contains(&signal.reliability.as_str()) {
            violations.push(format!(
                "signal '{name}': reliability '{}' not in allowlist {ALLOWED_RELIABILITIES:?}",
                signal.reliability
            ));
        }
    }

    violations
}

fn is_unit(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

/// (domain, kind) pairs the prompt v2 commits the LLM to produce. A
/// closed set: every assertion must use one of these. Expanded via PR
/// 0.3a from the original 7 to 22 pairs to give the human-seed drafts
/// enough vocabulary without letting the prompt invent an open
/// taxonomy. Promoted to v2 alongside the prompt's assertion-value
/// rule (PR 0.9); the allowlist content is unchanged from v1.
/// Further expansion bumps the prompt (and therefore
/// `prompt_v3_hash`, which invalidates cached extractions).
pub const PROMPT_V3_DOMAIN_KINDS: &[(&str, &str)] = &[
    ("identity", "profession"),
    ("identity", "family_structure"),
    ("identity", "role"),
    ("identity", "creative_origin"),
    ("identity", "mission"),
    ("identity", "project_identity"),
    ("identity", "name"),
    ("work", "current_stressor"),
    ("work", "past_event"),
    ("work", "job_security"),
    ("work", "training"),
    ("work", "customer_protocol"),
    ("work", "territory"),
    ("relationship", "collaboration"),
    ("relationship", "conflict"),
    ("person", "name"),
    ("person", "profession"),
    ("person", "role"),
    ("person", "location"),
    ("person", "age"),
    ("person", "relationship_status"),
    ("person", "transportation"),
    ("person", "trait"),
    ("person", "interest"),
    ("person", "goal"),
    ("person", "availability"),
    ("project", "provenance_engine"),
    ("project", "failed_project"),
    ("project", "creative_work"),
    ("emotion", "affect"),
    ("emotion", "stress_trigger"),
    ("goal", "current_pressure"),
    ("goal", "proof_requirement"),
    ("goal", "career_direction"),
];

/// Stricter validation layered on top of [`validate_observation`]. Adds
/// the prompt-v2 domain/kind allowlist: any assertion whose
/// `(domain, kind)` pair is not in [`PROMPT_V3_DOMAIN_KINDS`] is a
/// violation.
///
/// Layered (rather than parameterized) because the allowlist is
/// inseparable from the prompt it accompanies — a prompt vN edit ships
/// its own validate_against_prompt_vN.
pub fn validate_against_prompt_v3(observation: &LlmObservation) -> Vec<String> {
    let mut violations = validate_observation(observation);
    for (index, assertion) in observation.assertions.iter().enumerate() {
        let pair = (assertion.domain.as_str(), assertion.kind.as_str());
        if !PROMPT_V3_DOMAIN_KINDS.contains(&pair) {
            violations.push(format!(
                "assertion[{index}]: ({}, {}) not in prompt_v3 domain/kind allowlist",
                assertion.domain, assertion.kind
            ));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_signal() -> LlmSignal {
        LlmSignal {
            value: 0.8,
            confidence: 0.7,
            reliability: "learned".to_string(),
            evidence: Some("recently".to_string()),
        }
    }

    fn ok_assertion() -> LlmAssertion {
        LlmAssertion {
            domain: "work".to_string(),
            kind: "current_stressor".to_string(),
            value: "client deadline".to_string(),
            confidence: 0.74,
            evidence_span: Some("the client deadline has been weighing on me".to_string()),
        }
    }

    fn ok_observation() -> LlmObservation {
        let mut signals = BTreeMap::new();
        signals.insert(
            "temporal_relevance".to_string(),
            Some(LlmSignal {
                value: 0.86,
                ..ok_signal()
            }),
        );
        signals.insert("goal_pressure".to_string(), None);
        LlmObservation {
            assertions: vec![ok_assertion()],
            signals,
        }
    }

    #[test]
    fn validation_accepts_well_formed_observation() {
        assert!(validate_observation(&ok_observation()).is_empty());
    }

    #[test]
    fn validation_rejects_empty_assertion_fields() {
        let mut obs = ok_observation();
        obs.assertions[0].domain.clear();
        obs.assertions[0].kind.clear();
        obs.assertions[0].value.clear();
        let violations = validate_observation(&obs);
        assert_eq!(violations.len(), 3);
        assert!(violations.iter().any(|v| v.contains("domain")));
        assert!(violations.iter().any(|v| v.contains("kind")));
        assert!(violations.iter().any(|v| v.contains("value")));
    }

    #[test]
    fn validation_rejects_out_of_range_assertion_confidence() {
        let mut obs = ok_observation();
        obs.assertions[0].confidence = 1.7;
        let violations = validate_observation(&obs);
        assert!(violations.iter().any(|v| v.contains("confidence")));
    }

    #[test]
    fn validation_rejects_unknown_dimension() {
        let mut obs = ok_observation();
        obs.signals
            .insert("attention".to_string(), Some(ok_signal()));
        let violations = validate_observation(&obs);
        assert!(violations
            .iter()
            .any(|v| v.contains("attention") && v.contains("allowlist")));
    }

    #[test]
    fn validation_rejects_unknown_reliability() {
        let mut obs = ok_observation();
        if let Some(Some(signal)) = obs.signals.get_mut("temporal_relevance") {
            signal.reliability = "vibe".to_string();
        }
        let violations = validate_observation(&obs);
        assert!(violations.iter().any(|v| v.contains("reliability")));
    }

    #[test]
    fn validation_rejects_signal_value_or_confidence_out_of_range() {
        let mut obs = ok_observation();
        if let Some(Some(signal)) = obs.signals.get_mut("temporal_relevance") {
            signal.value = -0.1;
            signal.confidence = 2.0;
        }
        let violations = validate_observation(&obs);
        assert!(violations.iter().any(|v| v.contains("value")));
        assert!(violations.iter().any(|v| v.contains("confidence")));
    }

    #[test]
    fn validation_accepts_explicit_none_signal() {
        let mut obs = ok_observation();
        obs.signals.insert("identity_relevance".to_string(), None);
        assert!(validate_observation(&obs).is_empty());
    }

    #[test]
    fn validate_against_prompt_v3_accepts_well_formed_allowlisted_assertion() {
        let obs = ok_observation();
        // ok_assertion's (domain, kind) is ("work", "current_stressor"),
        // which is in the prompt-v2 allowlist (unchanged from v1).
        assert!(validate_against_prompt_v3(&obs).is_empty());
    }

    #[test]
    fn validate_against_prompt_v3_accepts_person_memory_assertions() {
        let mut obs = ok_observation();
        obs.assertions[0].domain = "person".to_string();
        obs.assertions[0].kind = "location".to_string();
        obs.assertions[0].value = "Chris lives in Iowa".to_string();
        obs.assertions[0].evidence_span = Some("Chris lives in Iowa".to_string());

        assert!(validate_against_prompt_v3(&obs).is_empty());
    }

    #[test]
    fn validate_against_prompt_v3_rejects_unlisted_domain_kind_pair() {
        let mut obs = ok_observation();
        obs.assertions[0].domain = "vibe".to_string();
        let violations = validate_against_prompt_v3(&obs);
        assert!(violations
            .iter()
            .any(|v| v.contains("vibe") && v.contains("allowlist")));
    }

    #[test]
    fn validate_against_prompt_v3_includes_base_schema_violations() {
        let mut obs = ok_observation();
        obs.assertions[0].confidence = 9.0; // out-of-range
        let violations = validate_against_prompt_v3(&obs);
        assert!(violations.iter().any(|v| v.contains("confidence")));
    }

    #[test]
    fn observation_round_trips_through_json_with_sorted_signal_keys() {
        // Sorted-key invariant is what the cache relies on for byte-identical
        // writes; lock it in at the schema level so a later change to the
        // signals container can't silently break canonical bytes.
        let obs = ok_observation();
        let text = serde_json::to_string(&obs).unwrap();
        let goal_pos = text.find("goal_pressure").unwrap();
        let temporal_pos = text.find("temporal_relevance").unwrap();
        assert!(
            goal_pos < temporal_pos,
            "BTreeMap should serialize signals alphabetically: {text}"
        );

        let parsed: LlmObservation = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, obs);
    }
}
