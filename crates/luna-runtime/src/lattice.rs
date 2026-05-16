use luna_core::{Signal, SignalReliability};
use serde::{Deserialize, Serialize};

use crate::{AssertionLifecycleStatus, MemoryClaim};

// ── Lattice types ───────────────────────────────────────────────────────────

/// Seven-dimensional cognitive field computed from typed assertions.
///
/// Each dimension is a Signal (value + confidence) derived deterministically
/// from memory state claims. This is the post-M1 lattice that supersedes the
/// v0 `AttentionLattice` in `luna_core`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionLattice {
    pub identity: Signal,
    pub meaning: Signal,
    pub goal: Signal,
    pub trust: Signal,
    pub attention: Signal,
    pub context: Signal,
    pub skill: Signal,
}

/// Maps each lattice dimension to its contributing source claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeProvenance {
    pub identity_sources: Vec<ContributionProvenance>,
    pub meaning_sources: Vec<ContributionProvenance>,
    pub goal_sources: Vec<ContributionProvenance>,
    pub trust_sources: Vec<ContributionProvenance>,
    pub attention_sources: Vec<ContributionProvenance>,
    pub context_sources: Vec<ContributionProvenance>,
    pub skill_sources: Vec<ContributionProvenance>,
}

/// Single provenance record: what claim, from what event, why it contributed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionProvenance {
    pub claim_id: String,
    pub source_event_id: String,
    pub source_event_hash: String,
    pub contribution_weight: f32,
    pub reason: String,
}

// ── Computation engine ──────────────────────────────────────────────────────

/// Compute a full AttentionLattice from memory state claims.
///
/// Every dimension value is traceable to source claims through the
/// returned `LatticeProvenance`.  Computation is pure — same inputs
/// always produce the same outputs.
pub fn compute_lattice(
    claims: &[MemoryClaim],
    turn_number: u32,
) -> (AttentionLattice, LatticeProvenance) {
    let current: Vec<&MemoryClaim> = claims
        .iter()
        .filter(|c| c.lifecycle_status == AssertionLifecycleStatus::Current)
        .collect();

    let (identity, identity_src) = compute_identity(&current);
    let (meaning, meaning_src) = compute_meaning(&current);
    let (goal, goal_src) = compute_goal(&current);
    let (trust, trust_src) = compute_trust(&current);
    let (attention, attention_src) = compute_attention(&current, turn_number);
    let (context, context_src) = compute_context(turn_number);
    let (skill, skill_src) = compute_skill(&current);

    (
        AttentionLattice {
            identity,
            meaning,
            goal,
            trust,
            attention,
            context,
            skill,
        },
        LatticeProvenance {
            identity_sources: identity_src,
            meaning_sources: meaning_src,
            goal_sources: goal_src,
            trust_sources: trust_src,
            attention_sources: attention_src,
            context_sources: context_src,
            skill_sources: skill_src,
        },
    )
}

// ── Dimension computers ─────────────────────────────────────────────────────

/// identity: count of identity:* claims.  Value = min(1.0, count / 5.0).
fn compute_identity(claims: &[&MemoryClaim]) -> (Signal, Vec<ContributionProvenance>) {
    let matching: Vec<&&MemoryClaim> = claims
        .iter()
        .filter(|c| c.domain == "identity")
        .collect();

    let count = matching.len();
    let value = (count as f32 / 5.0).min(1.0);
    let confidence = contribution_confidence(count, 1);

    let sources = matching
        .iter()
        .map(|c| provenance_for(c, 1.0 / 5.0, "identity claim"))
        .collect();

    (signal(value, confidence, count), sources)
}

/// meaning: sum of value lengths for person:*, project:*, manuscript:*.
/// Value = min(1.0, total_len / 500.0).
fn compute_meaning(claims: &[&MemoryClaim]) -> (Signal, Vec<ContributionProvenance>) {
    let matching: Vec<&&MemoryClaim> = claims
        .iter()
        .filter(|c| {
            c.domain == "person" || c.domain == "project" || c.domain == "manuscript"
        })
        .collect();

    let total_len: usize = matching.iter().map(|c| c.value.len()).sum();
    let count = matching.len();
    let value = (total_len as f32 / 500.0).min(1.0);
    let confidence = contribution_confidence(count, 1);

    let sources = matching
        .iter()
        .map(|c| {
            provenance_for(
                c,
                c.value.len() as f32 / 500.0,
                &format!("{} claim ({} chars)", c.domain, c.value.len()),
            )
        })
        .collect();

    (signal(value, confidence, count), sources)
}

