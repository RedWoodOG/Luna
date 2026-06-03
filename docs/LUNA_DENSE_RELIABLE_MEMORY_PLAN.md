# Luna Dense Reliable Memory Plan

Status: active build track

Purpose: move Luna from store-and-retrieve memory toward fixed-capacity
compressive memory: memory that learns by updating bounded state, reconstructs
answers from that state, and keeps enough event provenance to be trusted.

The primitive shift is:

```text
old shape: input -> store claim/chunk -> retrieve nearest match
new shape: input -> predict -> measure surprise -> update fixed memory state
           -> reconstruct answer -> cite source lineage
```

Luna still keeps the raw event log as audit truth. The new memory layer does not
replace provenance. It sits between event-sourced facts and bounded working
memory as a compact learned state that can be replayed, inspected, and tested.

## Canonical Direction

This section is the decision lock for dense memory work. Do not reopen these
choices unless a failing gate proves the direction is wrong.

- Build dense memory inside the existing runtime/event-log spine first.
- Keep `MemoryState`, episodes, topology bridge, and bounded working memory as
  the product recall path until a learned substrate beats them behind gates.
- Treat dense receipts as the first learned-memory interface. A future
  associative matrix, concept codebook, or neural memory must plug into the same
  receipt contract instead of replacing it.
- Do not add a `luna-dense` crate until there is a real reusable learned-state
  module with no dependency cycle against `luna-runtime`.
- Do not introduce `LearnedState` as a wrapper name for existing `MemoryState`.
  Existing state remains existing state. New names must describe actual new
  mechanics, such as `SurpriseAssessment`, `AssociativeMemory`, or
  `ConceptCodebook`.
- Do not replace `decide_memory_intake` in one move. First wrap the current
  intake/correction/reinforcement result in deterministic surprise receipts,
  then migrate policy behind that inspectable seam.
- Do not let reconstruction bypass lineage. Model-generated candidates are
  hints until an event/receipt chain proves the surfaced value.

Default build rule:

```text
current runtime behavior
-> receipt it
-> replay/audit it
-> scenario-gate it
-> only then make the underlying state denser
```

## Reliability Standard

Luna becomes reliable when each memory behavior has five inspectable surfaces:

- **Prediction**: what the current memory state expected.
- **Surprise**: what was novel, corrective, contradictory, or redundant.
- **Update**: which bounded memory state changed and by how much.
- **Reconstruction**: what answer was produced from memory state.
- **Lineage**: which raw events, assertions, receipts, and regressions prove it.

The first controlled trial established the repair loop:

```text
trial miss
-> scored packet
-> regression scenario
-> generic mechanism fix
-> rerun trial
-> committed gate
```

That loop remains the rule. The memory system learns from mistakes when a miss
becomes executable evidence, not when a note is written and forgotten.

## Concrete Memory Substrates

### 1. Surprise-Gated Neural Memory

Build a fixed-size memory model whose write operation is an update to model
state, not an append to a growing table.

Luna-native first slice:

- deterministic fixed-size state, implemented in Rust;
- `predict(assertion_or_fact) -> reconstruction_score`;
- `surprise_score = distance(input, prediction)`;
- update only when surprise, correction pressure, or salience crosses a
  threshold;
- repeated low-surprise facts reinforce existing state instead of duplicating
  memory.

Later neural slice:

- a small online-trainable memory module can be added behind the same trait;
- gradient/update details stay hidden behind a deterministic receipt interface;
- every update records input hash, prediction hash, surprise score, update norm,
  and resulting state hash.

Required interface:

```text
MemoryModel::predict(input) -> prediction
MemoryModel::surprise(input, prediction) -> score
MemoryModel::update(input, score) -> update_receipt
MemoryModel::reconstruct(query) -> candidates
```

Guardrail:

- repeated facts produce reinforcement receipts;
- novel facts produce update receipts;
- corrections produce supersession pressure;
- all receipts replay to the same state hash.

### 2. Associative Matrix

Add a fixed-size associative memory for "what goes with what."

Build shape:

- encode each typed assertion into a key vector and value vector;
- update a fixed `d x d` matrix with a bounded delta;
- retrieve candidate values by querying the matrix;
- decay or renormalize the matrix so magnitude does not grow without bound;
- keep source event ids/hashes outside the matrix in a receipt index.

