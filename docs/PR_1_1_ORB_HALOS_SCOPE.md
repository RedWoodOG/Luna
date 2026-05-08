# pr-1.1/orb-halos — scope

**Status.** Draft for review. Not yet implemented.
**Predecessor.** `pr-1.0/orb-schema` (commit `8404282`) — landed the
`luna-orbs` crate with `MemoryOrb`, `OrbId`, `OrbKind`, `OrbTether` as
types-only scaffolding. `OrbCore` and `HaloRef` are currently empty
`#[non_exhaustive]` skeletons. This slice fills them in.
**Successor.** `pr-1.2/orb-tethers` (typed lattice edges between orbs).

---

## What this slice is

Fill in the **two empty skeletons** from pr-1.0:

- **`OrbCore`** — the dense, condensed representation. The "gist." What
  survives consolidation. Compact enough to carry in a recall hit
  without dragging the halo with it.
- **`HaloRef`** — the windowed reference back into the event log.
  The "receipts." Lets a recall hit say *here are the source events
  this orb was built from* without copying them.

Doctrine line: **the core is the gist; the halo is the receipts.**

This slice is still **types-only and additive**. No runtime wiring.
No consolidation engine. No persistence. The types must compose
cleanly with what comes next:

- `pr-1.4/hybrid-recall` — must be able to score against an `OrbCore`
  and surface a `HaloRef` for citation.
- `pr-1.6/consolidate-engine` — must be able to *produce* an
  `OrbCore` and `HaloRef` from a window of events.
- `pr-1.9/attestation` — must be able to verify an `OrbCore` is
  reproducible from its `HaloRef`'s event range.

If a field doesn't compose with at least one of those, it doesn't
belong here.

---

## Proposed shape — `OrbCore`

```rust
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OrbCore {
    /// Natural-language gist. Empty until pr-1.6 populates it.
    /// Bounded length will be enforced by the consolidation engine,
    /// not here.
    pub summary: String,

    /// Atomic facts that survived consolidation. Each carries the
    /// event ids that produced it, so it remains citable.
    pub key_facts: Vec<KeyFact>,

    /// Confidence horizon for the orb as a whole. Maps from the
    /// existing AssertionConfidenceTier per memory_current_state.md:124.
    pub confidence_horizon: ConfidenceHorizon,

    /// Signals — directional cues, preferences, recurring patterns.
    /// Reuses the existing luna_core::Signal type.
    pub signals: Vec<luna_core::Signal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyFact {
    pub statement: KeyFactStatement,            // newtype, non-empty
    pub source_event_ids: Vec<uuid::Uuid>,      // ≥1 enforced
    pub confidence: f32,                        // [0.0, 1.0]
    pub last_reinforced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyFactStatement(String);  // strict, like OrbId / RecallReason

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceHorizon {
    #[default]
    Unconfirmed,
    Inferred,
    Confirmed,
}
```

**Why these fields:**

- `summary` carries the gist. Empty in pr-1.1 because consolidation
  doesn't run yet. Type signature reserves the slot.
- `key_facts` is the *citable* surface. Every fact carries its source
  event ids so attestation (pr-1.9) can replay them and verify.
  Constructor enforces ≥1 source event — empty provenance is forbidden,
  same shape as the slice 3c plan for `MemoryProvenance`.
- `confidence_horizon` exists because `memory_current_state.md:124`
  already maps this from `AssertionConfidenceTier`. Re-deriving here
  is cheap and surfaces the orb's overall belief level.
- `signals` reuses `luna_core::Signal` — no new type, no drift.
- **Deliberately omitted** for pr-1.1: `claims` and `decisions` as
  separate types. See open question Q1.

**Strict construction:**

- `KeyFactStatement::new("")` and `::new("   ")` return `Err`. Same
  pattern as `OrbId` and `RecallReason`.
- `KeyFact::new(stmt, sources, ..)` rejects empty `sources`. The type
  signature enforces "no fact without a citation."

---

## Proposed shape — `HaloRef`

```rust
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HaloRef {
    /// Bookend: oldest event in the halo window.
    pub first_event_id: uuid::Uuid,
    /// Bookend: newest event in the halo window.
    pub last_event_id: uuid::Uuid,
    /// Invariant: number of events in [first, last] that belong to this orb.
    /// Verifiable at attestation time.
    pub event_count: u32,
    /// Inclusive time window. Both endpoints are real event timestamps,
    /// not Utc::now() (R-002 closure pattern).
    pub time_range: HaloTimeRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HaloTimeRange {
    pub from: DateTime<Utc>,  // inclusive
    pub to: DateTime<Utc>,    // inclusive, ≥ from
}
```

**Why these fields:**

- `first_event_id` / `last_event_id` are bookends. They identify the
  window without enumerating every event. The full enumeration lives
  in the event log itself; halo just points at it.
- `event_count` is an **invariant** for verification: if a replay
  finds a different number of events that belong to this orb in
  `[first, last]`, attestation fails. It's a cheap consistency check.
