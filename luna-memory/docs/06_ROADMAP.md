# Luna Memory Topology Engine Roadmap

This roadmap starts with the smallest verifiable topology and expands only when
the previous layer has replay proof.

## Milestone 0: Concept Verification Package

Goal:

- Prove raw event -> node -> genesis certificate -> tether -> replay.

Pass condition:

- One input event is immutable, hash-stable, node-backed, genesis-certified,
  directionally tethered, and replayed into identical state.

Implemented crates:

- `luna-ledger`: append-only event log.
- `luna-node`: typed memory nodes.
- `luna-tether`: directional typed edges.
- `luna-genesis`: genesis certificates.
- `luna-replay`: deterministic reconstruction.
- `luna-tests`: fixtures and validation.

Build target:

- `cargo +stable test -p luna-tests`

The repository also contains pre-existing product-track Luna crates for runtime,
extraction, recall, metrics, benchmarks, storage, CLI behavior, and current core
types. Those crates are not Milestone 0 topology stubs. They remain in the root
workspace because they are landed project code with their own tests and runtime
scenario gates. The Milestone 0 graduation package is limited to the crates
listed above.

## Milestone 1: Provenance-Preserving Topology Events

Goal:

- Represent topology mutations as ledger events instead of direct state edits.

Required proof:

- Every node, tether, and certificate is rebuilt only from event history.

## Milestone 2: Dense Orb Formation

Goal:

- Group repeated, strongly connected topology into inspectable dense orbs.

Required proof:

- Orb membership can be serialized, logged, replayed, and rejected when cohesion
  rules fail.

## Milestone 3: Compression Without Lineage Loss

Goal:

- Compress dense orbs into smaller active representations while preserving raw
  event ancestry.

Required proof:

- Compression fidelity and provenance survival are measured separately.

## Milestone 4: Recognition Sparks

Goal:

- Activate relevant topology from triggering signals.

Required proof:

- Activation reports source, confidence, signals, lineage, and conflicts.

## Milestone 5: Sentinel Orbs

Goal:

- Monitor defects such as contradiction pressure, provenance loss, retrieval
  precision drop, and unsafe topology transitions.

Required proof:

- Sentinels flag, recommend, score, or block; they do not rewrite truth.

## Milestone 6: Splinter and Merge Mechanics

Goal:

- Evolve topology as dense regions become unstable or compatible.

Required proof:

- Splits and merges preserve ancestry, cause, reversible replay, and metrics.

## Milestone 7: Baseline Evaluation

Goal:

- Compare Luna topology against baseline RAG, baseline graph retrieval, and prior
  Luna versions.

Required metrics:

- Recall precision.
- Contradiction rate.
- Provenance survival.
- False activation.
- Retrieval latency.
- Compression fidelity.
- User correction rate.
