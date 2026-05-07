# Milestone 0-1 Test Oracles

These tests define the first hard verification package. They are intentionally
mechanical: a feature either preserves provenance and replays, or it fails.

## `test_event_is_immutable`

Append one raw event, then attempt to append another event with the same id and
different payload. The ledger must reject the conflicting duplicate.

## `test_event_hash_is_stable`

Hash the same raw event content more than once. The resulting hash must be
identical.

## `test_event_hash_excludes_recorded_at`

Change only the `recorded_at` timestamp. The event hash must remain identical,
and hash verification must still pass because the hash covers content, not
arrival time.

## `test_event_hash_changes_when_content_changes`

Change event content while keeping the event id fixed. The resulting hash must
change.

## `test_node_requires_source_event`

Attempt to create a node without source event provenance. Construction must
return an error.

## `test_genesis_certificate_is_created_once`

Create a node and genesis certificate, then attempt to create a second genesis
certificate for the same node. The topology builder must reject it.

## `test_tether_requires_direction`

Attempt to create a tether without directional semantic meaning. Construction
must return an error.

## `test_reverse_tether_has_distinct_meaning`

Create `A -> B = supported_by` and verify the reverse meaning is
`evidence_for`, not another copy of `supported_by`.

## `test_replay_reconstructs_identical_state`

Build the Milestone 0 topology through the live registry path while recording
the same topology events into the replay ledger. Replay from that ledger. The
replayed topology must equal the live topology.

## `test_replay_is_deterministic_for_same_ledger`

Replay the same ledger twice. Both replayed states must be identical. This test
proves determinism; the live-vs-replay test proves hidden mutation detection.

## `test_missing_provenance_fails`

Replay a node creation that references a missing source event. Replay must
return an error instead of creating orphaned derived state.

## Milestone 1 Oracles

### `test_mutations_flow_through_append_only_ledger`

Commit one raw event and the M0 topology mutations. The ordered ledger must
contain one `RawEventRecorded` event followed by `TopologyMutation` variants for
node creation, genesis attachment, and tether creation.

### `test_commit_pipeline_preserves_live_replay_equality`

Build topology through the commit path, replay from the same ledger, and assert
live state equals replayed state.

### `test_missing_source_event_rejects_with_specific_error`

Attempt to commit `NodeCreated` with a missing source event. Inspector rejection
must be `SourceEventMissing`.

### `test_source_hash_mismatch_rejects_with_specific_error`

Attempt to commit a mutation whose source event id exists but whose source hash
does not match the raw event. Inspector rejection must be
`SourceEventHashMismatch`.

### `test_duplicate_node_rejects_with_specific_error`

Attempt to commit a second node with the same id. Inspector rejection must be
`DuplicateNode`.

### `test_apply_rejected_preserves_topology_snapshot`

Attempt a mutation that passes contextual inspection but fails registry
application, such as an empty node label. Commit must return `ApplyRejected` and
the full live topology snapshot must remain unchanged.

### `test_tether_missing_direction_rejects_with_specific_error`

Attempt to commit `TetherCreated` without a forward direction. Inspector
rejection must be `DirectionMissing`.

### `test_duplicate_genesis_rejects_with_specific_error`

Attempt to attach a second genesis certificate to the same node. Inspector
rejection must be `DuplicateGenesis`.

### `test_duplicate_certificate_id_rejects_with_specific_error`

Attempt to reuse a genesis certificate id. Inspector rejection must be
`DuplicateCertificate`.

### `test_missing_genesis_node_rejects_with_specific_error`

Attempt to attach genesis to a missing node. Inspector rejection must be
`NodeMissing`.

### `test_genesis_source_mismatch_rejects_before_ledger_append`

Attempt to attach genesis using a different raw event than the node's source.
Inspector rejection must be `GenesisSourceMismatch`, and the ledger length must
not change.

### `test_unresolved_tether_endpoint_rejects_with_specific_error`

Attempt to create a tether whose endpoint is absent from the topology. Inspector
rejection must be `EndpointMissing`.

### `test_duplicate_tether_rejects_with_specific_error`

Attempt to reuse a tether id. Inspector rejection must be `DuplicateTether`.

### `test_reverse_meaning_rejects_with_specific_error`

Attempt to create a tether whose forward and reverse meanings are identical.
Inspector rejection must be `ReverseMeaningNotDistinct`.

### `test_each_successful_commit_prefix_replays`

After each successful commit prefix, replay the accepted ledger prefix and assert
it equals the live prefix. This catches crash/audit windows between related
mutation events.

### Compile-fail direct mutation contracts

Direct registry insertion and direct tether construction are compile-fail API
contracts. Safe direct ledger mutation append and safe fabricated inspection
pass minting are also compile-fail contracts. The public write path is the commit
pipeline.

## Future Oracle Template

Every future feature must define:

1. What data structure is introduced.
2. What lifecycle transition is allowed.
3. What invalid transition must fail.
4. What replay proves.
5. What metric or observable output falsifies the feature.
