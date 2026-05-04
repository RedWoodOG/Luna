# Luna Memory Milestone And Roadmap

This document records the current memory milestone and the remaining roadmap.
It is meant to keep each build day concrete: finish memory capabilities without
drifting into ordinary RAG, transcript stuffing, or impressive-sounding magic.

## Current Milestone

Luna now has the first defensible product-track memory loop:

```text
user turn
-> LLM or heuristic extraction
-> runtime fine capture
-> confidence-tiered assertions
-> append-only event log
-> rebuilt memory state
-> derived entity groups
-> derived memory map
-> bounded working memory
-> conversational reply or workbench output
```

What is real now:

- Runtime can use the LLM-backed extraction path.
- Runtime can run as a conversational terminal loop.
- Runtime stores memory as events and rebuilds state from those events.
- Assertions carry `Confirmed`, `Inferred`, or `Unconfirmed` tiers.
- Proof behavior and product behavior are separated.
- RootOrb exists as inspectable behavioral substrate, not user memory.
- Working memory enforces a small active packet.
- Entity memory groups are derived for `self`, people, and projects.
- The memory map now includes entity nodes, not only flat claim nodes.
- Person-specific recall can answer from an entity cluster.
- A runtime scenario covers the Joe / Chris / Francois dense-memory case.

What this milestone proves:

- Luna is no longer only a flat pile of JSON claims.
- Luna can derive stable entity clusters from event-sourced assertions.
- Luna can keep quiet memory retrievable without dumping everything into each
  answer.
- Luna can be tested with ENCODE -> DISTRACT -> RETRIEVE scenarios.

What this milestone does not prove yet:

- It does not prove long-horizon continuity.
- It does not prove manuscript one-read memory.
- It does not prove robust correction, contradiction, or stale fact handling.
- It does not prove question quality.
- It does not prove that activation is good enough as memory grows.

## North Star

The final memory destination is one-read continuity:

```text
read manuscript once
-> close source
-> survive time and interference
-> answer questions about characters, plotlines, timelines, flashbacks,
   contradictions, and unresolved arcs
```

The near-term acceptance test remains:

```text
10-turn real conversation
-> close terminal
-> return 24 hours later
-> ask 3 questions
-> answer correctly with confidence, unknowns, ambiguity, and recall reasons
```

Every memory PR should either move Luna toward that test or protect something
already needed for it.

## Core Architecture Target

The memory stack should keep this shape:

```text
raw event log
-> extracted assertions
-> intake policy
-> typed entities / relations / events
-> episodes
-> derived memory map
-> activation field
-> bounded working memory
-> response plan
-> model or tool output
-> event commit
```

The LLM may help extract, reason, and speak. It is not the memory authority.
The event log and derived memory structures are the authority.

## Roadmap

### 1. Entity Graph Hardening

Goal:

Turn entity groups into a stronger typed graph without making a giant ontology.

Needed:

- Represent people, projects, places, goals, relationships, and events as typed
  nodes.
- Convert flat assertions into relation edges where possible:

```text
person:Chris --lives_in--> place:Iowa
person:Chris --has_interest--> basketball
person:Chris --has_goal--> retire his wife
```

- Preserve original assertion text as provenance.
- Keep the typed graph derived from events, never hand-edited as truth.

Scenario gates:

- Chris and Francois share one trait but have different locations.
- Querying Chris does not return Francois-only facts.
- Querying Francois does not return Chris-only facts.
- Shared facts can appear for both when appropriate.

### 2. Memory Intake Policy

Goal:

Make Luna decide how a turn should affect memory before it blindly stores facts.

Policy actions:

```text
ACCEPT
STORE_WITH_UNCERTAINTY
ASK_FOR_ANCHOR
IGNORE_NOISE
SUPERSEDE_OR_CORRECT
MARK_UNKNOWN
```

Examples:

```text
"Chris lives in Iowa."
-> ACCEPT
```

```text
"he moved again."
-> ASK_FOR_ANCHOR
```

```text
"blue chair twelve river battery"
-> IGNORE_NOISE or ASK_FOR_ANCHOR
```

```text
"Actually Chris lives in Ohio now."
-> SUPERSEDE_OR_CORRECT
```

Scenario gates:

- Random word strings are not treated as deep personal memory.
- Ambiguous pronouns create useful questions, not assumptions.
- Corrections supersede old facts without deleting provenance.

