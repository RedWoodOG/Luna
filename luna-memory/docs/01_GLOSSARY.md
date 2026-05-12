# Luna Memory Topology Glossary

This glossary defines Luna topology terms as concrete implementation concepts.
If a term cannot map to data, lifecycle, failure, and replay behavior, it does
not belong in the engine.

## Raw Event

An immutable input record. A raw event has an id, timestamp, source, payload,
and stable content hash. Raw events are the root of provenance.

## Event Hash

A deterministic SHA-256 digest over the canonical serialized event content.
The hash changes if immutable event content changes. It does not depend on
runtime memory addresses or serialization map order.

## Ledger

The append-only storage boundary for raw events and topology mutation events.
The ledger is the source replay reads from.

## Node

A typed memory unit derived from one source event. Milestone 0 nodes carry id,
kind, label, source event id, and source event hash. The topology registry must
contain exactly one genesis certificate for each promoted node.

## Genesis Certificate

An immutable proof that a node entered the topology from a specific source
event. It records node id, source event id, source event hash, creation time,
and its own certificate hash. A node has exactly one genesis certificate.

## Tether

A directional typed edge between two nodes. A tether records source node,
target node, tether kind, traversal semantics, source event id, and source event
hash. Reverse direction is a separate tether with separate meaning.

## Direction

The explicit orientation of a tether. `A -> B` and `B -> A` are not equivalent.
For example, `A -> B = supported_by` means A depends on B as evidence, while
`B -> A = evidence_for` means B contributes evidence to A.

## Cluster

A future dense region of nodes and tethers that can be activated or compressed
as a group. In Milestone 0, clusters are documented but not implemented.

## Compression

A future operation that creates a smaller representation of topology. Compression
may reduce active context size. It must preserve source event lineage.

## Recognition Spark

A future activation input that marks a bounded part of topology as relevant.
Every activation must carry triggering signals, confidence, lineage, and conflict
report. In Milestone 0, recognition sparks are documented but not implemented.

## Sentinel Cluster

A future monitoring structure that flags defects such as contradiction pressure,
provenance loss, retrieval precision drop, or unsafe topology transitions.
Sentinels do not rewrite source truth.

## Inspector

A synchronous invariant checker that runs before a mutation commits. Inspectors
are binary: pass the mutation or reject it with an inspectable reason. They do
not repair malformed transitions.

## Gauge

A continuous numerical observer for flow rates and structural metrics. Gauges
report drift from thresholds or rolling baselines; they do not decide whether a
defect exists.

## Auditor

A periodic deep-replay verifier. Auditors replay a ledger window in isolation,
compare it against live state, and raise alarms or quarantine divergent derived
state for human review.

## Splinter

A future topology evolution where an unstable memory cluster divides into smaller
regions. Splinters must preserve parent lineage and replay reversibility.

## Merge

A future topology evolution where compatible regions combine. Merges must record
parents, cause, acceptance evidence, and replay trace.

## Replay

Deterministic reconstruction of derived topology from ledger events. Replay is
the proof that stored state is not relying on hidden mutation.

## Provenance

The chain that links a derived structure back to raw evidence. In Milestone 0,
required provenance is source event id and source event hash. Future structures
may add assertion ids, cluster ids, compression ids, or mutation event ids.
