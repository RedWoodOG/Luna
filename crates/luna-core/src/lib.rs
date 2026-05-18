use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, LunaError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LunaError {
    message: String,
}

impl LunaError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for LunaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for LunaError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalReliability {
    Heuristic,
    Statistical,
    Learned,
    UserConfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    value: f32,
    confidence: f32,
    reliability: SignalReliability,
    source_count: u8,
}

impl Signal {
    pub fn new(value: f32, confidence: f32, reliability: SignalReliability) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            confidence: confidence.clamp(0.0, 1.0),
            reliability,
            source_count: 1,
        }
    }

    pub fn fuse(signals: &[Signal]) -> Option<Self> {
        if signals.is_empty() {
            return None;
        }
        let total_weight = signals
            .iter()
            .map(|signal| signal.confidence.max(0.01))
            .sum::<f32>();
        let value = signals
            .iter()
            .map(|signal| signal.value * signal.confidence.max(0.01))
            .sum::<f32>()
            / total_weight;
        let confidence = (signals.iter().map(|signal| signal.confidence).sum::<f32>()
            / signals.len() as f32
            + (signals.len().saturating_sub(1) as f32 * 0.08))
            .clamp(0.0, 1.0);
        let reliability = strongest_reliability(signals);
        Some(Self {
            value: value.clamp(0.0, 1.0),
            confidence,
            reliability,
            source_count: signals.len().min(u8::MAX as usize) as u8,
        })
    }

    pub fn value(self) -> f32 {
        self.value
    }

    pub fn confidence(self) -> f32 {
        self.confidence
    }

    pub fn reliability(self) -> SignalReliability {
        self.reliability
    }

    pub fn with_source_count(self, _count: u8) -> Self { self }
    pub fn source_count(self) -> u8 {
        self.source_count
    }

    pub fn can_influence_recall(self) -> bool {
        self.source_count >= 2
    }
}

