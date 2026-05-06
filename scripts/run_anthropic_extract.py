#!/usr/bin/env python3
"""
Anthropic API extraction wrapper for `luna-cli runtime scenario --extractor command`.

Mirrors the contract of `scripts/run_llama_server_extract.py`:
  - Reads the rendered prompt from stdin (one full prompt per invocation).
  - Calls the Anthropic Messages API.
  - Writes the model's text content to stdout — nothing else.
  - The runtime parses stdout as JSON (`LlmObservation`); the prompt
    template tells Claude to return JSON.

This is the cloud-LLM alternative to the local llama-server path. Both
hit the same `command` extractor backend.

Required environment
  ANTHROPIC_API_KEY      your API key.

Optional environment
  ANTHROPIC_MODEL        default: 'claude-haiku-4-5'.
                         For richer extraction, try 'claude-sonnet-4-6'
                         or 'claude-opus-4-7'.
  LUNA_LLM_MAX_TOKENS    default: 2048. Upper bound on the response.
  LUNA_LLM_DEBUG_OUT     append-mode log path for raw model responses
                         (useful when inspecting cache misses).
  ANTHROPIC_API_BASE     default: 'https://api.anthropic.com'. Override
                         only if proxying.

Determinism
  Temperature is forced to 0. The cache key the runtime computes already
  includes `--model-id` (call it from the CLI), so a model swap forces
  re-extraction. Pass `--model-id` matching `ANTHROPIC_MODEL` so cache
  invalidation tracks model changes.

Errors
  Non-200 responses, missing API key, missing/empty content all raise.
  Failures must propagate so the harness's cache is not poisoned with
  bad output. (Same contract as run_llama_server_extract.py.)

Example
  export ANTHROPIC_API_KEY=sk-ant-...
  export ANTHROPIC_MODEL=claude-haiku-4-5
  ./target/release/luna runtime scenario \\
      scenarios/exploratory/stage7_dense_week.json \\
      --extractor command \\
      --command python3 \\
      --command-arg scripts/run_anthropic_extract.py \\
      --model-id claude-haiku-4-5
"""
import json
import os
import sys
import urllib.request


def extract_json_from_content(content: str) -> str:
    """Tolerantly recover JSON from LLM output that may be wrapped in
    markdown fences or surrounded by prose. See the matching helper in
    run_ollama_extract.py for the full contract."""
    s = content.strip()
    if s.startswith("```"):
        nl = s.find("\n")
        if nl != -1:
            s = s[nl + 1:]
        if s.endswith("```"):
            s = s[:-3].rstrip()
    open_idx = s.find("{")
    close_idx = s.rfind("}")
    if open_idx != -1 and close_idx > open_idx:
        s = s[open_idx:close_idx + 1]
    return s


def main() -> int:
    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        sys.stderr.write(
            "ANTHROPIC_API_KEY is not set. Export it before running this wrapper.\n"
        )
        return 2

    model = os.environ.get("ANTHROPIC_MODEL", "claude-haiku-4-5")
    max_tokens = int(os.environ.get("LUNA_LLM_MAX_TOKENS", "2048"))
    api_base = os.environ.get("ANTHROPIC_API_BASE", "https://api.anthropic.com").rstrip("/")

    prompt = sys.stdin.read()
    if not prompt.strip():
        sys.stderr.write("empty prompt on stdin; refusing to call API.\n")
        return 3

    payload = {
        "model": model,
        "max_tokens": max_tokens,
        "temperature": 0,
        "messages": [
            {"role": "user", "content": prompt},
        ],
    }
    data = json.dumps(payload).encode("utf-8")

    request = urllib.request.Request(
        f"{api_base}/v1/messages",
        data=data,
        headers={
            "content-type": "application/json; charset=utf-8",
            "anthropic-version": "2023-06-01",
            "x-api-key": api_key,
        },
        method="POST",
    )

    with urllib.request.urlopen(request, timeout=120) as response:
        body = json.loads(response.read().decode("utf-8"))

    # Anthropic Messages API: { "content": [ {"type":"text","text":"..."} ], ... }
    blocks = body.get("content")
    if not blocks:
        raise RuntimeError(f"Anthropic response had no content blocks: {body}")
    text_blocks = [b.get("text", "") for b in blocks if b.get("type") == "text"]
    content = "".join(text_blocks).strip()
    if not content:
        raise RuntimeError(f"Anthropic response had no text content: {body}")

    debug_path = os.environ.get("LUNA_LLM_DEBUG_OUT")
    if debug_path:
        with open(debug_path, "ab") as handle:
            handle.write(b"\n--- LUNA LLM OUTPUT ---\n")
            handle.write(content.encode("utf-8"))
            handle.write(b"\n")

    extracted = extract_json_from_content(content)
    sys.stdout.buffer.write(extracted.encode("utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
