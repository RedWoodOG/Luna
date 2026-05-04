# Luna Build Doctrine

Luna's goal is reliable continuity, not novelty for its own sake.

This document exists to keep the implementation from drifting into ordinary
"save facts, retrieve text, answer" memory. Every future memory change should be
judged against these rules.

## Product Thesis

Luna is a local-first episodic memory framework where an LLM sits inside a
governed memory runtime.

The model is not the memory authority. The model is a reasoning and language
engine that receives a bounded, evidence-backed memory state and speaks from it.

The intended runtime shape is:

```text
event evidence
-> memory intake policy
-> root ontology
-> entity/event graph
-> activation field over graph
-> bounded working set
-> response plan
-> model/tool output
-> event commit
```

Luna must become more coherent as memory grows. It must not become heavier,
noisier, or more reliant on transcript stuffing.

## Aura Lesson

AuraCore's stated architecture is useful as a warning and a reference point:

```text
runtime cognition before LLM rendering
bounded renderer capsule
field activation
scoped memory
planning before speech
post-render validation
```

That direction is valuable. The risk is language that hides implementation
truth behind phrases like "fields", "geometry", "cognition", or "mind".

For Luna, the claim must stay precise:

```text
No scripted user memories.
No scripted final answers.
Yes authored general mechanisms.
Yes inspectable rules, provenance, confidence, and activation.
```

"No hardcoding" is not enough. All software has authored code. The meaningful
standard is whether a behavior comes from a reusable mechanism or a scripted
outcome.

## Hardcoding Standard

Forbidden:

```text
if user asks "what is my name" -> answer Joe
if text contains "Chris" -> dump Chris facts
if this benchmark phrase appears -> emit expected memory
```

Allowed:

```text
detect any asserted name relation
store it with provenance
map it into an entity graph
activate it when a name/identity query appears
answer from the active working set
```

A change is doctrine-compliant only if the same mechanism can handle new
entities and new values without adding a new code path for each case.

Temporary failure patches are allowed only when:

- the failure is captured by a scenario;
- the patch is clearly scoped;
- the next generalization target is known;
- the patch does not become the primary architecture.

## Non-Negotiable Rules

1. Event Log Is Truth

Raw turns and system events are the source of truth. Derived memory maps,
working sets, summaries, and entity orbs must be rebuildable from events.

2. Memory Is Not A Summary Blob

Luna memory must remain layered:

```text
raw event log
-> extracted assertions/relations
-> entity/event graph
-> episodes
-> working memory packet
```

3. Root Memory Is Not User Memory

RootOrb and RootOntology define behavior and semantic grammar. They must not
store user facts, project facts, or manuscript facts.

4. Small Ontology, Rich Data

RootOntology should stay small and primitive:

```text
who / what / when / where / why / how
entity / relation / state / event / goal / contradiction / unknown
```

Specific people, projects, characters, locations, timelines, and story facts are
data derived from events, not root definitions.

5. Entity Memory Must Be Real

People, projects, places, events, goals, and relationships should become stable
nodes or typed structures. Luna must not rely only on flat strings like:

```text
person:location = Chris lives in Iowa
```

That string can exist as an assertion, but the deeper memory map should derive:

```text
person:Chris --lives_in--> place:Iowa
```

6. Working Set Is Tiny

Luna may store a large memory map, but each turn activates only a small,
budgeted working set. Memory that is quiet is not forgotten.

7. Ambiguity Is Preserved

Luna must preserve unknowns, uncertain labels, relationship ambiguity, stale
facts, contradictions, and superseded facts. Ambiguity must not be flattened into
clean certainty for convenience.

8. Proof And Product Stay Separate

Runtime may use unconfirmed and inferred memory. Proof benchmarks count only
confirmed/proof-eligible memory. Product usefulness must not weaken proof rules.

9. Recall Must Carry Why

Any surfaced memory should be explainable:

```text
why recalled
source event
confidence tier
relation/path if available
what was filtered out
```

10. No Unbounded Transcript Stuffing

Luna must not solve continuity by dumping old turns into context. That is the
failure mode Luna is trying to beat.

## Memory Intake Policy

Every turn should eventually pass through an explicit intake policy:

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
-> ACCEPT: person Chris, relation lives_in, place Iowa
```

```text
"he moved again."
-> ASK_FOR_ANCHOR: who moved?
```

```text
"blue chair twelve river battery"
-> ASK_FOR_ANCHOR or IGNORE_NOISE: no stable memory anchor unless user explains it
```

```text
"Actually Chris lives in Ohio now."
-> SUPERSEDE_OR_CORRECT: old residence stale, new residence current
```

## Activation Field

"Field" should mean computed activation over memory state, not mysticism.

Minimum defensible shape:

```text
activation(node) =
  entity_match
+ relation_match
+ cue_match
+ confidence
+ recency
+ unresolved_loop_fit
- staleness
- contradiction_pressure
- graph_distance_penalty
```

Then apply hard budgets:

```text
max active nodes
max active edges
max depth
max open questions
```

This lets Luna have large memory without letting memory dominate every turn.

## Scenario Gates

Every new memory feature should have at least one runtime scenario:

```text
ENCODE
DISTRACT
RETRIEVE
ASSERT
```

Scenario checks must include:

- what must be remembered;
- what must not be remembered;
- what must remain unknown;
- what should be stale or superseded if correction is involved.

Manual conversation remains necessary, but it is not enough. Regression
scenarios prevent the architecture from becoming vibes and patches.

## PR Checklist

Every memory PR should answer:

1. What failure mode does this escape?
2. Is this a general mechanism or a scripted outcome?
3. What new scenario proves it?
4. What must not happen?
5. What provenance is preserved?
6. What confidence tier is used?
7. What working-memory budget is enforced?
8. Does this strengthen the event-log -> graph -> activation path?
9. Does this preserve ambiguity instead of cleaning it away?
10. If this is a temporary patch, what is the planned generalization?

## Current Direction

The next necessary work is not UI and not more broad vocabulary.

The milestone roadmap is tracked in
[`LUNA_MEMORY_MILESTONE_ROADMAP.md`](LUNA_MEMORY_MILESTONE_ROADMAP.md).

The next necessary work is:

```text
entity orbs / typed graph mapping
entity-specific conversational recall
memory intake policy
correction and supersession
activation over graph nodes and relations
```

Only after those pieces are real should Luna expand toward the manuscript
one-read continuity test.
