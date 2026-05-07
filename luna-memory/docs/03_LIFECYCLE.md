# Milestone 0-1 Lifecycle

Milestone 0 lifecycle rules are small enough to test exhaustively. Later memory
features must add their own lifecycle rules before implementation.

## 1. Record Raw Event

Input content is wrapped as a raw event and assigned an id, timestamp, source,
payload, and stable hash.

Rules:

- Store by append-only ledger operation.
- Do not rewrite existing event content.
- Recomputeing the hash from event content must produce the stored hash.

## 2. Create Node From Event

A node is derived from the raw event.

Rules:

- Node creation requires source event id.
- Node creation requires source event hash.
- Node source hash must match the ledger event hash during replay.

## 3. Attach Genesis Certificate

The node receives one immutable genesis certificate.

Rules:

- Certificate creation happens once per node.
- Certificate records node id, event id, event hash, creation time, and hash.
- Later topology mutation events may reference genesis but may not modify it.

## 4. Create Directional Tether

A tether connects two nodes or a node to an evidence node with explicit direction.

Rules:

- Source node and target node are both required.
- Tether kind defines forward meaning.
- Reverse meaning is separate and must be named explicitly.

## 5. Replay Ledger

Replay reads the ordered topology replay ledger and reconstructs topology state.
The raw-event ledger remains part of the reconstructed state; the replay ledger
records which raw events, nodes, certificates, and tethers were applied.

Rules:

- Replay is deterministic for the same event sequence.
- Replay output must match the live topology built through the registry path.
- Replay verifies every provenance link.
- Replay fails closed on missing provenance.
- Replay output is compared against the live state as the proof oracle.

## 6. Reject Invalid Transitions

Invalid topology transitions become errors, not best-effort repairs.

Examples:

- Node without source event: reject.
- Tether without direction: reject.
- Duplicate genesis certificate for same node: reject.
- Source event hash mismatch: reject.

## 7. Commit Topology Mutation

Milestone 1 turns topology writes into inspected mutation events.

Commit path:

```text
proposed topology mutation
-> inspector chain
-> append-only ledger event
-> registry apply
```

Rules:

- `NodeCreated`, `GenesisAttached`, and `TetherCreated` are ledger events.
- Inspectors run before a mutation is appended.
- Rejected mutations return a typed rejection reason and do not enter the ledger.
- Registry application consumes mutation events; direct registry mutation is not
  the public write path.
- Replay reads the same raw and mutation ledger events in order.
- Any successful commit prefix must replay, even if later related mutations have
  not happened yet.
- Low-level unchecked append and fabricated inspection contexts are unsafe
  internals, not safe public write paths.

## 8. Monitor Roles

Monitoring has separate lifecycle roles. The roles are not interchangeable.

Rules:

- Inspectors run synchronously before mutation commit and reject malformed
  transitions.
- Gauges sample numerical metrics continuously and report drift from thresholds
  or rolling baselines.
- Sentinels run asynchronously over topology content and flag emergent defects.
- Auditors periodically replay ledger windows and compare replayed state against
  live state.
- No monitor role rewrites raw events, provenance, or genesis certificates.

The full role boundaries live in `07_MONITOR_ROLES.md`.
