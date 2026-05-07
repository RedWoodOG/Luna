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

Status: COMPLETE

Goal:

- Represent topology mutations as ledger events instead of direct state edits.
- Introduce inspector gates for the Milestone 0 invariants before mutation
  commit.

Required proof:

- Every node, tether, and certificate is rebuilt only from event history.
- Inspector rejection is recorded with mutation type, invariant, and reason.
- Orphan nodes, duplicate genesis certificates, unresolved tether endpoints, and
  undefined tether direction fail before commit.

Implemented:

- `NodeCreated`, `GenesisAttached`, and `TetherCreated` topology mutations.
- Ordered ledger records for raw events and topology mutations.
- `luna-inspector` typed rejection reasons.
- Single commit path: proposed mutation -> inspector chain -> ledger append ->
  registry apply.
- Replay from the same raw + mutation ledger.
- Compile-fail contracts for direct registry mutation bypasses.

## Milestone 2: Gauge Baselines

Status: COMPLETE

Goal:

- Add cheap continuous observers for topology flow and structural drift.

Required proof:

- Gauge metrics define formula, sampling interval, threshold or rolling baseline,
  and emitted observation event.
- Gauges report drift without rejecting writes or rewriting topology.

Initial gauges:

- Events per second into the ledger, including raw and mutation events.
- Mutation events per second.
- Average tether fan-out per node.
- Replay duration per thousand events.
- Inspector rejection rate.

Implemented:

- `luna-gauges` read-only `Gauge` trait.
- Rolling baseline window with mean and standard deviation.
- Drift detector returning `Stable` or `Drift(magnitude, direction)`.
- Separate append-only `GaugeReadingLog`.
- Gauge runtime tick loop with configurable interval and safe disabled state.
- CLI calibration command that writes reviewable threshold suggestions from
  historical JSONL reading data.

## Milestone 3: Sentinel Orbs

Status: COMPLETE

Goal:

- Add advisory topology sentinels for contradiction, provenance integrity, and
  splinter pressure defects.

Required proof:

- Sentinels declare name, defect class, score semantics, evidence, recommendation,
  and schedule.
- Sentinels consume read-only topology views.
- Sentinel reports are append-only and separate from topology truth.
- The system works with all sentinels disabled.

Implemented:

- `luna-sentinels` trait and read-only `TopologyView`.
- Append-only `SentinelReportLog`.
- Scheduler supporting on demand, every N events, and every N seconds.
- Contradiction sentinel.
- Provenance integrity sentinel.
- Splinter pressure sentinel over placeholder orb histories.

## Milestone 4: Dense Orb Formation

Status: NEXT

Goal:

- Group repeated, strongly connected topology into inspectable dense orbs.

Required proof:

- Orb membership can be serialized, logged, replayed, and rejected when cohesion
  rules fail.

## Milestone 5: Compression Without Lineage Loss

Goal:

- Compress dense orbs into smaller active representations while preserving raw
  event ancestry.

Required proof:

- Compression fidelity and provenance survival are measured separately.

## Milestone 6: Recognition Sparks

Goal:

- Activate relevant topology from triggering signals.

Required proof:

- Activation reports source, confidence, signals, lineage, and conflicts.

## Milestone 7: Auditor Deep Replay

Goal:

- Periodically replay ledger windows in isolation and compare against live state.

Required proof:

- Auditor reports ledger window, replay version, live snapshot hash, replayed
  state hash, and diff.
- Divergent derived state is quarantined for human review instead of silently
  repaired.

## Milestone 8: Splinter and Merge Mechanics

Goal:

- Evolve topology as dense regions become unstable or compatible.

Required proof:

- Splits and merges preserve ancestry, cause, reversible replay, and metrics.

## Milestone 9: Baseline Evaluation

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
