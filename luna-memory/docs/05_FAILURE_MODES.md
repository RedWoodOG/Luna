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

- Inspector rejects `NodeCreated` before commit with `SourceEventMissing` or
  `SourceEventHashMismatch`.
- Replay rejects orphaned node creation events from malformed ledgers.

## Duplicate Genesis Certificate

Failure:

- A node receives more than one genesis certificate.

Expected behavior:

- Inspector rejects the second certificate with `DuplicateGenesis`.
- Replay fails closed on duplicate genesis.

## Undefined Tether Direction

Failure:

- A tether says two nodes are "related" without traversal direction or meaning.

Expected behavior:

- Inspector rejects it with `DirectionMissing`.
- Tests verify reverse direction has distinct meaning.

## Missing Provenance During Replay

Failure:

- A replay event references missing source event, node, certificate, or tether.

Expected behavior:

- Replay returns an error.
- No partial topology is promoted as valid.

## Missing Provenance At Write Time

Failure:

- A mutation would create an orphan node, duplicate genesis certificate,
  unresolved tether endpoint, or compression without lineage.

Expected behavior:

- Inspector rejects the mutation before commit.
- Rejection reason is inspectable.
- The rejected transition has a replay trace.

Milestone 1 rejection reasons:

- `SourceEventMissing`
- `SourceEventHashMismatch`
- `DirectionMissing`
- `DuplicateGenesis`
- `EndpointMissing`
- `NodeMissing`
- `DuplicateNode`
- `DuplicateTether`
- `ReverseMeaningNotDistinct`
- `GenesisSourceMismatch`
- `DuplicateCertificate`
- `ApplyRejected`

## Direct Registry Mutation

Failure:

- Code bypasses inspectors and writes directly into node, genesis, or tether
  registries.

Expected behavior:

- Direct registry insertion is not a public API.
- Compile-fail contracts protect the removed bypass.
- Valid writes use `proposed mutation -> inspectors -> ledger append -> registry
  apply`.

## Hidden Mutation

Failure:

- Derived state changes without a ledger event.

Expected behavior:

- Replay output differs from live state.
- Replay identity test fails.
- Auditor raises an alarm and quarantines divergent derived state for human
  review.

## Numerical Drift Without Immediate Defect

Failure:

- Flow or structure shifts from baseline: event rate changes, tether fan-out
  rises, replay duration grows, hash collision rate changes, or orb density
  distribution moves.

Expected behavior:

- Gauge reports the threshold crossing or baseline shift.
- Gauge does not reject writes or rewrite topology.
- Sentinel or human review decides whether the drift indicates a defect.

## Monitor Role Collapse

Failure:

- A single monitor abstraction tries to reject writes, score content defects,
  report metrics, and prove replay equivalence.

Expected behavior:

- Architecture review rejects the abstraction.
- The behavior is split into inspector, gauge, sentinel, or auditor jurisdiction.

## Theory Drift

Failure:

- A concept enters the architecture without data model, lifecycle, failure mode,
  test oracle, and replay proof.

Expected behavior:

- The concept remains documentation backlog only.
- No production module depends on it.