This gives Luna fast association without a growing vector store. The matrix is
not authority; it is a bounded learned hint surface. The event log and receipts
remain authority.

Guardrail:

- project-name, project-purpose, and project-partner cues retrieve different
  candidates from the same related fact set.

### 3. Discrete Concept Codebook

Add a fixed-size concept library for gist.

Build shape:

- maintain `K` concept slots with stable ids;
- assign each assertion/relation/episode summary to the nearest concept;
- update the concept centroid with a bounded moving average;
- merge concepts that become too similar;
- replace dead concepts only through a receipt that preserves prior lineage;
- track included and excluded claim ids for every concept update.

The codebook is Luna's compact abstraction layer. It should answer questions
like "what kind of thing is this?" without carrying every raw episode into the
working packet.

Guardrail:

- a dense project conversation creates a stable project-purpose concept while
  preserving raw source events for name, purpose, correction, and pilot facts.

### 4. Reconstruction With Provenance

Luna should not merely return stored strings. It should reconstruct an answer
from bounded memory state, then validate that answer against event-backed
evidence before surfacing it.

Build shape:

- query the associative matrix for candidates;
- query the concept codebook for gist;
- query current typed claims for source-backed specifics;
- compose a response plan from those candidates;
- reject any answer value that lacks direct event or receipt lineage;
- record what was filtered out and why.

Guardrail:

- when asked what a project helps people do, Luna returns the purpose slice and
  excludes founder/pilot facts from answer, context packet, markdown, and
  working-memory surfaces.

### 5. Forgetting As Renormalization

Forgetting should mean bounded state maintenance, not silent deletion of truth.

Build shape:

- raw event log remains append-only;
- model state can decay, renormalize, merge, and prune;
- every decay/merge/prune operation creates a receipt;
- replay rebuilds model state from raw events plus receipts;
- recall can say whether detail came from raw event, compressed receipt,
  associative hint, or concept gist.

Guardrail:

- a pruned/merged concept cannot be used to answer unless the receipt can trace
  back to raw events.

## Build Sequence

The build sequence below is authoritative. Each slice must land with one
capability and one guardrail before the next slice starts. If a later idea looks
better, add it as a deferred note under the relevant future slice; do not
reshape the current slice unless it cannot pass its gate.

### C1: Miss-To-Regression Memory

Current status: first slice landed for project memory.

Evidence:

- failed packet:
  `.luna/controlled-human-trial/20260514-092422`
- fixed regression:
  `scenarios/runtime/controlled_trial_project_memory_regression.json`
- scenario is registered in `scenarios/runtime/SCENARIO_MANIFEST.txt`
- reruns answer current name, purpose, and pilot partner with clean audit.

Next build:

- add a miss index so every controlled-trial miss links to a scenario or a
  deferred issue.

### C2: Surprise And Update Receipts

Status:

- C2a `SurpriseUpdateReceipt` type and deterministic state hashing: LANDED.
- C2b runtime turn emission: LANDED.
- C2c replay/audit receipt verification: LANDED.
- C2d scenario proving repeated/novel/correction receipt behavior: LANDED.

Evidence:

- `LunaEvent::DenseUpdateReceipted` records surprise/update receipts in the
  event log.
- `RuntimeSession::process_turn` emits one receipt for each processed
  assertion.
- Runtime replay audit counts dense receipts, counts dense receipt hash
  mismatches, and quarantines logs with mismatched dense receipt hashes.
- `scenarios/runtime/dense_surprise_gate_baseline.json` proves novel,
  reinforcement, correction-pressure classifications, and clean dense receipt
  replay-audit counts.

Deliverable:

- runtime turn output includes prediction/surprise/update fields.

Implementation lock:

- C2 lives in `luna-runtime` and `luna-core` first.
- `SurpriseUpdateReceipt` becomes an event-log payload before any new dense
  crate is created.
- Receipt emission classifies current runtime outcomes as
  `reinforce_existing`, `novel_update`, `correction_pressure`, or
  `ignored_low_surprise`.
- `state_hash_before` and `state_hash_after` are hashes of the typed memory
  claims visible to runtime at that point.
- Prediction in C2 is the deterministic expectation implied by current typed
  memory, not a neural model.
