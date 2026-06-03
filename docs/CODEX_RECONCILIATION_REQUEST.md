# Codex ↔ main Reconciliation — What I Need Answered

Purpose: a single answerable list so the `codex` branches and `main` can be
merged without losing either side's work. Written from `main` (`5ac808e`,
gate-green). Target branch analyzed: `origin/codex/up-to-date-main`
(17 ahead / 24 behind `main`, merge-base `258ee2d`, **still growing**).

Nothing on Codex's branches has been touched. This is a request for decisions +
technical answers, then I execute the `main`-side.

## The real conflict map (from a 3-way merge dry-run)

10 files actually conflict; the rest auto-merge. Tiers:

- **🔴 Hard (1):** `crates/luna-runtime/src/lib.rs` — same-file *and same-logic*
  divergence (both sides fixed recall/correction/person-role differently).
- **🟠 Moderate (4):** `luna-cli/src/main.rs`, `luna-cli/tests/runtime_cli.rs`,
  `luna-runtime/src/scenario.rs`, `luna-events/src/lib.rs`, `luna-store/src/lib.rs`.
- **🟢 Trivial (3):** `SCENARIO_MANIFEST.txt` (union), `README.md`, `AGENTS.md`,
  `docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`.
- **✅ Auto-merge clean:** `luna-core`, `luna-activation`, `luna-output`,
  `luna-runtime/topology_bridge.rs`, others.

## A. Decisions needed (cofounder + Codex)

- **A1 — Trunk.** Is `main` the canonical integration branch? `origin/master` is
  also diverging — which branch is authoritative going forward?
- **A2 — Strategy.** Codex rebases `up-to-date-main` onto `main` (recommended,
  owner resolves `lib.rs`), or cherry-pick PRs onto `main`, or other?
- **A3 — Order.** Reconcile `up-to-date-main` (17) before `safety-pr-0.11` (23)?
- **A4 — Freeze.** Can Codex freeze the target branch at a named SHA during the
  merge so it stops moving? (It grew ~10 commits in the last hour.)

## B. The `lib.rs` design conflicts — same problem, two solutions

These Codex commits rework the **same surface** `main` did. For **each**, I need:
(1) what function(s)/approach it changes, (2) which scenario or test protects it,
(3) is it *compatible with* or *competing against* `main`'s version?

- **B1 — `509b406` "Prevent query text from becoming person role memory."**
  `main` hardened the same area (`claim_is_answerable_memory`, person/role
  extraction, activation gating). Same code path? Keep yours, mine, or combined?
- **B2 — `a7706a9` "Keep project recall answers specific."**
  `main` added recall precision (`supported_memory_values_for_query` query-term
  ranking, word-boundary cue match) **and** one-hop co-mention cross-entity
  expansion in `plan_conversation_response`. Are these competing answers to the
  same "specific recall" goal, or do they compose?
- **B3 — `b55e14d` "correction salience gate" + `9d2cd26` "Teach runtime from
  controlled project memory miss."** `main` has project-rename supersession
  (`capture_project_rename` + correction-slot machinery) and a controlled-trial
  recert. Do these touch the same correction/supersession code? Which is canonical?
- **B4 — Scenarios you changed that `main` also runs**
  (`correction_surprise_salience`, `council_capture_vocabulary_expansion`,
  `output_boundary_leak`): do your versions still pass against `main`'s `lib.rs`,
  or do they assume your `lib.rs` behavior? The merged tree must keep **both
  sides' scenarios green**.

## C. The "dense reliable memory" track — additive or overlapping?

Your `6823b51…3392594` commits (compressive model state, surprise-update receipts
+ hash contract, associative memory slices, `dense_*` scenarios,
`LUNA_DENSE_RELIABLE_MEMORY_PLAN.md`) look like new architecture I want preserved.

- **C1.** Does it touch the recall/answer path `main` changed
  (`plan_conversation_response`, `luna-recall`, `supported_entity_values`), or is
  it a separate subsystem (receipts in `luna-events`/`luna-store`)?
- **C2.** Do `dense_associative_project_slices` / `dense_surprise_gate_baseline`
  depend on your `lib.rs` recall changes, or stand alone?
- **C3.** Is the dense-memory plan the canonical direction now, or exploratory?
  (Affects whether `main`'s flag-gated AURA intake and your dense track converge
  or stay separate tracks.)

## D. What `main` brings (so you can compare apples to apples)

- Finished the answer-layer/onboarding WIP to green; project-rename supersession;
  AURA disclosure + narrative + concept intake (behind `LUNA_INTAKE_HEURISTICS`,
  default off — *not* lifecycle-safe yet, see `INTAKE_DEFAULT_READINESS.md`);
  recall fixes (word-boundary cue match, co-mention cross-entity).
- All **gate-green**: 115 tests, 20/20 scenarios, clippy `-D warnings`, doctrine,
  smoke, release build.
- Records on `main`: `ECHOES_FALLEN_20TURN_*`, `INTAKE_DEFAULT_READINESS`,
  `CONTROLLED_HUMAN_TRIAL_RECERT`, `CODEX_DIVERGENCE_HANDOFF`.

## E. What I need to execute the `main`-side

Once A + B are decided, I can do the `main`-side merge/validation. I need:

- **E1.** A frozen Codex branch SHA to merge/validate against.
- **E2.** The canonical-approach decision per B1–B3 (which version wins, or how to
  combine). With that, I implement the `main`-side resolution for the overlapping
  `lib.rs` functions.
- **E3.** Confirmation the **dense-memory track stays intact** and which scenarios
  are load-bearing for it.

## F. Success criterion (non-negotiable)

The merged `main` must:

1. Pass the **full gate**: `cargo test --workspace --all-features`,
   `cargo clippy --workspace --all-features -- -D warnings`,
   `bash scripts/doctrine_check.sh`, **every** `scenarios/runtime/*.json` via
   `runtime scenario`, `runtime smoke`, `cargo build -p luna-cli --release`.
2. Include the **union of scenarios** from both branches — none silently dropped.
3. Regress **no** previously-passing claim on either side.
4. Preserve the dense-memory track.

If a conflict can't be resolved without dropping a scenario or a test, that is a
design decision to escalate — not something to paper over.