- `time_range` is the time-bounded view. `pr-1.4/hybrid-recall` uses it
  for time-decay weighting; `pr-1.9/attestation` uses it to bound replay.
- **Deliberately omitted:** explicit `event_ids: Vec<Uuid>`. See Q2.

**Strict construction:**

- `HaloRef::new(...)` rejects `to < from`.
- `HaloTimeRange::new(...)` rejects inverted ranges.

---

## What this slice does NOT do

- Does not wire `OrbCore` / `HaloRef` into `RuntimeSession`. Runtime
  still operates on the legacy shape. Wiring is `pr-1.4`.
- Does not write `MemoryOrb` events to the event log. That's `pr-1.6`.
- Does not implement consolidation logic. The fields are *defined*
  here so that pr-1.6 has a target to fill.
- Does not change behavior of any existing scenario. `joe_chris_francois`
  must still pass with 18 memory checks.

---

## Open questions for review

### Q1. Do `claims` and `decisions` get their own types in pr-1.1, or do we wait?

- **Option A (recommended):** Don't add them yet. `KeyFact` is
  sufficient as the citable atomic unit for pr-1.1. `Claim` and
  `Decision` show up later when consolidation needs to distinguish
  *belief* (claim) from *committed choice* (decision). Defer the
  shape until we have a consolidation use case that needs the
  distinction.
- **Option B:** Add `Claim` and `Decision` as types now, even if
  empty `Vec`. Mirrors what I said in chat. Risk: we invent a shape
  before there's a consumer, and change it under pressure later.
- **Option C:** Subsume claims/decisions into a single
  `Vec<OrbAssertion>` that reuses `StructuredAssertion`. Less new
  vocabulary; relies on the existing assertion type to carry both
  belief-grade and decision-grade.

### Q2. Does `HaloRef` carry explicit `event_ids: Vec<Uuid>` or just bookends?

- **Option A (recommended):** Bookends + count. The full enumeration
  is a query against the event log, not data we duplicate. Cheap to
  store; doctrine-aligned (event log is the source of truth).
- **Option B:** Explicit `Vec<Uuid>` of every event in the halo.
  Self-contained, no query needed. Cost: O(n) per orb storage, and
  duplicates information already in the log.
- **Option C:** Both — bookends as a fast path, optional explicit
  list as an attestation aid.

### Q3. Does this slice introduce an `EventId(Uuid)` newtype?

- **Option A (recommended):** No. `Uuid` is fine for now.
  `luna_core::EventEnvelope.event_id` is already `Uuid`. A newtype
  would be a separate, broader doctrine slice (slice 4+ territory)
  and shouldn't be smuggled in through pr-1.1.
- **Option B:** Yes. Define `pub struct EventId(Uuid)` in `luna-core`,
  migrate envelope and halo at the same time. Bigger surface, one
  more thing to land cleanly.

### Q4. Does pr-1.1 close R-003 ("Assertion fine-capture outside event log")?

`risk_register.md:82` lists the target as "pr-1.0 or pr-1.1, alongside
the orb halo schema, since halos will reference these events directly."

- **Option A (recommended):** No, keep R-003 separate. pr-1.1 lands
  the *types* halos need. R-003 is about *emitting events* from a
  runtime path that currently bypasses the log. Different surface,
  different test, different risk profile. Bundle = bigger blast
  radius. Land R-003 as its own pr-1.1a follow-up immediately after.
- **Option B:** Yes, bundle. Get the halos and the events they cite
  in the same commit.

---

## Tests planned (≥10)

- `orb_core_default_is_empty_and_unconfirmed`
- `orb_core_round_trips_through_json`
- `key_fact_statement_rejects_empty_and_whitespace`
- `key_fact_rejects_empty_source_event_ids`
- `key_fact_clamps_confidence_to_unit_interval` (or rejects out-of-range — TBD)
- `confidence_horizon_serializes_snake_case`
- `halo_ref_rejects_inverted_time_range`
- `halo_ref_round_trips_through_json`
- `halo_time_range_rejects_to_before_from`
- `memory_orb_with_populated_core_and_halo_round_trips`
- `memory_orb_with_default_core_still_round_trips` (forward-compat
  with pr-1.0 envelopes)

---

## Gates

Same as pr-1.0:

1. `cargo build -p luna-orbs` — clean
2. `cargo test -p luna-orbs` — new tests + 12 existing
3. `cargo test --workspace --all-features` — no regressions
4. `bash scripts/doctrine_check.sh` — OK
5. `./target/release/luna runtime scenario joe_chris_francois.json` —
   18 memory checks, behavior unchanged
6. `Cargo.toml` of `luna-orbs` may grow only `chrono` / `serde` /
   `uuid` / `luna-core` deps (already there). No new transitive deps.

---

## Decision needed

Before implementation: answer Q1, Q2, Q3, Q4. The recommended path is
A on each (defer claims/decisions, bookend halos, no `EventId` newtype,
keep R-003 separate). That keeps pr-1.1 small, focused, and low-blast.
