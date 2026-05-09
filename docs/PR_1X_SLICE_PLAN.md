# pr-1.x slice plan — orb-network rebuild + council-packet integration

**Status.** Active plan. Updated when slices land or open questions resolve.
**Last updated against head.** `f9e6174` (pr-1.2 fixup: multiset NodeMerged diff).
**Predecessor doc.** `docs/LUNA_MEMORY_MILESTONE_ROADMAP.md` (canonical roadmap).
**Sister docs.** `docs/PR_1_1_ORB_HALOS_SCOPE.md`, `docs/PR_1_2_ORB_TETHERS_SCOPE.md`.

---

## Why this doc exists

Two roadmaps were in play after pr-1.2 landed:

1. **The existing pr-1.x slice cadence** — small, scope-doc-first slices,
   doctrine-gated, one risk-register item closed at a time. Reached pr-1.2.
   Next was pr-1.3/vector-field.
2. **The council packet** (`Luna: Proving & Hardening the Better LLM Memory`)
   — Stages A–F covering temporal windowing, supersession, intake policy,
   5-tier confidence, activation engine, TCCBDS benchmark, PyO3/MCP/LangGraph
   adapter, paper, and post-v1.0 branching/merging/attestation hardening.

**Decision (recorded).** Option A: continue the pr-1.x cadence, absorb council
items as later slices. Council Stage F's branching/merging/attestation already
exists as pr-1.7/1.8/1.9 on the canonical roadmap and is not duplicated.
Council Stage A is **net-new** work — temporal windowing, supersession, intake
policy runtime, and the 5-tier confidence migration are not in the existing
roadmap and must be added as their own slices.

This doc names the slice sequence so a future session (or a parallel
contributor) can pick up at the right point without re-deriving the order.

---

## Slice sequence

Each row is one slice. Each slice follows the established cadence:

1. Write `docs/PR_X_Y_SLICE_NAME_SCOPE.md` first.
2. Resolve the open questions in that doc (`Q1..Qn` with recommendation A).
3. Implement. Add ≥10 tests. Run all four gates.
4. Commit with the `pr-X.Y/slice-name` prefix; close any risk-register items.

| Slice | Status | Source | One-liner |
|-------|--------|--------|-----------|
| pr-1.0/orb-schema | landed `8404282` | roadmap | Introduce `luna-orbs`, types only. |
| pr-1.0a/event-log-hardening | landed `16e1736` | roadmap | Close R-001, R-002. R-009 gate. |
| pr-1.1/orb-halos | landed `e2e1585` | roadmap | Fill OrbCore + HaloRef. |
| pr-1.1a/raw-observation-capture | landed `5e7f682` | roadmap | Close R-003. |
| pr-1.2/orb-tethers | landed `c98519e` + `f9e6174` | roadmap | Tether provenance + R-005 closure. |
| **pr-1.3/vector-field** | **next** | roadmap | **Semantic vector substrate on orbs.** |
| pr-1.4/hybrid-recall | planned | roadmap | Orbs × vector field × tethers in recall scoring. |
| pr-1.5/temporal-windowing | planned | council Stage A.1 | `valid_from` / `valid_to` on `StructuredAssertion`. |
| pr-1.5a/supersession-chains | planned | council Stage A.2 | `AssertionSupersedes` event variant + chain query. |
| pr-1.5b/intake-policy-runtime | planned | council Stage A.3 | The 6 policy actions, runtime-enforced. |
| pr-1.5c/confidence-5-tier | planned | council Stage A.4 | 3→5 tier migration + discretization function + fixture migration. |
| pr-1.5d/replay-cache-lock | planned | council Stage A.6 | `RawObservationCaptured` mandatory on replay; no live LLM fallback. |
| pr-1.6/activation-engine | planned | council Stage B + roadmap | 11-term activation formula on the orb graph; bounded working memory; calibration loop. |
| pr-1.6a/consolidate-engine | planned | roadmap | Producer for `OrbTetherBound` events from runtime co-activation. |
| pr-1.7/branching | planned | roadmap (= council Stage F.1) | Speculative memory exploration. |
| pr-1.8/merging | planned | roadmap (= council Stage F.2) | Branch reconciliation. |
| pr-1.9/attestation | planned | roadmap (= council Stage F.4) | Signed memory state, hash-committed to log. |

