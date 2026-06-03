# Codex / main Divergence — Reconciliation Handoff

Coordination record (no action taken on Codex branches). `main` is at `82f49de`.
This documents the divergence so you and Codex can decide a reconciliation
strategy. **Nothing here was merged, rebased, or force-anything** — Codex's
branches and stash are untouched.

## The two Codex lines

### `codex/up-to-date-main` — 7 ahead / 19 behind `main` (merge-base `258ee2d`)
Recent work: correction-salience gate, PR-doctrine refresh, council-vocabulary
scenario, "prevent query text becoming person-role memory", council memory
capture, Luna memory structure contract. (+ a WIP stash on top.)

### `codex/safety-pr-0.11-case-cleanup` — 23 ahead / 25 behind `main`
The issue-driven PR sequence **PR 0.6 → 0.11**: CommandBackend + `--backend`
dispatch, detector-vocabulary expansion, prompt v2/v3 formation, must_recall
diagnostics, lifecycle reasons + inspect filters, runtime memory milestone. This
is Codex's primary build line.

## Conflict surface (the real problem)

Both lines and `main` heavily edit the same files. The hard-conflict set:

| File | Edited by main (answer-layer + AURA intake) | Edited by Codex |
|---|---|---|
| `crates/luna-runtime/src/lib.rs` | ✅ extensive | ✅ extensive |
| `crates/luna-cli/src/main.rs` | ✅ | ✅ |
| `crates/luna-cli/tests/runtime_cli.rs` | ✅ | ✅ |
| `crates/luna-runtime/src/scenario.rs` | (light) | ✅ |
| `scenarios/runtime/SCENARIO_MANIFEST.txt` | ✅ | ✅ |
| `scenarios/runtime/output_boundary_leak.json` | ✅ | ✅ |
| `AGENTS.md`, `README.md`, `Cargo.toml` | ✅ | ✅ |

`lib.rs` is the crux: `main` added the finished answer-layer fixes + the AURA
disclosure/narrative intake + recall precision; Codex's branches restructure
extraction/prompt/formation in the same file. A textual auto-merge will conflict
substantially.

## What's only on one side (so nothing is lost in planning)

- **Only on `main`:** answer-layer/onboarding WIP finish, project-rename
  supersession, AURA disclosure + narrative + concept intake (flagged), recall
  word-boundary fix, the Echoes Fallen + intake-readiness + controlled-trial
  records.
- **Only on Codex:** the PR 0.6–0.11 sequence (backends, prompt formation,
  detector vocabulary, lifecycle/inspect filters), correction-salience gate,
  council-memory work, memory-structure contract.

These are **largely complementary capabilities** that both want to survive —
which is exactly why a "take one side" merge would lose real work.

## Recommended reconciliation (decision is yours + Codex's)

1. **Codex rebases its branches onto current `main`** (preferred). The branch
   owner resolves the `lib.rs` conflicts because Codex knows the intent of its
   own extraction/prompt changes; `main`'s changes are documented in the commit
   trail and these records. Do the smaller branch (`up-to-date-main`, 7 commits)
   first as a rehearsal, then `safety-pr-0.11`.
2. **Alternative — selective cherry-pick onto `main`:** if Codex's PR sequence is
   logically ordered, cherry-pick PR 0.6→0.11 onto `main` one PR at a time,
   resolving `lib.rs` per step and keeping the gate green after each. Slower but
   incremental and reviewable.
3. **Do it soon.** Divergence is 19–25 commits and both sides keep editing
   `lib.rs`; every additional commit on either side raises the merge cost.

## Boundary

I (Claude/main) did not and will not touch Codex's branches or stash. This is a
read-only analysis to support a coordination decision. Pick a strategy with
Codex; I can execute `main`-side steps (e.g. prepare `main` for a cherry-pick
target, or review a rebased branch) on request.
