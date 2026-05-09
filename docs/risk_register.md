# risk_register.md

Risks identified during the audit (`docs/memory_current_state.md`). Each risk has an ID, severity, location, doctrine impact, and a mitigation tied to a specific PR. Update this file when a risk is closed.

## Summary

| ID | Severity | Title | Location | Status | Target |
|----|----------|-------|----------|--------|--------|
| R-001 | substrate **HIGH** / current-behavior MEDIUM | Confidence penalty without logged event | `luna-store:108` | **closed** `16e1736` | pr-1.0a |
| R-002 | medium | `Utc::now()` in replay creates time drift | `luna-store:109` | **closed** `16e1736` | pr-1.0a |
| R-003 | medium | Assertion fine-capture outside event log | `luna-runtime:722-728` | **closed** `pr-1.1a` | pr-1.1a |
| R-004 | medium | Hardcoded assertion-intent mapping | `luna-tcf:201-223` | open | pr-1.4 |
| R-005 | medium | MemoryMap node/edge merge is silent | `luna-runtime:686-708` | **closed `pr-1.2`** | pr-1.2 |
| R-006 | low | Question proposal rules scattered inline | `luna-runtime:1632-1677` | open | post-pr-1.0 |
| R-007 | low | RootOrb override policy never exercised | `luna-core:557-632` | open | pr-1.0 |
| R-008 | low | `AssertionCorrected`/`ContradictionDetected` defined but unused | enum variants | open | pr-1.6 |
| R-009 | medium | Determinism risk during event-schema migration | process-level | **gate landed** `16e1736` | every PR |
| R-010 | low | RootOrb principles not used in scoring or activation | `luna-runtime:593` | open | pr-1.4 (decision) |

## Detailed entries

### R-001 — Confidence penalty without logged event (substrate HIGH / current-behavior MEDIUM)

**Severity framing.** The doctrine violation is in the *substrate* (replay code on a hot path), so substrate severity is HIGH. But `ContradictionDetected` is currently emitted only by tests — no production extraction path produces it from natural input — so today's *exercised* behavior is unaffected. Current-behavior severity is therefore MEDIUM. The original framing implied immediate bleeding; this entry is corrected to be honest about that. The fix is still the right fix; the urgency was inflated.

**Location.** `crates/luna-store/src/lib.rs:108`.

**Description.** When a `ContradictionDetected` event is replayed, `rebuild_episodes()` mutates `episode.confidence -= 0.22` directly, without emitting a separate event that records the adjustment. The penalty value (`0.22`) is hardcoded in the replay logic.

**Doctrine impact.** Direct violation of "every transformation is itself a logged event." The system's confidence in a belief drops, and there is no event in the log that says *why by how much*. A future audit asking "why is this episode at 0.4 confidence?" can only point to the contradiction event; the penalty itself is invisible.

**Why it matters for the rebuild.** The consolidation engine (`pr-1.6`) is supposed to emit a `consolidation_delta` for every change to memory state. If the existing replay logic continues to silently adjust confidence, the consolidation engine inherits a substrate where some belief changes don't show up in deltas. The whole audit chain becomes leaky.

**Mitigation.** Replace the inline penalty with a logged adjustment event. Two acceptable approaches:

1. Add a new event variant (`ConfidenceAdjusted { episode_id, delta, reason, source_event_id }`) and emit it during contradiction handling. Replay reads the explicit delta.
2. Extend `ContradictionDetected` to carry the resulting confidence delta as a payload field. Replay applies the carried delta, not a hardcoded constant.

Approach (1) is cleaner long-term. Approach (2) is a smaller diff. Either way, the magic number `0.22` leaves the source.

**Target.** Must be fixed before `pr-1.6/consolidate-engine`. Ideally before `pr-1.0/orb-schema` so the orb schema lands on a clean substrate.

**Closure note (`16e1736`, `pr-1.0a/event-log-hardening`).** Approach 2 chosen. `ContradictionDetected` and `AssertionCorrected` now carry `confidence_delta: f32`. Replay applies the carried value via `episode.confidence + payload.confidence_delta`. Hardcoded `-0.22` removed from `luna-store`. Backward compat preserved via `#[serde(default = "legacy_contradiction_delta")]` / `legacy_correction_delta`, both documented in `luna-core` as "historical default for events emitted before pr-1.0a." Locked by test `tests::contradiction_uses_payload_delta_not_constant` (proves payload value is honored) and `tests::legacy_contradiction_event_replays_with_default_delta` (proves legacy events still replay identically).

---

### R-002 — `Utc::now()` in replay (medium)

