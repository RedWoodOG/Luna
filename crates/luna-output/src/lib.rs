use luna_core::{
    AssertionConfidenceTier, AssertionLifecycleStatus, MemoryNode, MemoryNodeKind,
    MemoryProvenance,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── core types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputPacket {
    pub items: Vec<OutputItem>,
    pub total_bytes: usize,
    pub budget: BudgetUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputItem {
    pub content: String,
    pub source: OutputSource,
    pub classification: Classification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Classification {
    Allowed,
    Suppressed(SuppressionReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuppressionReason {
    BudgetExceeded,
    KernelInternal,
    IdentityBleed { from_scope: String, to_scope: String },
    StaleData,
    UnconfirmedOnly,
    SafetyInternal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputSource {
    MemoryNode(String),
    RecallHit(Uuid),
    WorkingFact,
    ContextClue,
    SafetyInternal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetUsage {
    pub bytes_used: usize,
    pub bytes_max: usize,
    pub items_used: usize,
    pub items_max: usize,
    pub suppressed_count: usize,
}

// ── OutputConfig ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub max_bytes: usize,
    pub max_items: usize,
    pub max_kernel_items: usize,
    pub allow_unconfirmed: bool,
    pub allow_superseded: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            max_bytes: 4096,
            max_items: 12,
            max_kernel_items: 3,
            allow_unconfirmed: false,
            allow_superseded: false,
        }
    }
}

// ── OutputBuilder ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OutputBuilder {
    config: OutputConfig,
    items: Vec<OutputItem>,
    byte_count: usize,
    kernel_count: usize,
    suppressed: Vec<(OutputSource, SuppressionReason)>,
}

impl OutputBuilder {
    pub fn new(config: OutputConfig) -> Self {
        Self {
            config,
            items: Vec::new(),
            byte_count: 0,
            kernel_count: 0,
            suppressed: Vec::new(),
        }
    }

    /// Add a memory node. Returns the classification decision.
    /// Integrity rules applied in order: kernel gate, confidence gate,
    /// staleness gate, budget gate.
    pub fn add_memory_node(&mut self, node: &MemoryNode) -> Classification {
        let content = node.label.clone();
        let source = OutputSource::MemoryNode(node.id.clone());

        // 1. Kernel gate
        if node.kind == MemoryNodeKind::SystemKernel
            && self.kernel_count >= self.config.max_kernel_items
        {
            let reason = SuppressionReason::KernelInternal;
            self.suppressed.push((source, reason.clone()));
            return Classification::Suppressed(reason);
        }

        // 2. Confidence gate
        if node.confidence_tier == AssertionConfidenceTier::Unconfirmed
            && !self.config.allow_unconfirmed
        {
            let reason = SuppressionReason::UnconfirmedOnly;
            self.suppressed.push((source, reason.clone()));
            return Classification::Suppressed(reason);
        }

        // 3. Staleness gate
        if any_provenance_blocked(&node.provenance) && !self.config.allow_superseded {
            let reason = SuppressionReason::StaleData;
            self.suppressed.push((source, reason.clone()));
            return Classification::Suppressed(reason);
        }

        // 4. Budget gate
        if self.byte_count + content.len() > self.config.max_bytes
            || self.items.len() >= self.config.max_items
        {
            let reason = SuppressionReason::BudgetExceeded;
            self.suppressed.push((source, reason.clone()));
            return Classification::Suppressed(reason);
        }

        // Allowed
        self.byte_count += content.len();
        if node.kind == MemoryNodeKind::SystemKernel {
            self.kernel_count += 1;
        }
        self.items.push(OutputItem {
            content,
            source,
            classification: Classification::Allowed,
        });
        Classification::Allowed
    }

    /// Add arbitrary text from a known source.
    pub fn add_text(&mut self, content: &str, source: OutputSource) -> Classification {
        // Safety gate — source-tagged internal data must never cross.
        if matches!(source, OutputSource::SafetyInternal) {
            let reason = SuppressionReason::SafetyInternal;
            self.suppressed.push((source, reason.clone()));
            return Classification::Suppressed(reason);
        }

        if self.byte_count + content.len() > self.config.max_bytes
            || self.items.len() >= self.config.max_items
        {
            let reason = SuppressionReason::BudgetExceeded;
            self.suppressed.push((source, reason.clone()));
            return Classification::Suppressed(reason);
        }

        self.byte_count += content.len();
        self.items.push(OutputItem {
            content: content.to_string(),
            source,
            classification: Classification::Allowed,
        });
        Classification::Allowed
    }

    /// Build the final packet.
    pub fn build(self) -> OutputPacket {
        OutputPacket {
            total_bytes: self.byte_count,
            budget: BudgetUsage {
                bytes_used: self.byte_count,
                bytes_max: self.config.max_bytes,
                items_used: self.items.len(),
                items_max: self.config.max_items,
                suppressed_count: self.suppressed.len(),
            },
            items: self.items,
        }
    }

    /// List everything that was suppressed (for diagnostics).
    pub fn suppressed(&self) -> &[(OutputSource, SuppressionReason)] {
        &self.suppressed
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn any_provenance_blocked(provenance: &[MemoryProvenance]) -> bool {
    provenance.iter().any(|p| {
        p.lifecycle_status == Some(AssertionLifecycleStatus::Superseded)
    })
}


// ── unit tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(
        id: &str,
        label: &str,
        kind: MemoryNodeKind,
        tier: AssertionConfidenceTier,
    ) -> MemoryNode {
        MemoryNode {
            id: id.to_string(),
            label: label.to_string(),
            kind,
            confidence_tier: tier,
            density: 0.5,
            activation: 0.5,
            provenance: Vec::new(),
            created_at: None,
            contradiction_count: 0,
        }
    }

    fn make_node_with_provenance(
        id: &str,
        label: &str,
        kind: MemoryNodeKind,
        tier: AssertionConfidenceTier,
        lifecycle: AssertionLifecycleStatus,
    ) -> MemoryNode {
        MemoryNode {
            id: id.to_string(),
            label: label.to_string(),
            kind,
            confidence_tier: tier,
            density: 0.5,
            activation: 0.5,
            provenance: vec![MemoryProvenance {
                episode_id: None,
                turn_id: None,
                assertion_key: None,
                system_root: None,
                lifecycle_status: Some(lifecycle),
            }],
            created_at: None,
            contradiction_count: 0,
        }
    }

    #[test]
    fn kernel_items_limited_to_max_kernel_items() {
        let config = OutputConfig {
            max_kernel_items: 2,
            ..OutputConfig::default()
        };
        let mut builder = OutputBuilder::new(config);

        let k1 = make_node("k1", "kernel-1", MemoryNodeKind::SystemKernel, AssertionConfidenceTier::Confirmed);
        let k2 = make_node("k2", "kernel-2", MemoryNodeKind::SystemKernel, AssertionConfidenceTier::Confirmed);
        let k3 = make_node("k3", "kernel-3", MemoryNodeKind::SystemKernel, AssertionConfidenceTier::Confirmed);

        assert_eq!(builder.add_memory_node(&k1), Classification::Allowed);
        assert_eq!(builder.add_memory_node(&k2), Classification::Allowed);
        assert_eq!(
            builder.add_memory_node(&k3),
            Classification::Suppressed(SuppressionReason::KernelInternal)
        );

        let packet = builder.build();
        assert_eq!(packet.items.len(), 2);
        assert_eq!(packet.budget.suppressed_count, 1);
    }

    #[test]
    fn unconfirmed_nodes_suppressed_when_not_allowed() {
        let config = OutputConfig::default(); // allow_unconfirmed: false
        let mut builder = OutputBuilder::new(config);

        let n = make_node("n1", "unconfirmed-fact", MemoryNodeKind::User, AssertionConfidenceTier::Unconfirmed);

        assert_eq!(
            builder.add_memory_node(&n),
            Classification::Suppressed(SuppressionReason::UnconfirmedOnly)
        );

        let packet = builder.build();
        assert!(packet.items.is_empty());
        assert_eq!(packet.budget.suppressed_count, 1);
    }

    #[test]
    fn unconfirmed_nodes_allowed_when_configured() {
        let config = OutputConfig {
            allow_unconfirmed: true,
            ..OutputConfig::default()
        };
        let mut builder = OutputBuilder::new(config);

        let n = make_node("n1", "unconfirmed-fact", MemoryNodeKind::User, AssertionConfidenceTier::Unconfirmed);

        assert_eq!(builder.add_memory_node(&n), Classification::Allowed);

        let packet = builder.build();
        assert_eq!(packet.items.len(), 1);
        assert_eq!(packet.budget.suppressed_count, 0);
    }

    #[test]
    fn superseded_nodes_suppressed_when_not_allowed() {
        let config = OutputConfig::default(); // allow_superseded: false
        let mut builder = OutputBuilder::new(config);

        let n = make_node_with_provenance(
            "n1",
            "stale-fact",
            MemoryNodeKind::Attribute,
            AssertionConfidenceTier::Confirmed,
            AssertionLifecycleStatus::Superseded,
        );

        assert_eq!(
            builder.add_memory_node(&n),
            Classification::Suppressed(SuppressionReason::StaleData)
        );

        let packet = builder.build();
        assert!(packet.items.is_empty());
        assert_eq!(packet.budget.suppressed_count, 1);
    }

    #[test]
    fn superseded_nodes_allowed_when_configured() {
        let config = OutputConfig {
            allow_superseded: true,
            ..OutputConfig::default()
        };
        let mut builder = OutputBuilder::new(config);

        let n = make_node_with_provenance(
            "n1",
            "stale-fact",
            MemoryNodeKind::Attribute,
            AssertionConfidenceTier::Confirmed,
            AssertionLifecycleStatus::Superseded,
        );

        assert_eq!(builder.add_memory_node(&n), Classification::Allowed);

        let packet = builder.build();
        assert_eq!(packet.items.len(), 1);
        assert_eq!(packet.budget.suppressed_count, 0);
    }

    #[test]
    fn budget_exceeded_suppresses_remaining_items() {
        let config = OutputConfig {
            max_bytes: 200,
            max_items: 3,
            ..OutputConfig::default()
        };
        let mut builder = OutputBuilder::new(config);

        let n1 = make_node("n1", "item-one", MemoryNodeKind::User, AssertionConfidenceTier::Confirmed);
        let n2 = make_node("n2", "item-two", MemoryNodeKind::Person, AssertionConfidenceTier::Confirmed);
        let n3 = make_node("n3", "item-three", MemoryNodeKind::Place, AssertionConfidenceTier::Confirmed);
        let n4 = make_node("n4", "item-four", MemoryNodeKind::Goal, AssertionConfidenceTier::Confirmed);

        assert_eq!(builder.add_memory_node(&n1), Classification::Allowed);
        assert_eq!(builder.add_memory_node(&n2), Classification::Allowed);
        assert_eq!(builder.add_memory_node(&n3), Classification::Allowed);
        assert_eq!(
            builder.add_memory_node(&n4),
            Classification::Suppressed(SuppressionReason::BudgetExceeded)
        );

        let packet = builder.build();
        assert_eq!(packet.items.len(), 3);
        assert_eq!(packet.budget.items_used, 3);
        assert_eq!(packet.budget.items_max, 3);
        assert_eq!(packet.budget.suppressed_count, 1);
    }

    #[test]
    fn allowed_items_accumulate_correctly() {
        let config = OutputConfig::default();
        let mut builder = OutputBuilder::new(config);

        let n1 = make_node("n1", "alpha", MemoryNodeKind::User, AssertionConfidenceTier::Confirmed);
        let n2 = make_node("n2", "beta", MemoryNodeKind::Person, AssertionConfidenceTier::Confirmed);

        builder.add_memory_node(&n1);
        builder.add_memory_node(&n2);

        let packet = builder.build();
        assert_eq!(packet.items.len(), 2);
        assert!(packet.items[0].content.contains("alpha"));
        assert!(packet.items[1].content.contains("beta"));
        assert!(packet.total_bytes > 0);
    }

    #[test]
    fn build_returns_correct_budget_usage() {
        let config = OutputConfig {
            max_bytes: 4096,
            max_items: 12,
            max_kernel_items: 3,
            ..OutputConfig::default()
        };
        let mut builder = OutputBuilder::new(config);

        let n = make_node("n1", "test", MemoryNodeKind::User, AssertionConfidenceTier::Confirmed);
        builder.add_memory_node(&n);

        let packet = builder.build();
        assert_eq!(packet.budget.bytes_max, 4096);
        assert_eq!(packet.budget.items_max, 12);
        assert_eq!(packet.budget.items_used, 1);
        assert!(packet.budget.bytes_used > 0);
        assert_eq!(packet.budget.suppressed_count, 0);
    }

    #[test]
    fn suppressed_list_is_accurate() {
        let config = OutputConfig {
            max_kernel_items: 1,
            ..OutputConfig::default()
        };
        let mut builder = OutputBuilder::new(config);

        let k1 = make_node("k1", "kernel-1", MemoryNodeKind::SystemKernel, AssertionConfidenceTier::Confirmed);
        let k2 = make_node("k2", "kernel-2", MemoryNodeKind::SystemKernel, AssertionConfidenceTier::Confirmed);

        builder.add_memory_node(&k1);
        builder.add_memory_node(&k2);

        let suppressed = builder.suppressed();
        assert_eq!(suppressed.len(), 1);
        assert_eq!(suppressed[0].1, SuppressionReason::KernelInternal);
        match &suppressed[0].0 {
            OutputSource::MemoryNode(id) => assert_eq!(id, "k2"),
            _ => panic!("expected MemoryNode source"),
        }
    }

    #[test]
    fn staleness_checked_before_budget() {
        let config = OutputConfig {
            max_items: 1,
            ..OutputConfig::default()
        };
        let mut builder = OutputBuilder::new(config);

        // Fill budget first
        let n1 = make_node("n1", "first", MemoryNodeKind::User, AssertionConfidenceTier::Confirmed);
        assert_eq!(builder.add_memory_node(&n1), Classification::Allowed);

        // Next item: staleness checked first, so gets StaleData not BudgetExceeded
        let n2 = make_node_with_provenance(
            "n2",
            "second",
            MemoryNodeKind::Attribute,
            AssertionConfidenceTier::Confirmed,
            AssertionLifecycleStatus::Superseded,
        );

        assert_eq!(
            builder.add_memory_node(&n2),
            Classification::Suppressed(SuppressionReason::StaleData)
        );

        let packet = builder.build();
        assert_eq!(packet.budget.items_used, 1);
    }

    #[test]
    fn unconfirmed_checked_before_budget() {
        let config = OutputConfig {
            max_items: 1,
            ..OutputConfig::default()
        };
        let mut builder = OutputBuilder::new(config);

        let n1 = make_node("n1", "first", MemoryNodeKind::User, AssertionConfidenceTier::Confirmed);
        assert_eq!(builder.add_memory_node(&n1), Classification::Allowed);

        let n2 = make_node("n2", "unconfirmed", MemoryNodeKind::User, AssertionConfidenceTier::Unconfirmed);

        assert_eq!(
            builder.add_memory_node(&n2),
            Classification::Suppressed(SuppressionReason::UnconfirmedOnly)
        );

        let packet = builder.build();
        assert_eq!(packet.budget.items_used, 1);
    }

    #[test]
    fn add_text_budget_gate() {
        let config = OutputConfig {
            max_items: 2,
            ..OutputConfig::default()
        };
        let mut builder = OutputBuilder::new(config);

        assert_eq!(builder.add_text("item-1", OutputSource::WorkingFact), Classification::Allowed);
        assert_eq!(builder.add_text("item-2", OutputSource::ContextClue), Classification::Allowed);
        assert_eq!(
            builder.add_text("item-3", OutputSource::WorkingFact),
            Classification::Suppressed(SuppressionReason::BudgetExceeded)
        );

        let packet = builder.build();
        assert_eq!(packet.items.len(), 2);
    }
}
