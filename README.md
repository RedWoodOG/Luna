# Luna

> **Status: PR 0.1 — schema v1 only.** Run output is **NOT proof-eligible**
> until PR 0.4 closes Stage 0 (extractor formation gates). Numbers produced
> by `bench run` and `bench compare` reflect the current upstream-blocked
> state and are non-publishable. The five temporal cases ship as
> `proof_eligible: false` until PR 0.1b commits authorial timestamps and
> `benchmarks/temporal/RATIONALE.md`.

Luna is a local-first episodic memory framework that tests whether state-contour recall performs better than semantic retrieval across real multi-turn conversations.

## Destination

Luna's long-term target is one-read continuity over a complex manuscript:

1. Read a manuscript once.
2. Close the source.
3. Later answer questions about characters, plotlines, timelines, flashbacks, contradictions, and unresolved threads.

Pass:

- Luna answers from its event-sourced memory, not by rereading or searching the manuscript.
- Luna separates scene order from story chronology when flashbacks or nonlinear timelines appear.
- Luna preserves character relationships, plot state, and unresolved arcs across interference.
- Luna marks what is confirmed, inferred, unconfirmed, and unknown.
- Luna explains why each relevant memory was recalled.

The smaller multi-turn conversation test below is the first proving ground for the same shape: encode active truth, survive distraction, then retrieve without flattening uncertainty.

## Build Doctrine

Luna's implementation rules are captured in [`docs/LUNA_BUILD_DOCTRINE.md`](docs/LUNA_BUILD_DOCTRINE.md).
The current memory milestone and forward roadmap are tracked in
[`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`](docs/LUNA_MEMORY_MILESTONE_ROADMAP.md).

The short version:

- Luna must not become ordinary JSON memory plus retrieval.
- No scripted user facts or scripted final answers.
- Memory behavior must come from reusable intake, ontology, graph, activation, working-set, and response-planning mechanisms.
- Root memory defines behavior and semantic grammar; it does not store user facts.
- The event log remains the source of truth.
- Every new memory behavior should be protected by an ENCODE -> DISTRACT -> RETRIEVE runtime scenario.

## v1.0 Acceptance Test

Luna v1.0 passes when it can preserve project and personal continuity across a real gap in time:

1. Talk to Luna for 10 turns about a real week.
2. Close the terminal.
3. Come back at least 24 hours later.
4. Ask Luna three questions about what was said.

Pass:

- Luna answers all three questions correctly from its event-sourced memory.
- Each answer marks what is confirmed, inferred, and still unknown.
- Luna does not flatten ambiguity into certainty.
- Luna can explain why the relevant memory was recalled.

Until this test has passed once, defer work that does not directly move Luna toward passing it.

## Memory Working Set

Luna memory may grow large, but the active memory shown to a model or user must stay small, relevant, and explainable.

The event log remains the source of truth. The memory map can contain many nodes and edges. Each turn should activate only a bounded working set:

```text
event log
→ assertions and episodes
→ memory map
→ activation
→ filtered working memory packet
```

Rules:

- Store durable memory without forcing it into every turn.
- Recompute activation per turn instead of leaving memories permanently on.
- Surface only the highest-value nodes, edges, and unknowns within a fixed budget.
- Keep quiet memories retrievable without letting them become constant roadblocks.
- Track why memory was surfaced and what was filtered out.
- Fail safe: if filtering breaks, preserve the raw event log and avoid overclaiming.

This is the anti-scope-creep rule for memory growth: Luna can build a rich internal map, but product behavior must be governed by a compact working set.

## RootOrb

Luna starts with a small `RootOrb`: a transparent boot substrate for memory behavior.

RootOrb is not user memory and not a hardcoded factual trap. It defines starting invariants such as:

- Luna is a local-first memory runtime, not a conscious being.
- The event log is the source of truth.
- Memory must distinguish confirmed, inferred, unconfirmed, and unknown.
- Ambiguity must not be flattened into certainty.
- Only a bounded working set should enter the current turn.
- Surfaced memory should carry provenance and recall reasons.
- Proof behavior and product behavior stay separate.

Rules:

- RootOrb provenance is marked as `system_root`.
- RootOrb is versioned.
- RootOrb is inspectable.
- RootOrb is quiet unless Luna's identity, memory behavior, proof boundary, or certainty rules are relevant.
- RootOrb must not override event-sourced user evidence.
- RootOrb may evolve by version or configuration; it is a starting point, not a cage.

The v0.1 target is deliberately boring:

- input: benchmark JSON conversations
- output: scored reports with concrete metrics
- engines: keyword baseline and Luna TCF baseline
- persistence: append-only JSONL events

Run:

```powershell
cargo run -p luna-cli -- bench run ./benchmarks --engine tcf
cargo run -p luna-cli -- bench run ./benchmarks --engine keyword
cargo run -p luna-cli -- bench compare ./runs/latest
cargo run -p luna-cli -- report ./runs/latest --format markdown
```

Luna stores events, not final memory truth. Episodes are rebuilt from those events.

## Boundary Rule

Legacy Aura code is quarantined outside this workspace. Luna must not import or reference it.
