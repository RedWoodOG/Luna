use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    pub fn with_source_count(mut self, count: u8) -> Self {
        self.source_count = count.max(1);
        self
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_hash: Option<String>,
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
    MemoryIntakeDecided(MemoryIntakeDecision),
    AssertionExtracted(AssertionExtracted),
    DenseUpdateReceipted(SurpriseUpdateReceipt),
    EpisodeCreated(EpisodeCreated),
    EpisodeReinforced(EpisodeReinforced),
    EpisodeRecalled(EpisodeRecalled),
    RecallSucceeded(RecallSucceeded),
    RecallFailed(RecallFailed),
    AssertionCorrected(AssertionCorrected),
    ContradictionDetected(ContradictionDetected),
    EpisodeDecayed(EpisodeDecayed),
    TopologyBridgeCommitted(TopologyBridgeCommitted),
}

pub const SURPRISE_UPDATE_RECEIPT_SCHEMA_VERSION: &str = "luna.surprise_update_receipt.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateKind {
    ReinforceExisting,
    NovelUpdate,
    CorrectionPressure,
    IgnoredLowSurprise,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurpriseUpdateReceipt {
    pub schema_version: String,
    pub input_event_id: String,
    pub input_event_hash: String,
    pub prediction_hash: String,
    pub surprise_score: f32,
    pub redundancy_score: f32,
    pub correction_pressure: f32,
    pub update_kind: UpdateKind,
    pub state_hash_before: String,
    pub state_hash_after: String,
    pub lineage_hashes: Vec<String>,
    pub recorded_at: DateTime<Utc>,
    pub receipt_hash: String,
}

impl SurpriseUpdateReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        input_event_id: impl Into<String>,
        input_event_hash: impl Into<String>,
        prediction_hash: impl Into<String>,
        surprise_score: f32,
        redundancy_score: f32,
        correction_pressure: f32,
        update_kind: UpdateKind,
        state_hash_before: impl Into<String>,
        state_hash_after: impl Into<String>,
        lineage_hashes: Vec<String>,
        recorded_at: DateTime<Utc>,
    ) -> Result<Self> {
        let mut receipt = Self {
            schema_version: SURPRISE_UPDATE_RECEIPT_SCHEMA_VERSION.to_string(),
            input_event_id: input_event_id.into(),
            input_event_hash: input_event_hash.into(),
            prediction_hash: prediction_hash.into(),
            surprise_score: surprise_score.clamp(0.0, 1.0),
            redundancy_score: redundancy_score.clamp(0.0, 1.0),
            correction_pressure: correction_pressure.clamp(0.0, 1.0),
            update_kind,
            state_hash_before: state_hash_before.into(),
            state_hash_after: state_hash_after.into(),
            lineage_hashes,
            recorded_at,
            receipt_hash: String::new(),
        };
        receipt.validate_without_hash()?;
        receipt.receipt_hash = receipt.compute_receipt_hash();
        Ok(receipt)
    }

    pub fn compute_receipt_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hash_receipt_field(
            &mut hasher,
            "schema_version",
            SURPRISE_UPDATE_RECEIPT_SCHEMA_VERSION,
        );
        hash_receipt_field(&mut hasher, "input_event_id", &self.input_event_id);
        hash_receipt_field(&mut hasher, "input_event_hash", &self.input_event_hash);
        hash_receipt_field(&mut hasher, "prediction_hash", &self.prediction_hash);
        hash_receipt_field(
            &mut hasher,
            "surprise_score",
            &canonical_score(self.surprise_score),
        );
        hash_receipt_field(
            &mut hasher,
            "redundancy_score",
            &canonical_score(self.redundancy_score),
        );
        hash_receipt_field(
            &mut hasher,
            "correction_pressure",
            &canonical_score(self.correction_pressure),
        );
        hash_receipt_field(
            &mut hasher,
            "update_kind",
            &format!("{:?}", self.update_kind),
        );
        hash_receipt_field(&mut hasher, "state_hash_before", &self.state_hash_before);
        hash_receipt_field(&mut hasher, "state_hash_after", &self.state_hash_after);
        let mut lineage_hashes = self.lineage_hashes.clone();
        lineage_hashes.sort();
        for lineage_hash in lineage_hashes {
            hash_receipt_field(&mut hasher, "lineage_hash", &lineage_hash);
        }
        hash_receipt_field(&mut hasher, "recorded_at", &self.recorded_at.to_rfc3339());
        format!("{:x}", hasher.finalize())
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_without_hash()?;
        let recomputed = self.compute_receipt_hash();
        if self.receipt_hash != recomputed {
            return Err(LunaError::new("surprise update receipt hash mismatch"));
        }
        Ok(())
    }

    fn validate_without_hash(&self) -> Result<()> {
        if self.input_event_id.trim().is_empty() {
            return Err(LunaError::new(
                "surprise update receipt requires an input event id",
            ));
        }
        for (label, hash) in [
            ("input event hash", self.input_event_hash.as_str()),
            ("prediction hash", self.prediction_hash.as_str()),
            ("state hash before", self.state_hash_before.as_str()),
            ("state hash after", self.state_hash_after.as_str()),
        ] {
            if !valid_sha256_hash(hash) {
                return Err(LunaError::new(format!(
                    "surprise update receipt has invalid {label}"
                )));
            }
        }
        for lineage_hash in &self.lineage_hashes {
            if !valid_sha256_hash(lineage_hash) {
                return Err(LunaError::new(
                    "surprise update receipt has invalid lineage hash",
                ));
            }
        }
        Ok(())
    }
}

