# Milestone 0-2 Test Oracles

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

## Milestone 2 Oracles

### `test_gauge_trait_produces_stable_readings_on_stable_input`

Instantiate the five concrete gauges on stable input. Repeated reads must return
the same values and gauges must expose stable names, units, and formulas.

### `test_dynamic_gauge_reads_fresh_snapshot_each_tick`

A gauge backed by a live snapshot source must reflect changed source values on
later reads.

### `test_rolling_baseline_updates_across_window`

Push more readings than the configured window size. The baseline must retain
only the last N readings and compute mean and standard deviation from that
window.

### `test_drift_detector_fires_beyond_threshold`

Given a current reading and baseline statistics, drift detection must return
`Stable` inside the configured threshold and `Drift(magnitude, direction)` when
the reading exceeds it.

### `test_drift_detector_edge_cases_are_finite`

No-baseline, zero-count, zero-standard-deviation, downward drift, and exact
threshold cases must stay finite and deterministic.

### `test_gauge_reading_log_is_append_only`

Appending readings increases the log length and no mutable reading accessor is
available to tests.

### `test_gauge_reading_jsonl_round_trips`

Gauge reading persistence appends JSONL records and loads them back in order.

### `test_runtime_functions_with_all_gauges_disabled`

A gauge runtime with zero registered gauges must tick successfully, append no
readings, and leave topology correctness unaffected.

### `test_runtime_ticks_update_baselines_and_append_readings`

Runtime ticks call gauges, compare against previous baseline, append readings,
and update the rolling baseline.

### `test_runtime_rejects_duplicate_gauge_names`

Duplicate gauge names do not produce multiple same-name readings in one tick or
contaminate the rolling baseline.

### `test_calibration_suggests_threshold_from_historical_variance`

Historical gauge readings produce reviewable threshold suggestions. Calibration
does not mutate runtime thresholds.

## Milestone 3 Oracles

### `test_contradiction_sentinel_fires_on_known_bad_input`

Contradiction sentinel flags incompatible assertion tethers for the same node
and reports the conflicting tether ids.

### `test_sentinels_stay_quiet_on_known_good_input`

Contradiction, provenance, and splinter sentinels return `Quiet` for clean
topology input.

### `test_provenance_integrity_sentinel_fires_on_hash_mismatch`

Provenance sentinel reports node/certificate/tether provenance chains that do
not resolve to ledger raw event hashes or node provenance.

### `test_provenance_integrity_sentinel_fires_on_bad_tether_chain`

Provenance sentinel reports tethers with missing endpoints or mismatched source
event hashes.

### `test_splinter_pressure_sentinel_fires_on_density_precision_divergence`

Splinter pressure sentinel flags placeholder cluster history when density rises
while retrieval precision drops beyond threshold.

### `test_splinter_pressure_aggregates_across_orbs`

Splinter pressure accumulates positive pressure across multiple orbs before
comparing against the threshold.

### `test_multi_valued_predicates_are_not_contradictions`

Contradiction sentinel does not flag explicitly multi-valued predicates as
contradictions.

### `test_sentinel_reports_are_reproducible_from_same_view`

The same sentinel, topology view, and timestamp produce identical reports.

### `test_flagged_sentinel_reports_are_reproducible_from_same_view`

Flagged reports with evidence remain reproducible for the same view and
timestamp.

### `test_sentinel_report_log_is_append_only`

Appending reports preserves order and no mutable report accessor is available to
tests.

### `test_sentinel_report_jsonl_round_trips`

### `test_split_receipt_replays_with_reversible_parent_lineage`

Cluster split receipts retire the parent, create child clusters, and preserve source
event ancestry plus the split event id in child lineage.

### `test_split_receipt_appends_to_topology_ledger_and_replays`

A split receipt appended through the topology ledger replays into equivalent
cluster state with parent lineage intact.

### `test_merge_receipt_replays_without_losing_source_event_ancestry`

Cluster merge receipts retire compatible parents and create one child cluster that
retains the union of parent source event ancestry.

### `test_forged_split_receipt_is_gated_before_claiming_mechanics`

Tampering with a split receipt after hash calculation is rejected before the
cluster registry mutates.

Sentinel report persistence appends JSONL records and loads them back in order.

### `test_runtime_functions_with_all_sentinels_disabled`

Sentinel runtime with all sentinels disabled runs safely and appends no reports.

### `test_runtime_respects_event_and_time_schedules`

Sentinel runtime observes `EveryEvents` and `EverySeconds` schedules without
blocking writes.

### `test_runtime_event_schedule_handles_batched_event_counts`

Event schedules run when enough events elapsed even if polling skips exact
multiples.

### `test_runtime_rejects_duplicate_sentinel_names`

Duplicate sentinel names do not produce duplicate scheduler entries.

### `test_runtime_on_demand_is_separate_from_scheduled_polling`

On-demand sentinels do not run during scheduled polling and run only through the
explicit on-demand path.

### `test_sentinel_contract_metadata_is_declared`

Each sentinel declares stable name, defect class, and schedule.

### Compile-fail read-only topology view contract

Sentinels receive a read-only topology view; tests verify safe callers cannot
mutate view internals.

## Compression Receipt Oracles

### `test_accepted_compression_receipt_replays_with_raw_event_ancestry`

Append two raw events, issue an accepted compression receipt, and replay the
ledger. The replayed receipt must preserve the same raw event refs as the live
receipt.

### `test_compression_receipt_schema_examples_deserialize_and_validate_in_rust`

Validate every schema example with JSON Schema, deserialize it into the Rust
`CompressionReceipt`, and run Rust receipt validation. Field drift between the
schema and Rust struct must fail here.

### `test_compression_receipt_requires_recorded_output_artifact`

Issue a receipt with bound output metadata but no recorded artifact. The write
path must reject it before append.

### `test_compression_receipt_rejects_output_artifact_hash_mismatch`

Record an artifact under the receipt ref with different bytes. The write path
must reject the receipt before append with an artifact hash mismatch.

### `test_lossy_compression_receipt_is_rejected_without_accepted_record`

Issue a receipt whose retained event refs omit one input event. The receipt must
be rejected and must not appear in the accepted compression registry.

### `test_forged_compression_receipt_rejects_before_append`

Mutate a receipt field after proof creation. The write path must reject it
before append with a proof hash mismatch.

### `test_compression_receipt_requires_existing_raw_event_ancestry`

Issue a receipt that cites a missing raw event. Replay/write validation must
reject it instead of accepting under-proven compression.

## Future Oracle Template

Every future feature must define:

1. What data structure is introduced.
2. What lifecycle transition is allowed.
3. What invalid transition must fail.
4. What replay proves.
5. What metric or observable output falsifies the feature.
