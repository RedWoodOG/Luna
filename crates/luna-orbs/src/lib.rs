//! Memory orb species — the generalization of [`luna_core::RootOrb`].
//!
//! Pr-1.0 shipped this crate as **types-only scaffolding**; pr-1.1
//! fills in the [`OrbCore`] (the gist) and [`HaloRef`] (the receipts).
//! No runtime path uses these types yet; nothing in `luna-runtime`
//! changes; existing `RootOrb` seeding still happens the way it always
//! has. The crate exists so later phases (tethers in pr-1.2, vector
//! field in pr-1.3, hybrid recall in pr-1.4, consolidation engine in
//! pr-1.6, governance in pr-1.9) have stable shapes to bind against.
//!
//! ## What's here
//!
//! - [`MemoryOrb`] — the envelope type. Stable shape: id, kind,
//!   core_version, schema_version, ancestors, privileged flag, policy
//!   bindings, audit timestamps.
//! - [`OrbKind`] — the species discriminator. `system_root` is the
//!   original RootOrb generalized; the other kinds are user-domain
//!   specializations. New kinds require a schema migration.
//! - [`OrbId`] — strict-on-construction newtype. Empty / whitespace ids
//!   are rejected at construction time (mirrors the
//!   [`luna_core::RecallReason`] discipline).
//! - [`OrbCore`] — the dense, condensed representation. The "gist."
//!   Carries the [`KeyFact`]s that survived consolidation, each with
//!   the source event ids that produced it.
//! - [`HaloRef`] — windowed reference back into the event log. The
//!   "receipts." Bookend event ids + count + time range. Doesn't
//!   duplicate the log; points at it.
//! - [`OrbTether`] / [`TetherKind`] — typed graph edges between orbs,
//!   minimal envelope. pr-1.2 fills in provenance / weighting.
//! - [`MemoryOrb::from_root_orb`] — adapter from the existing singleton
//!   [`luna_core::RootOrb`] to a [`MemoryOrb`] of kind `SystemRoot`.
//!   The runtime does not call this yet; it exists so audit and
//!   inspection paths can treat RootOrb uniformly with other orbs once
//!   the runtime starts producing them.
//!
//! ## Doctrine
//!
//! - `schema_version` is a const string (`memory_schema_v1`). Bumping
//!   it forces a migration; nothing should silently coerce a different
//!   value through this layer.
//! - `core_version` starts at 1 and is monotonically increasing.
//!   Pr-1.6's consolidation engine produces version bumps; this crate
//!   does not.
//! - There is no constructor that produces an orb without an explicit
//!   `created_at` timestamp. Replay determinism (R-002) extends here:
//!   orb timestamps must be event-time, not wall-clock, when the
//!   runtime starts producing them in pr-1.1+.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Re-exports of orb-vocabulary types that live in `luna-core` so
/// `LunaEvent::OrbTetherBound` can carry them without inducing a
/// `luna-core ↔ luna-orbs` dependency cycle. Callers can import them
/// from either crate; the type identity is the same.
pub use luna_core::{OrbId, TetherKind};

/// Schema version this crate produces and accepts. Mirrors
/// `memory_schema_v1` in the JSON Schema family at the repo root.
pub const SCHEMA_VERSION: &str = "memory_schema_v1";

/// The species of orb. `SystemRoot` is the original RootOrb
/// generalized; the rest are user-domain specializations. New kinds
/// require a schema migration (bump `SCHEMA_VERSION`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrbKind {
    SystemRoot,
    UserPreference,
    Project,
    Relationship,
    Skill,
    Place,
    Tool,
    Research,
    Domain,
    TraumaOrError,
}

