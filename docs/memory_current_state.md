# memory_current_state.md

**Scope.** Inventory of Luna's memory system as of commit `a734e70` on `claude/terran-operational-interface-d94YA`. This is a *description* of what exists, not a proposal for what should exist. It is the reference the upcoming memory rebuild (`pr-1.0` and onward) checks itself against.

**Audience.** Anyone touching Luna's memory before `pr-1.0/orb-schema` lands. The "what we're not allowed to break, what we're free to extend, what's quietly violating doctrine and needs fixing first" reference.

**Companions.** `docs/memory_data_flow.mmd` (the same picture as a diagram). `docs/risk_register.md` (the things found that need fixing).

## Architecture today

Luna is event-sourced. Every meaningful turn-level signal lands as an immutable event in a JSONL log on disk. All higher-level state — episodes, the memory map, the working set — is *derived* on every turn by replaying the log. There is no hidden state. Current beliefs are a function of the events that produced them.

The pipeline:

```
turn → extractor → CognitiveObservation → fine-capture → assertions
   → events → JsonlEventLog (truth)
   → rebuild_episodes → Episodes
   → MemoryState::from_episodes → MemoryMap (typed graph)
   → activate_working_memory → WorkingMemory (budgeted)
+ recall: (Episodes, current observation) → RecallEngine → RecallSet
   → context_packet → response
```

RootOrb is seeded into the MemoryMap as a meta-substrate. It is not user memory.

## Crates inventory

| Crate | Lines | Memory role |
|-------|------:|-------------|
| `luna-core` | 819 | Canonical types: `RootOrb`, `RecallReason`, `Signal`, `Episode`, `StructuredAssertion`, `MemoryNode`/`MemoryEdge`, `MemoryMap`, `MemoryProvenance`, `CognitiveObservation`, `EpisodeContour`. Doctrine encoded in type constraints (e.g. `RecallReason::new` rejects empty strings). No mutation. |
| `luna-events` | 102 | Append-only event log. `JsonlEventLog::append(&event)` writes one JSON line. No in-memory state beyond the file path. Replay is deterministic (test proves 100-event idempotence). |
| `luna-store` | 163 | `rebuild_episodes()` — pure replay function. Reconstructs `Vec<Episode>` from events. Mutates episode confidence, `updated_at`, and merges assertions during replay. **Two doctrine gaps live here** — see misaligned section. |
| `luna-tcf` | 289 | Temporal contour + activation scoring. Pure functions. Output is `TcfScoreBreakdown` (inspectable activation components). The recall scoring foundation. |
| `luna-extract` | 3358 | Feature extraction. `FusedExtractor` / `LunaExtractor` / `HeuristicExtractor` / `EmbeddingExtractor` produce `CognitiveObservation`. `FileExtractionCache` memoizes LLM calls. |
| `luna-recall` | 166 | Pluggable recall engines: `TcfRecallEngine` (threshold ≥0.5), `KeywordRecallEngine` (threshold ≥0.34). Pure scoring + filter + truncate to top 3. |
| `luna-runtime` | 2284 | Orchestration. `RuntimeSession::process_turn()` is the entry point. Owns `MemoryState`, `WorkingMemory`, entity grouping, question proposal. **Hot path.** |
| `luna-cli` | 923 | CLI surface. Stateless. |
| `luna-metrics` | 243 | Benchmark aggregation. Proof-track / product-track split via `proof_eligible`. |
| `luna-bench` | 2395 | Formation engine for benchmark cases. LLM-driven memory construction + check verification. Offline harness. |

## Where memory state actually lives

**Event log (truth).** `.luna/runtime/events.jsonl`. `JsonlEventLog`. Append-only `EventEnvelope<LunaEvent>`. Event kinds in use:

```
TurnObserved · AssertionExtracted · EpisodeCreated · EpisodeReinforced ·
EpisodeRecalled · RecallSucceeded · RecallFailed · AssertionCorrected ·
ContradictionDetected · EpisodeDecayed
```

`AssertionCorrected` and `ContradictionDetected` are *defined* but not currently *emitted* by the production runtime; they exist as targets for future correction logic.

**Derived state (rebuilt every turn).**

- `Vec<Episode>` — produced by `luna_store::rebuild_episodes()`. Each episode owns assertions, contour, recall history, confidence, coherence, forgotten_risk.
- `MemoryState` — produced by `MemoryState::from_episodes()`. Groups claims by entity (self, people, projects).
- `MemoryMap` — typed graph. Node kinds: `User`, `Person`, `Project`, `RootOrb`, `Attribute`, plus assertion-typed nodes. Edge kinds: `DefinesRule`, `HasAttribute`, `HasGoal`, `RelatedTo`, `LocatedIn`, `Mentions`, `NeedsAnswer`, `Contradicts`, `Supersedes`, `Provenance`. Every node carries a `confidence_tier` and a `MemoryProvenance`.

