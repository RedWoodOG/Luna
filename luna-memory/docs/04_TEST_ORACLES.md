# Milestone 0 Test Oracles

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

## Future Oracle Template

Every future feature must define:

1. What data structure is introduced.
2. What lifecycle transition is allowed.
3. What invalid transition must fail.
4. What replay proves.
5. What metric or observable output falsifies the feature.
