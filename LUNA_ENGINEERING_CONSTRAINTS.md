# Luna Engineering Constraints

These rules prevent Luna from drifting from verifiable architecture into
untestable theory. Every system behavior must be representable, inspectable,
replayable, measurable, and falsifiable.

## Enforcement Status

Each rule is still binding, but not every rule is mechanically enforced yet.
When changing memory behavior, label the relevant rule as one of:

- **Build-enforced:** covered by Rust tests, doctrine lint, CI, or runtime
  scenarios.
- **Scenario-enforced:** covered by at least one runtime scenario.
- **Review-only:** checked by PR review until a mechanical gate lands.
- **Future gate:** known requirement with no sufficient enforcement yet.

Do not describe review-only or future-gate behavior as proven.

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

Infinite reflection, recursive cluster introspection, unrestricted sentinel chaining,
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

## 6a. No Provenance By Existence Alone

Existence is not lineage.

When code claims provenance, it must prove binding:

- Referenced nodes must be backed by the listed source events.
- Referenced tethers must connect the listed nodes.
- Referenced tethers must be backed by the listed source events.
- Rejected audit receipts still require valid provenance.
- Duplicate event identifiers must not be appendable.

Passing because an id exists somewhere in the ledger is not auditability.

## 6b. No Unframed Hash Inputs

Hashing variable-length fields by raw concatenation is forbidden.

Every replay, receipt, or provenance hash must use canonical framing:

- Stable field order.
- Field labels or schema version.
- Length prefixes or unambiguous separators.
- Canonicalized collections before hashing.

If two different records can produce the same preimage byte stream, the hash is
not an audit hash.

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

If density increases while retrieval precision drops, the cluster is unstable and
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

## 16. No Cross-Cluster Leakage Without Tethers

Information cannot propagate without a recorded path.

Every cross-cluster activation requires at least one of:

- Tether path.
- Recognition path.
- Lineage path.
- Workflow path.

## 17. Reserved: Topology-Backed Runtime Memory

Runtime memory and topology memory must converge. It is acceptable during early
development for runtime scenarios to use a derived memory map while memory-cluster
topology is proven in milestone tests, but that split is temporary.

Future gate:

- Runtime scenario logs produce bridge refs that can be projected into topology
  nodes/tethers; product cluster receipts are a later authority layer.
- Surfaced memory cites source event id/hash, node id, tether path, and recall
  reason.
- SystemKernel remains free of user, project, and manuscript facts.

Current status:

- `scenarios/runtime/council5_runtime_topology_bridge.json` proves runtime
  bridge artifacts from runtime logs: node refs, tether refs, verified runtime
  event hashes, recall reasons, SystemKernel leakage boundaries, and a persisted
  bridge-ref commit event that matches a scenario-local topology projection.
- It still does not prove durable topology-backed product memory. Runtime turns
  do not yet append those bridge artifacts into a long-lived product topology
  ledger as committed nodes, tethers, or memory-cluster receipts.

## 18. No Entity Matching By Substring

Entity matching must be token-based, id-based, or graph-based.

Forbidden:

- Matching `Chris` inside `Christopher`.
- Matching a shorter entity label before checking the requested longer entity.
- Answering from a stored entity when the queried entity is missing.

If an entity is requested and not exactly represented by normalized tokens,
Luna must treat it as unknown instead of borrowing a nearby entity.

## 19. No Output-Only Filtering

Filtering the final reply is not enough.

If a claim is excluded for a query, it must also be excluded from:

- Context packets.
- Runtime markdown.
- Recalled-claim summaries.
- Working-memory surfaces shown to downstream callers.

Do not let hidden context contradict a cleaned final answer.

## 20. No Green Gate Without Proof

A passing gate is only meaningful when it proves the stated claim.

Every scenario must contain executable checks. Every roadmap claim needs either:

- A scenario/test that fails when the claim regresses.
- A documented statement that the claim is not yet covered by the release gate.

Do not count "scenario ran" as proof when it made no assertions.

## 21. No Fixture-Literal Escape Hatch

Scenario fixture literals must not become production mechanisms.

Doctrine checks must assume fixture answers will be hardcoded unless blocked.
Scan for:

- Entity names.
- Lowercase multi-word values.
- Capitalized full-claim values.
- Phrase-to-answer control flow.

Tests may assert fixture literals. Production code must derive them from input,
events, extraction, graph state, or replayed memory.

## 22. No Human-Brain Assumptions

Luna is inspired by reasoning patterns. It is not neuroscience, consciousness
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
