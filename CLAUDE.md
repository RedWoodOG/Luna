# CLAUDE.md

Orientation for Claude Code sessions in this repo. Loaded automatically every
session. This file is short on purpose — it points to canonical sources and
names the **mechanical gates** that the build will reject changes against.

## Canonical sources (read before non-trivial work)

- **Doctrine:** [`docs/LUNA_BUILD_DOCTRINE.md`](docs/LUNA_BUILD_DOCTRINE.md)
- **Roadmap:** [`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`](docs/LUNA_MEMORY_MILESTONE_ROADMAP.md)
- **Acceptance test:** [`README.md`](README.md) (10-turn real-week → 24h+ → 3 questions)
- **Multi-agent orientation:** [`AGENTS.md`](AGENTS.md) (also for Codex)

## IMPORTANT — Best Idea Wins (Universal Gate)

Every change is judged on three criteria. **All three must hold or the change is not ready:**

1. **Working** — runs end-to-end on a stated case (scenario or test).
2. **Fixable** — small surface, inspectable state, next-week-you can debug it.
3. **Explainable** — design choice defended in plain language in the PR description.

Hierarchy is flat. Ideas from Claude, Cursor, Codex, or the user all compete on
these three axes. Whoever brings the strongest candidate wins.

## IMPORTANT — Real-World Inspiration vs Constraint

Computer science, math, physics, anatomy, neuroscience are **scaffolding**, not
constraints. Luna runs in binary. We choose the parameters. Plastic edge
weights, decay curves, working-memory budgets owe no one biological plausibility.
They owe us: works + doesn't break + can explain why.

What is forbidden is hiding implementation truth behind metaphor. "Activation
field" is a fine name *only* if the code is a score function and we'll say so
when asked. Borrow shapes freely; describe what the code actually does.

---

## Mechanically Enforced — These Gates Fire Automatically

### CI (`.github/workflows/doctrine.yml`)

Every push and PR runs:
1. `cargo test --workspace --all-features`
2. `bash scripts/doctrine_check.sh`
3. Every `scenarios/runtime/*.json` against `luna-cli runtime scenario`

Red build = no merge. Branch protection on `main` should require these to pass.

### Doctrine Lint (`scripts/doctrine_check.sh`)

Forbidden in production crate code (under `crates/`, outside `tests/`):
- `if x == "Joe"`-style conditional dispatch on hardcoded scenario-entity names
- `match { "Chris" => ... }` arms keyed on hardcoded scenario-entity names

Test assertions like `assert!(... == "Joe")` are fine — they observe; they do
not dispatch. The lint matches the prohibition positively, not by filtering
test code.

Also enforced: `scenarios/runtime/` non-empty.

### Type-System Gates (Compile-Time Impossible)

- **`luna_core::RecallReason`** — strict-on-construction newtype wrapping `String`.
  - `RecallReason::new("")` and `RecallReason::new("   ")` return `Err`.
  - `RecallHit.reason` and `EpisodeRecalled.reason` are `RecallReason`, not `String`.
  - **You cannot construct surfaced memory without an explanation.** The doctrine
    rule "recall must carry why" is a type signature now.
  - Deserialization is intentionally lenient: legacy event-log entries with
    empty `reason` strings load as the `<unrecorded>` sentinel rather than
    failing. New code cannot write empty.
  - For static reasons in source, use:
    `RecallReason::new("...").expect("static recall reason is non-empty")`.

### PR Template (`.github/pull_request_template.md`)

Every PR must answer:
- **Best Idea Check** — working / fixable / explainable
- **Memory Doctrine Check** — failure mode, mechanism, scenario, what must not happen
- **Hardcoding Review** — no scripted facts or final answers
- **Memory Architecture** — event log truth, tier preservation
- **Tests** — workspace, scenarios, doctrine check
- **Doctrine Revision** — link if relaxing a rule; else N/A

---

## Slice Roadmap (Doctrine-as-Build Buildout)

| Slice | Status   | Description                                                              |
|-------|----------|--------------------------------------------------------------------------|
| 1     | LANDED   | CI + `doctrine_check.sh` + PR template extensions.                       |
| 2     | LANDED   | `RecallReason` enforced on `RecallHit.reason` and `EpisodeRecalled.reason`. |
| 3a    | LANDED   | `QuestionCandidate.reason` → `RecallReason`.                             |
| 3b    | PLANNED  | `WorkingMemory.activation_reason` migration — needs `Default` design call. |
| 3c    | PLANNED  | `MemoryProvenance` constructor requires ≥1 source field set (forbid all-None). |
| 3d    | PLANNED  | `MemoryNode.provenance` and `MemoryEdge.provenance` non-empty Vec.       |
| 4+    | PLANNED  | Phrase→answer map detection lint; per-crate test-presence check.         |

Pick from PLANNED in order of leverage and isolation. Don't open a slice that
overlaps an in-flight one without coordinating.

---

## Reminders for Claude Specifically

- **Verify before scaffolding.** Codex and Cursor work in this repo too. Before
  proposing structural changes, read the current code state — don't trust a
  prior session's handoff verbatim. The same crate may have new construction
  sites or new types since last seen.
- **Check the slice table before opening new work.** If your idea is already
  on the PLANNED list, align with its scope rather than redoing it.
- **`MemoryProvenance` currently allows all-`None`.** Don't construct one until
  slice 3c lands (a constructor will appear that requires ≥1 source field).
- **Static strings** going through `RecallReason::new` use `.expect("...")`.
  **Dynamic strings** propagate the `Result` — do not unwrap user-derived input.
- **The acceptance test is the gate.** Defer work that does not directly move
  Luna toward the 10-turn real-conversation gate or protect something already
  needed for it.

---

## What This File Is Not

- Not a substitute for the canonical doctrine and roadmap. Read those.
- Not a list of every doctrine rule — it indexes the **mechanical gates** so
  you know what the build will reject. Doctrine rules without mechanical gates
  still apply via PR review (see PR template).
