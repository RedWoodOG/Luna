# scenarios/exploratory/

Fixtures used to **probe** what Luna can and cannot do today. Unlike `scenarios/runtime/`, fixtures here are **not gated by CI** — they may pass, fail, or partially pass. The point is the result, not the green check.

When a fixture here graduates (passes consistently with whatever extractor is canonical for its scope), move it to `scenarios/runtime/` and add it to CI.

## What's here

### `stage7_dense_week.json` and `stage7_dense_week_patterned.json`

A pair of fixtures probing Stage 7 of the memory milestone roadmap (10-turn conversation about a real week → recall test). Same characters, same week, same questions. The two differ only in prose style:

- `stage7_dense_week.json` — natural conversational prose (contractions, implicit subjects, varied sentence shape).
- `stage7_dense_week_patterned.json` — the same content rewritten in the explicit templates the heuristic extractor recognises.

**Status as of `7c99ee9`:** both fail. See `docs/STAGE7_FINDINGS.md` for the full report. The short version: the heuristic extractor matches almost nothing outside a narrow template set; both fixtures produce ≤1 assertion across 13 turns. The bottleneck for Stage 7 today is extraction, not memory.

These fixtures stay here, failing, as the documented baseline. Re-run them whenever the extraction path changes — heuristic-pattern expansion, LLM-backed extraction integration, or a different extractor entirely. The diff between today's failure and tomorrow's pass is the measurement.

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