**Location.** `crates/luna-store/src/lib.rs:109`.

**Description.** During `rebuild_episodes()`, `episode.updated_at` is set to `Utc::now()`, which is the *processing* time, not the *event* time. Two replays of the same log on different clocks produce different `updated_at` values.

**Doctrine impact.** Determinism is implicit in event-sourcing. Replay should be a pure function from log to state. Using wall-clock time during replay breaks that property.

**Why it matters for the rebuild.** Determinism tests would catch this if they checked `updated_at`. They currently don't (the test on `luna-events` checks idempotence of append, not field-by-field equality of rebuilt state). When `pr-1.0` adds orb-aware tests, this drift will surface as a flaky equality.

**Mitigation.** Set `episode.updated_at = event.timestamp`. One-line change. Add a determinism test in `luna-store` that rebuilds twice and asserts byte-for-byte equality of the rebuilt vector.

**Target.** Pre-`pr-1.0`. Trivial fix.

**Closure note (`16e1736`, `pr-1.0a/event-log-hardening`).** Replay now uses `event.timestamp` for `episode.updated_at` in both `ContradictionDetected` and `AssertionCorrected` arms. The unused `chrono::Utc` import was removed from `luna-store`. Locked by test `tests::updated_at_uses_event_timestamp_not_now` (sleeps 50ms between two replays and asserts identical `updated_at`).

---

### R-003 — Assertion fine-capture outside the event log (medium)

**Location.** `crates/luna-runtime/src/lib.rs:676-712`.

**Description.** `apply_runtime_fine_capture()` mutates the in-memory `CognitiveObservation` *before* logging — performing person→identity migration, role-rename, dedup, and unanchored-assertion pruning. The logged form is the post-normalization form. The raw extracted form is unrecoverable.

**Doctrine impact.** Borderline. The doctrine says memory state must derive from the log; it doesn't strictly require the *raw extractor output* to be in the log. But "every transformation is itself a logged event" is a stronger claim than "every state-of-the-graph is in the log." The normalization is a transformation; it is currently invisible.

**Why it matters for the rebuild.** A future operator audit asking "what did the extractor actually emit before normalization?" cannot be answered from the log. If extractor behavior changes (LLM updates, prompt edits), we cannot diff "extractor changed" from "normalization changed." This is the kind of opacity that bites three months in.

**Mitigation.** Two-event logging:

1. `AssertionExtractedRaw { observation }` — the unmodified extractor output.
2. `AssertionNormalized { from: AssertionExtractedRaw.event_id, deltas: [...] }` — the normalization, with explicit per-rule deltas.

Slightly more storage, but the audit chain becomes fully reconstructable. Alternative: keep the single event but expand its payload to include both `raw` and `normalized` fields plus a list of applied normalization rules.

**Target.** `pr-1.0` or `pr-1.1` (alongside the orb halo schema, since halos will reference these events directly).

**Closure note (`pr-1.1a/raw-observation-capture`).** Smaller alternative
chosen: a single new event variant
`LunaEvent::RawObservationCaptured { observation }` is logged
**before** `apply_runtime_fine_capture` mutates the observation. Replay
treats this event as informational — `luna-store::rebuild_episodes`
includes it in its no-op arm alongside `TurnObserved` and
`AssertionExtracted`. The post-normalization assertions still flow
through `AssertionExtracted` / `EpisodeCreated` / `EpisodeReinforced`,
which remain the source of truth for episode state. Audit chain is now
reconstructable: `RawObservationCaptured` carries the pre-normalization
observation, and the corresponding `AssertionExtracted` events carry
the post-normalization form. Diffing the two answers "extractor changed"
vs "normalization changed."

The richer two-event option (`AssertionNormalized` with explicit
per-rule deltas) was deferred — `apply_runtime_fine_capture` would have
to be refactored to be reflective, which is a much larger surface than
R-003 warrants. If a future audit needs per-rule deltas, the raw +
post-norm pair is enough to recompute them externally.

Locked by three tests in `luna-runtime`:
- `process_turn_emits_one_raw_observation_captured_event_per_turn` —
  every turn produces exactly one such event, observation matches the
  extractor's output (modulo `turn_id` re-derivation).