**Ephemeral runtime state (not persisted).**

- `WorkingMemory` — activation-scored subset of the MemoryMap. Default budget: 5 nodes, 10 edges, 1 question, depth 1. Recomputed per turn.
- `CognitiveObservation` — turn-level extracted state. Mutated by `apply_runtime_fine_capture` before being logged.
- `RecallSet` — computed fresh per turn, logged as `EpisodeRecalled` events.

**Non-event persisted state.**

- `FileExtractionCache` at `.luna/runtime_cache/` — LLM response cache, content+model keyed. Read-only, invalidates on prompt/model change. Not authoritative; pure performance optimization.

**Configuration state.**

- `RootOrb` — currently `RootOrb::default()` only, version `root-orb-v1`. 7 hardcoded principles. Versioning machinery (`override_policy: SystemVersioned`) exists but no code path exercises it.

## Mutation surfaces

| Surface | Location | Type | Logged? | Risk |
|---|---|---|---|---|
| Event append | `luna-events:24` | write to disk | yes | low |
| Assertion extraction | `luna-runtime:111-123` | generate event | yes | low |
| Episode recall | `luna-runtime:96-109` | generate event | yes | low |
| Episode reinforcement | `luna-runtime:127-138` | generate event | yes | low |
| **Confidence penalty on contradiction** | **`luna-store:108`** | **mutate episode (-0.22)** | **no** | **HIGH (R-001)** |
| `updated_at` timestamp | `luna-store:109` | uses `Utc::now()` | drift (R-002) | medium |
| Coherence score | `luna-store:110` | recompute (deterministic) | n/a | low |
| Activation scoring | `luna-runtime:1170-1195` | mutate `node.activation` | ephemeral | low |
| Working memory selection | `luna-runtime:1195` | truncate | ephemeral | low |
| Assertion fine-capture | `luna-runtime:676-712` | normalize in-place | logged after (R-003) | medium |
| MemoryMap node merge | `luna-runtime:571-580` | update in-place | no (R-005) | medium |

The HIGH-risk row (R-001) is the single most important finding. See `risk_register.md`.

## RootOrb today

- Defined in `crates/luna-core/src/lib.rs:557-632`.
- Single instance: `RootOrb::default()` returns version `root-orb-v1` with 7 hardcoded principles.
- Loaded into MemoryMap at every rebuild via `seed_root_orb()` in `luna-runtime:593`. Becomes a `RootOrb` node + 7 `Principle` nodes connected by `DefinesRule` edges.
- `version: String` is set but never compared against. `override_policy: SystemVersioned` is set but no code path exercises override (R-007).
- **Doctrine status:** compliant. RootOrb does not store user facts. It defines memory behavior, not memory contents.
- **Generalization status:** the existing struct is the seed for the orb-species generalization in `pr-1.0`. RootOrb becomes the first instance of `MemoryOrb { kind: system_root, ... }`.

## Recall path (end to end)

1. `RuntimeSession::process_turn()` (`luna-runtime:74-192`) — entry.
2. Extractor produces `CognitiveObservation` with semantic + intent vectors, 7 signal dimensions, assertions, cue terms, query intents.
3. `apply_runtime_fine_capture()` (`luna-runtime:676-712`) normalizes assertions in place (person→identity migration, dedup, role-rename).
4. `select_recall_mode()` (`luna-runtime:1318-1335`) infers `RecallMode` from observation signals.
5. Prior events loaded; `rebuild_episodes()` reconstructs `Vec<Episode>`.
6. `RecallEngine::recall()` — for each episode, `tcf_similarity()` produces a `TcfScoreBreakdown` (semantic cosine, intent cosine, assertion fit, signal-dim contributions, coherence, contradiction/forgotten penalties). Threshold ≥0.5 (TCF) or ≥0.34 (keyword). Top 3.
7. Events logged: `TurnObserved` + `EpisodeRecalled` (per hit) + `AssertionExtracted` (per assertion) + `EpisodeCreated`/`EpisodeReinforced`.
8. Memory state rebuilt from full event log.
9. `activate_working_memory()` (`luna-runtime:1146-1240`) scores the graph against query, cue terms, and recalled facts; budget-truncates to 5 nodes / 10 edges.
10. `propose_questions()` (`luna-runtime:1632-1677`) — heuristic rules + memory-state gaps.
11. Returns `RuntimeTurnResult { observation, recalled, working_memory, questions, context_packet, ... }`.

Recall is read-only over episodes. Activation is ephemeral. `RecallReason` is type-enforced non-empty.

## Already aligned with the rebuild

These are extension points, not refactor targets:

1. **Event log is source of truth.** `JsonlEventLog` is the substrate. The `consolidation_event` schema fits as a new event kind appended to the same log.
2. **RootOrb-as-substrate.** The struct, version field, and override policy already exist. Generalizing to a species is additive.
3. **Working-set budget.** Already enforced. The new architecture preserves this; it just renames "budget" consistently with the schema's working-set readout.
4. **Certainty markers.** `AssertionConfidenceTier` (Unconfirmed / Inferred / Confirmed) on every assertion, claim, and node. Maps cleanly to the orb-schema `confidence_horizon` enum.
5. **Recall reasons.** Type-enforced, mandatory. Extends naturally to the `MemoryBrief.recall_reasons` field.
6. **Provenance.** `MemoryProvenance` on every node and edge already. Becomes the lineage substrate for orb tethers.

## Misaligned — must be addressed before / during pr-1.0

1. **Confidence penalty without logged event** (`luna-store:108`). The single doctrine violation. R-001.
2. **`Utc::now()` in replay** (`luna-store:109`). `updated_at` should use `event.timestamp`. R-002.
3. **Assertion fine-capture outside the event log** (`luna-runtime:676-712`). Normalization happens before logging; raw extraction unrecoverable. R-003.
4. **Hardcoded assertion-intent mapping** (`luna-tcf:201-223`). Scripted lookup table; long-tail domains can't extend without code edits. R-004.
5. **MemoryMap node/edge merge silent** (`luna-runtime:571-580`). Insert-or-update without a logged delta event. R-005.
6. **Question proposal rules scattered** (`luna-runtime:1632-1677`). Inline heuristics without a registry. R-006.

Each has an entry in `risk_register.md` with severity, doctrine impact, and mitigation.

## Hot paths / do not break

These regions are exercised on every turn. Changes here ripple. Test before, during, and after any modification:

1. `RuntimeSession::process_turn()` — `luna-runtime:74-192`. Entry point.
2. `rebuild_episodes()` — `luna-store:7-135`. Replay logic; determinism is load-bearing.
3. `activate_working_memory()` — `luna-runtime:1146-1240`. Budget enforcement.
4. `tcf_score_breakdown()` — `luna-tcf:64-124`. Recall scoring weights.
5. `apply_runtime_fine_capture()` — `luna-runtime:676-712`. Assertion normalization.
6. `MemoryState::from_episodes()` — `luna-runtime:330-351`. Graph construction.

Any PR that touches these gets extra scrutiny: golden-file replay tests, byte-for-byte rebuilt-state equality, and benchmark deltas reported alongside the diff.

## Doctrine compliance — current status

Read against `docs/LUNA_BUILD_DOCTRINE.md` and verified by `scripts/doctrine_check.sh` (passing).

| Doctrine rule | Status |
|---|---|
| Event log is truth | ✓ implemented |
| Memory is layered (events → assertions → graph → working set) | ✓ implemented |
| RootOrb is not user memory | ✓ implemented |
| Small ontology | ✓ implemented |
| Entity memory must be real (typed nodes, not strings) | ✓ implemented |
| Working set is tiny | ✓ implemented (5 nodes / 10 edges) |
| Ambiguity preserved | ✓ implemented (`AssertionConfidenceTier`) |
| Proof and product separated | ✓ implemented (`proof_eligible` flag) |
| Recall must carry why | ✓ enforced (`RecallReason::new` rejects empty) |
| No transcript stuffing | ✓ implemented (working-set budget) |

Mechanical doctrine checks (`scripts/doctrine_check.sh`) pass:

- No hardcoded entity-name dispatch in `crates/`.
- `scenarios/runtime/` non-empty.

Borderline / unimplemented (not violations, gaps):

- **Memory intake policy** (ACCEPT / IGNORE_NOISE / ASK_FOR_ANCHOR) — not yet implemented. All extracted events accepted unconditionally.
- **Correction / supersession** — enum variants exist but no runtime path emits them in production (R-008).
- **Path-aware activation** — current activation is shallow (per-node + per-edge scoring). No graph traversal yet.

## What this audit unblocks

The audit gives `pr-1.0` and beyond a stable reference:

- `pr-1.0/orb-schema` can generalize RootOrb without breaking RootOrb's existing seed path.
- `pr-1.5/consolidation-event` (the schema landed in commit `a734e70`) has identified upstream events to bind against (`AssertionExtracted`, `EpisodeReinforced`, `ContradictionDetected`).
- Risk R-001 (confidence penalty without event) must be fixed *before* the consolidation engine ships, otherwise the engine inherits a quietly broken substrate.
- Hot paths are now flagged. Any PR that touches them gets extra scrutiny.

The doctrine status is good. The substrate is real. Two specific gaps (R-001, R-002) must be closed; everything else is straightforward extension.
