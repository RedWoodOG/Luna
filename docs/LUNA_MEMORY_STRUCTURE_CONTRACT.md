# Luna Memory Structure Contract

This contract is the build standard for Luna memory work.

Luna is not allowed to become ordinary saved facts, transcript stuffing, or a
phrase-to-answer map. Luna memory must become a durable, inspectable structure
that can survive dissection.

Every memory change must either build or protect the structure below.

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

If a change cannot be placed in that structure, it is not ready.

## The Contract

### 1. Event Log First

Every user, project, manuscript, or runtime memory must come from persisted event
evidence.

Allowed:

- derive memory from logged turns or durable system events;
- rebuild memory state from the event log;
- cite source event hashes or provenance when memory surfaces.

Forbidden:

- hidden memories;
- seed facts outside the event path;
- state that cannot be rebuilt from logged evidence.

### 2. Intake Before Storage

Every turn that affects memory must pass through an explicit intake decision.

Required intake outcomes:

- `Accept`
- `StoreWithUncertainty`
- `AskForAnchor`
- `IgnoreNoise`
- `SupersedeOrCorrect`
- `MarkUnknown`

The intake decision must be inspectable. If Luna stores, ignores, corrects, or
asks, the reason must be visible.

### 3. Typed Memory, Not Loose Notes

Memory must become typed structure.

Required direction:

- people become people;
- projects become projects;
- places become places;
- goals become goals;
- roles become roles;
- preferences and principles become inspectable claims;
- corrections become lifecycle transitions;
- uncertainty stays visible.

Flat assertions may exist, but they are not enough. The graph must be able to
derive entities, relations, confidence, lifecycle status, and source evidence.

### 4. Current, Old, Uncertain, And Corrected Must Coexist

Luna must not delete old memory just because newer memory exists.

Corrections must:

- preserve the old claim;
- mark the old claim non-current when appropriate;
- mark the new claim current when appropriate;
- prevent superseded memory from answering as current;
- keep the correction inspectable.

If old and new claims both matter, Luna must preserve the ambiguity instead of
flattening it into a clean story.

### 5. Tiny Working Memory

Luna may store a large memory graph, but each answer may use only a bounded
working set.

Required evidence:

- active nodes and edges are inspectable;
- filtered memory is counted or explainable;
- answers do not depend on dumping the transcript into context;
- distractor chatter stays out of the answer surface and downstream context.

### 6. Answers Need Provenance

Every answer from memory must be traceable.

Required evidence:

- what memory was recalled;
- why it was recalled;
- confidence tier;
- lifecycle status;
- source/provenance path when available;
- what was filtered out when that matters.

If Luna cannot explain why an answer used a memory, the answer is not trusted.

### 7. Dissection Is Mandatory

Every memory feature must be dissectable through repo tools.

At minimum, a memory PR must provide at least one of:

- runtime scenario;
- focused Rust test;
- inspect artifact;
- replay/audit artifact;
- controlled trial packet.

For user-facing memory behavior, prefer all four:

```text
scenario
inspect
audit
answer transcript
```

The evidence must fail when the promised behavior regresses.

## Vocabulary Expansion Rule

Luna needs a broad memory vocabulary, but vocabulary must be generic.

Allowed:

- generic relation capture such as project purpose, active focus, collaborator
  role, review criterion, principle, preference, responsibility, location, goal,
  and correction;
- values derived from input evidence;
- reusable subject/relation/object mechanisms;
- typed graph projection from those claims.

Forbidden:

- scenario-specific phrase patches;
- hardcoded people, projects, or expected answers;
- branches that answer because a benchmark phrase appeared;
- broad substring matching that confuses one entity for another.

The same mechanism must work for new entities and values without adding a new
code path for each case.

## PR Readiness

A memory PR is not ready until it can answer these questions in plain language:

1. What event evidence created or changed memory?
2. What intake decision was made, and why?
3. What typed claims or relations were created?
4. What lifecycle status changed?
5. What entered working memory, and what stayed out?
6. What provenance supports the answer?
7. What test, scenario, inspect output, or audit proves this?

If any answer is missing, the work is incomplete.

## No-Slip Rule

Do not weaken this contract to make a feature pass.

If the contract blocks a desired behavior, either:

- improve the implementation until it satisfies the contract; or
- revise the contract explicitly in a doctrine PR that explains the tradeoff.

Silent exceptions are not allowed.
