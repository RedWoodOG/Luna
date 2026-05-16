# Luna Memory Topology Engine Roadmap

This roadmap starts with the smallest verifiable topology and expands only when
the previous layer has replay proof.

Current product-memory constraint:

- Topology milestones must now connect back to runtime memory. Memory clusters,
  compression, recognition, replay, and splinter/merge work are not done until
  product-track runtime scenarios can use their provenance, not merely prove the
  topology crate in isolation.

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

## Milestone 3: Sentinel Clusters

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
- Splinter pressure sentinel over placeholder cluster histories.
- JSONL report persistence and schedule tests for batched event counts,
  duplicate sentinel names, and on-demand isolation.

## Milestone 4: Memory Cluster Formation

Status: IMPLEMENTED (first executable slice)

Goal:

- Group repeated, strongly connected topology into inspectable memory clusters.

Required proof:

- Cluster membership can be serialized, logged, replayed, and rejected when cohesion
  rules fail.

Implemented:

- `luna-cluster` memory-cluster receipts and registry.
- `LedgerEvent::ConsolidationEvent` append/replay support.
- Milestone 4 executable tests for accepted replay, rejected receipts, forged
  trace rejection, duplicate sources, missing provenance, and duplicate cluster ids.

Weak gates repaired after the first Milestone 4 slice:

- Schema examples deserialize into Rust and pass Rust validation.
- `recorded_at` is bound into consolidation replay validation and tampering is
  rejected.

Known weak gate before relying on Milestone 4 for product memory:

- Provenance sentinels must be able to inspect cluster source nodes, tethers, source
  events, and replay hashes.

## Milestone 4.5: Runtime-To-Topology Bridge

Status: IMPLEMENTED (bridge-artifact proof slice)

Goal:

- Convert runtime assertions, entity groups, relations, and recall evidence into
  topology artifact refs and then into durable topology ledger nodes/tethers and
  eligible memory-cluster receipts.

Implemented:

- Runtime event logs can be bridged into node/tether artifacts with source
  event ids and hashes.
- `scenarios/runtime/council5_runtime_topology_bridge.json` checks bridge node
  refs, tether refs, source hashes, recall reasons, and SystemKernel leakage
  boundaries.

Remaining proof before product use:

- Product runtime turns append bridge artifacts into the durable topology ledger.
- Surfaced memory cites durable node id, tether path, source event id/hash, and
  recall reason from the same committed topology path.
- Runtime-derived memory-cluster receipts form only when policy allows them.

This milestone still gates Milestone 5 for product use. Compression receipts are
now proven in the topology lane, but runtime recall is not yet consuming
compressed topology/cluster state.

## Milestone 5: Compression Without Lineage Loss

Status: IMPLEMENTED (topology-lane receipt slice)

Goal:

- Compress memory clusters into smaller active representations while preserving raw
  event ancestry.

Implemented:

- `CompressionReceipt` records accepted and rejected compression attempts.
- Accepted compression preserves raw source event ancestry.
- Forged, lossy, or under-proven compression is rejected before append/replay.
- Replay verifies raw event ancestry exists and hashes match before promotion.

Remaining proof before product use:

- Compression fidelity and provenance survival are measured separately.
- Runtime answers cite raw source events from accepted compression receipts.
- Compression reduces active context size while preserving answer correctness.

## Milestone 6: Recognition Sparks

Status: IMPLEMENTED (runtime activation contract slice)

Goal:

- Activate relevant topology from triggering signals.

Implemented:

- Runtime activation scores entity match, relation-like match, cue match,
  recalled match, confidence, lifecycle filtering, graph depth, and
  filtered-out matching memory.
- Current confirmed memory beats equal-match stale, superseded, contradicted,
  or unconfirmed memory.
- Quiet directly cued memory remains retrievable inside a tiny working-memory
  budget.

Remaining proof before product use:

- Activation reports cite durable topology lineage and conflict paths once
  runtime recall commits through the topology/cluster ledger.

## Milestone 7: Auditor Deep Replay

Status: IMPLEMENTED (topology snapshot audit slice)

Goal:

- Periodically replay ledger windows in isolation and compare against live state.

Implemented:

- Auditor reports ledger window, replay version, live snapshot hash, replayed
  state hash, and diff.
- Divergent derived state is quarantined for human review instead of silently
  repaired.
- Valid cluster-backed topology logs audit cleanly.
- Forced divergence is detected without mutating live topology.

Remaining proof before product use:

- Scheduled auditor integration over persisted product runtime topology logs.

## Milestone 8: Splinter and Merge Mechanics

Status: IMPLEMENTED (topology-lane receipt slice)

Goal:

- Evolve topology as dense regions become unstable or compatible.

Required proof:

- Splits and merges preserve ancestry, cause, reversible replay, and metrics.

Implemented:

- `luna-cluster` split/merge evolution receipts with cause, metric refs, decision,
  rejection reason, replay trace hash, and event id validation.
- Accepted split receipts retire one parent and create child clusters carrying the
  parent's source node, tether, raw event, and evolution lineage.
- Accepted merge receipts retire multiple parents and create one child cluster
  carrying the union of parent ancestry.
- `LedgerEvent::ClusterEvolutionEvent` append/replay support for the split path.
- Milestone tests for reversible split lineage, merge ancestry survival, ledger
  replay equivalence, and forged split rejection before mutation.

Known weak gates before relying on Milestone 8 for product memory:

- Sentinel pressure does not yet trigger split receipts automatically.
- Runtime recall is not yet driven by active/retired cluster evolution state.
- Product runtime memory is still not fully topology/cluster-backed.

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
