# Codex ↔ main — Post-Reconciliation Status & Guardrails

Written from `main` after the `codex/up-to-date-main` reconciliation landed.
Purpose: tell Codex exactly where the trunk is now, what got merged, and the
**load-bearing invariants not to break** so both of us can keep building forward
without re-opening the same conflict.

## Where we are

- **Trunk:** `origin/main` is canonical. Current HEAD: **`c209e9b`**
  ("Merge codex/up-to-date-main (frozen 3392594) into main").
- **Local `C:\Luna` and `origin/main` are mirrored** (0 ahead / 0 behind).
- **The merge is gate-green** on the merged tree:
  - `cargo test --workspace` → **480 passed**
  - `cargo clippy --workspace --all-features -- -D warnings` → clean
  - `bash scripts/doctrine_check.sh` → OK
  - **all 23 scenarios** pass (`runtime scenario`)
  - `runtime smoke` → success
  - `cargo build -p luna-cli --release` → ok
- **Your `codex/up-to-date-main` content through the frozen SHA `3392594` is fully
  represented in `c209e9b`.** That branch is done as a merge unit. Do not keep
  building on it — branch from `origin/main`.
- `origin/master` is treated as diverging/abandoned per your reconciliation note;
  `main` is the integration branch.

## What was merged (your side, preserved)

Everything you froze at `3392594` is in, including:

- **Dense reliable-memory track** (additive, parallel subsystem): surprise-update
  receipts + hash contract, `DenseUpdateReceipted` event, `UpdateKind`,
  associative-memory slices, `dense_associative_project_slices` +
  `dense_surprise_gate_baseline` scenarios, `LUNA_DENSE_RELIABLE_MEMORY_PLAN.md`.
- **`capture_person_project_plan`** (`person:project_plan`).
- **`capture_project_purpose`** (`project:purpose`) — added after main's
  project-rename capture, not replacing it.
- **`project_answer_kinds_for_query`** as a *narrowing* layer over main's recall.
- **`controlled_trial_project_memory_regression`** scenario + the council /
  correction-salience vocabulary work.

Scenario set is the **union: 23** (main's 20 + your 3). None dropped.

## What is canonical on `main` (do not regress)

The merge resolved `lib.rs` by design, with **main canonical for the
recall/answer path**. These are the invariants — breaking any of them is the
failure mode the merge specifically fixed:

1. **Word-boundary cue matching, never substring.** Activation/recall match on
   whole tokens (`evidence.split(' ').any(|t| t == term)`), not
   `string.contains(substr)`. Substring matching is a recurring footgun:
   - `project_answer_kinds_for_query` once matched **`"do"` ⊂ `"does"`** in
     *"Where does Chris live?"*, misrouted it to a project query, and broke
     `runtime smoke` (person-location answer suppressed). **This was the one real
     regression the merge introduced and fixed** — it now uses whole-word match.
   - Same class as `"ren"` ⊂ `"Renn"`. **If you add any query-classification or
     cue test, split on non-alphanumeric and compare whole words.**

2. **Co-mention cross-entity expansion** in `plan_conversation_response`
   (one-hop) is main's mechanism for cross-entity recall (the R-1 fix). Keep it;
   don't replace it with a competing path.

3. **`supported_memory_values_for_query` query-term ranking** is canonical for
   recall precision. Your `project_answer_kinds_for_query` narrowing composes
   *on top* of it, not instead of it.

4. **Correction-slot / supersession is the single lifecycle.**
   `capture_project_rename` + the correction-slot machinery is canonical. Your
   correction-salience gate is **additive** (it adapts to this model). **Do not
   introduce a second, parallel correction/supersession lifecycle.**

5. **Project-rename supersession scenario** and the controlled-trial recert stay
   green. The answer path must keep surfacing the *current* renamed value.

6. **Intake heuristics flag stays default OFF.** `LUNA_INTAKE_HEURISTICS`
   (AURA-ported disclosure + narrative + concept extractors) is **not
   lifecycle-safe** — with it on, ~8 scenarios fail because claims escape
   correction supersession. See `INTAKE_DEFAULT_READINESS.md`. Making it
   default-on requires lifecycle-aware intake first; that's deferred work, not a
   flip.

7. **Dense-memory is additive, not the answer path.** Keep dense receipts/slices
   in `luna-events`/`luna-store` and their own scenarios. They must not become a
   second source of truth for `plan_conversation_response` /
   `supported_entity_values` answers.

8. **Doctrine fixture-literal lint.** `scripts/doctrine_check.sh` rejects any
   capitalized scenario word used as a **quoted literal in production code**
   (test code may assert fixture facts). If you need a stopword/keyword list that
   happens to contain a scenario name, compare **lowercased** (the trick used in
   `narrative_extract.rs` `is_subject_stopword`). Don't branch production logic
   on fixture names/literals.

## The gate that must stay green (every push/PR)

```
cargo test --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings
bash scripts/doctrine_check.sh
<every scenarios/runtime/*.json> via  luna-cli runtime scenario
luna-cli runtime smoke
cargo build -p luna-cli --release
```

Local equivalent: `powershell -ExecutionPolicy Bypass -File .\scripts\gate.ps1`.

Success criterion for any future merge (unchanged): resolve `lib.rs` **by design,
not text convenience**; main canonical; union of scenarios, none dropped; regress
no previously-passing claim on either side; preserve the dense-memory track.

## Next unit — PORT/EXTRACTION, not a merge

**Corrected ref.** The branch is **`origin/pr-0.11/case-cleanup`** @
**`7f8fce4c54254b28f46e833f9badbdaa7c93b4cc`** (the name
`codex/safety-pr-0.11-case-cleanup` no longer exists on the remote).

**It has NO merge base with `origin/main` — unrelated histories.** Verified:
`git merge-base origin/main origin/pr-0.11/case-cleanup` returns nothing, and a
tree-to-tree diff marks **110 of 149 files as DELETED** from main (runtime
modules, docs, scenarios, scripts, the entire dense-memory track, etc.).

**Therefore: do NOT rebase or merge `pr-0.11/case-cleanup` wholesale.** Any
normal merge/rebase/`merge -X theirs` would gut current `main`. This unit is a
**capability port/extraction**, cherry-picked surface by surface onto `main`,
each validated against the full gate before it lands.

PR 0.6→0.11 capabilities to extract (and only these — the surrounding tree is
stale and must not come along):

- **backend selection** — `CommandBackend` + `--backend` dispatch
- **detector vocabulary** expansion
- **prompt v2/v3 formation**
- **must-recall diagnostics**
- **case cleanup**
- the **related CLI / test surfaces** for the above

Canonical-preservation rules (unchanged) apply to every extracted piece: main's
runtime/recall/correction/dense-memory path stays canonical; extracted
capabilities come in as **additive** surfaces; word-boundary (not substring)
matching; single correction lifecycle; scenario union, none dropped; full gate
green after each landed piece.

**Operating rules from here (agreed):**

- Branch all new Codex work from `C:\Luna` / `origin/main` — **not** OneDrive
  Luna, **not** `up-to-date-main`, **not** `master`.
- Do **not** touch `codex/up-to-date-main` (already merged, frozen `3392594`).
- Do **not** merge `origin/pr-0.11/case-cleanup` directly — extract only.
- Preserve main's current runtime / recall / correction / dense-memory path as
  canonical.
