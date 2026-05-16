use luna_core::{
    BondDecayedEvent, BondEvent, BondEventRecord, BondFormedEvent, BondGraph, BondKind,
    BondSupersededEvent, EntityBond, LunaEvent, MemoryProvenance, Signal, SignalReliability,
};

use crate::MemoryClaim;

/// Decay rate per turn for bonds that receive no new evidence.
const DECAY_RATE: f32 = 0.05;

// ── Kind mapping ────────────────────────────────────────────────────────────

fn bond_kind_from_claim(claim: &MemoryClaim) -> Option<BondKind> {
    // First check kind for explicit bond type
    match claim.kind.as_str() {
        "friend" => return Some(BondKind::Friend),
        "colleague" | "coworker" => return Some(BondKind::Colleague),
        "family" | "brother" | "sister" | "mother" | "father" | "parent" | "son"
        | "daughter" | "spouse" | "husband" | "wife" | "uncle" | "aunt" => {
            return Some(BondKind::Family)
        }
        "romantic" | "partner" | "girlfriend" | "boyfriend" | "fiance" | "fiancee" => {
            return Some(BondKind::Romantic)
        }
        "acquaintance" => return Some(BondKind::Acquaintance),
        "rival" | "enemy" | "adversary" => return Some(BondKind::Rival),
        "mentor" | "teacher" | "coach" | "advisor" => return Some(BondKind::Mentor),
        "trust_high" | "intimate" => return None,
        _ => {}
    }

    // Fall back to inspecting the claim value for relationship keywords
    let lower = claim.value.to_ascii_lowercase();
    if lower.contains("friend") {
        Some(BondKind::Friend)
    } else if lower.contains("colleague") || lower.contains("coworker") || lower.contains("co-worker") {
        Some(BondKind::Colleague)
    } else if lower.contains("girlfriend") || lower.contains("boyfriend") || lower.contains("fiance")
        || lower.contains("spouse") || lower.contains("wife") || lower.contains("husband")
        || lower.contains("partner") || lower.contains("romantic")
    {
        Some(BondKind::Romantic)
    } else if lower.contains("family") || lower.contains("mother") || lower.contains("father")
        || lower.contains("sister") || lower.contains("brother") || lower.contains("daughter")
        || lower.contains("son") || lower.contains("uncle") || lower.contains("aunt")
    {
        Some(BondKind::Family)
    } else {
        Some(BondKind::Acquaintance)
    }
}

fn bond_kind_trust(kind: BondKind) -> f32 {
    match kind {
        BondKind::Family => 0.85,
        BondKind::Romantic => 0.80,
        BondKind::Friend => 0.65,
        BondKind::Mentor => 0.60,
        BondKind::Colleague => 0.40,
        BondKind::Acquaintance => 0.20,
        BondKind::Rival => 0.15,
        BondKind::Stranger => 0.05,
    }
}

fn bond_kind_intimacy(kind: BondKind) -> f32 {
    match kind {
        BondKind::Romantic => 0.85,
        BondKind::Family => 0.80,
        BondKind::Friend => 0.55,
        BondKind::Mentor => 0.35,
        BondKind::Colleague => 0.25,
        BondKind::Acquaintance => 0.10,
        BondKind::Rival => 0.05,
        BondKind::Stranger => 0.01,
    }
}

// ── Bond identity ───────────────────────────────────────────────────────────

fn bond_id(source: &str, target: &str) -> String {
    format!(
        "bond:{}:{}",
        source.to_ascii_lowercase(),
        target.to_ascii_lowercase()
    )
}

fn target_entity_from_claim(claim: &MemoryClaim) -> Option<String> {
    let value = claim.value.trim();
    if value.is_empty() {
        return None;
    }
    // Try to extract a person name from the claim value
    let first_word = match value.split_whitespace().next() {
        Some(w) => w,
        None => return None,
    };
    let common_words = [
        "i", "i'm", "my", "a", "an", "the", "he", "she", "they", "we", "it",
        "is", "was", "are", "has", "have", "lives", "wants", "works", "likes",
    ];
    let first_lower = first_word.to_ascii_lowercase();

    if common_words.contains(&first_lower.as_str()) {
        // First word is a common word, look for a name later in the value
        for word in value.split_whitespace().skip(1) {
            let clean = word.trim_matches(|c: char| !c.is_ascii_alphabetic());
            if clean.is_empty() || common_words.contains(&clean.to_ascii_lowercase().as_str()) {
                continue;
            }
            let mut chars = clean.chars();
            let capitalized = match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => continue,
            };
            return Some(capitalized);
        }
        // Fallback: use the whole value as a label (for unknown entity patterns)
        let mut chars = value.chars();
        return Some(match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => value.to_string(),
        });
    }

    // First word looks like a name — capitalize it
    let mut chars = first_word.chars();
    let capitalized = match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => return None,
    };
    Some(capitalized)
}