/// A unit of compressed cognitive continuity.
///
/// Pr-1.0 ships the envelope. The detailed internals of `core` and
/// `halo_ref` arrive in pr-1.1; tether semantics fill in pr-1.2. The
/// fields here are stable: later phases extend the placeholder types,
/// they do not rename or remove fields on the envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryOrb {
    pub orb_id: OrbId,
    pub kind: OrbKind,
    /// Monotonically increasing. New orbs start at 1. Each successful
    /// consolidation produces a new `core_version` (pr-1.6 work).
    pub core_version: u32,
    /// Pinned to [`SCHEMA_VERSION`] today. Bumping requires a migration.
    pub schema_version: String,
    /// Dense compressed claims (the "gist"). pr-1.1 shape; pr-1.6 will
    /// populate it via consolidation.
    #[serde(default)]
    pub core: OrbCore,
    /// Reference back into the event log (the "receipts"). `None` until
    /// consolidation produces the first halo for this orb.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halo_ref: Option<HaloRef>,
    /// Typed graph edges. Skeleton in pr-1.0, detailed in pr-1.2.
    #[serde(default)]
    pub tethers: Vec<OrbTether>,
    /// Lineage chain — orb ids this orb descends from when it was
    /// produced by branching or merging. Empty for newly-created orbs.
    #[serde(default)]
    pub ancestors: Vec<OrbId>,
    /// If true, halo compression and core rewrite require an attestation
    /// event (pr-1.9 governance work). RootOrb is privileged by
    /// definition.
    #[serde(default)]
    pub privileged: bool,
    /// Optional governance rule references that bind this orb's
    /// behavior. Pr-1.9 wires enforcement.
    #[serde(default)]
    pub policy_bindings: Vec<String>,
    /// Wall-clock time the orb was first created. Audit field.
    pub created_at: DateTime<Utc>,
    /// Wall-clock time of the last consolidation. Audit field.
    pub last_consolidated_at: DateTime<Utc>,
}

impl MemoryOrb {
    /// Construct a new orb at `core_version = 1`. `now` should be the
    /// event-time the orb is being created at; for live runtime use
    /// that's the current turn's timestamp. Pr-1.0 has no runtime
    /// caller; pr-1.1+ does.
    pub fn new(orb_id: OrbId, kind: OrbKind, now: DateTime<Utc>) -> Self {
        Self {
            orb_id,
            kind,
            core_version: 1,
            schema_version: SCHEMA_VERSION.to_string(),
            core: OrbCore::default(),
            halo_ref: None,
            tethers: Vec::new(),
            ancestors: Vec::new(),
            privileged: false,
            policy_bindings: Vec::new(),
            created_at: now,
            last_consolidated_at: now,
        }
    }

    /// Produce a [`MemoryOrb`] representation of an existing
    /// [`luna_core::RootOrb`]. Used so audit and inspection paths can
    /// treat RootOrb uniformly with other orbs once they exist.
    ///
    /// **This does not replace the runtime's RootOrb path.** The
    /// runtime in pr-1.0 still seeds RootOrb into the MemoryMap the way
    /// it always has. This adapter exists for forward use.
    pub fn from_root_orb(
        root: &luna_core::RootOrb,
        now: DateTime<Utc>,
    ) -> Result<Self, OrbError> {
        let id_suffix = root.id.replace(':', ".");
        let orb_id = OrbId::new(format!("orb.system_root.{id_suffix}"))
            .map_err(|_| OrbError::EmptyOrbId)?;
        let core_version = parse_root_orb_version(&root.version);
        Ok(Self {
            orb_id,
            kind: OrbKind::SystemRoot,
            core_version,
            schema_version: SCHEMA_VERSION.to_string(),
            core: OrbCore::default(),
            halo_ref: None,
            tethers: Vec::new(),
            ancestors: Vec::new(),
            privileged: true,
            policy_bindings: Vec::new(),
            created_at: now,
            last_consolidated_at: now,
        })
    }
}

/// Dense, condensed representation of an orb — the "gist" that
/// survives consolidation. Compact enough to carry in a recall hit
/// without dragging the halo with it. Each [`KeyFact`] retains the
/// source event ids that produced it so the orb stays *citable*.
///
/// `#[non_exhaustive]` so pr-1.6 (consolidation) can add fields like
/// `open_questions` without a breaking schema bump at this layer.
///
/// Pr-1.1 reuses [`luna_core::Signal`] rather than introducing a
/// parallel type — fewer shapes, no drift.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OrbCore {
    /// Natural-language gist of the orb. Empty until pr-1.6's
    /// consolidation engine populates it. Bounded length will be
    /// enforced by consolidation, not here.
    #[serde(default)]
    pub summary: String,
    /// Atomic facts that survived consolidation. Each carries the
    /// source event ids that produced it. The constructor enforces
    /// non-empty source ids — empty provenance is forbidden.
    #[serde(default)]
    pub key_facts: Vec<KeyFact>,
    /// Confidence horizon for the orb as a whole. Maps from the
    /// existing `luna_core::AssertionConfidenceTier` semantics
    /// (memory_current_state.md:124).
    #[serde(default)]
    pub confidence_horizon: ConfidenceHorizon,
    /// Directional cues — preferences, recurring patterns. Reuses
    /// [`luna_core::Signal`].
    #[serde(default)]
    pub signals: Vec<luna_core::Signal>,
}

