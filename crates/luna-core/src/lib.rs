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
    pub event_id: Uuid,
    pub episode_id: Option<Uuid>,
    pub turn_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub source: EventSource,
    pub confidence: f32,
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    pub fn new(payload: T, source: EventSource, confidence: f32) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            episode_id: None,
            turn_id: None,
            timestamp: Utc::now(),
            source,
            confidence: confidence.clamp(0.0, 1.0),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: Role,
    pub content: String,
}

impl ConversationTurn {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredAssertion {
    pub domain: String,
    pub kind: String,
    pub value: String,
}

impl StructuredAssertion {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallMode {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig { pub max_items: usize, pub max_bytes: usize }
impl OutputConfig { pub fn default() -> Self { Self { max_items: 12, max_bytes: 4096 } } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputBuilder { cfg: OutputConfig }
impl OutputBuilder { pub fn new(cfg: OutputConfig) -> Self { Self { cfg } } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputPacket { pub items: Vec<String>, pub total_bytes: usize, pub budget: BudgetUsage }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetUsage { pub bytes_used: usize, pub bytes_max: usize, pub items_used: usize, pub items_max: usize, pub suppressed_count: usize }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivationConfig;
impl ActivationConfig { pub fn default() -> Self { Self } }
pub fn compute_activation(_n: &StructuredAssertion, _q: &str, _c: &ActivationConfig) -> f32 { 0.5 }
pub fn propagate_activation_with_context(_n: &mut [StructuredAssertion], _e: &[MemoryEdge], _c: &ActivationConfig) {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEdge { pub source: String, pub target: String, pub relation: String, pub strength: f32 }
