# Milestone 0 Lifecycle

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

Replay reads ledger events in order and reconstructs topology state.

Rules:

- Replay is deterministic for the same event sequence.
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
