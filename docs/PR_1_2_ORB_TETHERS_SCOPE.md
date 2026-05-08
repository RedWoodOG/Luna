# pr-1.2/orb-tethers — scope

**Status.** Draft for review. Not yet implemented.
**Predecessor.** `pr-1.1a/raw-observation-capture` (commit `5e7f682`)
— closed R-003 by adding `LunaEvent::RawObservationCaptured`.
Pr-1.0/orb-schema (`8404282`) shipped `OrbTether` and `TetherKind`
as types-only envelopes (from_orb, to_orb, kind, weight). This slice
fleshes them in.
**Successor.** `pr-1.3/vector-field` (perception layer), then
`pr-1.4/hybrid-recall` (orbs × vector field × tethers consumed
together).

---

## What this slice is

Two related concerns that share a code surface:

**1. Tether semantics — make the lattice edge real.**
Pr-1.0 shipped `OrbTether { from_orb, to_orb, kind, weight }`. The
shape exists; the **derivation, provenance, and traversal weight**
are undefined. This slice answers:

- *Where do tethers come from?* — typed events in the log.
- *How do we cite a tether?* — provenance pointing back at the
  events that caused it.
- *What does `weight` mean?* — bounded activation contribution at
  traversal time.

**2. R-005 closure — silent node merge.**
`MemoryState::from_episodes` currently mutates an existing
`MemoryNode` in place when a duplicate id arrives, without logging
the merge. Risk register flags this as making tether attribution
harder ("which event caused the tether between merged nodes?"). The
scope doc bundles this closure here because tether provenance is
exactly the surface the merge invisibility breaks.