/// An atomic, citable fact attached to an [`OrbCore`].
///
/// Strict on construction: the statement must be non-empty / non-whitespace
/// (via [`KeyFactStatement`]), and `source_event_ids` must be non-empty
/// — "no fact without a citation."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyFact {
    pub statement: KeyFactStatement,
    /// Event ids in the log that justify this fact. Non-empty by
    /// constructor.
    pub source_event_ids: Vec<Uuid>,
    /// Clamped to `[0.0, 1.0]` at construction (matches
    /// `luna_core::Signal::new` and `EventEnvelope::new`).
    pub confidence: f32,
    /// Wall-clock time of the most recent event that touched this
    /// fact. Audit field.
    pub last_reinforced_at: DateTime<Utc>,
}

impl KeyFact {
    pub fn new(
        statement: KeyFactStatement,
        source_event_ids: Vec<Uuid>,
        confidence: f32,
        last_reinforced_at: DateTime<Utc>,
    ) -> Result<Self, OrbError> {
        if source_event_ids.is_empty() {
            return Err(OrbError::KeyFactWithoutSourceEvents);
        }
        Ok(Self {
            statement,
            source_event_ids,
            confidence: confidence.clamp(0.0, 1.0),
            last_reinforced_at,
        })
    }
}

/// Strict-on-construction newtype for the natural-language statement
/// inside a [`KeyFact`]. Empty / whitespace statements are rejected
/// (mirrors [`OrbId`] and `luna_core::RecallReason`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyFactStatement(String);

impl KeyFactStatement {
    pub fn new(statement: impl Into<String>) -> Result<Self, OrbError> {
        let s = statement.into();
        if s.trim().is_empty() {
            return Err(OrbError::EmptyKeyFactStatement);
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for KeyFactStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Confidence horizon for an orb as a whole. Maps from the per-assertion
/// `luna_core::AssertionConfidenceTier` (`memory_current_state.md:124`)
/// — re-derived at the orb level so a recall hit can reason about an
/// orb's overall belief grade without traversing every key fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceHorizon {
    /// No assertion has reached confirmed status. Default for a
    /// freshly-created orb that has not yet been consolidated.
    #[default]
    Unconfirmed,
    /// Inferred from context but not directly confirmed.
    Inferred,
    /// At least one assertion has been confirmed by direct user signal
    /// or repeated co-activation.
    Confirmed,
}

/// Windowed reference back into the event log. Bookend event ids +
/// count + time range. The full enumeration of events lives in the
/// log; this type **points at it**, it does not duplicate it.
///
/// Designed to compose with:
/// - `pr-1.4/hybrid-recall` — `time_range` feeds time-decay weighting.
/// - `pr-1.6/consolidate-engine` — produces a `HaloRef` from the
///   window of events it consumed.
/// - `pr-1.9/attestation` — verifies that re-replaying the events in
///   `[first_event_id, last_event_id]` reproduces `event_count`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HaloRef {
    /// Bookend: oldest event in the halo window.
    pub first_event_id: Uuid,
    /// Bookend: newest event in the halo window.
    pub last_event_id: Uuid,
    /// Number of events in `[first_event_id, last_event_id]` that
    /// belong to this orb. Verified at attestation time — drift
    /// here means replay produced a different shape than was
    /// originally consolidated.
    pub event_count: u32,
    /// Inclusive time window. Both endpoints are real event
    /// timestamps, not `Utc::now()` (R-002 closure pattern).
    pub time_range: HaloTimeRange,
}

impl HaloRef {
    pub fn new(
        first_event_id: Uuid,
        last_event_id: Uuid,
        event_count: u32,
        time_range: HaloTimeRange,
    ) -> Self {
        Self {
            first_event_id,
            last_event_id,
            event_count,
            time_range,
        }
    }
}

/// Inclusive time window for a [`HaloRef`]. Constructor rejects
/// `to < from`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HaloTimeRange {
    /// Inclusive start.
    pub from: DateTime<Utc>,
    /// Inclusive end. Must be `>= from`.
    pub to: DateTime<Utc>,
}

