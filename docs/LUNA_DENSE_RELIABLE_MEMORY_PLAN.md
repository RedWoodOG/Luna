# Luna Dense Reliable Memory Plan

Status: active build track

Purpose: make Luna's memory denser, more selective, and more reliable as it
learns. Luna should gain usable memory without growing into transcript stuffing,
flat JSON recall, or a pile of unrelated facts.

The target is:

```text
experience
-> surprise and salience scoring
-> event-sourced episodic capture
-> replay and mistake review
-> lineage-preserving consolidation
-> bounded activation
-> specific answer with provenance
-> audit and regression memory
```

This plan treats human memory research as engineering inspiration. The code
truth remains Luna's build contract:

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

## Reliability Standard

Luna becomes reliable when each memory behavior has four inspectable surfaces:

- **Capture**: what Luna stored, with source event id/hash.
- **Selection**: why a memory entered or stayed out of working memory.
- **Consolidation**: what was compressed or clustered, with raw ancestry intact.
- **Learning From Misses**: every miss becomes a durable regression scenario or
  deferred issue with evidence.

The first controlled trial created the right loop:

```text
trial miss
-> scored packet
-> regression scenario
-> generic mechanism fix
-> rerun trial
-> commit gate
```

That loop is now the model for making Luna more dependable.

## Dense Memory Architecture

### 1. Surprise-Gated Intake

Luna should store the difference between what the current memory model already
predicts and what the new turn adds.

Build shape:

- compute a per-turn `surprise_score`;
- compare new assertions with current entity/project/person/manuscript state;
- classify turns into expected, reinforcing, novel, corrective, contradictory,
  or unresolved;
- store high-surprise and high-salience facts with full source lineage;
- store low-surprise repeated facts as reinforcement rather than duplicate
  claims.

Inspectable output:

- intake report shows `surprise_score`, `redundancy_score`,
  `correction_pressure`, and selected action.

First guardrail:

- scenario where repeated project facts reinforce existing memory, while a
  rename and a new pilot partner create separate current claims.

### 2. Episodic Buffer With Fixed Budget

Luna should keep raw event truth forever, while active episodic memory stays
bounded.

Build shape:

- preserve raw event log as authority;
- add an episodic-buffer view with fixed active capacity;
- rank episodes by surprise, recency, correction pressure, user salience,
  unresolved-loop value, and recall success/failure;
- expose which episodes were active, quiet, or eligible for consolidation.

Inspectable output:

- runtime inspect shows active episode budget and why each episode was retained
  or quieted.

First guardrail:

- scenario where a quiet but directly cued episode remains retrievable without
  expanding the working-memory packet.

### 3. Replay Review

Luna should periodically replay its own event evidence and repair its derived
state before memory expands.

Build shape:

- sample recent, high-surprise, corrected, and failed-recall episodes;
- rebuild memory state from raw events;
- compare live and replayed topology/cluster hashes;
- emit a replay review report with repaired claims, unstable clusters, and
  required regression candidates.

Inspectable output:

- `runtime audit` reports replay review status, not only clean/diverged hashes.

First guardrail:

- forced stale answer in a trial packet produces a regression candidate with the
  exact question, answer, expected evidence note, and source event hashes.

### 4. Lineage-Preserving Consolidation

Luna should compress dense related memory only when it can still prove every raw
source behind the compressed artifact.

Build shape:

- use existing compression receipts as the consolidation contract;
- form cluster/interface nodes for repeated people, projects, relationships,
  manuscript arcs, and corrections;
- record included claims, excluded claims, source event ids/hashes, and reason;
- allow compressed memory into working memory only when verified source hashes
  are available.

Inspectable output:

- context packet cites raw source event ids/hashes and cluster receipt id
  together.

First guardrail:

- project-memory scenario where a compressed project cluster answers purpose,
  current name, and pilot-partner questions without surfacing excluded stale or
  irrelevant details.

### 5. Specific Activation

Luna should activate the right slice of memory, not merely the nearest entity.

Build shape:

- activation components include entity match, relation/kind match, cue match,
  lifecycle, confidence, recency, correction pressure, cluster authority, and
  query specificity;
- project-purpose queries prefer purpose claims;
- name/current-state queries prefer identity/correction claims;
- partner/pilot/who-with queries prefer relationship or participant claims;
- filtered-out memory is visible in inspect, context, and scenario reports.

Inspectable output:

- activation report lists selected and filtered claims with component scores.

First guardrail:

- `controlled_trial_project_memory_regression.json` proves Luna can remember
  multiple related facts while answering the specific requested one.

### 6. Mistake Memory

Luna should remember its own failures as build evidence, not as user facts.

Build shape:

- controlled-trial scoring creates a structured miss artifact;
- misses map to regression scenario candidates;
- regression scenarios keep the original source turns, questions, expected
  evidence, forbidden leakage, and audit requirements;
- Luna's docs and release packets distinguish passed trial evidence from
  regression evidence.

Inspectable output:

- miss packet links to committed scenario id and current pass/fail status.

First guardrail:

- script that checks every controlled-trial miss is either linked to a scenario
  file or marked deferred with a reason.

## Build Sequence

### D1: Controlled Trial Miss Loop

Current status: first slice landed for project memory.

Evidence:

- failed packet:
  `.luna/controlled-human-trial/20260514-092422`
- fixed regression:
  `scenarios/runtime/controlled_trial_project_memory_regression.json`
- scenario is registered in `scenarios/runtime/SCENARIO_MANIFEST.txt`
- trial rerun answered current name, purpose, and pilot partner with clean audit.

Next build:

- add a miss-to-regression index so future controlled-trial misses cannot stay
  as free-text notes.

### D2: Surprise-Gated Intake Report

Deliverable:

- runtime turn output includes `surprise_score`, `redundancy_score`, and
  `correction_pressure`.

Guardrail:

- repeated facts reinforce; genuinely new facts store; corrections supersede;
  ambiguous facts ask for an anchor.

### D3: Specific Activation Components

Deliverable:

- activation report exposes component scores and filtered claims.

Guardrail:

- same entity with multiple facts answers purpose/name/partner/location
  questions with the requested slice only.

### D4: Replay Review Packet

Deliverable:

- replay review identifies unstable memory and creates regression candidates.

Guardrail:

- a seeded bad answer produces a review artifact that points to the source log,
  question, wrong answer, expected evidence, and scenario candidate.

### D5: Product Consolidation Receipts

Deliverable:

- product runtime can feed verified compression receipts into recall.

Guardrail:

- compressed context answers correctly while preserving raw event citations and
  excluding stale or irrelevant details.

### D6: Dense Memory Trial

Deliverable:

- 10+ turns with multiple related people/projects/corrections/distractors;
- reopen same log;
- ask detail-specific questions;
- require specific answers, confidence, provenance, and clean replay.

Guardrail:

- packet passes only when Luna remembers multiple related facts and chooses the
  requested slice.

## Current North Star

The near-term proof is no longer just "does Luna remember a fact?" It is:

```text
Can Luna learn many related facts,
retain them through replay,
compress or quiet what is not needed,
and answer the specific question from provenance-backed memory?
```

That is the path from a memory concept to a memory system we can rely on.