fn canonical_score(score: f32) -> String {
    format!("{:.6}", score.clamp(0.0, 1.0))
}

fn hash_receipt_field(hasher: &mut Sha256, label: &str, value: &str) {
    hasher.update(label.as_bytes());
    hasher.update([0]);
    hasher.update(value.len().to_string().as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hasher.update([0xff]);
}

fn valid_sha256_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TopologyBridgeCommitted {
    pub node_refs: Vec<String>,
    pub tether_refs: Vec<String>,
    pub source_event_hashes: Vec<String>,
    pub orb_refs: Vec<String>,
    #[serde(default)]
    pub accepted_orb_refs: Vec<String>,
    #[serde(default)]
    pub rejected_orb_refs: Vec<String>,
    #[serde(default)]
    pub ledger_event_count: usize,
    #[serde(default)]
    pub ledger_event_hash: String,
    #[serde(default)]
    pub ledger_events_json: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryIntakeDecision {
    pub action: MemoryIntakeAction,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryIntakeAction {
    Accept,
    StoreWithUncertainty,
    AskForAnchor,
    IgnoreNoise,
    SupersedeOrCorrect,
    MarkUnknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnObserved {
    pub turn: ConversationTurn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertionExtracted {
    pub assertion: StructuredAssertion,
    pub observation: TurnReading,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeCreated {
    pub assertion: StructuredAssertion,
    pub observation: TurnReading,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeReinforced {
    pub assertion: StructuredAssertion,
    pub observation: TurnReading,
}

/// Why a memory was recalled, or why the working memory packet is what it is.
///
/// **Strict on construction.** `RecallReason::new` rejects empty and
/// whitespace-only strings. The doctrine rule "recall must carry why" stops
/// being a checklist item and becomes a type signature: production code
/// cannot construct unattributed surfaced memory.
///
/// **Lenient on deserialization.** Legacy event-log entries with empty
/// `reason` strings load as the `<unrecorded>` sentinel rather than failing,
/// so existing memory state remains accessible after this gate lands.
/// New events written after this point cannot have empty reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RecallReason(String);

impl RecallReason {
    pub fn new(reason: impl Into<String>) -> Result<Self> {
        let reason = reason.into();
        let trimmed = reason.trim();
        if trimmed.is_empty() {
            Err(LunaError::new(
                "RecallReason cannot be empty: surfaced memory must explain why",
            ))
        } else {
            Ok(Self(trimmed.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RecallReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<RecallReason> for String {
    fn from(reason: RecallReason) -> Self {
        reason.0
    }
}

impl<'de> Deserialize<'de> for RecallReason {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            Ok(Self("<unrecorded>".to_string()))
        } else {
            Ok(Self(trimmed.to_string()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeRecalled {
    pub score: f32,
    pub reason: RecallReason,
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
    /// Event-time: when the conversation turn occurred.
    ///
    /// Distinct from [`EventEnvelope::timestamp`], which is processing-time
    /// (when Luna observed or replayed the event). For v0.1 benchmarks with
    /// explicit per-turn timestamps these will coincide. For Stage 3
    /// longitudinal cases they will diverge: case JSON fixes event-time
    /// while replay-time is determined by the run. For live use the field
    /// may be `None` if the source did not provide one.
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

    pub fn user_at(content: impl Into<String>, timestamp: DateTime<Utc>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            timestamp: Some(timestamp),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredAssertion {
    pub domain: String,
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub lifecycle_status: AssertionLifecycleStatus,
    #[serde(default)]
    pub source_count: u8,
    #[serde(default)]
    pub reinforcement_count: u32,
    #[serde(default)]
    pub confidence_tier: AssertionConfidenceTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssertionLifecycleStatus {
    #[default]
    Current,
    Stale,
    Superseded,
    Contradicted,
}

impl StructuredAssertion {
    pub fn new(
        domain: impl Into<String>,
        kind: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            kind: kind.into(),
            value: value.into(),
            lifecycle_status: AssertionLifecycleStatus::Current,
            source_count: 1,
            reinforcement_count: 0,
            confidence_tier: AssertionConfidenceTier::Unconfirmed,
        }
    }

    pub fn inferred(
        domain: impl Into<String>,
        kind: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        let mut assertion = Self::new(domain, kind, value);
        assertion.confidence_tier = AssertionConfidenceTier::Inferred;
        assertion
    }

    pub fn with_source_count(mut self, source_count: u8) -> Self {
        self.source_count = source_count.max(1);
        if self.source_count >= 2 {
            self.confidence_tier = AssertionConfidenceTier::Confirmed;
        }
        self
    }

    pub fn reinforced(mut self) -> Self {
        self.reinforcement_count = self.reinforcement_count.saturating_add(1);
        self.source_count = self.source_count.max(2);
        self.confidence_tier =
            AssertionConfidenceTier::from_counts(self.source_count, self.reinforcement_count);
        self
    }

    pub fn key(&self) -> String {
        format!(
            "{}:{}={}",
            self.domain,
            self.kind,
            self.value.replace(' ', "_")
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssertionConfidenceTier {
    #[default]
    Unconfirmed,
    Inferred,
    Confirmed,
}

impl AssertionConfidenceTier {
    pub fn from_counts(source_count: u8, reinforcement_count: u32) -> Self {
        if source_count >= 2 {
            Self::Confirmed
        } else if reinforcement_count > 0 {
            Self::Inferred
        } else {
            Self::Unconfirmed
        }
    }

    pub fn is_confirmed(self) -> bool {
        matches!(self, Self::Confirmed)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnReading {
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
pub struct EpisodeProfile {
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
    pub profile: EpisodeProfile,
    pub recall_history: Vec<RecallRecord>,
    pub confidence: f32,
    pub coherence_score: f32,
    pub forgotten_risk: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryNode {
    pub id: String,
    pub label: String,
    pub kind: MemoryNodeKind,
    pub confidence_tier: AssertionConfidenceTier,
    pub density: f32,
    pub activation: f32,
    pub provenance: Vec<MemoryProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub contradiction_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryNodeKind {
    SystemKernel,
    User,
    Person,
    Character,
    Scene,
    Project,
    Place,
    Goal,
    Relationship,
    Attribute,
    Unknown,
    Assertion,
    Episode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEdge {
    pub source: String,
    pub target: String,
    pub relation: MemoryRelationKind,
    pub confidence_tier: AssertionConfidenceTier,
    pub strength: f32,
    pub activation: f32,
    pub provenance: Vec<MemoryProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRelationKind {
    DefinesRule,
    HasAttribute,
    HasInterest,
    HasGoal,
    RelatedTo,
    AliasOf,
    AppearsBefore,
    OccursBefore,
    OccursIn,
    CoFounderWith,
    LocatedIn,
    ProvenanceFor,
    Mentions,
    NeedsAnswer,
    Contradicts,
    Supersedes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryProvenance {
    pub episode_id: Option<Uuid>,
    pub turn_id: Option<Uuid>,
    pub assertion_key: Option<String>,
    #[serde(default)]
    pub system_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_status: Option<AssertionLifecycleStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MemoryMap {
    pub nodes: Vec<MemoryNode>,
    pub edges: Vec<MemoryEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemKernel {
    pub id: String,
    pub version: String,
    pub label: String,
    pub override_policy: KernelOverridePolicy,
    pub principles: Vec<KernelPrinciple>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelPrinciple {
    pub id: String,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelOverridePolicy {
    SystemVersioned,
    UserConfigurable,
    Locked,
}

impl Default for SystemKernel {
    fn default() -> Self {
        Self {
            id: "root:luna".to_string(),
            version: "root-orb-v1".to_string(),
            label: "Luna SystemKernel".to_string(),
            override_policy: KernelOverridePolicy::SystemVersioned,
            principles: vec![
                KernelPrinciple {
                    id: "root:luna:identity".to_string(),
                    label: "I am Luna".to_string(),
                    text: "Luna is a local-first memory runtime, not a conscious being."
                        .to_string(),
                },
                KernelPrinciple {
                    id: "root:luna:event_log".to_string(),
                    label: "Event log is source truth".to_string(),
                    text: "Durable memory comes from event-sourced turns and system events."
                        .to_string(),
                },
                KernelPrinciple {
                    id: "root:luna:tiers".to_string(),
                    label: "Separate certainty tiers".to_string(),
                    text: "Memory must distinguish confirmed, inferred, unconfirmed, and unknown."
                        .to_string(),
                },
                KernelPrinciple {
                    id: "root:luna:ambiguity".to_string(),
                    label: "Preserve ambiguity".to_string(),
                    text: "Luna must not flatten ambiguity into certainty.".to_string(),
                },
                KernelPrinciple {
                    id: "root:luna:working_set".to_string(),
                    label: "Working set stays tiny".to_string(),
                    text: "Only a bounded activated working set should enter the current turn."
                        .to_string(),
                },
                KernelPrinciple {
                    id: "root:luna:provenance".to_string(),
                    label: "Explain recall".to_string(),
                    text: "Surfaced memory should carry provenance and recall reasons.".to_string(),
                },
                KernelPrinciple {
                    id: "root:luna:proof_boundary".to_string(),
                    label: "Proof and product stay separate".to_string(),
                    text:
                        "Runtime may use weaker memory; proof metrics count only confirmed memory."
                            .to_string(),
                },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkingMemoryBudget {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_questions: usize,
    pub max_activation_depth: usize,
}

impl Default for WorkingMemoryBudget {
    fn default() -> Self {
        Self {
            max_nodes: 5,
            max_edges: 10,
            max_questions: 1,
            max_activation_depth: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorkingMemory {
    pub nodes: Vec<MemoryNode>,
    pub edges: Vec<MemoryEdge>,
    pub filtered_node_count: usize,
    pub filtered_edge_count: usize,
    pub activation_reason: String,
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
    Similarity,
}

impl std::str::FromStr for EngineKind {
    type Err = LunaError;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "none" | "no-memory" | "no_memory" => Ok(Self::NoMemory),
            "full-context" | "full_context" => Ok(Self::FullContext),
            "keyword" => Ok(Self::Keyword),
            "embedding" => Ok(Self::Embedding),
            "similarity" | "luna-match" => Ok(Self::Similarity),
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
            Self::Similarity => "similarity",
        };
        write!(f, "{name}")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallHit {
    pub episode_id: Uuid,
    pub score: f32,
    pub assertions: Vec<StructuredAssertion>,
    pub reason: RecallReason,
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
    use chrono::TimeZone;

    fn test_hash(ch: char) -> String {
        ch.to_string().repeat(64)
    }

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

    #[test]
    fn assertion_tiers_preserve_runtime_vs_proof_distinction() {
        let unconfirmed = StructuredAssertion::new("identity", "profession", "mechanical engineer");
        let inferred =
            StructuredAssertion::inferred("identity", "profession", "mechanical engineer");
        let confirmed = inferred.clone().with_source_count(2);

        assert_eq!(
            unconfirmed.confidence_tier,
            AssertionConfidenceTier::Unconfirmed
        );
        assert_eq!(inferred.confidence_tier, AssertionConfidenceTier::Inferred);
        assert_eq!(
            confirmed.confidence_tier,
            AssertionConfidenceTier::Confirmed
        );
        assert!(confirmed.confidence_tier.is_confirmed());
    }

    #[test]
    fn root_orb_is_versioned_and_not_locked() {
        let root = SystemKernel::default();

        assert_eq!(root.version, "root-orb-v1");
        assert_eq!(root.override_policy, KernelOverridePolicy::SystemVersioned);
        assert!(!root.principles.is_empty());
    }

    #[test]
    fn memory_node_kind_includes_character_without_root_pollution() {
        let kind = MemoryNodeKind::Character;

        assert_ne!(kind, MemoryNodeKind::SystemKernel);
        assert_ne!(kind, MemoryNodeKind::User);
    }

    #[test]
    fn memory_relation_kind_supports_alias_edges() {
        assert_eq!(
            serde_json::to_string(&MemoryRelationKind::AliasOf).unwrap(),
            "\"alias_of\""
        );
    }

    #[test]
    fn recall_reason_rejects_empty_and_whitespace() {
        assert!(RecallReason::new("").is_err());
        assert!(RecallReason::new("   ").is_err());
        assert!(RecallReason::new("\t\n").is_err());
    }

    #[test]
    fn recall_reason_trims_and_accepts_nonempty() {
        let r = RecallReason::new("  keyword overlap  ").unwrap();
        assert_eq!(r.as_str(), "keyword overlap");
    }

    #[test]
    fn recall_reason_legacy_empty_deserializes_as_sentinel() {
        let r: RecallReason = serde_json::from_str("\"\"").unwrap();
        assert_eq!(r.as_str(), "<unrecorded>");
        let r: RecallReason = serde_json::from_str("\"   \"").unwrap();
        assert_eq!(r.as_str(), "<unrecorded>");
    }

    #[test]
    fn recall_reason_serializes_as_plain_string() {
        let r = RecallReason::new("state contour activation").unwrap();
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, "\"state contour activation\"");
    }

    #[test]
    fn surprise_update_receipt_hash_is_deterministic() {
        let recorded_at = Utc.with_ymd_and_hms(2026, 5, 14, 12, 0, 0).unwrap();
        let first = SurpriseUpdateReceipt::new(
            "event-1",
            test_hash('a'),
            test_hash('b'),
            0.7,
            0.1,
            0.0,
            UpdateKind::NovelUpdate,
            test_hash('c'),
            test_hash('d'),
            vec![test_hash('e')],
            recorded_at,
        )
        .unwrap();
        let second = SurpriseUpdateReceipt::new(
            "event-1",
            test_hash('a'),
            test_hash('b'),
            0.7,
            0.1,
            0.0,
            UpdateKind::NovelUpdate,
            test_hash('c'),
            test_hash('d'),
            vec![test_hash('e')],
            recorded_at,
        )
        .unwrap();

        assert_eq!(first.receipt_hash, second.receipt_hash);
        assert_eq!(first.compute_receipt_hash(), first.receipt_hash);
        first.validate().unwrap();
    }

    #[test]
    fn surprise_update_receipt_hash_changes_with_update_kind() {
        let recorded_at = Utc.with_ymd_and_hms(2026, 5, 14, 12, 0, 0).unwrap();
        let reinforced = SurpriseUpdateReceipt::new(
            "event-1",
            test_hash('a'),
            test_hash('b'),
            0.05,
            0.95,
            0.0,
            UpdateKind::ReinforceExisting,
            test_hash('c'),
            test_hash('d'),
            vec![test_hash('e')],
            recorded_at,
        )
        .unwrap();
        let novel = SurpriseUpdateReceipt::new(
            "event-1",
            test_hash('a'),
            test_hash('b'),
            0.7,
            0.1,
            0.0,
            UpdateKind::NovelUpdate,
            test_hash('c'),
            test_hash('d'),
            vec![test_hash('e')],
            recorded_at,
        )
        .unwrap();

        assert_ne!(reinforced.receipt_hash, novel.receipt_hash);
    }

    #[test]
    fn surprise_update_receipt_rejects_bad_lineage_hash() {
        let recorded_at = Utc.with_ymd_and_hms(2026, 5, 14, 12, 0, 0).unwrap();
        let error = SurpriseUpdateReceipt::new(
            "event-1",
            test_hash('a'),
            test_hash('b'),
            0.7,
            0.1,
            0.0,
            UpdateKind::NovelUpdate,
            test_hash('c'),
            test_hash('d'),
            vec!["not-a-hash".to_string()],
            recorded_at,
        )
        .unwrap_err();

        assert!(error.to_string().contains("lineage hash"));
    }
}
