# Human Seed Promotion Spec

This document defines how human memory seeds can become Luna benchmark cases.

`SEEDS.md` preserves the raw human meaning. `DERIVED_BENCHMARK_PLAN.md` proposes candidate cases. This file is the final gate between candidate material and executable benchmark JSON.

No case derived from the human seeds is proof-counted until it satisfies this spec.

## Promotion States

### 1. Seeded

The memory exists only as human source material in `SEEDS.md`.

Allowed:

- preserve the story
- summarize meaning
- identify possible probes

Not allowed:

- run as benchmark
- count in metrics
- tune extractors or recall against it

### 2. Planned

The memory has one or more candidate cases in `DERIVED_BENCHMARK_PLAN.md`.

Allowed:

- draft probes
- draft `must_recall`
- draft `must_not_claim`
- assign likely categories and target dimensions

Not allowed:

- mark proof eligible
- use scores to choose timestamps or expected answers
- silently rewrite the seed meaning

### 3. Draft JSON

The candidate has been converted into a schema-v1 benchmark JSON file.

Required fields:

```json
{
  "schema_version": 1,
  "id": "human_example_001",
  "proof_category": "human_seed_validation",
  "proof_eligible": false,
  "category": "identity_continuity",
  "target_dimensions": ["identity_relevance"],
  "timestamp_origin": "authorial",
  "turns": [],
  "expected": {
    "must_recall": [],
    "must_not_claim": []
  }
}
```

Allowed:

- parse in CI
- run locally for debugging
- inspect formation diagnostics

Not allowed:

- include in proof metrics
- compare engines as evidence
- adjust expected answers based on engine output

### 4. Review Locked

The case has been human-reviewed and locked for a specific non-proof evaluation run.

Requirements:

- `must_recall` values are exact assertion-value expectations
- `must_not_claim` values are explicit false-memory boundaries
- timestamps are intentional and documented when temporal meaning matters
- target dimensions are justified by the seed
- no engine scores were used to choose the answer key

Allowed:

- run with `bench formation`
- run with `bench run --explain`
- use diagnostics to classify failures

Not allowed:

- change expected values without creating a new case id
- promote to proof eligibility without a pre-registration

### 5. Proof Eligible

The case is included in a pre-registered benchmark set.

Requirements:

- listed in a manifest
- linked to a pre-registration document
- frozen by git tag
- `proof_eligible: true`
- all timestamps, prompts, expected values, and target dimensions locked

Allowed:

- count in published metrics
- compare engines
- report statistical results

Not allowed:

- edit the case after seeing results
- tune one engine against the case while comparing against another
- remove the case because it is inconvenient

## Human-Seed Benchmark Directory

When promoted to JSON, human-seed cases should live outside the original frozen synthetic suite:

```text
benchmarks/human_seed_cases/
```

Recommended subdirectories:

```text
benchmarks/human_seed_cases/draft/
benchmarks/human_seed_cases/review_locked/
benchmarks/human_seed_cases/proof_eligible/
```

Only the final directory may be included in a proof manifest.

## Case ID Rules

Use stable ids with the source seed and category visible:

```text
human_kisan_temporal_001
human_butch_false_memory_001
human_chris_hype_001
human_sasquatch_origin_001
human_klank_failure_001
human_dreams_mission_001
```

If expected values materially change after review, create a new id:

```text
human_butch_false_memory_002
```

Do not mutate the old case silently.

## Required Authorial Checks

Before draft JSON becomes review locked, answer these questions:

```text
What human seed does this case come from?
What exact memory should Luna retrieve?
What should Luna avoid claiming?
Which target dimensions should matter?
Does timing affect the answer?
Were any engine results used to write this answer key?
```

The last answer must be:

```text
No.
```

## Formation Gate

Before a human-seed case can enter recall evaluation, it must pass formation:

```text
expected episode stored: yes
must_recall appears in assertion.value: yes
must_not_claim appears in assertion.value: no
target dimensions populated: yes
target dimensions source_count >= 2: yes
proof_eligible: false unless pre-registered
```

If formation fails, fix extraction or case wording. Do not tune recall.

## Recall Gate

After formation passes, recall diagnostics may be evaluated.

Classify every failure as one of:

```text
NoExpectedEpisodeStored
NoCandidateSelected
WrongEpisodeSelected
RightEpisodeWrongDimensions
RightEpisodeSurfaceMiss
FalseMemory
Passed
```

Layer ownership:

```text
NoExpectedEpisodeStored      -> extraction or storage
NoCandidateSelected          -> threshold or confidence calibration
WrongEpisodeSelected         -> recall scoring
RightEpisodeWrongDimensions  -> axis extraction or weights
RightEpisodeSurfaceMiss      -> output formatting or evaluator
FalseMemory                  -> extraction strictness or recall gating
```

## First Recommended Human-Seed Batch

For the first human-seed draft JSON batch, promote one case per seed:

```text
human_kisan_temporal_001
human_butch_job_stress_001
human_chris_hype_001
human_sasquatch_origin_001
human_klank_failure_001
human_dreams_mission_001
```

Purpose:

```text
coverage across temporal, emotional, identity, goal, and false-memory boundaries
without over-expanding the benchmark set too early
```

These should start as:

```text
proof_eligible: false
proof_category: "human_seed_validation"
```

They become proof material only after a separate pre-registration.

## Non-Negotiable Rule

The human seeds are personal source material, not test results.

Do not treat Luna performing well on these cases as proof of the geometric hypothesis until:

1. the cases are pre-registered,
2. the benchmark set is frozen,
3. baselines are run fairly,
4. diagnostics show the right layer did the work,
5. negative results are reported with the same prominence as wins.