impl HaloTimeRange {
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Self, OrbError> {
        if to < from {
            return Err(OrbError::InvertedTimeRange);
        }
        Ok(Self { from, to })
    }
}

/// Typed graph edge between two orbs, derived from a logged bind
/// event. Pr-1.0 shipped the envelope; pr-1.2 fills in provenance,
/// `bound_at` audit timestamp, and weight semantics.
///
/// **No tether without a citation.** [`OrbTether::new`] rejects empty
/// `provenance` — every tether traces back to at least one event in
/// the log, mirroring the [`KeyFact::source_event_ids`] discipline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbTether {
    pub from_orb: OrbId,
    pub to_orb: OrbId,
    pub kind: TetherKind,
    /// 0..=1 activation weight. 0 means the tether exists but contributes
    /// nothing to traversal scoring. Clamped at construction by
    /// [`OrbTether::new`] (matches `KeyFact::confidence`,
    /// `Signal::new`, `EventEnvelope::new`).
    #[serde(default)]
    pub weight: f32,
    /// Events that produced this tether — non-empty by constructor.
    /// Same shape rule as [`KeyFact::source_event_ids`]: no tether
    /// without a citation.
    #[serde(default)]
    pub provenance: Vec<TetherProvenance>,
    /// Wall-clock time the tether was first bound. Audit field;
    /// matches [`MemoryOrb::created_at`] discipline. For runtime
    /// emitters (pr-1.6) this MUST be the event-time of the bind
    /// event, not `Utc::now()` (R-002 closure pattern).
    pub bound_at: DateTime<Utc>,
}

impl OrbTether {
    /// Strict constructor. Rejects empty `provenance`. Clamps `weight`
    /// to `[0.0, 1.0]`. The doctrinal rule "every tether traces back
    /// to a logged event" is a type signature here — production code
    /// cannot construct an unattributed tether.
    pub fn new(
        from_orb: OrbId,
        to_orb: OrbId,
        kind: TetherKind,
        weight: f32,
        provenance: Vec<TetherProvenance>,
        bound_at: DateTime<Utc>,
    ) -> Result<Self, OrbError> {
        if provenance.is_empty() {
            return Err(OrbError::TetherWithoutProvenance);
        }
        Ok(Self {
            from_orb,
            to_orb,
            kind,
            weight: weight.clamp(0.0, 1.0),
            provenance,
            bound_at,
        })
    }
}

/// One citation backing an [`OrbTether`]. Carries an event id that
/// can be joined against the event log to recover full context, plus
/// a [`TetherBinding`] tag classifying *what kind of bind* the event
/// represented. Mirrors how [`luna_core::EpisodeRecalled`] carries
/// just enough to be auditable without duplicating the log.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TetherProvenance {
    /// The event in the log that caused this tether to bind.
    pub event_id: Uuid,
    /// What kind of binding signal this event represented. Lets audit
    /// answer "was this from a derivation, a co-activation, or an
    /// explicit user signal?"
    pub binding: TetherBinding,
}

impl TetherProvenance {
    pub fn new(event_id: Uuid, binding: TetherBinding) -> Self {
        Self { event_id, binding }
    }
}

/// Classifier for what kind of bind event produced a tether. Lets the
/// audit path distinguish e.g. "this CoActiveWith tether emerged from
/// repeated co-activation" from "this AncestorOf tether came from a
/// consolidation lineage event".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TetherBinding {
    /// `OrbTetherBound` event — explicit consolidation-time bind.
    Explicit,
    /// `EpisodeRecalled` events repeatedly co-activating two orbs —
    /// a `CoActiveWith` tether emerged from frequency.
    CoActivation,
    /// An `AssertionCorrected` event surfaced a contradiction
    /// between two orbs — a `Contradicts` tether.
    ContradictionEvent,
    /// A consolidation produced a child orb from a parent — an
    /// `AncestorOf` / `Specializes` tether.
    LineageEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrbError {
    EmptyOrbId,
    EmptyKeyFactStatement,
    KeyFactWithoutSourceEvents,
    InvertedTimeRange,
    TetherWithoutProvenance,
}

