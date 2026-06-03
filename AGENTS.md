# AGENTS.md

Orientation for any AI agent working in this repo (Codex, Cursor, Claude, etc.).
This file is short on purpose. The canonical sources are:

- **Doctrine:** [`docs/LUNA_BUILD_DOCTRINE.md`](docs/LUNA_BUILD_DOCTRINE.md)
- **Memory structure contract:** [`docs/LUNA_MEMORY_STRUCTURE_CONTRACT.md`](docs/LUNA_MEMORY_STRUCTURE_CONTRACT.md)
- **Roadmap:** [`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`](docs/LUNA_MEMORY_MILESTONE_ROADMAP.md)
- **Build plan:** [`docs/LUNA_BUILD_PLAN.md`](docs/LUNA_BUILD_PLAN.md)
- **Dense reliable memory plan:** [`docs/LUNA_DENSE_RELIABLE_MEMORY_PLAN.md`](docs/LUNA_DENSE_RELIABLE_MEMORY_PLAN.md)
- **Aura comparison build plan:** [`docs/LUNA_OVER_AURA_BUILD_PLAN.md`](docs/LUNA_OVER_AURA_BUILD_PLAN.md)
- **Current next artifact:** [`docs/LOCAL_RUNTIME_PRODUCTIZATION_PROTOCOL.md`](docs/LOCAL_RUNTIME_PRODUCTIZATION_PROTOCOL.md)
- **Current dense-memory artifact:** C2 surprise/update receipts in
  [`docs/LUNA_DENSE_RELIABLE_MEMORY_PLAN.md`](docs/LUNA_DENSE_RELIABLE_MEMORY_PLAN.md)
- **Acceptance test:** [`README.md`](README.md) (10-turn real-week -> 24h+ -> 3 questions)

Read those before non-trivial work. This file tells you what is mechanically
enforced so you do not propose work that the build will reject.

The memory structure contract is binding for all memory work. Every memory
change must preserve the chain:

```text
event log -> intake -> typed assertions/relations -> entity graph -> lifecycle
-> bounded working memory -> response plan -> answer provenance -> replay audit
```

If the change cannot be inspected through that chain, it is not ready.

Current ordering guard: build the local runtime product loop before attempting
the 24-hour continuity or full-manuscript marathon trials. The current product
artifacts are `luna-cli runtime smoke`, `scripts/local-memory-loop.ps1`, and the
archive-grade `scripts/testing-readiness.ps1` packet builder.

---

## Best Idea Wins

Every change is judged on three criteria. All three must hold:

1. **Working** - runs end-to-end on a stated case (scenario or test).
2. **Fixable** - surface is small, state is inspectable, next-week-you can debug it.
3. **Explainable** - design choice defended in plain language in the PR description.

If you cannot defend all three, the change is not ready. Iterate before opening
a PR.

Hierarchy is flat: ideas from Codex, Cursor, Claude, or the user all compete on
these three axes. Whoever brings the strongest candidate wins.

## Real-World Inspiration vs Constraint

Computer science, math, physics, anatomy, and neuroscience are scaffolding, not
constraints. Luna runs in binary. We can pick the parameters. Plastic edge
weights, decay curves, and working-memory budgets owe us only this: works,
does not break, and can explain why.

What is forbidden is hiding implementation truth behind metaphor. "Activation
field" is a fine name only if the code is just a score function and we will say
so when asked. Borrow shapes freely; describe what the code actually does.

---

## Doctrine-As-Build

These gates run in CI and locally. Do not propose changes that violate them.

### CI (`.github/workflows/doctrine.yml`)

Every push and PR runs:

1. `cargo test --workspace --all-features`
2. `bash scripts/doctrine_check.sh` (forbidden-pattern lint)
3. `cargo build -p luna-cli --release`
4. Every `scenarios/runtime/*.json` against the built `luna-cli runtime scenario`
5. `luna-cli runtime smoke` as the local product-loop gate
6. On pull requests only, `scripts/validate_pr_doctrine_template.py` rejects
   missing or blank doctrine-template answers from the PR body.

Red build = no merge when GitHub branch protection requires these checks. If
branch protection is not enabled, this is a process rule rather than a technical
block.

CI scenarios use deterministic/heuristic extraction. They do not prove
LLM-backed extraction quality.

### `scripts/doctrine_check.sh`

Currently checks for:

- Hardcoded scenario-entity dispatch in production crate code
  (`if x == "Joe"`, `match { "Chris" => ... }`-style branches in `crates/`).
  Test assertions are fine; control-flow dispatch is not.
- Hardcoded scenario `value` literals in production crate code, including
  lower-case and capitalized multi-word values. Tests may assert fixtures;
  runtime mechanisms may not.
- `scenarios/runtime/` non-empty.
- Every runtime scenario has at least one executable check. A scenario that
  merely runs turns is not proof.
- The manuscript one-read protocol scenario is proof-eligible and is not
  weakened into an ordinary fixture-only run.

Extend by appending `fail()` blocks. Keep output greppable (`file:line: detail`).

### Strict-Engineering Lessons From Review

Apply these before claiming a gate is green:

