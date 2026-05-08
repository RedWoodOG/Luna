# scenarios/exploratory/

Fixtures used to **probe** what Luna can and cannot do today. Unlike `scenarios/runtime/`, fixtures here are **not gated by CI** — they may pass, fail, or partially pass. The point is the result, not the green check.

When a fixture here graduates (passes consistently with whatever extractor is canonical for its scope), move it to `scenarios/runtime/` and add it to CI.

## What's here

### `stage7_dense_week_patterned.json` (kept as a baseline)

A historical probe fixture testing whether the heuristic extractor handles deliberately-templated prose (the same Stage 7 conversation rewritten in "I am X" / "X is Y" templates). It does not — the heuristic extractor produces only `identity:name=Joe` on turn 1 and 0 assertions elsewhere. Kept here as the documented baseline for what the heuristic path can and cannot do; useful if heuristic-pattern expansion is ever pursued.

### `stage7_dense_week.json` (graduated → `scenarios/runtime-llm/`)

The natural-prose Stage 7 fixture has graduated. After the prompt + validator updates (`6305b66` and `64eb1b1`), it passes 13/13 against `glm-4.6:cloud`. It now lives at `scenarios/runtime-llm/stage7_dense_week.json` and is gated by the manually-triggered `.github/workflows/llm-scenarios.yml` workflow. See `docs/STAGE7_FINDINGS.md` for the full result.

## Running

```bash
cargo build -p luna-cli --release
./target/release/luna runtime scenario scenarios/exploratory/stage7_dense_week.json
./target/release/luna runtime scenario scenarios/exploratory/stage7_dense_week_patterned.json
```

Add `--extractor command --command <path-to-llm>` to test against an LLM-backed extractor.

## Why a separate directory

CI iterates `scenarios/runtime/*.json` and fails on any failing fixture (`.github/workflows/doctrine.yml`). Failing fixtures there block the build. Failing fixtures here document what is currently broken without breaking everything else.

Promotion path: when a fixture passes, `git mv` it to `scenarios/runtime/` and update its CI expectation accordingly.
