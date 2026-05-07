# Milestone 0 Failure Modes

The engine fails safely when provenance or determinism is at risk. It should not
repair corrupt topology silently.

## Event Mutation

Failure:

- A stored event changes content after append.

Expected behavior:

- Hash verification fails.
- Replay rejects the event sequence if stored hash and recomputed hash differ.

## Hash Instability

Failure:

- Equivalent event content produces different hashes across calls.

Expected behavior:

- Hash stability test fails.
- Derived structures are not trusted.

## Orphan Node

Failure:

- A node exists without a source event id or source event hash.

Expected behavior:

- Constructor rejects it.
- Replay rejects orphaned node creation events.

## Duplicate Genesis Certificate

Failure:

- A node receives more than one genesis certificate.

Expected behavior:

- Topology builder rejects the second certificate.
- Replay fails closed on duplicate genesis.

## Undefined Tether Direction

Failure:

- A tether says two nodes are "related" without traversal direction or meaning.

Expected behavior:

- Constructor rejects it.
- Tests verify reverse direction has distinct meaning.

## Missing Provenance During Replay

Failure:

- A replay event references missing source event, node, certificate, or tether.

Expected behavior:

- Replay returns an error.
- No partial topology is promoted as valid.

## Hidden Mutation

Failure:

- Derived state changes without a ledger event.

Expected behavior:

- Replay output differs from live state.
- Replay identity test fails.

## Theory Drift

Failure:

- A concept enters the architecture without data model, lifecycle, failure mode,
  test oracle, and replay proof.

Expected behavior:

- The concept remains documentation backlog only.
- No production module depends on it.
