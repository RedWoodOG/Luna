# Stage 7 with LLM extraction — setup

How to run Stage 7 fixtures against the `command` extractor backend, which is the path the Stage 7 findings (`STAGE7_FINDINGS.md`) named as the v1.0 prerequisite. The infrastructure has been there since the beginning; this document is the missing entry point.

Three backends, identical contract:

```text
stdin   = rendered prompt (build_prompt_v3 output)
stdout  = JSON the runtime parses as LlmObservation
exit 0  = cache the result; non-zero = bail, do not cache
```

You only need to set up one. Pick whichever fits.

## Windows / PowerShell quick reference

The bash blocks below use Linux/macOS syntax. PowerShell differences:

| What | Bash | PowerShell |
|---|---|---|
| Set env var (one shot) | `export VAR=val` | `$env:VAR = "val"` |
| Line continuation | `\` | `` ` `` (backtick) |
| Binary path | `./target/release/luna` | `./target/release/luna.exe` |
| Python interpreter | `python3` | `python` (typical Windows install) |

So a bash invocation like:

```bash
export OLLAMA_MODEL=glm-4.6:cloud
./target/release/luna runtime scenario \
    scenarios/exploratory/stage7_dense_week.json \
    --extractor command \
    --command python3 \
    --command-arg scripts/run_ollama_extract.py \
    --model-id glm-4.6-cloud \
    --timeout-secs 300
```

becomes in PowerShell:

```powershell
$env:OLLAMA_MODEL = "glm-4.6:cloud"
./target/release/luna.exe runtime scenario `
    scenarios/exploratory/stage7_dense_week.json `
    --extractor command `
    --command python `
    --command-arg scripts/run_ollama_extract.py `
    --model-id glm-4.6-cloud `
    --timeout-secs 300
```

Or as a one-liner: drop the backticks and put everything on one line. The same translation applies to every example below.

Make sure you're inside a Luna clone before running anything (`cd $HOME\Luna` or wherever you cloned it). `cargo` will fail if the cwd isn't inside the workspace.

## Option 1 — Local llama.cpp server (offline, no API costs)

Best for repeated runs, deterministic over a fixed model, no network roundtrip per turn.

### Install and start

```bash
# install llama.cpp (any of the standard methods; example with brew)
brew install llama.cpp

# pull a small instruction-tuned model (any GGUF that supports tool/JSON output)
# e.g. Qwen2.5-7B-Instruct or Llama-3.1-8B-Instruct
huggingface-cli download bartowski/Qwen2.5-7B-Instruct-GGUF Qwen2.5-7B-Instruct-Q4_K_M.gguf --local-dir ./models

# start the server on the address the wrapper expects
llama-server \
    --model ./models/Qwen2.5-7B-Instruct-Q4_K_M.gguf \
    --host 127.0.0.1 \
    --port 8181 \
    --ctx-size 8192 \
    --n-gpu-layers -1
```

Wait for `HTTP server listening` in the logs.

### Run a Stage 7 fixture

```bash
cargo build -p luna-cli --release

./target/release/luna runtime scenario \
    scenarios/exploratory/stage7_dense_week.json \
    --extractor command \
    --command python3 \
    --command-arg scripts/run_llama_server_extract.py \
    --model-id qwen2.5-7b-instruct-q4_k_m
```

Notes:

- `--model-id` is an opaque cache key. Match it to whatever you actually loaded so swapping models invalidates the cache. Format is your choice.
- Cache lives at `.luna/runtime_cache/`. Delete that directory to force re-extraction.
- The PowerShell variant (`scripts/run_llama_server_extract.ps1`) is the same script for Windows hosts.

## Option 2 — Anthropic API (cloud, no local compute)

Best for one-off measurement, unfamiliar prose, or when you don't want to host a model.

### Configure

```bash
export ANTHROPIC_API_KEY=sk-ant-...
export ANTHROPIC_MODEL=claude-haiku-4-5    # or claude-sonnet-4-6 / claude-opus-4-7
```

### Run

```bash
cargo build -p luna-cli --release

./target/release/luna runtime scenario \
    scenarios/exploratory/stage7_dense_week.json \
    --extractor command \
    --command python3 \
    --command-arg scripts/run_anthropic_extract.py \
    --model-id claude-haiku-4-5
```

The wrapper (`scripts/run_anthropic_extract.py`) reads the prompt on stdin, calls `POST /v1/messages` with `temperature: 0`, returns the model's text content on stdout. It uses `urllib` only — no `pip install` step.

Same caching semantics as Option 1: cache key includes `--model-id`, so a model swap forces re-extraction.

## Option 3 — Ollama (local or cloud models)

Best when you already have Ollama installed. Single tool covers both local-compute and cloud-routed models. Cloud models (e.g. `glm-4.6:cloud`, `gpt-oss:20b-cloud`) execute on Ollama's hosted infrastructure but are invoked through the same local API.