- `raw_observation_captured_uses_turn_event_time` — R-002 pattern
  honored (event timestamp is the turn's event-time, not `Utc::now()`).
- `rebuild_episodes_ignores_raw_observation_captured_events` —
  doctrine invariant: rebuild output is byte-identical with or without
  the audit event in the log. If this ever fails, replay has started
  silently deriving state from the audit record, defeating its purpose.

---

### R-004 — Hardcoded assertion-intent mapping (medium)

**Location.** `crates/luna-tcf/src/lib.rs:201-223`.

**Description.** `assertion_intent_fit()` is a hand-coded match expression mapping query intents to assertion `domain`/`kind` pairs (e.g. `"identity.profession.query"` ↔ `domain == "identity" && kind == "profession"`). Adding a new domain/kind requires a code change.

**Doctrine impact.** No direct violation, but the doctrine prefers behavior to come from "reusable intake, ontology, graph, activation, working-set, and response-planning mechanisms." Hardcoded match arms aren't reusable; they're scripted.

**Why it matters for the rebuild.** When the vector field lands (`pr-1.3`) and the orb network is fully populated, the right intent → orb resonance signal will come from vector similarity + tether traversal, not from a lookup table. This function will be re-evaluated. Until then, it is a working scoring component.

**Mitigation.** Three options:

1. Leave it for now; replace wholesale when hybrid recall lands in `pr-1.4`.
2. Refactor to load the intent → domain/kind map from RootOrb principles.
3. Build a small registry struct (`IntentMappingRegistry`) and seed it from the existing match arms.

Option (1) is the path of least disturbance and matches the planned rebuild order.

**Target.** Subsumed by `pr-1.4/hybrid-recall`.

---

### R-005 — MemoryMap node/edge merge is silent (medium) — **closed `pr-1.2`**

**Location.** `crates/luna-runtime/src/lib.rs:686-708` (`insert_node`).

**Description.** When `MemoryState::from_episodes()` builds the MemoryMap, `insert_node()` updates an existing node's confidence tier, density, and provenance in place if the node id already exists. No event was logged for the merge.

**Doctrine impact.** The merge happens *during graph construction*, which is itself derived from events. So strictly the doctrine "event log is truth" still holds — the merged node is fully reconstructable from the events. But the *fact of merge*, the moment two pieces of evidence collapsed into one node, wasn't visible without rebuilding and diffing.

**Why it mattered for the rebuild.** Tethers (`pr-1.2`) made this worse: a tether is a typed edge derived from a bind-event in the log. If node merges happen during graph construction without their own events, tether attribution gets harder — which event "caused" the tether between merged nodes?

**Closure (pr-1.2).** Three changes land:

1. `LunaEvent::NodeMerged` variant in `luna-core` carries `{ node_id, merged_density_delta, previous_confidence_tier, new_confidence_tier, merged_provenance_count }` — *what changed*, in the audit log.
2. `MemoryState::from_episodes_with_merges` returns `(Self, Vec<NodeMerged>)` so the merge moment is observable. The pre-existing `from_episodes` becomes a thin wrapper that discards merges (preserves call-site stability for `inspect`).
3. `process_turn` diffs the post-turn merge set against `prior_merged_ids` (computed from `previous_episodes`) and emits one `NodeMerged` event per *fresh* merge — so per-turn audit isn't duplicated by re-derivation of historical merges. `luna-store::rebuild_episodes` ignores the variant (replay invariant: state is identical with/without `NodeMerged` in the log).

**Alternative considered.** Per scope-doc Q2, a "merge ledger" inside `MemoryState` queryable at audit time. Rejected: the audit log is the single source of truth; introducing a parallel ledger means two places to keep in sync.

**Locking tests.** `from_episodes_emits_node_merged_when_existing_node_extended`, `from_episodes_does_not_emit_node_merged_for_first_insert`, `rebuild_episodes_ignores_node_merged_events`, `process_turn_emits_node_merged_only_for_fresh_merges`.

**Target.** `pr-1.2/orb-tethers` — landed.

---

### R-006 — Question proposal rules scattered inline (low)

**Location.** `crates/luna-runtime/src/lib.rs:1632-1677`.

**Description.** Heuristic rules (`mentions_job`, `mentions_ambiguous_they`, etc.) live as inline functions, each producing a `ProposedQuestion`. No registry, no priority table.

**Doctrine impact.** None directly. The rules emit questions transparently and the questions are logged.

**Why it matters for the rebuild.** Once the orb network exists, question proposal should be a function of orb state ("this orb has open questions") rather than turn-level heuristics. Refactoring to a registry now is wasted work; better to deprecate the heuristic path when the orb network can answer "what should I ask?" directly.

**Mitigation.** Defer. Document that the heuristic path is the legacy path; new questions should come from orb-level open-question fields once `pr-1.0` lands.

**Target.** Post-`pr-1.0`.

---

### R-007 — RootOrb override policy never exercised (low)

**Location.** `crates/luna-core/src/lib.rs:557-632`.

**Description.** `RootOrb::override_policy: SystemVersioned` is set but no code path checks it before applying RootOrb principles. There is also no test for "RootOrb v2 supersedes v1."

**Doctrine impact.** None — current default is `SystemVersioned` which permits override. The risk is silent: when we *do* want to bump the RootOrb version, we may discover the policy is decorative.

**Mitigation.** When `pr-1.0` generalizes RootOrb to a species, add a version-bump unit test that exercises `SystemVersioned` semantics. Adds confidence; small effort.

**Target.** `pr-1.0`.

---

### R-008 — Defined-but-unused event variants (low)

**Location.** `LunaEvent::AssertionCorrected`, `LunaEvent::ContradictionDetected`.

**Description.** Variants exist in the event enum but no production runtime code path emits `AssertionCorrected`. `ContradictionDetected` is emitted in tests but no production extraction path produces it from natural input.

**Doctrine impact.** None directly. Unused variants are dead weight, not violations.

**Why it matters for the rebuild.** The consolidation engine (`pr-1.6`) needs correction/contradiction signals to drive arbitration logic. If those events are never produced by the runtime, the consolidation engine has no input. This is a *gap*, not a *bug* — but the gap needs filling before consolidation does anything interesting.

**Mitigation.** `pr-1.6` includes a `contradiction_detector` task that emits `ContradictionDetected` when two assertions for the same entity disagree. Until then, document that consolidation triggers are limited to the `halo_size` and `periodic` paths.

**Target.** `pr-1.6/consolidate-engine`.

---

### R-009 — Determinism risk during event-schema migration (medium)

**Location.** Process-level risk, no single file.

**Description.** Each PR in the memory rebuild touches the event log: new event kinds, new payload fields, new constraints. Replay must remain deterministic across these changes. Old events must remain replayable; new events must not break old replay paths.

**Doctrine impact.** "Event log is truth" requires that the log + the schema-as-of-event-time fully determine the state. Breaking determinism breaks the doctrine.

**Why it matters for the rebuild.** ~12 PRs across six phases. Every one risks introducing a non-deterministic replay if not carefully gated. The existing test in `luna-events` (100-event idempotence) is too narrow to catch field-level drift.

**Mitigation.** Per-PR gates:

- Every PR that adds an event kind must add a fixture-replay test (golden-file rebuild).
- Every PR that changes a payload must include a migration plan (forward-compatible parsing or explicit version bump).
- The `luna-store` determinism test must be expanded to byte-for-byte rebuilt-state equality, not just episode count.

**Target.** Every PR in `pr-1.0` through `pr-1.13`. Gate condition.

**Gate-landed note (`16e1736`, `pr-1.0a/event-log-hardening`).** The byte-for-byte determinism test now exists at `luna-store/src/lib.rs::tests::rebuild_is_deterministic_byte_for_byte`. It builds a fixture covering create/reinforce/contradict, calls `rebuild_episodes` twice with a 20ms sleep between calls, and asserts both `serde_json::to_vec(...)` byte-equality and `PartialEq` agreement. Future PRs that touch replay must keep this test passing — that is the gate.

---

### R-010 — RootOrb principles not used in scoring or activation (low)

**Location.** `luna-runtime:593` (seed) but nowhere downstream.

**Description.** RootOrb's 7 principles are seeded into the MemoryMap as `Principle` nodes connected by `DefinesRule` edges. Activation, recall scoring, and question proposal don't read them.

**Doctrine impact.** None. RootOrb compliance is meant to shape behavior, but the current implementation places that influence at the type level (e.g. `RecallReason` being non-empty), not at the scoring level. That's fine.

**Why it matters for the rebuild.** When the orb network exists and RootOrb is one species among many, the question of "do RootOrb principles bias activation?" becomes live. If yes, principle-as-edge becomes a real bias factor. If no, principles remain doctrine — read by humans, not by the runtime.

**Mitigation.** Decision needed during `pr-1.4/hybrid-recall`. Either:

1. Principles remain doctrine (no scoring effect). Simpler. Current behavior.
2. Principles bias resonance (e.g., "preserve ambiguity" lowers activation of unconfirmed claims to *increase* their working-set residence). More cognitive, more risky.

Default to (1) until there is evidence that (2) improves recall quality.

**Target.** Decision in `pr-1.4`.

## Closing risks

When a risk is mitigated, mark its row in the summary table with `closed` and the merge SHA, and append the closure note to the detailed entry. Closed risks remain in this document as historical reference; they are not deleted.