/// goal: presence of person:goal or project:deadline claims.
/// Value = min(1.0, count / 3.0).
fn compute_goal(claims: &[&MemoryClaim]) -> (Signal, Vec<ContributionProvenance>) {
    let matching: Vec<&&MemoryClaim> = claims
        .iter()
        .filter(|c| {
            (c.domain == "person" && c.kind == "goal")
                || (c.domain == "project" && c.kind == "deadline")
        })
        .collect();

    let count = matching.len();
    let value = (count as f32 / 3.0).min(1.0);
    let confidence = contribution_confidence(count, 1);

    let sources = matching
        .iter()
        .map(|c| provenance_for(c, 1.0 / 3.0, &format!("{}:{} claim", c.domain, c.kind)))
        .collect();

    (signal(value, confidence, count), sources)
}

/// trust: count of relationship:* claims.  Value = min(1.0, count / 4.0).
fn compute_trust(claims: &[&MemoryClaim]) -> (Signal, Vec<ContributionProvenance>) {
    let matching: Vec<&&MemoryClaim> = claims
        .iter()
        .filter(|c| c.domain == "relationship")
        .collect();

    let count = matching.len();
    let value = (count as f32 / 4.0).min(1.0);
    let confidence = contribution_confidence(count, 1);

    let sources = matching
        .iter()
        .map(|c| provenance_for(c, 1.0 / 4.0, "relationship claim"))
        .collect();

    (signal(value, confidence, count), sources)
}

/// attention: working-memory activation score.
///
/// Since MemoryClaim does not carry per-claim turn numbers, recency is
/// approximated by treating every current claim as equally recent and
/// scaling by the total claim count relative to turn progression.
/// Value = min(1.0, count / 10.0).
fn compute_attention(
    claims: &[&MemoryClaim],
    _turn_number: u32,
) -> (Signal, Vec<ContributionProvenance>) {
    let count = claims.len();
    let value = (count as f32 / 10.0).min(1.0);
    let confidence = contribution_confidence(count, 1);

    let sources = claims
        .iter()
        .map(|c| provenance_for(c, 1.0 / 10.0, "active working-memory claim"))
        .collect();

    (signal(value, confidence, count), sources)
}

/// context: situational grounding from turn position.
/// Value = min(1.0, turn_number / 10.0).  Early turns = low grounding.
fn compute_context(turn_number: u32) -> (Signal, Vec<ContributionProvenance>) {
    let value = (turn_number as f32 / 10.0).min(1.0);
    let confidence = if turn_number >= 10 { 0.9 } else { 0.5 };

    let sources = vec![ContributionProvenance {
        claim_id: String::new(),
        source_event_id: String::new(),
        source_event_hash: String::new(),
        contribution_weight: 1.0,
        reason: format!("turn position {turn_number}"),
    }];

    (Signal::new(value, confidence, SignalReliability::Heuristic), sources)
}