### One-time setup

```bash
# install ollama (linux/macOS — see https://ollama.com for windows)
curl -fsSL https://ollama.com/install.sh | sh

# verify ollama is running
ollama list

# for local models — pull anything that returns reasonable JSON
ollama pull qwen2.5:7b

# for cloud models — sign in once, then pull the cloud tag
ollama signin
ollama pull glm-4.6:cloud           # or whatever cloud tag you want
```

Verify the model is available:

```bash
ollama list
# NAME                ID       SIZE      MODIFIED
# glm-4.6:cloud       ...      ...       ...
```

If the exact tag you want is unclear, browse https://ollama.com/library — model pages list their cloud tags. The user-facing name from `ollama list` is the value you pass to `OLLAMA_MODEL`.

### Run

```bash
export OLLAMA_MODEL=glm-4.6:cloud    # or your local model tag

cargo build -p luna-cli --release

./target/release/luna runtime scenario \
    scenarios/exploratory/stage7_dense_week.json \
    --extractor command \
    --command python3 \
    --command-arg scripts/run_ollama_extract.py \
    --model-id glm-4.6-cloud \
    --timeout-secs 300
```

`--timeout-secs 300` is the per-call ceiling; cloud models can be slow on the first call of a session (cold start). Drop it for fast local models if you want.

The wrapper (`scripts/run_ollama_extract.py`) targets Ollama's OpenAI-compatible endpoint at `http://127.0.0.1:11434/v1/chat/completions`. Override with `OLLAMA_HOST` if Ollama runs elsewhere.

### Common Ollama failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `urlopen error [Errno 111] Connection refused` | `ollama serve` not running | `ollama serve` (in another terminal) or restart the daemon |
| `404` or `model not found` | model not pulled | `ollama pull <model-tag>` |
| Cloud model: `unauthorized` or sign-in error | not signed in | `ollama signin` |
| Hangs near timeout | model loading from cold; cloud cold-start | bump `--timeout-secs`; second run will be fast |

## What success looks like

A passing Stage 7 fixture run prints per-turn assertion counts > 0 and ends with:

```text
PASS: N memory check(s)
```

The probe fixtures' `must_contain` checks are loose substrings (`Mira`, `Daniel`, `Beacon`, `17th`, `session`) so any extractor producing reasonable assertions for those entities should satisfy them. If the run passes, that is **the first measurement** that Luna's existing memory architecture handles a 10-turn case once extraction works — which is the architectural answer the Stage 7 probe was designed to surface.

If the run fails, the failure mode tells the next thing:

| Failure mode | What it tells you |
|---|---|
| Some assertions extracted, some checks fail | Memory is working but extraction isn't covering specific facts. Iterate the prompt, not the architecture. |
| Many assertions extracted but recall returns empty | Extraction is fine; activation/recall is the issue. Look at `tcf_score_breakdown` thresholds. |
| Assertions extracted then quietly disappear later | The fine-capture / merge path (R-003 / R-005) may be dropping things. |
| Every turn 0 assertions | The wrapper's stdout isn't reaching the runtime, or the JSON isn't validating. Set `LUNA_LLM_DEBUG_OUT=/tmp/luna-llm.log` and inspect raw responses. |

## Why this is not in CI yet

The `.github/workflows/doctrine.yml` gate uses the heuristic extractor for determinism — no network, no model dependency, no flake. Adding LLM-backed scenarios to CI requires a separate workflow that:

1. Caches the model + cache dir aggressively (so reruns are cheap).
2. Pins a model + prompt hash so re-extraction triggers only on intended changes.
3. Treats LLM-backed scenarios as a different gate class — perhaps required only on `main` merges, or surfaced as a status check rather than a hard gate.

That work is downstream of *first proving* an LLM-backed Stage 7 run passes locally. Don't wire CI for a green path that doesn't exist yet.

## What this setup does not do

- It does not extend the heuristic extractor's pattern set. That's a separate path; see the priority list in `docs/STATUS.md`.
- It does not add the time-decay process required for the 24h-gap portion of Stage 7. The probe fixtures here exercise only the dense-recall portion.
- It does not commit any API keys. The Anthropic wrapper reads `ANTHROPIC_API_KEY` from the environment per invocation.

## Next, after a successful local run

1. Capture the output. Note which `must_contain` checks pass and which fail.
2. If the run passes: graduate the fixture from `scenarios/exploratory/` to `scenarios/runtime/` (with a CI strategy decision — see "Why this is not in CI yet" above).
3. If the run fails: open a finding under `docs/` with the failure mode, similar to `STAGE7_FINDINGS.md`. The failure tells the next priority.

Either result moves the v1.0 work forward more than further architecture would.