impl std::fmt::Display for OrbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrbError::EmptyOrbId => f.write_str("orb_id must not be empty or whitespace"),
            OrbError::EmptyKeyFactStatement => {
                f.write_str("key_fact statement must not be empty or whitespace")
            }
            OrbError::KeyFactWithoutSourceEvents => {
                f.write_str("key_fact must cite at least one source event id")
            }
            OrbError::InvertedTimeRange => {
                f.write_str("halo time range `to` must be >= `from`")
            }
            OrbError::TetherWithoutProvenance => {
                f.write_str("orb tether must cite at least one TetherProvenance")
            }
        }
    }
}

impl std::error::Error for OrbError {}

/// Parse the integer version trailer from a RootOrb version string.
/// Format: `root-orb-v<N>`. Falls back to 1 on any other shape so
/// migration of legacy logs doesn't fail.
fn parse_root_orb_version(version: &str) -> u32 {
    version
        .strip_prefix("root-orb-v")
        .and_then(|n| n.parse().ok())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 8, 12, 0, 0).unwrap()
    }

    #[test]
    fn schema_version_is_pinned() {
        assert_eq!(SCHEMA_VERSION, "memory_schema_v1");
    }

    #[test]
    fn orb_id_rejects_empty_and_whitespace() {
        assert!(OrbId::new("").is_err());
        assert!(OrbId::new("   ").is_err());
        assert!(OrbId::new("\t\n").is_err());
        assert!(OrbId::new("orb.x").is_ok());
    }

    #[test]
    fn orb_id_round_trips_via_serde_as_a_string() {
        let id = OrbId::new("orb.system_root").unwrap();
        let s = serde_json::to_string(&id).unwrap();
        assert_eq!(s, "\"orb.system_root\"");
        let back: OrbId = serde_json::from_str(&s).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn orb_kind_serializes_snake_case() {
        let cases = [
            (OrbKind::SystemRoot, "\"system_root\""),
            (OrbKind::UserPreference, "\"user_preference\""),
            (OrbKind::TraumaOrError, "\"trauma_or_error\""),
        ];
        for (kind, expected) in cases {
            assert_eq!(serde_json::to_string(&kind).unwrap(), expected);
            let back: OrbKind = serde_json::from_str(expected).unwrap();
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn tether_kind_serializes_snake_case() {
        let cases = [
            (TetherKind::DerivedFrom, "\"derived_from\""),
            (TetherKind::CoActiveWith, "\"co_active_with\""),
            (TetherKind::AncestorOf, "\"ancestor_of\""),
        ];
        for (k, expected) in cases {
            assert_eq!(serde_json::to_string(&k).unwrap(), expected);
            let back: TetherKind = serde_json::from_str(expected).unwrap();
            assert_eq!(back, k);
        }
    }

    #[test]
    fn new_orb_starts_at_core_version_one_and_pins_schema_version() {
        let id = OrbId::new("orb.user_preference.review_style").unwrap();
        let now = fixed_time();
        let orb = MemoryOrb::new(id.clone(), OrbKind::UserPreference, now);
        assert_eq!(orb.orb_id, id);
        assert_eq!(orb.kind, OrbKind::UserPreference);
        assert_eq!(orb.core_version, 1);
        assert_eq!(orb.schema_version, SCHEMA_VERSION);
        assert!(!orb.privileged);
        assert!(orb.tethers.is_empty());
        assert!(orb.ancestors.is_empty());
        assert!(orb.policy_bindings.is_empty());
        assert_eq!(orb.created_at, now);
        assert_eq!(orb.last_consolidated_at, now);
    }

    #[test]
    fn from_root_orb_produces_privileged_system_root_orb() {
        let root = luna_core::RootOrb::default();
        let now = fixed_time();
        let orb = MemoryOrb::from_root_orb(&root, now).unwrap();
        assert_eq!(orb.kind, OrbKind::SystemRoot);
        assert!(
            orb.privileged,
            "RootOrb must always map to a privileged orb"
        );
        assert_eq!(orb.core_version, 1, "default RootOrb is version 1");
        assert!(
            orb.orb_id.as_str().starts_with("orb.system_root."),
            "id must be in the orb.system_root namespace, got {}",
            orb.orb_id
        );
        assert_eq!(orb.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn from_root_orb_parses_version_trailer() {
        let mut root = luna_core::RootOrb::default();
        root.version = "root-orb-v3".to_string();
        let orb = MemoryOrb::from_root_orb(&root, fixed_time()).unwrap();
        assert_eq!(orb.core_version, 3);
    }

    #[test]
    fn from_root_orb_falls_back_to_one_on_unparseable_version() {
        let mut root = luna_core::RootOrb::default();
        root.version = "experimental".to_string();
        let orb = MemoryOrb::from_root_orb(&root, fixed_time()).unwrap();
        assert_eq!(orb.core_version, 1);
    }

    #[test]
    fn memory_orb_round_trips_through_json() {
        let id = OrbId::new("orb.project.beacon").unwrap();
        let now = fixed_time();
        let mut orb = MemoryOrb::new(id, OrbKind::Project, now);
        let tether = OrbTether::new(
            OrbId::new("orb.project.beacon").unwrap(),
            OrbId::new("orb.system_root.root.luna").unwrap(),
            TetherKind::DerivedFrom,
            0.42,
            vec![TetherProvenance::new(
                Uuid::new_v4(),
                TetherBinding::LineageEvent,
            )],
            now,
        )
        .unwrap();
        orb.tethers.push(tether);
        orb.ancestors
            .push(OrbId::new("orb.project.beacon-draft").unwrap());

        let json = serde_json::to_string(&orb).unwrap();
        let back: MemoryOrb = serde_json::from_str(&json).unwrap();
        assert_eq!(orb, back);
    }

    #[test]
    fn memory_orb_with_default_optional_fields_round_trips() {
        // Forward-compatibility: a JSON document missing the optional
        // fields (tethers, ancestors, privileged, policy_bindings, core,
        // halo_ref) must still deserialize via serde defaults.
        let id = OrbId::new("orb.tool.cargo").unwrap();
        let json = serde_json::json!({
            "orb_id": id.as_str(),
            "kind": "tool",
            "core_version": 1,
            "schema_version": SCHEMA_VERSION,
            "created_at": "2026-05-08T12:00:00Z",
            "last_consolidated_at": "2026-05-08T12:00:00Z",
        });
        let orb: MemoryOrb = serde_json::from_value(json).unwrap();
        assert_eq!(orb.orb_id, id);
        assert_eq!(orb.kind, OrbKind::Tool);
        assert!(orb.tethers.is_empty());
        assert!(orb.ancestors.is_empty());
        assert!(!orb.privileged);
        assert!(orb.policy_bindings.is_empty());
        assert_eq!(orb.core, OrbCore::default());
        assert!(
            orb.halo_ref.is_none(),
            "fresh orb must have no halo until consolidation produces one"
        );
    }

    // ---------- pr-1.1/orb-halos: OrbCore + KeyFact ----------

    #[test]
    fn orb_core_default_is_empty_and_unconfirmed() {
        let core = OrbCore::default();
        assert!(core.summary.is_empty());
        assert!(core.key_facts.is_empty());
        assert!(core.signals.is_empty());
        assert_eq!(core.confidence_horizon, ConfidenceHorizon::Unconfirmed);
    }

    #[test]
    fn key_fact_statement_rejects_empty_and_whitespace() {
        assert!(KeyFactStatement::new("").is_err());
        assert!(KeyFactStatement::new("   ").is_err());
        assert!(KeyFactStatement::new("\t\n").is_err());
        assert!(KeyFactStatement::new("Joe prefers reviews on Fridays").is_ok());
    }

    #[test]
    fn key_fact_rejects_empty_source_event_ids() {
        let stmt = KeyFactStatement::new("Joe prefers Fridays").unwrap();
        let err = KeyFact::new(stmt, vec![], 0.7, fixed_time()).unwrap_err();
        assert_eq!(err, OrbError::KeyFactWithoutSourceEvents);
    }

    #[test]
    fn key_fact_clamps_confidence_to_unit_interval() {
        let stmt = KeyFactStatement::new("x").unwrap();
        let high = KeyFact::new(stmt.clone(), vec![Uuid::new_v4()], 9.5, fixed_time()).unwrap();
        assert_eq!(high.confidence, 1.0);
        let low = KeyFact::new(stmt, vec![Uuid::new_v4()], -0.4, fixed_time()).unwrap();
        assert_eq!(low.confidence, 0.0);
    }

    #[test]
    fn confidence_horizon_serializes_snake_case() {
        let cases = [
            (ConfidenceHorizon::Unconfirmed, "\"unconfirmed\""),
            (ConfidenceHorizon::Inferred, "\"inferred\""),
            (ConfidenceHorizon::Confirmed, "\"confirmed\""),
        ];
        for (h, expected) in cases {
            assert_eq!(serde_json::to_string(&h).unwrap(), expected);
            let back: ConfidenceHorizon = serde_json::from_str(expected).unwrap();
            assert_eq!(back, h);
        }
    }

    // ---------- pr-1.1/orb-halos: HaloRef + HaloTimeRange ----------

    #[test]
    fn halo_time_range_rejects_to_before_from() {
        let from = fixed_time();
        let earlier = from - chrono::Duration::hours(1);
        assert_eq!(
            HaloTimeRange::new(from, earlier).unwrap_err(),
            OrbError::InvertedTimeRange
        );
        // Equal endpoints are allowed (a single-event halo).
        assert!(HaloTimeRange::new(from, from).is_ok());
    }

    #[test]
    fn halo_ref_round_trips_through_json() {
        let first = Uuid::new_v4();
        let last = Uuid::new_v4();
        let from = fixed_time();
        let to = from + chrono::Duration::hours(6);
        let halo = HaloRef::new(first, last, 17, HaloTimeRange::new(from, to).unwrap());
        let json = serde_json::to_string(&halo).unwrap();
        let back: HaloRef = serde_json::from_str(&json).unwrap();
        assert_eq!(halo, back);
    }

    #[test]
    fn memory_orb_with_populated_core_and_halo_round_trips() {
        let id = OrbId::new("orb.relationship.joe").unwrap();
        let now = fixed_time();
        let mut orb = MemoryOrb::new(id, OrbKind::Relationship, now);

        let event_a = Uuid::new_v4();
        let event_b = Uuid::new_v4();
        let stmt = KeyFactStatement::new("Joe prefers reviews on Fridays").unwrap();
        let fact = KeyFact::new(stmt, vec![event_a, event_b], 0.85, now).unwrap();

        orb.core = OrbCore {
            summary: "Joe is a reviewer who prefers end-of-week sessions.".to_string(),
            key_facts: vec![fact],
            confidence_horizon: ConfidenceHorizon::Confirmed,
            signals: vec![luna_core::Signal::new(
                0.7,
                0.9,
                luna_core::SignalReliability::UserConfirmed,
            )],
        };
        orb.halo_ref = Some(HaloRef::new(
            event_a,
            event_b,
            2,
            HaloTimeRange::new(now, now + chrono::Duration::hours(2)).unwrap(),
        ));

        let json = serde_json::to_string(&orb).unwrap();
        let back: MemoryOrb = serde_json::from_str(&json).unwrap();
        assert_eq!(orb, back);
    }

    #[test]
    fn key_fact_round_trips_through_json_preserving_source_event_ids() {
        let stmt = KeyFactStatement::new("Chris owns the dispatcher").unwrap();
        let event_id = Uuid::new_v4();
        let fact = KeyFact::new(stmt, vec![event_id], 0.6, fixed_time()).unwrap();
        let json = serde_json::to_string(&fact).unwrap();
        let back: KeyFact = serde_json::from_str(&json).unwrap();
        assert_eq!(fact, back);
        assert_eq!(back.source_event_ids, vec![event_id]);
    }

    #[test]
    fn fresh_orb_has_no_halo_until_consolidation() {
        // Doctrine: a brand-new orb has no halo. `Option::None` is the
        // honest representation, not a sentinel HaloRef with zero count.
        let orb = MemoryOrb::new(
            OrbId::new("orb.project.beacon").unwrap(),
            OrbKind::Project,
            fixed_time(),
        );
        assert!(orb.halo_ref.is_none());
        assert_eq!(orb.core, OrbCore::default());
    }

    #[test]
    fn parse_root_orb_version_handles_typical_shapes() {
        assert_eq!(parse_root_orb_version("root-orb-v1"), 1);
        assert_eq!(parse_root_orb_version("root-orb-v42"), 42);
        // unparseable falls back to 1, doesn't panic
        assert_eq!(parse_root_orb_version("garbage"), 1);
        assert_eq!(parse_root_orb_version(""), 1);
        assert_eq!(parse_root_orb_version("root-orb-v"), 1);
    }

    // ---------- pr-1.2/orb-tethers: OrbTether + TetherProvenance ----------

    fn sample_provenance() -> TetherProvenance {
        TetherProvenance::new(Uuid::new_v4(), TetherBinding::Explicit)
    }

    #[test]
    fn orb_tether_rejects_empty_provenance() {
        let err = OrbTether::new(
            OrbId::new("orb.a").unwrap(),
            OrbId::new("orb.b").unwrap(),
            TetherKind::DerivedFrom,
            0.5,
            vec![],
            fixed_time(),
        )
        .unwrap_err();
        assert_eq!(err, OrbError::TetherWithoutProvenance);
    }

    #[test]
    fn orb_tether_clamps_weight_to_unit_interval() {
        let high = OrbTether::new(
            OrbId::new("orb.a").unwrap(),
            OrbId::new("orb.b").unwrap(),
            TetherKind::CoActiveWith,
            9.5,
            vec![sample_provenance()],
            fixed_time(),
        )
        .unwrap();
        assert_eq!(high.weight, 1.0);

        let low = OrbTether::new(
            OrbId::new("orb.a").unwrap(),
            OrbId::new("orb.b").unwrap(),
            TetherKind::CoActiveWith,
            -0.4,
            vec![sample_provenance()],
            fixed_time(),
        )
        .unwrap();
        assert_eq!(low.weight, 0.0);
    }

    #[test]
    fn orb_tether_round_trips_through_json() {
        let now = fixed_time();
        let tether = OrbTether::new(
            OrbId::new("orb.relationship.joe").unwrap(),
            OrbId::new("orb.project.beacon").unwrap(),
            TetherKind::CoActiveWith,
            0.42,
            vec![
                TetherProvenance::new(Uuid::new_v4(), TetherBinding::CoActivation),
                TetherProvenance::new(Uuid::new_v4(), TetherBinding::Explicit),
            ],
            now,
        )
        .unwrap();
        let json = serde_json::to_string(&tether).unwrap();
        let back: OrbTether = serde_json::from_str(&json).unwrap();
        assert_eq!(tether, back);
        assert_eq!(back.provenance.len(), 2);
        assert_eq!(back.bound_at, now);
    }

    #[test]
    fn tether_provenance_round_trips() {
        let event_id = Uuid::new_v4();
        let provenance = TetherProvenance::new(event_id, TetherBinding::LineageEvent);
        let json = serde_json::to_string(&provenance).unwrap();
        let back: TetherProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(provenance, back);
        assert_eq!(back.event_id, event_id);
        assert_eq!(back.binding, TetherBinding::LineageEvent);
    }

    #[test]
    fn tether_binding_serializes_snake_case() {
        let cases = [
            (TetherBinding::Explicit, "\"explicit\""),
            (TetherBinding::CoActivation, "\"co_activation\""),
            (TetherBinding::ContradictionEvent, "\"contradiction_event\""),
            (TetherBinding::LineageEvent, "\"lineage_event\""),
        ];
        for (binding, expected) in cases {
            assert_eq!(serde_json::to_string(&binding).unwrap(), expected);
            let back: TetherBinding = serde_json::from_str(expected).unwrap();
            assert_eq!(back, binding);
        }
    }
}
