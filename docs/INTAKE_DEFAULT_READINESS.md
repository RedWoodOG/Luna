# Heuristic Intake — Default-On Readiness (Make It Real, assessed)

Status of promoting the imported heuristic intake layer (disclosure + narrative +
concept, behind `LUNA_INTAKE_HEURISTICS`) from opt-in to Luna's **default**
extraction path. Assessed on commit `82f49de`.

## Verdict: NOT ready for default. Keep the flag.

Running the 20 manifest scenarios with the flag **ON** fails **8 of 20**:

```
correction_surprise_salience            council5_runtime_topology_bridge
council_capture_vocabulary_expansion    identity_profession_correction
project_graph_hardening                 project_identity_correction
real_conversation_gate                  real_week_24h_acceptance
```

(With the flag OFF — the shipped default — all 20 pass, plus 114 luna-runtime
tests, clippy, doctrine, smoke, release build.)

## Root cause (diagnosed, not guessed)

The failures are not noise — they are **fundamental interference with the
correction/lifecycle machinery**. Example, `project_identity_correction`:

```
- working memory contains forbidden label: Atlas Loom is my planning engine
- answer turn 4 contains forbidden text:    Atlas Loom is my planning engine
```

"Atlas Loom is my planning engine" is supposed to be **superseded** by "Atlas
Loom is my project workspace". But the disclosure extractor re-captures the
first-person "X is my Y" sentence and maps it (`map_disclosure_domain`) to an
`identity:*` claim with a **different slot key** than `project:identity`. That
parallel claim **escapes the project-identity supersession**, so the retired
value resurfaces in working memory and the answer.

In short: **intake claims do not yet participate in supersession / correction /
contradiction**. They are additive, not lifecycle-aware.

## What this means for "make it real"

- **Provenance is already real.** Every intake assertion flows through the event
  log as an `AssertionExtracted` event with `turn_id` and a persisted event hash;
  the source span is the recorded turn content. The remaining gap (explicit
  per-assertion *extractor version/name*) is minor and not the blocker.
- **The blocker to default-on is lifecycle integration**, not provenance. To
  promote intake to default, intake-produced claims must route through the same
  correction-slot / supersession logic as the native extractor — so a re-stated
  or corrected fact supersedes rather than duplicates. That is a real, scoped
  task, not a flag flip.

## Path to default (future, ordered)

1. Make intake claims lifecycle-aware: share `correction_slot` keys with the
   native extractor so supersession/contradiction apply to them.
2. Re-run the 20 scenarios with the flag ON until all pass.
3. Only then flip `LUNA_INTAKE_HEURISTICS` default to on (or remove the flag).

Until step 2 is green, the intake correctly stays **opt-in**. This is the honest
"make it real" status: the layer is proven to extract real, provenance-backed
memory (see `ECHOES_FALLEN_20TURN_INTAKE.md`, 19/25 replied), but it is not yet
lifecycle-safe enough to be the default, and we do not ship it as default until
it is.
