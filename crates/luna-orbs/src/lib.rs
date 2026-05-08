//! Memory orb species — the generalization of [`luna_core::RootOrb`].
//!
//! This crate ships in `pr-1.0/orb-schema` as **types only**. No
//! runtime path uses these types yet; nothing in `luna-runtime`
//! changes; existing `RootOrb` seeding still happens the way it always
//! has. The crate exists so later phases (halos in pr-1.1, tethers in
//! pr-1.2, vector field in pr-1.3, hybrid recall in pr-1.4,
//! consolidation engine in pr-1.6, governance in pr-1.9) have stable
//! shapes to bind against.
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
//! - [`OrbCore`], [`HaloRef`] — skeleton types marked `#[non_exhaustive]`
//!   so pr-1.1 can fill them in without a breaking schema bump at this
//!   layer.
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

/// Stable identifier for a memory orb. Conventionally namespaced
/// (e.g. `orb.system_root`, `orb.user_preference.review_style`).
///
/// Strict on construction: empty and whitespace-only ids are rejected.
/// Mirrors the [`luna_core::RecallReason`] discipline — invariants live
/// at the type level, not in scattered runtime checks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrbId(String);

impl OrbId {
    pub fn new(id: impl Into<String>) -> Result<Self, OrbError> {
        let s = id.into();
        if s.trim().is_empty() {
            return Err(OrbError::EmptyOrbId);
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

impl std::fmt::Display for OrbId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
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
    /// Dense compressed claims. Skeleton in pr-1.0, detailed in pr-1.1.
    #[serde(default)]
    pub core: OrbCore,
    /// Halo cursor. Skeleton in pr-1.0, detailed in pr-1.1.
    #[serde(default)]
    pub halo_ref: HaloRef,
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
            halo_ref: HaloRef::default(),
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
        let orb_id = OrbId::new(format!("orb.system_root.{id_suffix}"))?;
        let core_version = parse_root_orb_version(&root.version);
        Ok(Self {
            orb_id,
            kind: OrbKind::SystemRoot,
            core_version,
            schema_version: SCHEMA_VERSION.to_string(),
            core: OrbCore::default(),
            halo_ref: HaloRef::default(),
            tethers: Vec::new(),
            ancestors: Vec::new(),
            privileged: true,
            policy_bindings: Vec::new(),
            created_at: now,
            last_consolidated_at: now,
        })
    }
}

/// Dense, compressed claims of an orb. Skeleton in pr-1.0; pr-1.1 adds
/// versioned claim sets, contradictions, open questions. Marked
/// `#[non_exhaustive]` so pr-1.1 can extend without a SemVer-equivalent
/// break at this layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OrbCore {}

/// Reference to the orb's halo — a derived view of the event log
/// scoped to the orb. Skeleton in pr-1.0; pr-1.1 fills in cursor /
/// query / weight signals.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HaloRef {}

/// Typed graph edge between two orbs, derived from a logged bind
/// event. Pr-1.0 ships the envelope; pr-1.2 detail-fills provenance
/// and weight semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbTether {
    pub from_orb: OrbId,
    pub to_orb: OrbId,
    pub kind: TetherKind,
    /// 0..=1 activation weight. 0 means the tether exists but contributes
    /// nothing to traversal scoring. Default 0.0 is intentionally
    /// non-active so pr-1.2 has to deliberately weight new tethers.
    #[serde(default)]
    pub weight: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TetherKind {
    /// "B's content was derived from A" — typically a bud relationship.
    DerivedFrom,
    /// "B is a specialization of A" — branching produces this.
    Specializes,
    /// "A and B contradict" — surfaced for arbitration.
    Contradicts,
    /// "A and B repeatedly co-activate" — merge candidate signal.
    CoActiveWith,
    /// "A is a prior version / parent of B" — lineage record.
    AncestorOf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrbError {
    EmptyOrbId,
}

impl std::fmt::Display for OrbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrbError::EmptyOrbId => f.write_str("orb_id must not be empty or whitespace"),
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
        orb.tethers.push(OrbTether {
            from_orb: OrbId::new("orb.project.beacon").unwrap(),
            to_orb: OrbId::new("orb.system_root.root.luna").unwrap(),
            kind: TetherKind::DerivedFrom,
            weight: 0.42,
        });
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
}
