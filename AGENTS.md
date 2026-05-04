# AGENTS.md

Orientation for any AI agent working in this repo (Codex, Cursor, Claude, etc.).
This file is short on purpose. The canonical sources are:

- **Doctrine:** [`docs/LUNA_BUILD_DOCTRINE.md`](docs/LUNA_BUILD_DOCTRINE.md)
- **Roadmap:** [`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`](docs/LUNA_MEMORY_MILESTONE_ROADMAP.md)
- **Acceptance test:** [`README.md`](README.md) (10-turn real-week → 24h+ → 3 questions)

Read those before non-trivial work. This file tells you what is **mechanically
enforced** so you don't propose work that the build will reject.

---

## Best Idea Wins

Every change is judged on three criteria. All three must hold:

1. **Working** — runs end-to-end on a stated case (scenario or test).
2. **Fixable** — surface is small, state is inspectable, next-week-you can debug it.
3. **Explainable** — design choice defended in plain language in the PR description.

If you cannot defend all three, the change is not ready. Iterate before opening a PR.

Hierarchy is flat: ideas from Codex, Cursor, Claude, or the user all compete on
these three axes. Whoever brings the strongest candidate wins.

## Real-World Inspiration vs Constraint

Computer science, math, physics, anatomy, neuroscience are **scaffolding**, not
constraints. Luna runs in binary. We can pick the parameters. Plastic edge
weights, decay curves, working-memory budgets — none of these owe anyone
biological plausibility. They owe us: works + doesn't break + can explain why.

What is forbidden is hiding implementation truth behind metaphor. "Activation
field" is a fine name *only* if the code is just a score function and we'll
say so when asked. Borrow shapes freely; describe what the code actually does.

---

## Doctrine-as-Build (Mechanically Enforced)

These gates fire automatically. Do not propose changes that violate them.

### CI (`.github/workflows/doctrine.yml`)

Every push and PR runs:
1. `cargo test --workspace --all-features`
2. `bash scripts/doctrine_check.sh` (forbidden-pattern lint)
3. Every `scenarios/runtime/*.json` against the built `luna-cli runtime scenario`

Red build = no merge. Branch protection on `main` should require these to pass.

### `scripts/doctrine_check.sh` (Forbidden Patterns)

Currently checks for:
- Hardcoded scenario-entity dispatch in production crate code
  (`if x == "Joe"`, `match { "Chris" => ... }`-style branches in `crates/`).
  Test assertions are fine; control-flow dispatch is not.
- `scenarios/runtime/` non-empty (don't quietly drain the suite).

Extend by appending `fail()` blocks. Keep output greppable (`file:line: detail`).

### Type-System Gates (Compile-Time Impossible)

- **`luna_core::RecallReason`** — strict-on-construction newtype wrapping `String`.
  - `RecallReason::new("")` and `RecallReason::new("   ")` return `Err`.
  - `RecallHit.reason` and `EpisodeRecalled.reason` are `RecallReason`, not `String`.
  - **You cannot construct surfaced memory without an explanation.** This is the
    doctrine rule "recall must carry why" enforced as a type signature.
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

---

## Slice Roadmap (Doctrine-as-Build Buildout)

Tracking what type-system / lint gates exist vs what is planned:

| Slice | Status   | Description                                                              |
|-------|----------|--------------------------------------------------------------------------|
| 1     | LANDED   | CI workflow + doctrine_check.sh (entity-dispatch lint, scenario presence) + PR template extensions (Best Idea Check, Doctrine Revision). |
| 2     | LANDED   | `RecallReason` newtype enforced on `RecallHit.reason` and `EpisodeRecalled.reason`. |
| 3a    | PLANNED  | Apply `RecallReason` (or sibling newtype) to `QuestionCandidate.reason`. |
| 3b    | PLANNED  | `WorkingMemory.activation_reason` migration — needs design call on `Default` derive. |
| 3c    | PLANNED  | `MemoryProvenance` constructor that requires ≥1 source field set (forbid all-None provenance). |
| 3d    | PLANNED  | `MemoryNode.provenance` and `MemoryEdge.provenance` non-empty Vec. |
| 4+    | PLANNED  | Phrase→answer map detection lint; per-crate test-presence check. |

Pick from PLANNED in order of leverage and isolation. Don't open a slice that
overlaps an in-flight one without coordinating.

---

## What This File Is Not

- Not a substitute for reading `LUNA_BUILD_DOCTRINE.md` and the roadmap.
- Not a list of every doctrine rule. It indexes the **mechanical gates** so you
  know what the build will reject. Doctrine rules without mechanical gates still
  apply — they're just enforced via PR review (see PR template).
- Not the v1.0 acceptance test. That lives in `README.md`. Every memory PR
  should move toward it or protect something already needed for it.
