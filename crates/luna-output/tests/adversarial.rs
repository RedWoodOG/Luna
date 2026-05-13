//! Adversarial scenarios proving the output boundary catches violations
//! with 100% reliability.
//!
//! M6: Render Boundary With Provable Integrity — adversarial test suite.
//!
//! Every test in this file is deterministic: given a known set of inputs
//! and a known configuration, the boundary MUST produce a specific
//! classification.  No probabilistic or model-dependent logic is tested
//! here.

use luna_core::{
    AssertionConfidenceTier, AssertionLifecycleStatus, MemoryNode, MemoryNodeKind, MemoryProvenance,
};
use luna_output::{Classification, OutputBuilder, OutputConfig, OutputSource, SuppressionReason};

// ── helpers ────────────────────────────────────────────────────────────

fn node(
    id: &str,
    label: &str,
    kind: MemoryNodeKind,
    confidence: AssertionConfidenceTier,
) -> MemoryNode {
    MemoryNode {
        id: id.to_string(),
        label: label.to_string(),
        kind,
        confidence_tier: confidence,
        density: 1.0,
        activation: 0.0,
        provenance: vec![],
        created_at: None,
        contradiction_count: 0,
    }
}

fn node_with_lifecycle(
    id: &str,
    label: &str,
    kind: MemoryNodeKind,
    confidence: AssertionConfidenceTier,
    lifecycle: AssertionLifecycleStatus,
) -> MemoryNode {
    MemoryNode {
        id: id.to_string(),
        label: label.to_string(),
        kind,
        confidence_tier: confidence,
        density: 1.0,
        activation: 0.0,
        provenance: vec![
            MemoryProvenance::from_assertion("test".to_string()).with_lifecycle_status(lifecycle)
        ],
        created_at: None,
        contradiction_count: 0,
    }
}

// ══════════════════════════════════════════════════════════════════════
// Test: kernel_internals_never_leak
// ══════════════════════════════════════════════════════════════════════

#[test]
fn kernel_internals_never_leak() {
    // Create 10 SystemKernel nodes, but only allow 3 through the gate.
    let config = OutputConfig {
        max_kernel_items: 3,
        ..Default::default()
    };
    let mut builder = OutputBuilder::new(config);

    let kernels: Vec<MemoryNode> = (0..10)
        .map(|i| {
            node(
                &format!("kernel:sys:{i}"),
                &format!("system_kernel_item_{i}"),
                MemoryNodeKind::SystemKernel,
                AssertionConfidenceTier::Confirmed,
            )
        })
        .collect();

    let results: Vec<Classification> = kernels.iter().map(|k| builder.add_memory_node(k)).collect();

    // First 3 are Allowed
    for (i, cls) in results.iter().enumerate().take(3) {
        assert!(
            matches!(cls, Classification::Allowed),
            "kernel node {i} should be Allowed, got {cls:?}"
        );
    }

    // Remaining 7 are Suppressed(KernelInternal)
    for (i, cls) in results.iter().enumerate().skip(3) {
        assert_eq!(
            *cls,
            Classification::Suppressed(SuppressionReason::KernelInternal),
            "kernel node {i} should be Suppressed(KernelInternal), got {cls:?}"
        );
    }

    // No kernel node content appears after the 3rd in allowed items.
    let packet = builder.build();
    let allowed_count = packet
        .items
        .iter()
        .filter(|item| matches!(item.classification, Classification::Allowed))
        .count();
    assert_eq!(allowed_count, 3);

    // Suppressed count tracks all 7 kernel rejects
    assert_eq!(packet.budget.suppressed_count, 7);
    assert_eq!(packet.budget.items_used, 3);
}

// ══════════════════════════════════════════════════════════════════════
// Test: unconfirmed_data_blocked_by_default
// ══════════════════════════════════════════════════════════════════════

#[test]
fn unconfirmed_data_blocked_by_default() {
    let config = OutputConfig::default(); // allow_unconfirmed: false
    let mut builder = OutputBuilder::new(config);

    let unconfirmed = node(
        "u:1",
        "unconfirmed assertion",
        MemoryNodeKind::Assertion,
        AssertionConfidenceTier::Unconfirmed,
    );
    let inferred = node(
        "i:1",
        "inferred assertion",
        MemoryNodeKind::Assertion,
        AssertionConfidenceTier::Inferred,
    );
    let confirmed = node(
        "c:1",
        "confirmed assertion",
        MemoryNodeKind::Assertion,
        AssertionConfidenceTier::Confirmed,
    );

    // Unconfirmed node MUST be suppressed
    assert_eq!(
        builder.add_memory_node(&unconfirmed),
        Classification::Suppressed(SuppressionReason::UnconfirmedOnly)
    );

    // Inferred node MUST be allowed (it's not Unconfirmed)
    assert!(matches!(
        builder.add_memory_node(&inferred),
        Classification::Allowed
    ));

    // Confirmed node MUST be allowed
    assert!(matches!(
        builder.add_memory_node(&confirmed),
        Classification::Allowed
    ));

    // Verify the packet only contains 2 allowed items
    let packet = builder.build();
    let allowed_count = packet
        .items
        .iter()
        .filter(|item| matches!(item.classification, Classification::Allowed))
        .count();
    assert_eq!(allowed_count, 2);
    assert_eq!(packet.budget.suppressed_count, 1);
    assert_eq!(packet.budget.items_used, 2);
}