// ── Signal computation ──────────────────────────────────────────────────────

fn compute_trust_signal(kind: BondKind, evidence_count: usize) -> Signal {
    let base = bond_kind_trust(kind);
    let boost = (evidence_count as f32).min(5.0) * 0.04;
    let value = (base + boost).min(1.0);
    let confidence = (evidence_count as f32 * 0.15).min(0.9);
    let reliability = if evidence_count >= 3 {
        SignalReliability::Statistical
    } else {
        SignalReliability::Heuristic
    };
    Signal::new(value, confidence, reliability)
}

fn compute_intimacy_signal(kind: BondKind, evidence_count: usize) -> Signal {
    let base = bond_kind_intimacy(kind);
    let boost = (evidence_count as f32).min(5.0) * 0.03;
    let value = (base + boost).min(1.0);
    let confidence = (evidence_count as f32 * 0.12).min(0.85);
    let reliability = if evidence_count >= 3 {
        SignalReliability::Statistical
    } else {
        SignalReliability::Heuristic
    };
    Signal::new(value, confidence, reliability)
}

fn apply_decay(trust: &Signal, intimacy: &Signal) -> (Signal, Signal) {
    let new_trust_val = (trust.value() * (1.0 - DECAY_RATE)).max(0.0);
    let new_intimacy_val = (intimacy.value() * (1.0 - DECAY_RATE)).max(0.0);
    (
        Signal::new(
            new_trust_val,
            trust.confidence() * 0.95,
            trust.reliability(),
        ),
        Signal::new(
            new_intimacy_val,
            intimacy.confidence() * 0.95,
            intimacy.reliability(),
        ),
    )
}

// ── Core computation ────────────────────────────────────────────────────────

