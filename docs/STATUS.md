# STATUS.md

What's real, what's designed, what's simulated. The first stop for anyone landing on this repo. Updated when state changes; otherwise stable.

If you only read one thing in this file: the **canonical roadmap to v1.0** is [`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`](LUNA_MEMORY_MILESTONE_ROADMAP.md). The orb-network architecture in `memory_schema_v1/` and the bridge mockup in `mockups/` are **v2 design work**, not v1 prerequisites.

---

## What is running today

Code paths exercised by tests, scenarios, or CI.

| Component | Status | Where |
|---|---|---|
| Append-only event log | running | `crates/luna-events` |
| Episode rebuild from events | running, deterministic | `crates/luna-store::rebuild_episodes` |
| Heuristic + LLM-backed extraction | running | `crates/luna-extract` |
| TCF + keyword recall engines | running | `crates/luna-recall` |
| Entity-typed memory map | running | `crates/luna-runtime::MemoryState` |
| RootOrb v0.1 substrate | running, single instance | `crates/luna-core::RootOrb` |
| Working-memory budget enforcement | running, defaults 5 nodes / 10 edges | `crates/luna-runtime::activate_working_memory` |
| Type gate: `RecallReason` non-empty | enforced at compile time | `crates/luna-core::RecallReason` |
| One runtime scenario | passing 18 memory checks | `scenarios/runtime/joe_chris_francois.json` |
| CI: workspace tests | enforced on push + PR | `.github/workflows/doctrine.yml` gate 1 |
| CI: doctrine lint | enforced on push + PR | `.github/workflows/doctrine.yml` gate 2 |
| CI: scenario gates | enforced on push + PR | `.github/workflows/doctrine.yml` gate 3 |
| Replay determinism gate | byte-for-byte stability test | `crates/luna-store/src/lib.rs::tests` |

Test count: 126 across the workspace at last run.

## What is documented (about the current code)

Reference material that describes what is. Not future-state.

| Document | Role |
|---|---|
| [`LUNA_BUILD_DOCTRINE.md`](LUNA_BUILD_DOCTRINE.md) | The doctrine. What memory must and must not do. |
| [`LUNA_MEMORY_MILESTONE_ROADMAP.md`](LUNA_MEMORY_MILESTONE_ROADMAP.md) | **Canonical v1.0 roadmap.** Stages 1–8. |
| [`memory_current_state.md`](memory_current_state.md) | Audit of crates, mutation surfaces, hot paths, doctrine compliance. |
| [`memory_data_flow.mmd`](memory_data_flow.mmd) | Mermaid diagram of the live pipeline. |
| [`risk_register.md`](risk_register.md) | 10 risks identified, 2 closed (R-001, R-002), 1 gate landed (R-009). |
| [`../CLAUDE.md`](../CLAUDE.md) | Mechanical gates index + slice roadmap (doctrine-as-build infra). |
| [`../AGENTS.md`](../AGENTS.md) | Multi-agent orientation. |

## What is designed (not yet implemented)

Specified at the document layer. **No runtime enforcement.** Reading these schemas should not be confused with the system having these capabilities.

| Artifact | Status | Notes |
|---|---|---|
| [`memory_schema_v1/consolidation_event.schema.json`](../memory_schema_v1/consolidation_event.schema.json) | detailed v1 schema | Contract format. No emitter. No validator wired up yet. |
| [`memory_schema_v1/memory_event.schema.json`](../memory_schema_v1/memory_event.schema.json) | skeleton | Detailed in a future PR. |
| [`memory_schema_v1/orb.schema.json`](../memory_schema_v1/orb.schema.json) | skeleton | Detailed in a future PR. |
| [`memory_schema_v1/memory_brief.schema.json`](../memory_schema_v1/memory_brief.schema.json) | skeleton | Detailed in a future PR. |
| Orb network (cores, halos, tethers, branching, merging) | designed, not built | No `luna-orbs` crate exists. |
| Vector field (broadband perception) | designed, not built | No `luna-embed` crate exists. |
| Hybrid recall (memory brief output) | designed, not built | No engine exists. |
| Consolidation engine (compress / branch / merge) | designed, not built | No `luna-consolidate` crate exists. |
| Governance attestation, privileged halos | designed, not built | Schema field exists; no `luna-govern` crate. |

These items are part of the **v2 architecture** (AI · provenance · governance). They are **not** prerequisites for v1.0 acceptance.

## What is simulated (design fiction)

Visual prototypes. Not connected to the runtime.

| Artifact | Status |
|---|---|
| [`mockups/terran-os-bridge.html`](mockups/terran-os-bridge.html) | Self-contained HTML. Simulated state. Renders the v2 architecture as if running. **No connection to Luna's runtime.** |

## The three roadmaps

Each lives. Each owns a different track.

### 1. Memory Milestone Roadmap — `docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`