fn strongest_reliability(signals: &[Signal]) -> SignalReliability {
    signals
        .iter()
        .map(|signal| signal.reliability)
        .max_by_key(|reliability| match reliability {
            SignalReliability::Heuristic => 0,
            SignalReliability::Statistical => 1,
            SignalReliability::Learned => 2,
            SignalReliability::UserConfirmed => 3,
        })
        .unwrap_or(SignalReliability::Heuristic)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    pub event_id: String,
    pub episode_id: Option<Uuid>,
    pub turn_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub source: EventSource,
    pub confidence: f32,
    pub event_hash: Option<String>,
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    pub fn new(payload: T, source: EventSource, confidence: f32) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            episode_id: None,
            turn_id: None,
            timestamp: Utc::now(),
            source,
            confidence: confidence.clamp(0.0, 1.0),
            event_hash: None,
            payload,
        }
    }

    pub fn with_episode_id(mut self, episode_id: Uuid) -> Self {
        self.episode_id = Some(episode_id);
        self
    }

    pub fn with_turn_id(mut self, turn_id: Uuid) -> Self {
        self.turn_id = Some(turn_id);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    User,
    Assistant,
    HeuristicExtractor,
    EmbeddingExtractor,
    ClassifierExtractor,
    RecallEngine,
    BenchmarkOracle,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum LunaEvent {
    TurnObserved(TurnObserved),
    AssertionExtracted(AssertionExtracted),
    EpisodeCreated(EpisodeCreated),
    EpisodeReinforced(EpisodeReinforced),
    EpisodeRecalled(EpisodeRecalled),
    RecallSucceeded(RecallSucceeded),
    RecallFailed(RecallFailed),
    AssertionCorrected(AssertionCorrected),
    ContradictionDetected(ContradictionDetected),
    EpisodeDecayed(EpisodeDecayed),
    LatticeComputed(AttentionLattice),
    RuntimeTurnReceipted(RuntimeTurnReceipt),
    TopologyBridgeCommitted(TopologyBridgeCommitted),
    BondFormed(BondFormedEvent),
    BondSuperseded(BondSupersededEvent),
    BondDecayed(BondDecayedEvent),
    MemoryIntakeDecided(MemoryIntakeDecision),
    AssertionLifecycleChanged(AssertionLifecycleChanged),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnObserved {
    pub turn: ConversationTurn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertionExtracted {
    pub assertion: StructuredAssertion,
    pub observation: CognitiveObservation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeCreated {
    pub assertion: StructuredAssertion,
    pub observation: CognitiveObservation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeReinforced {
    pub assertion: StructuredAssertion,
    pub observation: CognitiveObservation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeRecalled {
    pub score: f32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallSucceeded {
    pub expected: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallFailed {
    pub expected: Vec<String>,
    pub actual: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertionCorrected {
    pub old_assertion: StructuredAssertion,
    pub new_assertion: StructuredAssertion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContradictionDetected {
    pub left: StructuredAssertion,
    pub right: StructuredAssertion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeDecayed {
    pub forgotten_risk: f32,
}

pub type StoredEvent = EventEnvelope<LunaEvent>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: Role,
    pub content: String,
    pub timestamp: Option<DateTime<Utc>>,
}

impl ConversationTurn {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            timestamp: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StructuredAssertion {
    pub domain: String, pub kind: String, pub value: String,
    pub source_count: u8, pub confidence_tier: AssertionConfidenceTier, pub lifecycle_status: AssertionLifecycleStatus,
}
impl StructuredAssertion {
    pub fn new(domain: impl Into<String>, kind: impl Into<String>, value: impl Into<String>) -> Self {
        Self { domain: domain.into(), kind: kind.into(), value: value.into(), source_count: 1, confidence_tier: AssertionConfidenceTier::Confirmed, lifecycle_status: AssertionLifecycleStatus::Current }
    }
    pub fn inferred(domain: impl Into<String>, kind: impl Into<String>, value: impl Into<String>) -> Self {
        Self { domain: domain.into(), kind: kind.into(), value: value.into(), source_count: 1, confidence_tier: AssertionConfidenceTier::Inferred, lifecycle_status: AssertionLifecycleStatus::Current }
    }
    pub fn with_source_count(self, _count: u8) -> Self { self }
    pub fn key(&self) -> String {
        format!(
            "{}:{}={}",
            self.domain,
            self.kind,
            self.value.replace(' ', "_")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CognitiveObservation {
    pub turn_id: Uuid,
    pub semantic: Option<Vec<f32>>,
    pub intent: Option<Vec<f32>>,
    pub attention: Option<Signal>,
    pub goal_pressure: Option<Signal>,
    pub emotional_valence: Option<Signal>,
    pub emotional_arousal: Option<Signal>,
    pub identity_relevance: Option<Signal>,
    pub trust_relevance: Option<Signal>,
    pub social_frame: Option<Signal>,
    pub temporal_relevance: Option<Signal>,
    pub uncertainty: Signal,
    pub cue_terms: Vec<String>,
    pub query_intents: Vec<String>,
    pub assertions: Vec<StructuredAssertion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EpisodeContour {
    pub semantic: Option<Vec<f32>>,
    pub intent: Option<Vec<f32>>,
    pub attention: Option<Signal>,
    pub goal_pressure: Option<Signal>,
    pub emotional_valence: Option<Signal>,
    pub emotional_arousal: Option<Signal>,
    pub identity_relevance: Option<Signal>,
    pub trust_relevance: Option<Signal>,
    pub social_frame: Option<Signal>,
    pub temporal_relevance: Option<Signal>,
    pub reinforcement_count: u32,
    pub contradiction_count: u32,
    pub successful_recall_count: u32,
    pub failed_recall_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallRecord {
    pub turn_id: Uuid,
    pub recalled_at: DateTime<Utc>,
    pub score: f32,
    pub succeeded: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub assertions: Vec<StructuredAssertion>,
    pub contour: EpisodeContour,
    pub recall_history: Vec<RecallRecord>,
    pub confidence: f32,
    pub coherence_score: f32,
    pub forgotten_risk: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecallMode {
    #[default]
    FullContext,
    Factual,
    IdentityContinuity,
    GoalContinuity,
    EmotionalContinuity,
    RelationshipContinuity,
    ContradictionCheck,
    OpenEnded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    NoMemory,
    FullContext,
    Keyword,
    Embedding,
    Tcf,
}

impl std::str::FromStr for EngineKind {
    type Err = LunaError;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "none" | "no-memory" | "no_memory" => Ok(Self::NoMemory),
            "full-context" | "full_context" => Ok(Self::FullContext),
            "keyword" => Ok(Self::Keyword),
            "embedding" => Ok(Self::Embedding),
            "tcf" | "luna-tcf" => Ok(Self::Tcf),
            other => Err(LunaError::new(format!("unknown engine: {other}"))),
        }
    }
}

impl fmt::Display for EngineKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::NoMemory => "no_memory",
            Self::FullContext => "full_context",
            Self::Keyword => "keyword",
            Self::Embedding => "embedding",
            Self::Tcf => "tcf",
        };
        write!(f, "{name}")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallHit {
    pub episode_id: Uuid,
    pub score: f32,
    pub assertions: Vec<StructuredAssertion>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RecallSet {
    pub hits: Vec<RecallHit>,
    pub latency_ms: f32,
}

impl RecallSet {
    pub fn rendered_claims(&self) -> Vec<String> {
        self.hits
            .iter()
            .flat_map(|hit| hit.assertions.iter())
            .map(|assertion| assertion.value.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_constructor_sets_reliability_and_clamps_values() {
        let signal = Signal::new(2.0, -1.0, SignalReliability::Heuristic);

        assert_eq!(signal.value(), 1.0);
        assert_eq!(signal.confidence(), 0.0);
        assert_eq!(signal.reliability(), SignalReliability::Heuristic);
        assert_eq!(signal.source_count(), 1);
        assert!(!signal.can_influence_recall());
    }

    #[test]
    fn fused_signal_can_influence_recall() {
        let signal = Signal::fuse(&[
            Signal::new(0.4, 0.7, SignalReliability::Heuristic),
            Signal::new(0.6, 0.8, SignalReliability::Statistical),
        ])
        .unwrap();

        assert!(signal.can_influence_recall());
        assert_eq!(signal.source_count(), 2);
        assert_eq!(signal.reliability(), SignalReliability::Statistical);
    }
}


// ── Memory enums ────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssertionConfidenceTier { Confirmed, Inferred, #[default] Unconfirmed }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssertionLifecycleStatus { #[default] Current, Superseded, Stale, Contradicted }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryNodeKind { Assertion, Entity, System, SystemKernel, User, Person, Character, Scene, Project, Place, Goal, Relationship, Attribute, Episode }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRelationKind { #[default] DefinesRule, HasAttribute, RelatedTo, Contradicts, Supersedes, Supports, LocatedIn, HasGoal, HasInterest, AliasOf, ProvenanceFor, AppearsBefore, OccursBefore, OccursIn, Mentions }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BondKind { Colleague, Friend, Family, Romantic, Acquaintance, Rival, Mentor, Stranger }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BondEvent { Disclosure, Correction, Decay, Reinforcement }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum MemoryIntakeAction {
    #[default]
    Accept, Reject, Supersede, SupersedeOrCorrect, AskForAnchor, IgnoreNoise, MarkUnknown, StoreWithUncertainty }

// ── Memory structs ──────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryProvenance { pub claim_id: String, pub source_event_id: String, pub source_event_hash: String, pub contribution_weight: f32, pub reason: String, pub lifecycle_status: Option<AssertionLifecycleStatus>, pub assertion_key: Option<String>, pub episode_id: Option<Uuid>, pub turn_id: Option<Uuid>, pub system_root: Option<String> }
impl MemoryProvenance { pub fn from_assertion(claim_id: impl Into<String>) -> Self { Self { claim_id: claim_id.into(), source_event_id: String::new(), source_event_hash: String::new(), contribution_weight: 1.0, reason: String::new(), lifecycle_status: None, assertion_key: None, episode_id: None, turn_id: None, system_root: None } } pub fn from_system_root(root_id: impl Into<String>) -> Self { let id: String = root_id.into(); Self { claim_id: format!("sys-{}", id), source_event_id: String::new(), source_event_hash: String::new(), contribution_weight: 1.0, reason: "system root".into(), lifecycle_status: None, assertion_key: None, episode_id: None, turn_id: None, system_root: Some(id) } } pub fn with_episode_id(self, episode_id: Uuid) -> Self { Self { episode_id: Some(episode_id), ..self } } pub fn with_turn_id(self, turn_id: Uuid) -> Self { Self { turn_id: Some(turn_id), ..self } } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryNode { pub id: String, pub label: String, pub kind: MemoryNodeKind, pub confidence_tier: AssertionConfidenceTier, pub density: f32, pub activation: f32, pub provenance: Vec<MemoryProvenance>, pub created_at: Option<DateTime<Utc>>, pub contradiction_count: u32 }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MemoryEdge { pub source: String, pub target: String, pub relation: MemoryRelationKind, pub strength: f32, pub confidence_tier: AssertionConfidenceTier, pub activation: f32, pub provenance: Vec<MemoryProvenance> }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MemoryMap { pub nodes: Vec<MemoryNode>, pub edges: Vec<MemoryEdge> }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorkingMemory { pub nodes: Vec<MemoryNode>, pub edges: Vec<MemoryEdge>, pub filtered_node_count: u32, pub filtered_edge_count: u32, pub activation_reason: String }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorkingMemoryBudget { pub max_nodes: usize, pub max_edges: usize, pub max_activation_depth: u32, pub max_questions: u32 }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TurnReading { pub assertions: Vec<StructuredAssertion>, pub contour: EpisodeContour, pub turn_id: Option<Uuid>, pub cue_terms: Vec<String>, pub query_intents: Vec<String>, pub uncertainty: f32, pub goal_pressure: Option<Signal>, pub identity_relevance: Option<Signal>, pub emotional_arousal: Option<Signal> }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SystemKernel { pub id: String, pub label: String, pub axioms: Vec<String>, pub principles: Vec<SystemKernelPrinciple> }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RuntimeTurnReceipt { pub turn_id: Uuid, pub created_claim_keys: Vec<String>, pub corrected_claim_keys: Vec<String>, pub reinforced_claim_keys: Vec<String>, pub assertion_count: u32, pub working_node_count: u32, pub working_edge_count: u32, pub filtered_node_count: u32, pub filtered_edge_count: u32, pub output_item_count: u32, pub output_total_bytes: usize, pub activation_reason: String, pub response_actions: Vec<String>, pub recall_mode: RecallMode, pub recalled_episode_ids: Vec<Uuid>, pub source_event_ids: Vec<String>, pub source_event_hashes: Vec<String>, pub intake_action: MemoryIntakeAction, pub intake_reason: String, pub contradiction_count: u32, pub topology_node_refs: Vec<String>, pub topology_tether_refs: Vec<String>, pub topology_ledger_event_hash: String }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatticeDimension { pub score: f32, pub confidence: f32, pub sources: usize, pub reason: String, pub provenance: Vec<MemoryProvenance> }
impl LatticeDimension { pub fn default() -> Self { Self { score: 0.0, confidence: 0.0, sources: 0, reason: String::new(), provenance: vec![] } } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttentionLattice { pub identity: LatticeDimension, pub meaning: LatticeDimension, pub goal: LatticeDimension, pub trust: LatticeDimension, pub attention: LatticeDimension, pub context: LatticeDimension, pub skill: LatticeDimension }
impl AttentionLattice { pub fn default() -> Self { let d = LatticeDimension::default(); Self{identity:d.clone(),meaning:d.clone(),goal:d.clone(),trust:d.clone(),attention:d.clone(),context:d.clone(),skill:d} } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BondEventRecord { pub event_type: BondEvent, pub source_event_id: String, pub source_event_hash: String, pub timestamp: i64, pub turn_number: u32, pub detail: String }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityBond { pub bond_id: String, pub source_entity: String, pub target_entity: String, pub bond_kind: BondKind, pub event_history: Vec<BondEventRecord>, pub superseded_by: Option<String>, pub trust: Signal, pub intimacy: Signal, pub provenance: Vec<MemoryProvenance> }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BondGraph { pub bonds: Vec<EntityBond>, pub computed_at_turn: u32 }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct BondFormedEvent { pub bond: EntityBond, pub turn_number: u32 }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct BondSupersededEvent { pub old_bond_id: String, pub new_bond: EntityBond, pub reason: String, pub turn_number: u32 }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct BondDecayedEvent { pub bond_id: String, pub turn_number: u32, pub previous_trust: Signal, pub new_trust: Signal, pub previous_intimacy: Signal, pub new_intimacy: Signal }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TopologyBridgeCommitted { pub commit_hash: String, pub ledger_event_count: usize, pub ledger_event_hash: String, pub ledger_events_json: Vec<String>, pub node_refs: Vec<String>, pub tether_refs: Vec<String>, pub orb_refs: Vec<String>, pub accepted_orb_refs: Vec<String>, pub rejected_orb_refs: Vec<String>, pub source_event_hashes: Vec<String> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryIntakeDecision { pub action: MemoryIntakeAction, pub reason: String }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig { pub max_items: usize, pub max_bytes: usize }
impl OutputConfig { pub fn default() -> Self { Self { max_items: 12, max_bytes: 4096 } } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputBuilder { cfg: OutputConfig }
impl OutputBuilder { pub fn new(cfg: OutputConfig) -> Self { Self { cfg } } pub fn add_memory_node(&mut self, _node: &MemoryNode) {} pub fn build(self) -> OutputPacket { OutputPacket::default() } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OutputPacket { pub items: Vec<String>, pub total_bytes: usize, pub budget: BudgetUsage }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BudgetUsage { pub bytes_used: usize, pub bytes_max: usize, pub items_used: usize, pub items_max: usize, pub suppressed_count: usize }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SystemKernelPrinciple { pub id: String, pub name: String, pub label: String, pub statement: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertionLifecycleChanged { pub assertion_key: String, pub old_status: AssertionLifecycleStatus, pub new_status: AssertionLifecycleStatus, pub reason: String }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivationConfig { pub recency_weight: f32, pub importance_weight: f32, pub contradiction_bonus: f32, pub decay_rate: f32 }