// ══════════════════════════════════════════════════════════════════════
// Test: superseded_data_blocked_by_default
// ══════════════════════════════════════════════════════════════════════

#[test]
fn superseded_data_blocked_by_default() {
    let config = OutputConfig::default(); // allow_superseded: false
    let mut builder = OutputBuilder::new(config);

    let current = node_with_lifecycle(
        "cur:1",
        "current fact",
        MemoryNodeKind::Assertion,
        AssertionConfidenceTier::Confirmed,
        AssertionLifecycleStatus::Current,
    );
    let superseded = node_with_lifecycle(
        "sup:1",
        "superseded fact",
        MemoryNodeKind::Assertion,
        AssertionConfidenceTier::Confirmed,
        AssertionLifecycleStatus::Superseded,
    );

    // Current node MUST be allowed
    assert!(matches!(
        builder.add_memory_node(&current),
        Classification::Allowed
    ));

    // Superseded node MUST be suppressed as StaleData
    assert_eq!(
        builder.add_memory_node(&superseded),
        Classification::Suppressed(SuppressionReason::StaleData)
    );

    let packet = builder.build();
    let allowed_count = packet
        .items
        .iter()
        .filter(|item| matches!(item.classification, Classification::Allowed))
        .count();
    assert_eq!(allowed_count, 1);
    assert_eq!(packet.budget.suppressed_count, 1);
    assert_eq!(packet.budget.items_used, 1);
}

// ══════════════════════════════════════════════════════════════════════
// Test: budget_exceeded_first_in_wins
// ══════════════════════════════════════════════════════════════════════

#[test]
fn budget_exceeded_first_in_wins() {
    // 50 nodes, each with 200-byte labels.
    // 50 nodes, each with 200-byte labels = 200-byte content each.
    // max_bytes = 2000 → 2000 / 200 = 10 nodes fit.
    // max_items = 20 (looser than byte budget).
    let config = OutputConfig {
        max_bytes: 2000,
        max_items: 20,
        ..Default::default()
    };
    let mut builder = OutputBuilder::new(config);

    // Build 50 nodes, each with exactly 200-byte label content.
    let nodes: Vec<MemoryNode> = (0..50)
        .map(|i| {
            let index_str = format!("{:03}", i);
            let padding_len = 200 - 1 - index_str.len(); // 'N' + index + padding
            let label = format!("N{}{}", index_str, "x".repeat(padding_len));
            assert_eq!(label.len(), 200, "node {i} label must be exactly 200 bytes");
            node(
                &format!("n:{i}"),
                &label,
                MemoryNodeKind::Assertion,
                AssertionConfidenceTier::Confirmed,
            )
        })
        .collect();

    let mut allowed = 0;
    let mut exceeded = 0;
    for node in &nodes {
        let cls = builder.add_memory_node(node);
        if matches!(cls, Classification::Allowed) {
            allowed += 1;
        } else {
            assert_eq!(
                cls,
                Classification::Suppressed(SuppressionReason::BudgetExceeded)
            );
            exceeded += 1;
        }
    }

    // 2000 bytes budget, each content is 200 bytes → 2000/200 = 10
    assert_eq!(allowed, 10, "first 10 fit within 2000-byte budget");
    assert_eq!(exceeded, 40, "remaining 40 exceed budget");
    assert_eq!(allowed + exceeded, 50);

    let packet = builder.build();
    assert_eq!(packet.budget.items_used, 10);
    assert_eq!(packet.budget.suppressed_count, 40);
}