**Canonical for v1.0.** Eight stages from current memory loop to the manuscript-memory target. The README's acceptance test (10-turn real-week → 24h+ → 3 questions) is Stage 7.

Stages 1–6 build the substrate; Stage 7 is the trial; Stage 8 begins the manuscript track only after Stage 7 has passed once.

### 2. Doctrine-as-Build Slice Roadmap — `CLAUDE.md`

Owns mechanical-gate infrastructure (CI, type system, lint). Slices 1 and 2 LANDED (CI + `RecallReason` enforcement). Slices 3a–d PLANNED (`QuestionCandidate.reason`, `WorkingMemory.activation_reason`, `MemoryProvenance` constructor, node/edge non-empty provenance).

Orthogonal to the milestone roadmap. Both should be active concurrently.

### 3. Orb Network Rebuild — `memory_schema_v1/`

**v2 work.** The AI · provenance · governance architecture: orb species, vector field, hybrid recall, consolidation engine, governance attestation. Begun as forward design while Stage 7 is still pending.

**This track does not block v1.0.** When it resumes in earnest, the order is:
- Phase 0 audit ✓ done
- Phase 1 schema family draft (consolidation_event detailed; 3 skeletons) ✓ done
- Phase 2 event log hardening (R-001, R-002 closed; R-009 gate landed) ✓ done
- Phase 3+ orb schema, vector field, hybrid recall, consolidation engine — paused until Stage 7 closes.

## What needs to happen next

**Stage 7 dense-recall passes.** As of `64eb1b1`, the Stage 7 probe fixture (`scenarios/exploratory/stage7_dense_week.json`) passes 13/13 memory checks against `glm-4.6:cloud`. ~26 assertions extracted across natural-prose input, working memory bounded throughout, two distinct people not conflated, self-introduction and time-anchored personal facts both captured. The architectural answer is durably in: **memory works on natural prose given working extraction.**

See [`STAGE7_FINDINGS.md`](STAGE7_FINDINGS.md) (Final result section) for the full evidence.

**The orb-network rebuild is v2 architecture, not a v1.0 prerequisite.** The v1.0 critical path is now extraction-side and time-decay-side, not memory-architecture-side.

Revised priority order:

1. ~~**Establish working extraction on natural prose.**~~ **DONE** — three backends scaffolded, GLM-cloud run executed.
2. ~~**Architectural answer.**~~ **DONE** — memory works given extraction.
3. ~~**Close prompt-vocabulary gaps the run surfaced.**~~ **DONE** — `identity:name` and `person:availability` added to both the markdown prompt and the Rust validator. Stage 7 dense-recall now passes 13/13.
4. **Fixture graduation + CI strategy for LLM-backed scenarios.** The Stage 7 dense-recall fixture is now graduation-eligible from `scenarios/exploratory/` to `scenarios/runtime/`, but the existing CI gate uses the heuristic extractor — graduating naively would break CI. Decision needed: either a parallel CI workflow for LLM-backed scenarios (with cache + model pinning), or a per-scenario `requires_extractor` field consumed by both the harness and CI. Smallest viable next move.
5. **Add a time-decay process.** `EpisodeDecayed` driven from elapsed event-time so `forgotten_risk` actually moves on its own. Required before the 24h portion of Stage 7 is measurable. Today, a 0-second gap and a 24-hour gap behave identically.
6. **Build the Stage 7 fixture with a 24h gap and run it.** Needs (5) and a small harness extension for simulated time (per-turn `event_time` or cumulative `gap_seconds`).
7. **Resume the orb-network rebuild only after (6) closes.** v2 architecture; the schemas in `memory_schema_v1/` remain forward design until then.

Latent issue from this run, worth flagging in any extraction-vocabulary work: the markdown allowlist in `crates/luna-extract/prompts/extract_v3.md` and the Rust mirror const `PROMPT_V3_DOMAIN_KINDS` in `crates/luna-extract/src/llm_observation.rs` must be updated together but have no automated check that they agree. The next vocabulary extension will hit the same trap `64eb1b1` resolved. Add either a unit test that parses the markdown table and asserts it matches the Rust const, or generate the const from the markdown at build time.

Defer: full audit of `luna-extract` and `luna-bench`; R-005 retrofit; regression tests for hot paths beyond `process_turn` and `rebuild_episodes`.

## How to read the repo, in order

1. `README.md` — project intent and the v1.0 acceptance test.
2. `STATUS.md` (this file) — what's real today.
3. `docs/LUNA_BUILD_DOCTRINE.md` — what memory must and must not do.
4. `docs/LUNA_MEMORY_MILESTONE_ROADMAP.md` — the v1.0 path.
5. `CLAUDE.md` — mechanical gates and the slice roadmap.
6. `docs/memory_current_state.md` — audit of where memory state actually lives.
7. `docs/risk_register.md` — what's known to be wrong, ordered by severity.

Only after those: the v2 design (`memory_schema_v1/`, `docs/mockups/`).
