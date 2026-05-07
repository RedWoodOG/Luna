# Milestone 0-1 Data Model

Milestone 0 keeps the model intentionally narrow. It proves that one raw event
can create one node, one genesis certificate, one directional tether, and one
replayable topology state without losing provenance.

Milestone 1 adds topology mutation events and an inspected commit path. Derived
state is no longer written by direct registry mutation; it is applied from
append-only ledger events.

## Raw Event

Required fields:

- `id`: stable event identifier.
- `recorded_at`: event processing timestamp.
- `source`: actor or subsystem that supplied the event.
- `payload`: canonical event content.
- `hash`: stable digest of canonical event content.

Invariant:

- Once stored, a raw event must not be mutated.

Hash contract:

- Raw event hash version is `luna.raw_event.v1`.
- The hash is SHA-256 over a hand-rolled canonical byte sequence:
  `version`, `id`, `source`, payload type, and payload value.
- Each canonical field is encoded as `<byte_length>:<raw_bytes>\n`.
- `recorded_at` is deliberately excluded. The hash identifies event content,
  not arrival time.
- Source tags are lowercase ASCII (`user`, `assistant`, `system`).
- Text payloads use payload type `text` and UTF-8 text bytes.
- A future hash format change must introduce a new hash version and migration
  plan instead of silently changing existing hashes.

## Memory Node

Required fields:

- `id`: node identifier.
- `kind`: typed node category.
- `label`: human-readable label.
- `source_event_id`: event that produced the node.
- `source_event_hash`: hash of the source event.

Invariant:

- A node cannot be constructed without source event id and source event hash.
- A node must have exactly one genesis certificate in the topology registry
  before replay can promote the reconstructed state.

## Topology Mutation Events

Topology mutations share the same ordered append-only ledger as raw events. A
ledger record is either `RawEventRecorded` or `TopologyMutation`.

### `NodeCreated`

Required fields:

- `node_id`: node identifier.
- `kind`: typed node category.
- `label`: human-readable label.
- `source_event_id`: raw event that produced the node.
- `source_event_hash`: hash of that raw event.

Inspector invariants:

- Source event exists.
- Source event hash matches.
- Node id is not already present.

### `GenesisAttached`

Required fields:

- `certificate_id`: certificate identifier.
- `node_id`: node receiving genesis.
- `source_event_id`: raw event used for genesis.
- `source_event_hash`: hash of that raw event.
- `created_at`: immutable certificate creation timestamp.

Inspector invariants:

- Source event exists.
- Source event hash matches.
- Node exists.
- Node does not already have a genesis certificate.

### `TetherCreated`

Required fields:

- `tether_id`: tether identifier.
- `source_node_id`: traversal source.
- `target_node_id`: traversal target.
- `kind`: forward tether meaning.
- `reverse_kind`: expected reverse meaning.
- `source_event_id`: raw event that justified the tether.
- `source_event_hash`: hash of that raw event.

Inspector invariants:

- Source event exists.
- Source event hash matches.
- Direction is explicit.
- Reverse meaning is distinct.
- Source and target endpoints exist.
- Tether id is not already present.

## Genesis Certificate

Required fields:

- `id`: certificate identifier.
- `node_id`: node created by the certificate.
- `source_event_id`: raw event id used for creation.
- `source_event_hash`: raw event hash used for creation.
- `created_at`: immutable creation timestamp.
- `certificate_hash`: digest over certificate content.

Invariants:

- A node has exactly one genesis certificate.
- A genesis certificate is immutable.
- Runtime mutation of genesis fields is forbidden.

## Tether

Required fields:

- `id`: tether identifier.
- `source_node_id`: traversal source.
- `target_node_id`: traversal target.
- `kind`: semantic tether type.
- `reverse_kind`: expected distinct meaning for the opposite direction.
- `source_event_id`: event that justified the tether.
- `source_event_hash`: hash of the source event.

Invariants:

- Direction is required.
- Reverse direction is not implied.
- Reverse direction must have distinct meaning if represented.

## Replayed Topology

Required fields:

- `ledger`: ordered raw and topology mutation events consumed by replay.
- `events`: raw events keyed by id.
- `nodes`: nodes keyed by id.
- `genesis_certificates`: certificates keyed by id.
- `tethers`: tethers keyed by id.

Replay must reject:

- A node with missing source event.
- A node whose source hash does not match the event hash.
- A node with missing genesis certificate.
- A certificate whose node or event is missing.
- A tether whose source node, target node, or source event is missing.

## Commit Pipeline

All topology writes use one path:

```text
proposed mutation
-> inspector chain
-> append ledger event
-> apply registry mutation
```

Direct registry mutation is not a public write path.