**Out of band of this slice list:**

- **Council Stage C** (TCCBDS benchmark + competitor adapters) runs as a
  parallel track once the engine is settled enough to evaluate. Likely starts
  during pr-1.6 or pr-1.6a. Tracked separately because it's a benchmark
  build-out and competitor-adapter-writing exercise, not a Luna code slice.
- **Council Stage D** (PyO3 / MCP / LangGraph adapter) — best after pr-1.9
  lands, so the engine surface is stable before exposing it. Will get its own
  slice plan doc when we reach it.
- **Council Stage E** (technical report, open-source release, v1.0 acceptance
  test pass) — final stage, assumes everything above lands.

---

## Per-slice open questions to resolve before implementation

Listed for the next four slices. Slices further out get their open-questions
section when the prior slice lands and the surface is concrete.

### pr-1.3/vector-field — open questions

**Q1. Vector storage shape.** Embed at the orb level (per-orb dense vector
summarizing the core), at the assertion level (per-assertion vector), at the
event level (per-turn vector), or all three?

- Recommendation pending. Per-orb is cheapest; per-event is most flexible
  but multiplies storage cost. Likely answer: per-orb + per-event-on-demand
  (cached in `RawObservationCaptured`-style events).

**Q2. Embedding source.** Local model (`all-MiniLM-L6-v2` or similar via
`candle` / `ort`) for replay determinism, or an external API call cached in
the event log?

- Local for determinism; the council packet's recommendation aligns. Open
  question is which crate (`candle` is pure Rust, `ort` wraps ONNX) and
  whether we vendor weights or fetch on first run.

**Q3. Wire format.** Vectors on disk as JSON arrays (simple, slow), as a
sidecar binary file referenced by event id (fast, more moving parts), or
as base64-encoded blobs inside the JSONL log (compact, opaque)?

- Recommendation pending. JSONL transparency is doctrine-aligned; sidecar
  files break the "event log is truth" line. Lean toward JSON arrays for
  pr-1.3 and revisit if storage becomes a measurable constraint.

**Q4. Scope boundary.** What pr-1.3 explicitly does NOT do:

- Recall integration is pr-1.4's job (hybrid scoring against vectors).
- Consolidation that produces a vector embedding of an orb's halo is
  pr-1.6's job (consolidate-engine slice).
- Cross-orb similarity-graph construction is pr-1.4 / pr-1.6 territory.
- pr-1.3 lands the substrate only: a `Vector` type, the embed-on-write
  hook, and a stable serialization. No reads beyond round-trip tests.

### pr-1.4/hybrid-recall — open questions

(Sketched; full open-questions section lands when pr-1.3 closes.)

- Q1: How are vector-similarity, tether-traversal, and entity-group recall
  combined into a single ranked list? Weighted sum, cascade, or rerank?
- Q2: Does `RecallReason` carry the dominant signal that produced the hit
  (semantic / tether / entity), or is the reason narrative-only?

### pr-1.5/temporal-windowing — open questions

(Sketched; full open-questions section lands when pr-1.4 closes.)

- Q1: `valid_from` / `valid_to` on `StructuredAssertion` directly, or on
  a wrapper type (`TemporalAssertion`) so legacy assertions don't gain
  a field that's `None` in 99% of cases?
- Q2: When is `valid_to` set? On supersession only, or also on explicit
  user retraction events?
- Q3: Does the activation formula's `recency` term use event time or
  `valid_from`? They differ for backfilled facts.

### pr-1.5a/supersession-chains — open questions

(Sketched.)

- Q1: New event variant `AssertionSuperseded { old_id, new_id, reason }`,
  or carry the `supersedes: Option<Uuid>` directly on
  `AssertionExtracted` / `EpisodeCreated`?
- Q2: Does `MemoryState::from_episodes` filter superseded assertions out
  of `claims`, or surface them with a `superseded_by: Option<Uuid>` flag
  so consumers can ask point-in-time questions?

---

## Cadence rules (carry-forward from pr-1.0 → pr-1.2)

- **Scope-doc first.** Every slice gets `docs/PR_X_Y_SLICE_NAME_SCOPE.md`
  committed before implementation. The doc states what changes, what does not
  change, the open questions, and the recommended path on each.
