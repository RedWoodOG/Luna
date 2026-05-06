#!/usr/bin/env python3
"""
Ollama extraction wrapper for `luna-cli runtime scenario --extractor command`.

Targets Ollama's OpenAI-compatible endpoint at
`http://127.0.0.1:11434/v1/chat/completions`. Works with both local
models (e.g. `qwen2.5:7b`) and cloud models (e.g. `glm-4.6:cloud`,
`gpt-oss:20b-cloud`) — Ollama transparently routes to its cloud
infrastructure for `:cloud` tags after `ollama signin`.

Same contract as `run_llama_server_extract.py`:
  - stdin = rendered prompt (one full prompt per invocation)
  - stdout = the model's text content (parsed by the runtime as JSON)
  - exit 0 = cache the result; non-zero = bail, do not cache

Required environment
  OLLAMA_MODEL          model tag, e.g. 'glm-4.6:cloud' or 'qwen2.5:7b'.
                        Run `ollama list` to see what's available
                        locally. For cloud models, sign in first:
                        `ollama signin && ollama pull <model>:cloud`.

Optional environment
  OLLAMA_HOST           default: 'http://127.0.0.1:11434'. Override if
                        Ollama runs on a different host or port.
  LUNA_LLM_MAX_TOKENS   default: 8192. For reasoning models (GLM-4.x,
                        Qwen3 with thinking, etc.) bump to 16384 or
                        32768 — they spend tokens on chain-of-thought
                        before emitting the answer, and a low ceiling
                        produces empty `content` with non-empty
                        `reasoning`.
  LUNA_LLM_DEBUG_OUT    append-mode log path for raw model output.
                        Useful for debugging extraction failures.

Determinism
  Temperature pinned to 0; seed pinned to 42. The runtime's cache key
  already includes `--model-id`, so pass `--model-id` matching the
  model tag so cache invalidation tracks model changes.

Example
  # one-time setup for cloud models
  ollama signin
  ollama pull glm-4.6:cloud

  # per-run
  export OLLAMA_MODEL=glm-4.6:cloud

  ./target/release/luna runtime scenario \\
      scenarios/exploratory/stage7_dense_week.json \\
      --extractor command \\
      --command python3 \\
      --command-arg scripts/run_ollama_extract.py \\
      --model-id glm-4.6-cloud \\
      --timeout-secs 300
"""
import json
import os
import sys
import urllib.error
import urllib.request


def main() -> int:
    model = os.environ.get("OLLAMA_MODEL")
    if not model:
        sys.stderr.write(
            "OLLAMA_MODEL is not set. Export it before running, e.g.\n"
            "  export OLLAMA_MODEL=glm-4.6:cloud\n"
            "Run `ollama list` to see installed models. For cloud models,\n"
            "first run `ollama signin` and `ollama pull <model>:cloud`.\n"
        )
        return 2

    host = os.environ.get("OLLAMA_HOST", "http://127.0.0.1:11434").rstrip("/")
    max_tokens = int(os.environ.get("LUNA_LLM_MAX_TOKENS", "8192"))

    prompt = sys.stdin.read()
    if not prompt.strip():
        sys.stderr.write("empty prompt on stdin; refusing to call API.\n")
        return 3

    payload = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        # response_format is supported by Ollama's OpenAI-compat layer on
        # models that can constrain output; harmless for those that can't,
        # since the prompt itself instructs JSON output.
        "response_format": {"type": "json_object"},
        "temperature": 0,
        "top_p": 1,
        "seed": 42,
        "max_tokens": max_tokens,
        "stream": False,
    }
    data = json.dumps(payload).encode("utf-8")

    request = urllib.request.Request(
        f"{host}/v1/chat/completions",
        data=data,
        headers={"Content-Type": "application/json; charset=utf-8"},
        method="POST",
    )

    try:
        with urllib.request.urlopen(request, timeout=300) as response:
            body = json.loads(response.read().decode("utf-8"))
    except urllib.error.URLError as err:
        sys.stderr.write(
            f"Ollama request to {host} failed: {err}\n"
            f"  - Is `ollama serve` running on {host}?\n"
            f"  - For :cloud models, did you run `ollama signin` first?\n"
            f"  - Is OLLAMA_MODEL ('{model}') actually pulled? Try `ollama list`.\n"
        )
        return 4

    choices = body.get("choices") or []
    if not choices:
        raise RuntimeError(f"Ollama response had no choices: {body}")
    message = choices[0].get("message") or {}
    content = (message.get("content") or "").strip()
    finish_reason = choices[0].get("finish_reason")

    if not content:
        # Specific diagnostic for the most common failure on reasoning
        # models: chain-of-thought consumed all tokens before the answer
        # was emitted. The content field is empty; the reasoning field
        # holds the partial thinking.
        if finish_reason == "length":
            reasoning = (message.get("reasoning") or "").strip()
            usage = body.get("usage") or {}
            completion_tokens = usage.get("completion_tokens", "?")
            reasoning_hint = ""
            if reasoning:
                preview = reasoning[:240].replace("\n", " ")
                reasoning_hint = (
                    f"\n  Model produced reasoning but ran out of tokens before emitting the answer.\n"
                    f"  reasoning length: {len(reasoning)} chars; preview: {preview!r}...\n"
                )
            raise RuntimeError(
                f"Ollama response truncated by max_tokens "
                f"(finish_reason='length', completion_tokens={completion_tokens}, "
                f"current LUNA_LLM_MAX_TOKENS={max_tokens}).\n"
                f"  For reasoning models (GLM-4.x, Qwen3 thinking, etc.), bump it:\n"
                f"    PowerShell: $env:LUNA_LLM_MAX_TOKENS = \"16384\"   # or \"32768\"\n"
                f"    bash:       export LUNA_LLM_MAX_TOKENS=16384"
                f"{reasoning_hint}"
            )
        raise RuntimeError(
            f"Ollama response had no content (finish_reason={finish_reason!r}): {body}"
        )

    debug_path = os.environ.get("LUNA_LLM_DEBUG_OUT")
    if debug_path:
        with open(debug_path, "ab") as handle:
            handle.write(b"\n--- LUNA LLM OUTPUT ---\n")
            handle.write(content.encode("utf-8"))
            handle.write(b"\n")

    sys.stdout.buffer.write(content.encode("utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