- The scenario gate must fail if receipts are absent, hashes are malformed, or
  correction/reinforcement/novel classifications regress.

Receipt fields:

```text
input_event_id
input_event_hash
prediction_hash
surprise_score
redundancy_score
correction_pressure
update_kind
state_hash_before
state_hash_after
```

Guardrail:

- C2a unit tests prove state hashes are order-independent, claim changes alter
  state hashes, receipt hashes change when update kind changes, and lineage
  hashes are required.
- repeated project facts reinforce existing state;
- renamed project facts create correction pressure;
- replay recomputes identical state hashes.

### C3: Fixed Associative Matrix Prototype

Status: LANDED.

Deliverable:

- deterministic fixed-size associative matrix for typed assertions.

Evidence:

- `AssociativeMemory` builds a fixed 16 x 16 matrix from current typed claims.
- `associative_candidates_for_query` returns bounded hint candidates without
  making the matrix answer authority.
- `scenarios/runtime/dense_associative_project_slices.json` proves project
  name, purpose, and pilot queries retrieve different requested slices while
  storage remains fixed at 256 cells.

Open only after:

- C2b runtime receipt emission is landed;
- C2c replay/audit receipt verification is landed;
- C2d scenario proves repeated/novel/correction receipt behavior.

Implementation lock:

- The matrix is a bounded hint surface, not authority.
- Matrix updates must create receipts that point back to event-log assertions.
- Retrieval candidates cannot surface in answers without typed/event-backed
  lineage.

Guardrail:

- the same entity with name, purpose, and pilot facts retrieves the requested
  slice for each query type without growing storage per query.

### C4: Concept Codebook Prototype

Deliverable:

- fixed-size concept slots for project/person/manuscript gist.

Open only after:

- C3 proves a bounded learned state can be updated and queried without breaking
  runtime recall.

Implementation lock:

- Concept slots have fixed ids and bounded count.
- Merge, replace, and prune operations require receipts.
- Every concept update records included and excluded claim ids.

Guardrail:

- dense related facts compress into a concept with included/excluded claim ids
  and raw event lineage.

### C5: Reconstruction Gate

Deliverable:

- response planning can use model candidates, associative candidates, concept
  gist, and typed claims, but only event-backed values can reach the answer.

Open only after:

- C3 and C4 expose inspectable candidates with receipt lineage.

Implementation lock:

- Reconstruction is an answer-planning input, not final truth.
- Unsupported model candidates must be visible in inspect output and filtered
  from answer/context/markdown/working-memory surfaces.

Guardrail:

- unsupported reconstruction candidates are visible in inspect and suppressed
  from answer/context/markdown.

### C6: Dense Memory Trial

Deliverable:

- 10+ turns with related people, projects, corrections, repeated facts,
  distractors, and detail-specific questions.

Pass:

- fixed memory state remains bounded;
- raw event lineage remains complete;
- repeated facts reinforce instead of duplicating;
- novel facts update;
- corrections supersede;
- answer chooses the requested slice;
- replay audit is clean.

## Immediate Next Artifact

Build `C4: concept codebook prototype`.

This is the first compact gist layer behind the receipt contract. C2 records
surprise/update receipts in the runtime event log and replay audit quarantines
receipt hash divergence. C3 added a deterministic fixed-size associative hint
surface for typed assertions without replacing event-log lineage. C4 should add
bounded concept slots for project/person/manuscript gist.

Do not redesign the dense-memory architecture while building C4. The accepted
shape is:

```text
typed claims
-> deterministic concept assignment
-> bounded concept-slot update
-> concept receipt with included/excluded claim ids
-> gist candidates stay hints until lineage-backed
```

The first implementation should be deterministic and boring:

```text
current typed memory state
-> fixed-size concept slots
-> merge/replace/prune only through inspectable receipts
-> scenario proves dense related facts compress into stable gist
```

Once that is inspectable and gated, the underlying model can become more
powerful without changing the proof contract.

## Current North Star

Luna should become a bounded memory system that learns continuously:

```text
predict what it already knows,
update only when the world teaches it something,
compress repeated structure into fixed state,
reconstruct the requested answer,
and prove the answer from raw lineage.
```

That is the path from a memory prototype to memory we can rely on.