- **Open questions get answered before code.** "Defer to recommendations" is
  a valid answer; silent assumptions are not.
- **Risk register closure.** When a slice closes a risk-register item (R-001
  through R-010), the slice doc names which one and the closure note in
  `docs/risk_register.md` is updated in the same commit.
- **Four gates per slice.**
  1. `cargo build --workspace` clean.
  2. `cargo test --workspace --all-features` no regressions; ≥10 new tests.
  3. `bash scripts/doctrine_check.sh` OK.
  4. `./target/release/luna runtime scenario joe_chris_francois.json` PASS
     (18 memory checks unchanged).
- **`#[non_exhaustive]` on extension points.** Any struct that a later slice
  will add fields to should be `#[non_exhaustive]` from the start.
- **Strict-on-construction newtypes.** Any new string-typed identifier or
  domain value (e.g. `OrbId`, `KeyFactStatement`, `RecallReason`) gets a
  `new()` that rejects empty / whitespace and a transparent serde shape.
- **Audit-only events stay informational.** New event variants that don't
  derive episode state (e.g. `RawObservationCaptured`, `OrbTetherBound`,
  `NodeMerged`) get added to `luna-store::rebuild_episodes`'s no-op arm
  with a regression test that proves replay is byte-identical with or
  without the variant in the log.

---

## Risk register status (as of `f9e6174`)

| ID    | Status | Closed in | Note |
|-------|--------|-----------|------|
| R-001 | closed | pr-1.0a   | decay applied at event time, not wall clock |
| R-002 | closed | pr-1.0a   | `Utc::now()` no longer leaks into replay paths |
| R-003 | closed | pr-1.1a   | `RawObservationCaptured` event captures pre-norm extractor output |
| R-004 | open   | —         | (per `docs/risk_register.md`) |
| R-005 | closed | pr-1.2    | `NodeMerged` event makes silent merges visible |
| R-006 | open   | —         | |
| R-007 | open   | —         | |
| R-008 | open   | —         | |
| R-009 | gate   | pr-1.0a   | replay-determinism gate added; ongoing |
| R-010 | open   | —         | |

A slice that closes a risk item updates this table along with the closure
note in `docs/risk_register.md`.

---

## Council-packet integration notes

The council's packet identified five things the existing roadmap did not name
explicitly. Each maps to a slice in the table above:

1. **Temporal windowing on assertions** → pr-1.5/temporal-windowing.
2. **Supersession chains** → pr-1.5a/supersession-chains.
3. **Intake policy runtime** → pr-1.5b/intake-policy-runtime.
4. **5-tier confidence** → pr-1.5c/confidence-5-tier.
5. **Replay-cache lock (no live LLM during replay)** → pr-1.5d/replay-cache-lock.

The packet also identified two things the existing roadmap already covers:

1. **Activation engine with the 11-term formula** → pr-1.6/activation-engine
   (already on the roadmap as part of post-vector-field hybrid recall +
   working-memory budget; the council packet's contribution is the
   calibration loop).
2. **Branching / merging / attestation** → pr-1.7 / pr-1.8 / pr-1.9
   (already named; council Stage F was redundant).

The packet also raised TCCBDS as a competitive benchmark and PyO3 / MCP /
LangGraph as the adoption surface. Both are tracked outside this slice list
because they're not Luna engine slices — they're a benchmark build-out and a
binding layer respectively. They get their own plan docs when reached.

---

## What this doc does NOT do

- It does not commit Luna to the council packet's exact week-count timeline.
  Slice cadence is driven by gates passing, not by a calendar.
- It does not modify `docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`. The roadmap
  remains canonical for stage definitions; this doc names the slice sequence
  underneath.
- It does not pre-commit to the open-question recommendations on slices later
  than pr-1.3. Those resolve when the prior slice lands.
- It does not absorb council Stage E (publication). That's a v1.0 milestone
  outside the slice cadence.

---

## Immediate next action

Draft `docs/PR_1_3_VECTOR_FIELD_SCOPE.md` with the four open questions in
§"pr-1.3/vector-field — open questions" above as Q1–Q4, each with a
recommended answer. Same review pattern as pr-1.1 and pr-1.2: scope doc
committed first, user signs off (or "defers to recommendations"), then
implementation lands behind the four gates.