### 3. Correction, Supersession, And Staleness

Goal:

Make memory temporal and revisable.

Needed:

- Mark facts as current, stale, superseded, or contradicted.
- Keep old facts in provenance.
- Prefer current facts during recall.
- Explain when an answer changed because a later turn corrected it.

Scenario gates:

- Encode: Chris lives in Iowa.
- Correct: Actually Chris moved to Ohio.
- Retrieve: Luna answers Ohio and can say Iowa was older/superseded.

### 4. Activation Over Graph Paths

Goal:

Replace shallow recall with computed activation over entities, relations, and
episodes.

Minimum activation shape:

```text
activation =
  entity_match
+ relation_match
+ cue_match
+ confidence
+ recency
+ unresolved_loop_fit
+ reinforcement
- staleness
- contradiction_pressure
- uncertainty
- graph_distance_penalty
```

Hard limits:

- max active nodes
- max active edges
- max graph depth
- max open question
- max token budget for context packet

Scenario gates:

- A query about Chris activates Chris facts first.
- A query about MKPE activates project facts first.
- Old quiet memory remains retrievable when directly cued.
- Irrelevant memory does not enter the working set.

### 5. Unknown And Question Priority

Goal:

Stop every missing fact from becoming a question.

Needed:

- Unknowns become scored candidates.
- Only the most foundational unknown surfaces.
- Emotional turns balance care with fact acquisition.
- Luna can choose no question when the turn does not need one.

Scenario gates:

- "I hate her..." asks who she is, not a pile of questions.
- "I asked for a raise..." asks what the user does for work.
- Dense paragraphs do not trigger stale template questions.

### 6. Conversational Response Planner

Goal:

Make the terminal loop conversational without turning memory into scripted
answers.

Needed:

- Response plan from memory state:

```text
acknowledge
answer
ask one question
state uncertainty
cite recalled memory
avoid answering
```

- The LLM can render the response later, but the plan should be inspectable.
- Runtime must not answer from unsupported memory.

Scenario gates:

- "Who am I?" uses self entity memory.
- "What do you know about Chris?" uses Chris entity memory.
- "What did I say about MKPE?" uses project memory.
- Unknown answers say what is missing.

### 7. Real Conversation Gate

Goal:

Run the first honest 10-turn memory trial.

Required before the trial:

- LLM-backed runtime is usable.
- Entity-specific recall works.
- Inspect view is readable.
- Runtime scenario harness covers at least one dense-memory case.
- Working memory stays bounded.

Pass:

- Three later questions are answered correctly.
- Answers preserve confirmed / inferred / unconfirmed / unknown.
- Luna explains why each memory was recalled.
- Bad answers become new scenario gates.

### 8. Manuscript Memory Track

Goal:

Begin the one-read manuscript test only after conversation memory has passed at
least once.

Needed:

- Scene/event ingestion.
- Character entity graph.
- Relationship graph.
- Timeline graph.
- Flashback handling.
- Plot-state and open-loop tracking.
- One-read lockout: no rereading during retrieval.

Scenario gates:

- Character identity survives aliases and nicknames.
- Scene order and story chronology are separated.
- Flashback facts do not overwrite present-time facts incorrectly.
- Unresolved arcs remain open until resolved.

## Daily Build Rule

Each day should finish at least one concrete memory capability and one concrete
guardrail.

Capability examples:

- better entity graph mapping
- correction handling
- activation scoring
- question priority
- response planning

Guardrail examples:

- scenario test
- inspect output
- provenance check
- confidence-tier check
- working-memory budget check

If a change cannot be inspected, tested, or explained, it is not ready to become
architecture.

## Anti-Magic Checks

Before accepting a memory change, ask:

1. Is this a reusable mechanism or a scripted outcome?
2. Can it handle new names, projects, and values without another special case?
3. Is the event log still the source of truth?
4. Can the derived structure be rebuilt?
5. Does it preserve ambiguity?
6. Does it avoid dumping too much memory into context?
7. Does it explain why memory surfaced?
8. Does it have a scenario that can fail?
9. Does it help the 10-turn acceptance test?
10. Does it help the manuscript one-read target?

If the answer is no, the idea may still be useful, but it belongs in backlog
until it can be made concrete.