// ══════════════════════════════════════════════════════════════════════
// Test: identity_bleed_caught
// ══════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "requires character scope isolation (M7)"]
fn identity_bleed_caught() {
    // When character scope isolation is implemented:
    // - Create nodes from two different character scopes
    // - Assert that cross-scope nodes are caught as IdentityBleed

    let config = OutputConfig::default();
    let mut builder = OutputBuilder::new(config);

    // Scope A: character "Alice"
    let _alice_node = node_with_lifecycle(
        "alice:private",
        "Alice's secret",
        MemoryNodeKind::Character,
        AssertionConfidenceTier::Confirmed,
        AssertionLifecycleStatus::Current,
    );

    // Scope B: character "Bob" — this node has provenance from a different scope
    let bob_node = node_with_lifecycle(
        "bob:private",
        "Bob's secret",
        MemoryNodeKind::Character,
        AssertionConfidenceTier::Confirmed,
        AssertionLifecycleStatus::Current,
    );

    // In a full implementation, adding alice_node first would set the active scope.
    // Adding bob_node would trigger IdentityBleed if scope isolation is enforced.
    builder.add_memory_node(&_alice_node);
    let bob_result = builder.add_memory_node(&bob_node);

    assert_eq!(
        bob_result,
        Classification::Suppressed(SuppressionReason::IdentityBleed {
            from_scope: "alice".to_string(),
            to_scope: "bob".to_string()
        })
    );
}

// ══════════════════════════════════════════════════════════════════════
// Test: safety_internals_blocked
// ══════════════════════════════════════════════════════════════════════

#[test]
fn safety_internals_blocked() {
    let config = OutputConfig::default();
    let mut builder = OutputBuilder::new(config);

    // Text tagged with SafetyInternal source must never cross the boundary.
    let result = builder.add_text(
        "safety:critical:override_policy:system_versioned",
        OutputSource::SafetyInternal,
    );

    assert_eq!(
        result,
        Classification::Suppressed(SuppressionReason::SafetyInternal)
    );

    // Verify no items were added (safety gate fires before budget tracking).
    let packet = builder.build();
    assert_eq!(packet.items.len(), 0);
    assert_eq!(packet.total_bytes, 0);
    assert_eq!(packet.budget.bytes_used, 0);
    assert_eq!(packet.budget.items_used, 0);
    assert_eq!(packet.budget.suppressed_count, 1);
}

// ══════════════════════════════════════════════════════════════════════
// Test: mixed_pipeline_correct_ordering
// ══════════════════════════════════════════════════════════════════════
//
// Ensures gates fire in the correct priority order when a node would
// trigger multiple gates simultaneously.

#[test]
fn kernel_gate_takes_priority_over_confidence_gate() {
    let config = OutputConfig {
        max_kernel_items: 1,
        ..Default::default() // allow_unconfirmed: false
    };
    let mut builder = OutputBuilder::new(config);

    // First kernel fits
    let k1 = node(
        "k1",
        "kernel one",
        MemoryNodeKind::SystemKernel,
        AssertionConfidenceTier::Confirmed,
    );
    assert!(matches!(
        builder.add_memory_node(&k1),
        Classification::Allowed
    ));

    // Second kernel is also Unconfirmed — kernel gate should fire first
    let k2 = node(
        "k2",
        "kernel two",
        MemoryNodeKind::SystemKernel,
        AssertionConfidenceTier::Unconfirmed,
    );
    assert_eq!(
        builder.add_memory_node(&k2),
        Classification::Suppressed(SuppressionReason::KernelInternal)
    );
}

#[test]
fn confidence_gate_takes_priority_over_staleness_gate() {
    let config = OutputConfig::default(); // both unconfirmed and superseded blocked
    let mut builder = OutputBuilder::new(config);

    // Node is both Unconfirmed and Superseded — confidence gate fires first
    let bad = node_with_lifecycle(
        "bad",
        "bad data",
        MemoryNodeKind::Assertion,
        AssertionConfidenceTier::Unconfirmed,
        AssertionLifecycleStatus::Superseded,
    );

    assert_eq!(
        builder.add_memory_node(&bad),
        Classification::Suppressed(SuppressionReason::UnconfirmedOnly)
    );
}

#[test]
fn staleness_gate_takes_priority_over_budget_gate() {
    let config = OutputConfig {
        max_bytes: 1,         // tiny budget
        ..Default::default()  // allow_superseded: false
    };
    let mut builder = OutputBuilder::new(config);

    // This node is Superseded and also exceeds budget.
    // Staleness gate fires first (position 3 in pipeline).
    let stale = node_with_lifecycle(
        "stale",
        "stale data that is too big",
        MemoryNodeKind::Assertion,
        AssertionConfidenceTier::Confirmed,
        AssertionLifecycleStatus::Superseded,
    );

    assert_eq!(
        builder.add_memory_node(&stale),
        Classification::Suppressed(SuppressionReason::StaleData)
    );
}
