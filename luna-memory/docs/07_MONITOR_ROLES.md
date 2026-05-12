# Monitor Roles

Luna needs more than sentinel orbs. A single monitoring abstraction becomes a
catch-all and eventually hides which layer failed. Monitor roles are separated
by jurisdiction, cost, timing, allowed action, and replay trace.

None of these roles rewrite truth. They observe, reject, report, flag, block, or
raise alarms according to their jurisdiction.

## Clean Separation

| Role | Timing | Cost | Jurisdiction | Allowed actions |
| --- | --- | --- | --- | --- |
| Inspector | Synchronous on mutation | Fast | Structural invariants | Reject malformed transition |
| Gauge | Continuous / every tick | Cheap | Numerical drift from baseline | Report threshold or baseline shift |
| Sentinel | Asynchronous | Moderate | Emergent topology defects | Flag, recommend, score, block unsafe transition |
| Auditor | Periodic background | Expensive | Live state vs replay equivalence | Raise alarm, quarantine divergent state |

## Inspectors

Inspectors run before a mutation commits. They are binary: the invariant holds
or the transition is rejected.

Examples:

- Every new node has exactly one genesis certificate before promotion.
- Every tether has resolvable endpoints.
- Every tether has explicit direction and distinct reverse meaning.
- Every compression event carries lineage to its source orbs.
- No genesis certificate field changes after creation.

Required structure:

- Inspector id.
- Mutation event type.
- Invariant checked.
- Pass or reject result.
- Rejection reason.
- Replay trace showing the checked mutation and result.

Allowed:

- Reject malformed transitions.
- Return inspectable reasons.
- Emit audit events about rejected transitions.

Forbidden:

- Rewrite the mutation into something valid.
- Delete evidence.
- Invent missing provenance.
- Promote partial topology.

Build order:

- Inspectors come first after Milestone 0 because they protect the same failure
  modes already named in `05_FAILURE_MODES.md`.

## Gauges

Gauges are continuous numerical observers. They are not defect judges; they
surface drift from baseline so sentinels or humans can ask why.

Examples:

- Events per second into the ledger, including raw and mutation events.
- Average tether fan-out per node.
- Replay duration per thousand events.
- Hash collision rate.
- Cluster density distribution.

Required structure:

- Gauge id.
- Metric name.
- Formula.
- Sampling interval.
- Rolling baseline definition.
- Threshold or standard-deviation trigger.
- Emitted observation event.

Allowed:

- Report metric changes.
- Trigger notifications or downstream sentinel review.

Forbidden:

- Reject mutations.
- Rewrite topology.
- Treat drift as proof of defect by itself.

## Sentinels

Sentinels monitor emergent topology defects over time. They are content-aware and
scored, not simple binary invariant gates.

Examples:

- Contradiction pressure rising inside a cluster.
- Retrieval precision dropping as cluster density rises.
- Provenance survival falling after compression.
- False activation rate increasing for a recognition path.

Required structure:

- Sentinel id.
- Defect class.
- Score formula.
- Inputs.
- Thresholds.
- Recommended action.
- Lineage and conflict report.
- Replay trace for the evidence examined.

Allowed:

- Flag.
- Recommend.
- Score.
- Block unsafe transitions.

Forbidden:

- Delete evidence.
- Redefine provenance.
- Auto-confirm contradictions.

## Auditors

Auditors are periodic deep-replay verifiers. They replay a ledger window in
isolation and compare the reconstructed state against live state.

Examples:

- Replay the last N topology events and compare node/tether/certificate maps.
- Replay a time window after compression and compare provenance survival.
- Replay a suspect cluster lineage after sentinel pressure rises.

Required structure:

- Auditor id.
- Ledger window.
- Replay version.
- Live state snapshot hash.
- Replayed state hash.
- Diff report.
- Quarantine decision.

Allowed:

- Raise alarms.
- Quarantine divergent derived state for human review.
- Trigger focused sentinel or inspector review.

Forbidden:

- Rewrite live state silently.
- Repair lineage without a mutation event.
- Delete raw events.

## Role Graduation Rule

Each monitor role must graduate like any other Luna feature:

1. Data structure.
2. Lifecycle rule.
3. Failure mode.
4. Test oracle.
5. Replay proof.

If a monitor cannot provide those five pieces, it remains architecture backlog.
