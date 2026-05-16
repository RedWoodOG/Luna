# Luna Memory Topology Engine Concept

Luna Memory Topology Engine is the proof track for turning raw experience into
auditable memory topology. The engine starts from immutable raw events, derives
typed nodes, connects nodes with directional tethers, groups dense regions into
orbs, compresses only with retained lineage, and reconstructs all derived state
by replay.

The first objective is deliberately small:

```text
raw event
-> memory node
-> genesis certificate
-> directional tether
-> deterministic replay
-> identical reconstructed state
```

This package verifies that the foundation is representable, inspectable,
replayable, measurable, and falsifiable before any higher-order reasoning-style
features are allowed.

## What This Engine Is

- An event-sourced memory topology.
- A typed graph of nodes and directional tethers derived from raw events.
- A replayable system where derived state is never the source of truth.
- A provenance-preserving compression target for future memory clusters.
- A guardrail against metaphor-only memory claims.

## What This Engine Is Not

- Not consciousness, sentience, emotion, or biological emulation.
- Not ordinary RAG with poetic names.
- Not a summary store that replaces source events.
- Not a hidden mutable state machine.
- Not a place for unverifiable intelligence claims.

## Milestone 0 Verification Target

Given one input event:

1. The event is stored immutably.
2. The event hash is stable.
3. A node is created from the event.
4. The node links back to the event.
5. A genesis certificate exists for the node.
6. A tether direction is explicit.
7. Replay reconstructs the same state.
8. Replay fails if any provenance link is missing.

Passing this target proves only the foundation. It does not prove useful recall,
cluster compression, sentinel behavior, splintering, merging, or long-horizon memory.
Those features must each graduate through their own data structure, lifecycle
rule, failure mode, test oracle, and replay proof.

## Design Commitments

- Raw events are append-only and hash-addressed.
- Nodes cannot exist without a source event.
- Genesis certificates are created once and never mutated.
- Tethers are directional; reverse tethers have distinct meaning.
- Replay is deterministic and treats missing provenance as an error.
- Compression may reduce context size but may not erase origin lineage.
- Sentinel systems may flag or block unsafe transitions but may not rewrite truth.

## Graduation Rule

No feature graduates until it has:

1. Data structure.
2. Lifecycle rule.
3. Failure mode.
4. Test oracle.
5. Replay proof.