- **Provenance means binding, not existence.** If a receipt references nodes,
  tethers, or events, prove they are connected by lineage, not merely present.
- **Hash inputs must be framed.** Never hash raw concatenated variable-length
  fields for replay, receipts, or audit identity.
- **Schema and Rust validation must agree.** If JSON Schema rejects a value,
  Rust should reject it too; schema examples must deserialize into Rust and
  pass Rust validation.
- **Entity matching is not substring matching.** `Chris` must not satisfy a
  query for `Christopher`.
- **Filter every downstream surface.** If a claim is excluded from an answer,
  it must also be absent from context packets, markdown, and working-memory
  surfaces passed downstream.
- **A gate must prove the claim.** Scenario presence or command success is not
  enough; checks must fail when the promised behavior regresses.
- **Doctrine should assume fixture hardcoding will happen.** Add lint coverage
  for the class of fixture literal before relying on review discipline.
- **Status docs are part of the gate.** README and roadmap must say what the
  current build proves, what is present but not gated, and what remains false.
- **Runtime memory must converge with topology memory.** Do not keep proving
  memory clusters in isolation while product recall depends on a parallel memory map.

### Type-System Gates

- **`luna_core::RecallReason`** - strict-on-construction newtype wrapping
  `String`.
  - `RecallReason::new("")` and `RecallReason::new("   ")` return `Err`.
  - `RecallHit.reason` and `EpisodeRecalled.reason` are `RecallReason`, not
    `String`.
  - You cannot construct surfaced memory without an explanation.
  - Deserialization is intentionally lenient: legacy event-log entries with
    empty `reason` strings load as the `<unrecorded>` sentinel, not as errors.
  - For static reasons in source, use:
    `RecallReason::new("...").expect("static recall reason is non-empty")`.

### PR Template (`.github/pull_request_template.md`)

Required sections every PR must answer:

- **Best Idea Check** (working / fixable / explainable boxes)
- **Memory Doctrine Check** (failure mode, mechanism, scenario, etc.)
- **Hardcoding Review** (no scripted facts/answers)
- **Memory Architecture** (event log truth, tier preservation)
- **Tests** (workspace tests, scenarios, doctrine check)
- **Doctrine Revision** (link if relaxing a rule, else N/A)

On pull requests, `.github/workflows/doctrine.yml` validates that these sections
and required prompts are present and substantively answered. The doctrine
revision field may be `N/A`; the rest must not be blank or placeholder-only.

---

## Slice Roadmap

Tracking what type-system / lint gates exist vs what is planned:

| Slice | Status | Description |
|-------|--------|-------------|
| 1 | LANDED | CI workflow + doctrine_check.sh entity-dispatch lint, scenario presence, and PR template extensions. |
| 2 | LANDED | `RecallReason` newtype enforced on `RecallHit.reason` and `EpisodeRecalled.reason`. |
| 3a | PLANNED | Apply `RecallReason` or sibling newtype to `QuestionCandidate.reason`. |
| 3b | PLANNED | `WorkingMemory.activation_reason` migration; needs design call on `Default` derive. |
| 3c | PLANNED | `MemoryProvenance` constructor that requires at least one source field set. |
| 3d | PLANNED | `MemoryNode.provenance` and `MemoryEdge.provenance` non-empty Vec. |
| 4a | LANDED | Schema examples deserialize into Rust and pass Rust validation. |
| 4b | LANDED | Consolidation timestamp integrity rule rejects `recorded_at` tampering. |
| 4c | SCENARIO-ENFORCED | `correction_back_to_superseded_value.json` proves the person-location return path; generalized correction slots still need unit/lifecycle proof. |
| 4d | LANDED | Activation component contract covers confidence, lifecycle filtering, graph depth, and filtered-memory reporting. |
| 4e | LANDED | PR-body doctrine-template completeness check runs on pull requests. |
| 4f | SCENARIO-ENFORCED | `council5_runtime_topology_bridge.json` proves runtime-to-topology bridge artifacts with source hashes and SystemKernel leakage checks, but not durable topology-ledger product commits. |
| 5 | LANDED | Compression receipts preserve raw event ancestry and reject forged/lossy/under-proven receipts. |
| 6 | LANDED | Replay auditor compares live and replayed snapshot hashes and quarantines divergence. |
| 7 | LANDED | Split/merge evolution receipts preserve reversible ancestry in the topology lane. |
| 8+ | PLANNED | Phrase-to-answer map detection lint; per-crate test-presence check; runtime use of durable topology/cluster commits. |

Pick from PLANNED in order of leverage and isolation. Do not open a slice that
overlaps an in-flight one without coordinating.

---

## What This File Is Not

- Not a substitute for reading `LUNA_BUILD_DOCTRINE.md` and the roadmap.
- Not a list of every doctrine rule. It indexes the mechanical gates so you
  know what the build will reject. Doctrine rules without mechanical gates still
  apply; they are just enforced via PR review.
- Not the v1.0 acceptance test. That lives in `README.md`. Every memory PR
  should first move the local runtime product loop toward that test, or protect
  something already needed for that loop.