/// skill: presence of identity:profession or person:role claims.
/// Value = min(1.0, count / 2.0).
fn compute_skill(claims: &[&MemoryClaim]) -> (Signal, Vec<ContributionProvenance>) {
    let matching: Vec<&&MemoryClaim> = claims
        .iter()
        .filter(|c| {
            (c.domain == "identity" && c.kind == "profession")
                || (c.domain == "person" && c.kind == "role")
        })
        .collect();

    let count = matching.len();
    let value = (count as f32 / 2.0).min(1.0);
    let confidence = contribution_confidence(count, 1);

    let sources = matching
        .iter()
        .map(|c| provenance_for(c, 1.0 / 2.0, &format!("{}:{} claim", c.domain, c.kind)))
        .collect();

    (signal(value, confidence, count), sources)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn signal(value: f32, confidence: f32, source_count: usize) -> Signal {
    let reliability = if source_count >= 3 {
        SignalReliability::Statistical
    } else if source_count >= 2 {
        SignalReliability::Heuristic
    } else {
        SignalReliability::Heuristic
    };
    Signal::new(value, confidence, reliability).with_source_count(source_count.max(1) as u8)
}

fn contribution_confidence(count: usize, min_for_full: usize) -> f32 {
    if count >= min_for_full {
        0.9
    } else if count > 0 {
        0.5
    } else {
        0.0
    }
}

fn provenance_for(
    claim: &MemoryClaim,
    weight: f32,
    reason: &str,
) -> ContributionProvenance {
    ContributionProvenance {
        claim_id: claim.key.clone(),
        source_event_id: String::new(),
        source_event_hash: String::new(),
        contribution_weight: weight,
        reason: reason.to_string(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(domain: &str, kind: &str, value: &str) -> MemoryClaim {
        MemoryClaim {
            key: format!("{domain}:{kind}:{value}"),
            domain: domain.to_string(),
            kind: kind.to_string(),
            value: value.to_string(),
            status: luna_core::AssertionConfidenceTier::Unconfirmed,
            lifecycle_status: AssertionLifecycleStatus::Current,
        }
    }

    // ── empty assertions ────────────────────────────────────────────────

    #[test]
    fn empty_claims_produces_zero_lattice() {
        let (lattice, prov) = compute_lattice(&[], 1);

        assert_eq!(lattice.identity.value(), 0.0);
        assert_eq!(lattice.meaning.value(), 0.0);
        assert_eq!(lattice.goal.value(), 0.0);
        assert_eq!(lattice.trust.value(), 0.0);
        assert_eq!(lattice.attention.value(), 0.0);
        assert_eq!(lattice.skill.value(), 0.0);

        assert!(prov.identity_sources.is_empty());
        assert!(prov.meaning_sources.is_empty());
    }

    // ── identity ────────────────────────────────────────────────────────

    #[test]
    fn identity_dimension_computes_from_identity_claims() {
        let claims = vec![
            claim("identity", "name", "Luna"),
            claim("identity", "profession", "mechanical engineer"),
            claim("identity", "location", "Detroit"),
            claim("identity", "trait", "curious"),
            claim("identity", "trait", "analytical"),
        ];
        let (lattice, prov) = compute_lattice(&claims, 1);

        // 5 identity claims → 5/5 = 1.0
        assert!((lattice.identity.value() - 1.0).abs() < f32::EPSILON);
        assert!(lattice.identity.confidence() > 0.8);
        assert_eq!(prov.identity_sources.len(), 5);
    }

    #[test]
    fn identity_partial_with_few_claims() {
        let claims = vec![claim("identity", "name", "Luna")];
        let (lattice, _) = compute_lattice(&claims, 1);

        // 1 identity claim → 1/5 = 0.2
        assert!((lattice.identity.value() - 0.2).abs() < f32::EPSILON);
    }

    // ── meaning ─────────────────────────────────────────────────────────

    #[test]
    fn meaning_accumulates_value_length() {
        let claims = vec![
            claim("person", "name", "Alice"),                     // len 5
            claim("project", "title", "Operation Paperclip"),     // len 19
            claim("manuscript", "scene", "The sun set over the industrial skyline"), // len 39
        ];
        let total_len: usize = claims.iter().map(|c| c.value.len()).sum();
        let expected = total_len as f32 / 500.0;
        let (lattice, prov) = compute_lattice(&claims, 1);

        assert!((lattice.meaning.value() - expected).abs() < 0.001);
        assert_eq!(prov.meaning_sources.len(), 3);
    }

    #[test]
    fn meaning_saturates_at_one() {
        let long_value = "x".repeat(600);
        let claims = vec![claim("person", "bio", &long_value)];
        let (lattice, _) = compute_lattice(&claims, 1);

        assert!((lattice.meaning.value() - 1.0).abs() < f32::EPSILON);
    }

    // ── goal ────────────────────────────────────────────────────────────

    #[test]
    fn goal_from_person_goal_and_project_deadline() {
        let claims = vec![
            claim("person", "goal", "finish the prototype"),
            claim("person", "goal", "learn Rust"),
            claim("project", "deadline", "2026-06-01"),
        ];
        let (lattice, prov) = compute_lattice(&claims, 1);

        // 3 goal claims → 3/3 = 1.0
        assert!((lattice.goal.value() - 1.0).abs() < f32::EPSILON);
        assert_eq!(prov.goal_sources.len(), 3);
    }

    #[test]
    fn goal_ignores_other_kinds() {
        let claims = vec![
            claim("person", "name", "Alice"),
            claim("project", "title", "Luna"),
        ];
        let (lattice, _) = compute_lattice(&claims, 1);

        assert_eq!(lattice.goal.value(), 0.0);
    }

    // ── trust ───────────────────────────────────────────────────────────

    #[test]
    fn trust_from_relationship_claims() {
        let claims = vec![
            claim("relationship", "friend", "Alice"),
            claim("relationship", "collaborator", "Bob"),
            claim("relationship", "mentor", "Carol"),
            claim("relationship", "trusted", "Dave"),
        ];
        let (lattice, prov) = compute_lattice(&claims, 1);

        // 4 relationship → 4/4 = 1.0
        assert!((lattice.trust.value() - 1.0).abs() < f32::EPSILON);
        assert_eq!(prov.trust_sources.len(), 4);
    }

    // ── attention ───────────────────────────────────────────────────────

    #[test]
    fn attention_scales_with_claim_count() {
        let claims: Vec<MemoryClaim> = (0..5)
            .map(|i| claim("identity", "trait", &format!("trait{i}")))
            .collect();
        let (lattice, _) = compute_lattice(&claims, 1);

        // 5 claims → 5/10 = 0.5
        assert!((lattice.attention.value() - 0.5).abs() < f32::EPSILON);
    }

    // ── context ─────────────────────────────────────────────────────────

    #[test]
    fn context_grows_with_turn_number() {
        let (early, _) = compute_lattice(&[], 1);
        let (mid, _) = compute_lattice(&[], 5);
        let (late, _) = compute_lattice(&[], 10);

        assert!((early.context.value() - 0.1).abs() < 0.001);
        assert!((mid.context.value() - 0.5).abs() < 0.001);
        assert!((late.context.value() - 1.0).abs() < f32::EPSILON);
    }

    // ── skill ───────────────────────────────────────────────────────────

    #[test]
    fn skill_from_profession_and_role() {
        let claims = vec![
            claim("identity", "profession", "mechanical engineer"),
            claim("person", "role", "project lead"),
        ];
        let (lattice, prov) = compute_lattice(&claims, 1);

        // 2 claims → 2/2 = 1.0
        assert!((lattice.skill.value() - 1.0).abs() < f32::EPSILON);
        assert_eq!(prov.skill_sources.len(), 2);
    }

    // ── determinism ─────────────────────────────────────────────────────

    #[test]
    fn lattice_is_deterministic() {
        let claims = vec![
            claim("identity", "name", "Luna"),
            claim("identity", "profession", "mechanical engineer"),
            claim("relationship", "friend", "Alice"),
            claim("person", "goal", "finish prototype"),
        ];

        let (a, _) = compute_lattice(&claims, 3);
        let (b, _) = compute_lattice(&claims, 3);

        assert!((a.identity.value() - b.identity.value()).abs() < f32::EPSILON);
        assert!((a.identity.confidence() - b.identity.confidence()).abs() < f32::EPSILON);
        assert!((a.meaning.value() - b.meaning.value()).abs() < f32::EPSILON);
        assert!((a.goal.value() - b.goal.value()).abs() < f32::EPSILON);
        assert!((a.trust.value() - b.trust.value()).abs() < f32::EPSILON);
        assert!((a.attention.value() - b.attention.value()).abs() < f32::EPSILON);
        assert!((a.context.value() - b.context.value()).abs() < f32::EPSILON);
        assert!((a.skill.value() - b.skill.value()).abs() < f32::EPSILON);
    }

    // ── non-current claims ignored ──────────────────────────────────────

    #[test]
    fn ignores_non_current_claims() {
        let claims = vec![
            MemoryClaim {
                key: "identity:name:Luna".to_string(),
                domain: "identity".to_string(),
                kind: "name".to_string(),
                value: "Luna".to_string(),
                status: luna_core::AssertionConfidenceTier::Unconfirmed,
                lifecycle_status: AssertionLifecycleStatus::Superseded,
            },
        ];
        let (lattice, _) = compute_lattice(&claims, 1);

        assert_eq!(lattice.identity.value(), 0.0);
    }

    // ── serialization ───────────────────────────────────────────────────

    #[test]
    fn attention_lattice_serializes() {
        let claims = vec![claim("identity", "name", "Luna")];
        let (lattice, prov) = compute_lattice(&claims, 1);

        let json = serde_json::to_string(&lattice).unwrap();
        assert!(json.contains("identity"));
        assert!(json.contains("meaning"));
        assert!(json.contains("goal"));

        let prov_json = serde_json::to_string(&prov).unwrap();
        assert!(prov_json.contains("identity_sources"));
    }

    #[test]
    fn attention_lattice_deserializes() {
        let json = r#"{
            "identity":{"value":0.2,"confidence":0.5,"reliability":"heuristic","source_count":1},
            "meaning":{"value":0.0,"confidence":0.0,"reliability":"heuristic","source_count":1},
            "goal":{"value":0.0,"confidence":0.0,"reliability":"heuristic","source_count":1},
            "trust":{"value":0.0,"confidence":0.0,"reliability":"heuristic","source_count":1},
            "attention":{"value":0.0,"confidence":0.0,"reliability":"heuristic","source_count":1},
            "context":{"value":0.1,"confidence":0.5,"reliability":"heuristic","source_count":1},
            "skill":{"value":0.0,"confidence":0.0,"reliability":"heuristic","source_count":1}
        }"#;
        let lattice: AttentionLattice = serde_json::from_str(json).unwrap();
        assert!((lattice.identity.value() - 0.2).abs() < f32::EPSILON);
    }
}