This slice is **types + a small runtime change**, not a wiring of
tethers into recall (that's pr-1.4). Specifically, this slice will:

- Land a `TetherProvenance` type and add `provenance: Vec<TetherProvenance>`
  to `OrbTether`.
- Land a `LunaEvent::OrbTetherBound` variant (the "bind-event" the
  doctrine refers to) so tether creation is logged.
- Close R-005 by emitting `LunaEvent::NodeMerged` from
  `MemoryState::from_episodes` whenever the existing-node branch in
  `insert_node` is taken.
- **Not** populate tethers from runtime data automatically. That
  needs the consolidation engine (pr-1.6). Pr-1.2 lands the
  vocabulary; pr-1.6 produces tethers at scale.

---

## Proposed shape — `OrbTether` (extended)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbTether {
    pub from_orb: OrbId,
    pub to_orb: OrbId,
    pub kind: TetherKind,
    /// Activation weight clamped to [0.0, 1.0] at construction.
    /// `0.0` means the tether exists but contributes nothing to
    /// traversal scoring.
    pub weight: f32,
    /// Events that produced this tether — non-empty by constructor.
    /// "No tether without a citation" — same shape rule as
    /// `KeyFact::source_event_ids`.
    pub provenance: Vec<TetherProvenance>,
    /// Wall-clock time the tether was first bound. Audit field;
    /// matches `MemoryOrb::created_at` discipline.
    pub bound_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TetherProvenance {
    /// The event in the log that caused this tether to bind.
    pub event_id: uuid::Uuid,
    /// What kind of binding signal this event represented. Lets
    /// audit answer "was this from a derivation, a co-activation,
    /// or an explicit user signal?"
    pub binding: TetherBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TetherBinding {
    /// `OrbTetherBound` event — explicit consolidation-time bind.
    Explicit,
    /// `EpisodeRecalled` events repeatedly co-activating two orbs —
    /// a CoActiveWith tether emerges from frequency.
    CoActivation,
    /// An `AssertionCorrected` event surfaced a contradiction
    /// between two orbs — a Contradicts tether.
    ContradictionEvent,
    /// A consolidation produced a child orb from a parent — an
    /// AncestorOf / Specializes tether.
    LineageEvent,
}
```

**Strict construction:**

- `OrbTether::new(from, to, kind, weight, provenance, bound_at)`
  rejects empty `provenance`.
- `weight` clamped to `[0.0, 1.0]` (matches `KeyFact::confidence`,
  `Signal::new`, `EventEnvelope::new`).
- A new `OrbError::TetherWithoutProvenance` variant is added.

---

## Proposed event variant — `LunaEvent::OrbTetherBound`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbTetherBound {
    pub from_orb: OrbId,
    pub to_orb: OrbId,
    pub kind: TetherKind,
    pub initial_weight: f32,
    /// Why this bind happened. Matches the audit rule that every
    /// transformation carries its reason.
    pub reason: RecallReason,
}
```

- Added to `LunaEvent` enum in `luna-core`.
- `luna-store::rebuild_episodes` adds `OrbTetherBound` to its no-op
  arm (informational at this slice — pr-1.6 introduces the
  rebuild path that derives an actual `Vec<OrbTether>` from these
  events).
- pr-1.2 does **not** emit this event from runtime code yet. The
  variant exists so pr-1.6 has a target. Tests construct it
  directly.

---

## Proposed event variant — `LunaEvent::NodeMerged` (R-005 closure)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeMerged {
    pub node_id: String,
    pub merged_density_delta: f32,
    pub previous_confidence_tier: AssertionConfidenceTier,
    pub new_confidence_tier: AssertionConfidenceTier,
    pub merged_provenance_count: usize,
}
```

Emitted from `MemoryState::from_episodes` whenever
`insert_node` finds an existing node and extends its
density / confidence_tier / provenance. The event records *what
changed*. Replay does not derive state from `NodeMerged` (the same
`MemoryState` path will run again on rebuild) — it is purely an
audit record, like `RawObservationCaptured`.

R-005 alternative considered: a "merge ledger" inside `MemoryState`
queryable at audit time. Rejected because the audit log is the
single source of truth; introducing a parallel ledger means two
places to keep in sync.

---

## What this slice does NOT do

- Does not wire tethers into recall scoring. That's `pr-1.4`.
- Does not produce tethers automatically from runtime activity.
  That's `pr-1.6` (consolidation engine emits `OrbTetherBound`
  events; rebuild derives `Vec<OrbTether>` from them).
- Does not change the `MemoryEdge` / `MemoryRelationKind` shape in
  the working-memory graph. Tethers are a different layer (orb
  lattice) from working-memory edges (activation graph).
- Does not change replay determinism — `OrbTetherBound` and
  `NodeMerged` are informational.

---

## Open questions for review

### Q1. Does the event log gain `OrbTetherBound` now, or wait for pr-1.6?

- **Option A (recommended):** Land it now as an unused variant.
  Doctrine says "tether is a typed edge derived from a bind-event in
  the log" — pr-1.6 will need this variant. Adding it now (with
  `luna-store` no-op handling) means pr-1.6 only writes producer
  code, not also event-schema code. Smaller blast radius spread
  across two PRs.
- **Option B:** Defer to pr-1.6. Pr-1.2 ships only the `OrbTether`
  type extension (provenance, bound_at). Pr-1.6 adds the event
  variant and the producer in one slice. Risk: pr-1.6 grows.

### Q2. Does pr-1.2 close R-005 (silent node merge), or split?

- **Option A (recommended):** Bundle. R-005 is targeted at pr-1.2
  in the risk register precisely because tether attribution gets
  harder when merges are silent. Closing it here means tethers land
  on a clean substrate.
- **Option B:** Split into pr-1.2a/node-merge-events. Smaller per-PR
  surface, but pr-1.2 lands tether types over an unfixed
  provenance hole.

### Q3. Does `TetherProvenance` carry the event id, or a richer reference?

- **Option A (recommended):** Just `event_id: Uuid` + `binding: TetherBinding`.
  Audit can join against the event log to recover full context.
  Mirrors how `MemoryProvenance` works (id-only references, log is
  the source of truth).
- **Option B:** Embed the full `EventEnvelope<LunaEvent>` for fast
  read paths. Larger storage, duplicates the log, harder to keep
  consistent if events are ever rewritten. Worth it only if
  attestation needs offline verification — which is pr-1.9, not
  pr-1.2.

### Q4. `OrbTether::weight` — clamp at construction, or scoring time?

- **Option A (recommended):** Clamp at construction. Matches every
  other bounded float in the system (`KeyFact::confidence`,
  `Signal::new`, `EventEnvelope::new`, `EpisodeDecayed::forgotten_risk`).
  Consistent surface; no question about whether a stored value is
  in-range.
- **Option B:** Free range, clamp at scoring time. Lets the
  consolidation engine record raw signals and defer clamping. Risk:
  another implicit invariant; "stored" and "scored" diverge.

---

## Tests planned (≥10)

- `orb_tether_rejects_empty_provenance`
- `orb_tether_clamps_weight_to_unit_interval`
- `orb_tether_round_trips_through_json`
- `tether_provenance_round_trips`
- `tether_binding_serializes_snake_case`
- `orb_tether_bound_event_round_trips_through_json`
- `node_merged_event_round_trips_through_json`
- `rebuild_episodes_ignores_orb_tether_bound_events` (replay
  invariant: state is identical with/without the variant in the log)
- `rebuild_episodes_ignores_node_merged_events` (same invariant)
- `from_episodes_emits_node_merged_when_existing_node_extended` —
  the runtime change. The MemoryState path must produce a
  `NodeMerged` event whenever `insert_node` finds an existing id.
- `from_episodes_does_not_emit_node_merged_for_first_insert` —
  negative case; only second-and-later inserts merge.

---

## Gates

1. `cargo build --workspace` — clean
2. `cargo test --workspace --all-features` — no regressions; +≥10
3. `bash scripts/doctrine_check.sh` — OK
4. `./target/release/luna runtime scenario joe_chris_francois.json` —
   18 memory checks, behavior unchanged

---

## Decision needed

Before implementation: answer Q1, Q2, Q3, Q4. Recommended path is
A on each (land event variant now, bundle R-005 closure, id-only
TetherProvenance, clamp weight at construction). That keeps tethers
landing on a clean substrate and hands pr-1.6 a producer-only job.
