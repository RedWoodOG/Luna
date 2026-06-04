# Luna Governance Lockdown

Date: 2026-06-04

This document records the repo governance reset after the external Luna review
identified a mismatch between Luna's doctrine process and GitHub enforcement.

## Current Authority

`origin/main` is canonical.

As of this update, GitHub branch protection is enabled for `main`:

- Pull requests are required before changes can land on `main`.
- Required status checks are strict and must pass on the latest branch head:
  - `build-and-check`
  - `windows-gate`
- Admin enforcement is enabled.
- Force pushes are disabled.
- Branch deletion is disabled.
- Conversation resolution is required.
- Required approving review count is `0` for now, so a solo repo is not blocked
  on a second human approval while the PR/check gate becomes mandatory.

This is a process lock, not a memory-quality proof. It does not prove runtime
recall quality, LLM extraction quality, desktop integration quality, or any
marathon/continuity packet. It only makes the existing doctrine gate harder to
bypass accidentally.

## Landing Rule

All new Luna work must follow this path:

1. Branch from `origin/main`.
2. Change one capability or one guardrail.
3. Stage only the intended files.
4. Open a pull request with the doctrine template answered.
5. Wait for `build-and-check` and `windows-gate` to pass.
6. Merge only after the PR is green.

Do not push directly to `main`.

## Stranded `pr-0.x` Branches

The old `origin/pr-0.x/*` branches are not merge candidates. Every branch in
the table below has no merge base with current `origin/main`. A direct merge or
rebase would be a stale-tree operation and can delete large parts of the current
mainline.

Treat these branches as source material for port/extraction only.

| Branch | Head | Commits not in main | Merge base | Tree adds | Tree mods | Tree deletes | Last commit date | Head subject |
| --- | --- | ---: | --- | ---: | ---: | ---: | --- | --- |
| `origin/pr-0.1/schema-v1` | `2d39755` | 2 | none | 24 | 28 | 150 | 2026-05-03 | PR 0.1: schema v1 hard break, eligibility plumbing, formation invariants |
| `origin/pr-0.1a/repo-hygiene` | `5c53c26` | 3 | none | 0 | 28 | 149 | 2026-05-03 | PR 0.1a: repo hygiene - gitignore runs/latest, normalize line endings |
| `origin/pr-0.1b/temporal-authorial` | `fb7e8b9` | 4 | none | 0 | 24 | 147 | 2026-05-03 | PR 0.1b: temporal authorial pass - RATIONALE + timestamp commitment |
| `origin/pr-0.1c/human-seed-scaffolding` | `a68e3a2` | 5 | none | 0 | 24 | 138 | 2026-05-03 | PR 0.1c: human-seed scaffolding - promotion spec + draft validation cases |
| `origin/pr-0.2/extraction-schema` | `549aaf7` | 6 | none | 0 | 25 | 136 | 2026-05-03 | PR 0.2: extraction observation schema and content-addressed cache |
| `origin/pr-0.3/llm-extractor` | `f0ee844` | 7 | none | 1 | 28 | 133 | 2026-05-03 | PR 0.3: deterministic LLM extractor adapter (cache-first, fake-only) |
| `origin/pr-0.3a/prompt-vocabulary-expand` | `5c33e02` | 8 | none | 1 | 28 | 133 | 2026-05-03 | PR 0.3a: prompt vocabulary expansion - 7 -> 22 (domain, kind) pairs |
| `origin/pr-0.4/fusion` | `d5cfe4c` | 9 | none | 1 | 31 | 130 | 2026-05-03 | PR 0.4: deterministic second sources + two-source fusion -> CognitiveObservation |
| `origin/pr-0.5a/formation-engine` | `17957e3` | 10 | none | 1 | 32 | 129 | 2026-05-03 | PR 0.5a: formation engine - gate logic + report structs + CountingBackend |
| `origin/pr-0.5b/formation-cli` | `1ffec3c` | 11 | none | 1 | 32 | 129 | 2026-05-03 | PR 0.5b: bench formation CLI + FixtureBackend |
| `origin/pr-0.6/command-backend` | `a91612f` | 12 | none | 1 | 32 | 129 | 2026-05-03 | PR 0.6: CommandBackend + CLI --backend fixture\|command dispatch |
| `origin/pr-0.7/detector-vocabulary` | `8bf5a31` | 15 | none | 1 | 33 | 125 | 2026-05-03 | record PR 0.7 detector formation run |
| `origin/pr-0.8/must-recall-diagnostics` | `e35aa76` | 17 | none | 1 | 33 | 124 | 2026-05-03 | record PR 0.8 must_recall diagnostics formation run |
| `origin/pr-0.9/prompt-v2` | `7bb9750` | 19 | none | 1 | 33 | 123 | 2026-05-03 | record PR 0.9 prompt v2 formation run |
| `origin/pr-0.10/prompt-v3` | `405fbcf` | 21 | none | 0 | 34 | 121 | 2026-05-03 | record PR 0.10 prompt v3 formation run |
| `origin/pr-0.11/case-cleanup` | `7f8fce4` | 22 | none | 0 | 37 | 110 | 2026-05-04 | Add Luna runtime memory milestone |

## Extraction Rule

For any `pr-0.x` branch:

1. Create a clean worktree from `origin/main`.
2. Inspect the exact capability, not the whole tree.
3. Port the smallest useful surface.
4. Add or preserve the scenario/guardrail that proves it.
5. Run the local gate that matches the surface.
6. Open a PR and let the protected GitHub gate decide merge readiness.

No direct merge, no rebase, no wholesale cherry-pick series.

## Initial Triage

The already-landed PR #3 covers the backend-selection guardrail on current
`main`, but it does not make the stale `origin/pr-0.6/command-backend` branch
safe to merge or safe to delete.

Recommended extraction order:

1. `origin/pr-0.7/detector-vocabulary`
2. `origin/pr-0.8/must-recall-diagnostics`
3. `origin/pr-0.9/prompt-v2`
4. `origin/pr-0.10/prompt-v3`
5. `origin/pr-0.11/case-cleanup`

Older branches should be audited for reusable schema/cache/formation pieces
only after the active extraction queue above is resolved. Delete none of these
remote branches until the repo has an explicit "ported or abandoned" note for
that branch.