/// Compute or update the Entity Bond Graph from relationship claims.
///
/// Returns the updated BondGraph and bond-related LunaEvents emitted this turn.
pub fn compute_bonds(
    claims: &[MemoryClaim],
    previous_graph: Option<&BondGraph>,
    turn_number: u32,
    turn_timestamp: i64,
) -> (BondGraph, Vec<LunaEvent>) {
    let mut events: Vec<LunaEvent> = Vec::new();

    // Collect relationship and person claims that may describe bonds
    let relationship_claims: Vec<&MemoryClaim> = claims
        .iter()
        .filter(|c| {
            (c.domain == "relationship" || c.domain == "person")
                && c.lifecycle_status == luna_core::AssertionLifecycleStatus::Current
        })
        .collect();

    // Group claims by target entity
    type ClaimGroup<'a> = Vec<(&'a MemoryClaim, BondKind)>;
    let mut target_groups: std::collections::BTreeMap<String, ClaimGroup> =
        std::collections::BTreeMap::new();
    let mut trust_boosts: std::collections::BTreeMap<String, f32> =
        std::collections::BTreeMap::new();
    let mut intimacy_boosts: std::collections::BTreeMap<String, f32> =
        std::collections::BTreeMap::new();

    for claim in &relationship_claims {
        match claim.kind.as_str() {
            "trust_high" => {
                if let Some(target) = target_entity_from_claim(claim) {
                    *trust_boosts.entry(target.to_ascii_lowercase()).or_insert(0.0) += 0.2;
                }
            }
            "intimate" => {
                if let Some(target) = target_entity_from_claim(claim) {
                    *intimacy_boosts
                        .entry(target.to_ascii_lowercase())
                        .or_insert(0.0) += 0.3;
                }
            }
            _ => {
                if let Some(target) = target_entity_from_claim(claim) {
                    if let Some(kind) = bond_kind_from_claim(claim) {
                        target_groups
                            .entry(target.to_ascii_lowercase())
                            .or_default()
                            .push((claim, kind));
                    }
                }
            }
        }
    }

    let source_entity = "luna";
    let mut new_bonds: Vec<EntityBond> = Vec::new();
    let mut refreshed_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    // Build previous bond index
    let prev_index: std::collections::BTreeMap<String, &EntityBond> = previous_graph
        .map(|g| g.bonds.iter().map(|b| (b.bond_id.clone(), b)).collect())
        .unwrap_or_default();

    for (target_lower, claim_list) in &target_groups {
        // Determine the display target name from the first claim
        let display_target = target_entity_from_claim(claim_list[0].0)
            .unwrap_or_else(|| target_lower.clone());
        let id = bond_id(source_entity, &display_target);
        refreshed_ids.insert(id.clone());

        // Choose dominant bond kind (highest trust, preferring non-Stranger)
        let dominant_kind = claim_list
            .iter()
            .max_by_key(|(_, kind)| {
                let t = bond_kind_trust(*kind);
                if matches!(kind, BondKind::Stranger) {
                    (0u8, (t * 100.0) as u32)
                } else {
                    (1u8, (t * 100.0) as u32)
                }
            })
            .map(|(_, kind)| *kind)
            .unwrap_or(BondKind::Stranger);

        let evidence_count = claim_list.len();

        // Compute base signals
        let mut trust_signal = compute_trust_signal(dominant_kind, evidence_count);
        let mut intimacy_signal = compute_intimacy_signal(dominant_kind, evidence_count);

        // Apply boost effects
        if let Some(&boost) = trust_boosts.get(&target_lower.to_ascii_lowercase()) {
            let new_val = (trust_signal.value() + boost).min(1.0);
            trust_signal = Signal::new(
                new_val,
                (trust_signal.confidence() + 0.1).min(1.0),
                trust_signal.reliability(),
            );
        }
        if let Some(&boost) = intimacy_boosts.get(&target_lower.to_ascii_lowercase()) {
            let new_val = (intimacy_signal.value() + boost).min(1.0);
            intimacy_signal = Signal::new(
                new_val,
                (intimacy_signal.confidence() + 0.1).min(1.0),
                intimacy_signal.reliability(),
            );
        }

        // Build event history and provenance from claims
        let mut history: Vec<BondEventRecord> = Vec::new();
        let mut bond_provenance: Vec<MemoryProvenance> = Vec::new();

        for (claim, _kind) in claim_list {
            let prov = MemoryProvenance {
                episode_id: None,
                turn_id: None,
                assertion_key: Some(claim.key.clone()),
                system_root: None,
                lifecycle_status: None,
            };

            history.push(BondEventRecord {
                event_type: BondEvent::Disclosure,
                source_event_id: claim.key.clone(),
                source_event_hash: claim.key.clone(), // use key as hash stand-in
                timestamp: turn_timestamp,
                turn_number,
                detail: format!("{}:{}={}", claim.domain, claim.kind, claim.value),
            });

            bond_provenance.push(prov);
        }

        let bond = EntityBond {
            bond_id: id.clone(),
            source_entity: source_entity.to_string(),
            target_entity: display_target,
            bond_kind: dominant_kind,
            trust: trust_signal.clone(),
            intimacy: intimacy_signal.clone(),
            event_history: history,
            provenance: bond_provenance,
            superseded_by: None,
        };

        // Check for supersession (bond kind changed from previous)
        if let Some(prev_bond) = prev_index.get(&id) {
            if prev_bond.bond_kind != dominant_kind {
                events.push(LunaEvent::BondSuperseded(BondSupersededEvent {
                    old_bond_id: id.clone(),
                    new_bond: bond.clone(),
                    reason: format!(
                        "bond kind changed from {:?} to {:?}",
                        prev_bond.bond_kind, dominant_kind
                    ),
                    turn_number,
                }));
            }
            events.push(LunaEvent::BondFormed(BondFormedEvent {
                bond: bond.clone(),
                turn_number,
            }));
        } else {
            events.push(LunaEvent::BondFormed(BondFormedEvent {
                bond: bond.clone(),
                turn_number,
            }));
        }

        new_bonds.push(bond);
    }

    // Carry forward decayed bonds from previous graph that weren't refreshed
    if let Some(prev_graph) = previous_graph {
        for prev_bond in &prev_graph.bonds {
            if !refreshed_ids.contains(&prev_bond.bond_id) {
                let (decayed_trust, decayed_intimacy) =
                    apply_decay(&prev_bond.trust, &prev_bond.intimacy);

                if (prev_bond.trust.value() - decayed_trust.value()).abs() > 0.001
                    || (prev_bond.intimacy.value() - decayed_intimacy.value()).abs() > 0.001
                {
                    events.push(LunaEvent::BondDecayed(BondDecayedEvent {
                        bond_id: prev_bond.bond_id.clone(),
                        previous_trust: prev_bond.trust,
                        new_trust: decayed_trust,
                        previous_intimacy: prev_bond.intimacy,
                        new_intimacy: decayed_intimacy,
                        turn_number,
                    }));
                }

                let mut decayed_bond = prev_bond.clone();
                decayed_bond.trust = decayed_trust;
                decayed_bond.intimacy = decayed_intimacy;
                new_bonds.push(decayed_bond);
            }
        }
    }

    // Sort for deterministic output
    new_bonds.sort_by(|a, b| a.bond_id.cmp(&b.bond_id));

    (
        BondGraph {
            bonds: new_bonds,
            computed_at_turn: turn_number,
        },
        events,
    )
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use luna_core::AssertionConfidenceTier;
    use luna_core::AssertionLifecycleStatus;

    fn claim(
        domain: &str,
        kind: &str,
        value: &str,
    ) -> MemoryClaim {
        MemoryClaim {
            key: format!("{}:{}={}", domain, kind, value),
            domain: domain.to_string(),
            kind: kind.to_string(),
            value: value.to_string(),
            status: AssertionConfidenceTier::Unconfirmed,
            lifecycle_status: AssertionLifecycleStatus::Current,
        }
    }

    fn superseded_claim(
        domain: &str,
        kind: &str,
        value: &str,
    ) -> MemoryClaim {
        MemoryClaim {
            key: format!("{}:{}={}", domain, kind, value),
            domain: domain.to_string(),
            kind: kind.to_string(),
            value: value.to_string(),
            status: AssertionConfidenceTier::Unconfirmed,
            lifecycle_status: AssertionLifecycleStatus::Superseded,
        }
    }

    #[test]
    fn empty_assertions_produces_empty_graph() {
        let claims: Vec<MemoryClaim> = vec![];
        let (graph, events) = compute_bonds(&claims, None, 0, 0);
        assert!(graph.bonds.is_empty());
        assert_eq!(graph.computed_at_turn, 0);
        assert!(events.is_empty());
    }

    #[test]
    fn single_relationship_creates_bond() {
        let claims = vec![claim("relationship", "friend", "Alice")];
        let (graph, events) = compute_bonds(&claims, None, 1, 1000);

        assert_eq!(graph.bonds.len(), 1);
        let bond = &graph.bonds[0];
        assert_eq!(bond.source_entity, "luna");
        assert_eq!(bond.target_entity, "Alice");
        assert_eq!(bond.bond_kind, BondKind::Friend);
        assert!(bond.trust.value() > 0.0);
        assert!(bond.intimacy.value() > 0.0);
        assert_eq!(bond.event_history.len(), 1);
        assert_eq!(bond.event_history[0].event_type, BondEvent::Disclosure);
        assert!(bond.superseded_by.is_none());

        // Should emit BondFormed
        assert!(events.iter().any(|e| matches!(e, LunaEvent::BondFormed(_))));
    }

    #[test]
    fn disclosure_increases_intimacy() {
        let claims = vec![
            claim("relationship", "friend", "Alice"),
            claim("relationship", "friend", "Alice"),
            claim("relationship", "friend", "Alice"),
        ];
        let (graph, _) = compute_bonds(&claims, None, 2, 2000);

        assert_eq!(graph.bonds.len(), 1);
        let bond = &graph.bonds[0];
        // 3 evidence points should boost intimacy above the base 0.55 for Friend
        assert!(bond.intimacy.value() >= 0.55);
        assert_eq!(bond.event_history.len(), 3);
    }

    #[test]
    fn correction_supersedes_old_bond() {
        // First turn: friend bond
        let claims1 = vec![claim("relationship", "friend", "Alice")];
        let (graph1, _) = compute_bonds(&claims1, None, 1, 1000);
        assert_eq!(graph1.bonds.len(), 1);

        // Second turn: same entity now claimed as rival
        let claims2 = vec![claim("relationship", "rival", "Alice")];
        let (graph2, events) = compute_bonds(&claims2, Some(&graph1), 2, 2000);

        assert_eq!(graph2.bonds.len(), 1);
        let bond = &graph2.bonds[0];
        assert_eq!(bond.bond_kind, BondKind::Rival);

        // Should emit a BondSuperseded event
        let has_superseded = events
            .iter()
            .any(|e| matches!(e, LunaEvent::BondSuperseded(_)));
        assert!(has_superseded);
    }

    #[test]
    fn bond_decays_after_inactivity() {
        // Create a bond at turn 1
        let claims = vec![claim("relationship", "friend", "Alice")];
        let (graph1, _) = compute_bonds(&claims, None, 1, 1000);
        let trust_before = graph1.bonds[0].trust.value();

        // Turn 2 with no relationship claims
        let no_claims: Vec<MemoryClaim> = vec![];
        let (graph2, events) = compute_bonds(&no_claims, Some(&graph1), 2, 2000);

        assert_eq!(graph2.bonds.len(), 1);
        let bond = &graph2.bonds[0];
        let expected_trust = trust_before * (1.0 - DECAY_RATE);
        assert!(
            (bond.trust.value() - expected_trust).abs() < 0.001,
            "expected trust {} got {}",
            expected_trust,
            bond.trust.value()
        );

        let has_decay = events
            .iter()
            .any(|e| matches!(e, LunaEvent::BondDecayed(_)));
        assert!(has_decay);
    }

    #[test]
    fn bond_graph_is_deterministic() {
        let claims = vec![
            claim("relationship", "friend", "Bob"),
            claim("relationship", "colleague", "Alice"),
            claim("relationship", "mentor", "Charlie"),
        ];

        let (graph1, _) = compute_bonds(&claims, None, 3, 3000);
        let (graph2, _) = compute_bonds(&claims, None, 3, 3000);

        assert_eq!(graph1.bonds.len(), graph2.bonds.len());
        for (a, b) in graph1.bonds.iter().zip(graph2.bonds.iter()) {
            assert_eq!(a.bond_id, b.bond_id);
            assert_eq!(a.trust.value(), b.trust.value());
            assert_eq!(a.intimacy.value(), b.intimacy.value());
            assert_eq!(a.bond_kind, b.bond_kind);
        }
    }

    #[test]
    fn trust_high_boosts_trust() {
        let claims = vec![
            claim("relationship", "friend", "Alice"),
            claim("relationship", "trust_high", "Alice"),
        ];
        let (graph, _) = compute_bonds(&claims, None, 1, 1000);

        assert_eq!(graph.bonds.len(), 1);
        let bond = &graph.bonds[0];
        // Base Friend trust = 0.65 + evidence boost + trust_high boost 0.2
        assert!(bond.trust.value() >= 0.80);
    }

    #[test]
    fn intimate_boosts_intimacy() {
        let claims = vec![
            claim("relationship", "friend", "Alice"),
            claim("relationship", "intimate", "Alice"),
        ];
        let (graph, _) = compute_bonds(&claims, None, 1, 1000);

        assert_eq!(graph.bonds.len(), 1);
        let bond = &graph.bonds[0];
        // Base Friend intimacy = 0.55 + intimate boost 0.3 = 0.85+
        assert!(bond.intimacy.value() >= 0.80);
    }

    #[test]
    fn multiple_bond_kinds_supported() {
        let claims = vec![
            claim("relationship", "family", "Mom"),
            claim("relationship", "romantic", "Partner"),
            claim("relationship", "rival", "Enemy"),
            claim("relationship", "acquaintance", "Neighbor"),
        ];
        let (graph, _) = compute_bonds(&claims, None, 1, 1000);

        assert_eq!(graph.bonds.len(), 4);

        let kinds: Vec<BondKind> = graph.bonds.iter().map(|b| b.bond_kind).collect();
        assert!(kinds.contains(&BondKind::Family));
        assert!(kinds.contains(&BondKind::Romantic));
        assert!(kinds.contains(&BondKind::Rival));
        assert!(kinds.contains(&BondKind::Acquaintance));
    }

    #[test]
    fn superseded_claims_are_ignored_in_bond_creation() {
        let claims = vec![
            superseded_claim("relationship", "friend", "Alice"),
        ];
        let (graph, _) = compute_bonds(&claims, None, 1, 1000);

        // Superseded claims should not create bonds (they are not Current)
        assert!(graph.bonds.is_empty());
    }
}
