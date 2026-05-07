# Luna Engineering Constraints

These rules prevent Luna from drifting from verifiable architecture into
untestable theory. Every system behavior must be representable, inspectable,
replayable, measurable, and falsifiable.

## 1. No Metaphor-Only Structures

Every concept must map to a concrete data structure, state machine, or
measurable process.

Reject a concept when it cannot be serialized, logged, replayed, or failed.

## 2. No Unbounded Recursion

Every recursive system must define:

- Depth limit.
- Cost budget.
- Termination condition.
- Replay trace.

Infinite reflection, recursive orb introspection, unrestricted sentinel chaining,
and summaries of summaries without bounds are forbidden.

## 3. No Compression Without Provenance

Compressed memory may reduce context size. It may never erase origin lineage.

Forbidden:

- Deleting source memories after summarization.
- Replacing raw events with abstractions.
- Lossy merge without lineage.
- Memory cleanup that destroys auditability.

## 4. No Hidden State Mutation

Every mutation requires:

- Mutation event.
- Timestamp.
- Cause.
- Prior state reference.
- Replay visibility.

Derived state may change only because replayable evidence says it changed.

## 5. No Undefined Tether Meaning

Every tether must define:

- Direction.
- Traversal rules.
- Weight semantics.
- Lifecycle behavior.

`A -> B = supported_by` and `B -> A = evidence_for` are distinct meanings.
`A <-> B because related` is not acceptable.

## 6. No Memory Activation Without Explainability

Every recall or activation requires:

- Activation source.
- Confidence.
- Triggering signals.
- Lineage trace.
- Conflict report.

Luna must answer "why did this activate?" with evidence.

## 7. No Sentinel Authority Over Truth

Sentinels monitor. They do not rewrite reality.

Allowed sentinel actions:

- Flag.
- Recommend.
- Score.
- Block unsafe transitions.

Forbidden sentinel actions:

- Delete evidence.
- Redefine provenance.
- Auto-confirm contradictions.

Every sentinel must declare:

- Defect class.
- Evidence type.
- Score semantics.
- Schedule.

Sentinel reports are advisory. Luna must function correctly when every sentinel
is disabled.

## 8. No Monitor Role Collapse

Monitoring must stay separated by jurisdiction:

- Inspectors block malformed transitions at write time.
- Gauges report numerical drift from baseline.
- Sentinels flag emergent topology defects.
- Auditors verify live state against replay.

Each monitor must define:

- What it can do.
- What it cannot do.
- What event or metric it observes.
- What its replay trace contains.

No monitor rewrites truth. Inspectors reject. Gauges report. Sentinels flag,
recommend, score, or block unsafe transitions. Auditors raise alarms and
quarantine divergent derived state for human review.

## 9. No Gauge Authority Over Truth

Gauges are read-only numerical observers.

Required:

- Gauge output is advisory.
- Gauge readings live in a separate append-only log from topology truth.
- Luna must function correctly when every gauge is disabled.
- Threshold calibration produces reviewable configuration suggestions only.

Forbidden:

- Gauges mutating topology.
- Gauges writing to the source-of-truth ledger.
- Gauges rejecting commits.
- Gauges changing thresholds automatically at runtime.

## 10. No Abstract Intelligence Claims

Luna is not conscious, self-aware, alive, emotional, or sentient.

Use grounded terms:

- Activation.
- Density.
- Reinforcement.
- Confidence.
- Topology.
- Compression.
- Retrieval.
- Recognition.

Avoid consciousness metaphors in code and architecture docs.

## 11. No Opaque Scoring

Every score must define:

- Formula.
- Weights.
- Inputs.
- Thresholds.
- Failure conditions.

Scores based on "AI intuition" are not architecture.

## 12. No Irreversible Splits Or Merges

All topology evolution must preserve:

- Parent lineage.
- Split origin.
- Merge origin.
- Reversible replay.

Untraceable topology evolution eventually becomes unverifiable state.

## 13. No Giant Generalized Orbs

Large undifferentiated orbs create retrieval mud.

If density increases while retrieval precision drops, the orb is unstable and
splinter pressure must increase.

## 14. No Evaluation By "Feels Smarter"

Every architecture change must be evaluated against:

- Baseline RAG.
- Baseline graph retrieval.
- Prior Luna version.

Metrics:

- Recall precision.
- Contradiction rate.
- Provenance survival.
- False activation.
- Retrieval latency.
- Compression fidelity.
- User correction rate.

## 15. No Runtime Mutation Of Genesis Certificates

Genesis certificates are immutable.

Never modify:

- Origin timestamp.
- Root lineage.
- Source identity.
- Original provenance hash.

Everything else may evolve by replayable mutation. Genesis may not.

## 16. No Cross-Orb Leakage Without Tethers

Information cannot propagate without a recorded path.

Every cross-orb activation requires at least one of:

- Tether path.
- Recognition path.
- Lineage path.
- Workflow path.

## 17. No Human-Brain Assumptions

Luna is inspired by cognition patterns. It is not neuroscience, consciousness
simulation, or biological emulation.

Optimize for:

- Recall quality.
- Continuity.
- Auditability.
- Adaptability.
- Topology stability.
- Operational usefulness.

Do not optimize for "feels brain-like."

## Final Rule

If a system behavior is not representable, inspectable, replayable, measurable,
and falsifiable, it does not enter Luna.
