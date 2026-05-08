# scenarios/runtime-llm/

Scenarios that gate **memory behavior under LLM-backed extraction**. Parallel to `scenarios/runtime/` (which gates the heuristic-extractor path). Different CI workflow, different cost profile, same semantic role: a scenario here passes means a specific memory contract holds.

## Why a separate directory

- `scenarios/runtime/` is iterated by the always-on `.github/workflows/doctrine.yml` workflow on every push and PR. It uses the heuristic extractor — deterministic, free, fast. CI runs it without thinking about it.
- `scenarios/runtime-llm/` is iterated by the manually-triggered `.github/workflows/llm-scenarios.yml` workflow. It uses the `command` extractor backed by an LLM (Ollama, Anthropic, or local llama.cpp). Network-dependent, sometimes paid, slower. CI does not run it on every push by design.

If a scenario can pass under the heuristic extractor, it belongs in `scenarios/runtime/`. If it requires LLM-backed extraction (because the input is natural prose outside the heuristic's narrow pattern coverage), it belongs here.

## What's here

| Fixture | Status | Backend verified | Notes |
|---|---|---|---|
| `stage7_dense_week.json` | PASSING (13/13) | `glm-4.6:cloud` via Ollama | The dense-recall portion of Stage 7. 10-turn natural-prose conversation about a real week + 3 question turns. See `docs/STAGE7_FINDINGS.md` for the full result. |
| `stage7_dense_week_with_24h_gap.json` | UNVERIFIED (awaits LLM run) | — | Same content as `stage7_dense_week.json` plus `turn_offsets_seconds` to lay the 10 conversation turns over 5 work-days with a 40-hour gap before the 3 question turns. Exercises time-decay (`luna-runtime/decay`): day-1 episodes hit ~0.45 forgotten_risk by the question turns under the 7-day default half-life. Smoke-tested under heuristic extraction (harness parses + processes correctly); awaits an LLM run to verify the must_contain checks survive decay. |

## Running locally

The wrappers live in `scripts/`. Three options, identical contract.

**Ollama (local or cloud models)**

```powershell
# PowerShell
$env:OLLAMA_MODEL = "glm-4.6:cloud"     # or a local tag like "qwen2.5:7b"
$env:LUNA_LLM_MAX_TOKENS = "16384"

./target/release/luna.exe runtime scenario `
    scenarios/runtime-llm/stage7_dense_week.json `
    --extractor command `
    --command python `
    --command-arg scripts/run_ollama_extract.py `
    --model-id glm-4.6-cloud `
    --timeout-secs 300
```

```bash
# bash
export OLLAMA_MODEL=glm-4.6:cloud
export LUNA_LLM_MAX_TOKENS=16384
./target/release/luna runtime scenario \
    scenarios/runtime-llm/stage7_dense_week.json \
    --extractor command \
    --command python3 \
    --command-arg scripts/run_ollama_extract.py \
    --model-id glm-4.6-cloud \
    --timeout-secs 300
```

**Anthropic API**

```bash
export ANTHROPIC_API_KEY=sk-ant-...
export ANTHROPIC_MODEL=claude-haiku-4-5
./target/release/luna runtime scenario \
    scenarios/runtime-llm/stage7_dense_week.json \
    --extractor command \
    --command python3 \
    --command-arg scripts/run_anthropic_extract.py \
    --model-id claude-haiku-4-5
```

**Local llama.cpp**

See `docs/STAGE7_LLM_SETUP.md` for the llama-server setup; once running:

```bash
./target/release/luna runtime scenario \
    scenarios/runtime-llm/stage7_dense_week.json \
    --extractor command \
    --command python3 \
    --command-arg scripts/run_llama_server_extract.py \
    --model-id <your-model-id>
```

## Running in CI

`.github/workflows/llm-scenarios.yml` exists for this. Triggered manually:

```text
GitHub UI → Actions → llm-scenarios → Run workflow
```

Or via the GitHub CLI:

```bash
gh workflow run llm-scenarios --ref claude/terran-operational-interface-d94YA
```

The workflow uses the Anthropic API by default. It requires the `ANTHROPIC_API_KEY` secret to be set on the repo. See the workflow file's comments for details.

The workflow is **manual-trigger only**. It does not fire on push or PR. This is by design: each run costs API credits, and we have not established that automated LLM-scenario gating is worth the cost relative to manual verification before tagging releases.

## Promotion path

A scenario gets here when it has been verified PASS-state at least once against a specific backend, and that result is recorded in a finding doc under `docs/`. The result note should specify which backend was used; cross-backend portability is not assumed.

A scenario leaves here only if its requirements change (e.g., extraction patterns expand to the point where the heuristic extractor handles it, in which case it could move to `scenarios/runtime/` and be CI-gated everywhere).

A scenario does not leave here just because it starts failing — failing is a finding to write up, not a reason to delete.

## Why not just add a `requires_extractor` field to the scenario JSON

Considered. That approach is more elegant — one source of truth — but requires:
- Changes to `RuntimeScenarioFile` in `luna-cli`
- Conditional CLI flag handling in the harness
- Both CI workflows reading the field to decide who runs it

Parallel directories are dumber and smaller. They cost one extra YAML file and one extra README (this one). If the directory layout becomes painful at scale, revisit then.
