# Luna Over Aura Build Plan

Status: active engineering plan  
Purpose: make Luna strictly stronger than Aura without weakening Luna's memory
contract.

This is not a merge plan. Aura is a reference system. Luna remains the trunk.
Anything borrowed from Aura must be rebuilt as Luna-native behavior:

```text
event log
-> intake decision
-> typed assertions and relations
-> entity/event graph
-> lifecycle status
-> bounded working memory
-> response plan
-> answer with provenance
-> replay audit
```

No hidden memory, prompt-only cognition, phrase-to-answer maps, silent merge
loss, or contract exceptions are allowed.

## Superiority Standard

Luna is better than Aura only when the repo can prove all of these:

- it remembers useful personal, project, manuscript, and relationship context;
- it knows what is current, old, uncertain, corrected, contradicted, or unknown;
- it explains why memory surfaced and why other memory did not;
- it keeps working memory bounded instead of dumping transcripts;
- it can rebuild durable state from event evidence;
- it can replay/audit the derived state;
- it has a usable runtime/workbench surface for inspection;
- every user-visible memory behavior has a failing guardrail.

If a feature sounds more cognitive but cannot be inspected, replayed, or tested,
it does not count as progress.

## Head-To-Head Build Targets

| Area | Aura advantage | Luna superiority target | Required proof |
|------|----------------|-------------------------|----------------|
| Cognitive field | Seven-dimensional field drives recall and state. | Add an inspectable activation field derived from typed event-backed memory, not hash-only geometry. | Unit tests, runtime scenario, inspect output showing component scores. |
| Turn pipeline | Ordered cognition before renderer. | Add a typed runtime turn receipt that records intake, extraction, lifecycle, activation, response planning, output, and commit. | Scenario log schema check and replay audit. |
| Render boundary | LLM is renderer, not mind. | Harden Luna output boundary so memory authority stays in event/graph/working-memory evidence and the model only verbalizes. | Output-boundary scenario and doctrine lint for unsupported memory leakage. |
| Relationship modeling | Trust, familiarity, recency, and valence bias recall. | Add relationship claims and relationship activation as ordinary typed memory with provenance and lifecycle. | Relationship correction scenario, inspect path, recall reason. |
| Identity/self model | Identity axioms and self coherence exist. | Expand SystemKernel/RootOrb as behavior substrate only, never user memory, with inspectable axioms and no leakage. | System-kernel leakage scenario and replay audit. |
| Memory consolidation | ShapeMergeEngine reduces fragmentation. | Add cluster split/merge receipts driven by runtime events, preserving raw ancestry and reversible inspection. | Receipt tests, scenario, audit proof. |
| Vault pressure | Bounded 128-episode vault keeps footprint small. | Add bounded working-memory and derived-cluster budgets without deleting event truth. | Budget tests and "why not remembered" inspect output. |
| Safety/observers | Safety state and observer council influence response. | Add sentinels/gauges that produce typed, logged, inspectable pressure signals. | Sentinel scheduling tests and scenario-visible recommendations. |
| Product shell | Desktop shell and diagnostics are more compelling. | Build a Luna memory workbench that shows event log, entities, lifecycle, activation, provenance, and replay status. | UI smoke test and fixture-backed screenshots when UI exists. |
| Multi-device mesh | Peer/device architecture exists. | Defer until durable topology store exists; then add consented replicated event logs with hash/audit checks. | Cross-store replay/audit tests. |

## Execution Order

### M1: Runtime Turn Receipt

Make every runtime turn produce one typed receipt with:

- source event ids and hashes;
- extraction mode and intake decision;
- created/updated claims;
- lifecycle transitions;
- activation inputs and scores;
- bounded working-memory selection;
- response-plan action;
- output boundary result;
- committed event/hash.

Guardrails:

- scenario log must include the receipt;
- replay audit must reject missing or mismatched receipt hashes;
- inspect must print the receipt for a turn.

Why this beats Aura: Aura has a rich pipeline, but Luna will have a rich pipeline
that can be replayed and dissected.

### M2: Activation Field, Luna-Native

Add a named activation field built from Luna evidence:

- relevance;
- confidence;
- recency;
- lifecycle status;
- correction/surprise pressure;
- entity/relationship proximity;
- provenance strength;
- open-loop/project pressure.

The field is not a hidden brain metaphor. It is a score report. Every component
must be printable.

Guardrails:

- unit tests for each component;
- scenario where correction/surprise beats stale distraction;
- inspect output showing why a low-scoring memory stayed out.

Why this beats Aura: Luna gets field-like recall depth without losing typed
truth, lifecycle, or provenance.

### M3: Relationship Memory

Add typed relationship memory for:

- user preference;
- communication preference;
- trust/repair event;
- collaborator role;
- recurring project context;
- emotional salience when explicitly evidenced;
- uncertainty and correction.

These are claims and relations, not personality prompt text.

Guardrails:

- relationship scenario with correction and distractors;
- inspect current and superseded relationship facts;
- answer includes recall reason and confidence;
- proof mode excludes unconfirmed relationship claims unless allowed.

Why this beats Aura: relationship state becomes evidence-backed and correctable,
not just a field bias.

### M4: Cluster Receipts In Product Recall

Move current topology cluster receipts from proof lane into product recall only
when they provide:

- source event ids;
- source event hashes;
- accepted compression reason;
- list of preserved claims;
- list of excluded claims;
- reversible inspect path.

Guardrails:

- forged receipt rejected;
- lossy receipt rejected;
- accepted receipt can reduce working memory;
- "why not remembered" identifies cluster budget/filter reasons.

Why this beats Aura: Luna can consolidate aggressively without silent memory
loss.

### M5: Sentinels And Observer Pressure

Turn Luna sentinels/gauges into runtime-visible pressure:

- contradiction pressure;
- provenance integrity pressure;
- stale-current conflict;
- identity/system leakage;
- over-broad recall;
- unsupported answer risk.

Pressure can influence response planning only through typed logged artifacts.

Guardrails:

- sentinel scheduling test;
- runtime scenario where sentinel changes the response plan;
- inspect output showing sentinel evidence and recommendation.

Why this beats Aura: observer behavior becomes auditable, not atmospheric.

### M6: Renderer Boundary

Define the final response boundary:

- memory answer packet;
- uncertainty packet;
- suppressed-memory packet;
- unsupported/ask-clarification packet;
- model-render packet.

The model may phrase the answer. It may not invent memory authority.

Guardrails:

- output-boundary leak scenario;
- phrase-to-answer lint remains enforced;
- runtime answer can cite evidence without transcript stuffing.

Why this beats Aura: Luna gets Aura's renderer separation plus stronger memory
proof.

### M7: Memory Workbench

Build the user-facing workbench:

- event timeline;
- entity graph;
- current/superseded/unknown claims;
- activation field view;
- working-memory packet;
- "why remembered" and "why not remembered";
- replay/audit status;
- scenario runner results.

Guardrails:

- UI fixture uses real scenario output;
- no UI-owned memory mutation;
- UI smoke test verifies key panels render.

Why this beats Aura: the compelling surface shows why Luna is more trustworthy,
not merely that it stores facts.

### M8: Real Trial Superiority Packet

Add a reproducible benchmark packet comparing:

- Luna;
- transcript stuffing;
- simple RAG;
- Aura-style geometric recall if implemented as a baseline adapter.

The benchmark must include corrections, distractors, ambiguity, relationship
facts, manuscript continuity, and 24-hour reopen trials.

Guardrails:

- every miss becomes a scenario;
- no benchmark-only code path;
- packet archives logs, hashes, answers, inspect output, and scoring.

Why this beats Aura: superiority becomes measured evidence, not a claim.

## Forbidden Moves

- Do not import Aura's vault eviction as deletion of Luna event truth.
- Do not merge memory at write time without reversible receipts.
- Do not add a "field" that inspect cannot explain.
- Do not use hash geometry as a replacement for typed assertions.
- Do not store relationship or identity facts in prompts.
- Do not make UI the owner of memory state.
- Do not call deterministic scenarios proof of real LLM quality.
- Do not weaken the Luna memory contract to win a comparison.

## Immediate Next Commit

Start with M1.

Implementation slice:

1. Add a runtime turn receipt type for scenario logs.
2. Include intake, lifecycle, activation, working-memory, response-plan, and
   commit summary fields that already exist in runtime state.
3. Add a scenario/log check that fails if the receipt is missing.
4. Add inspect support for printing the receipt by turn id.
5. Run the local gate.

This gives Luna the first thing Aura appears to have more of: a rich turn
pipeline. Luna's version will be stronger because it is event-backed,
inspectable, and replay-audited from day one.
