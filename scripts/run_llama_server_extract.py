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
    prompt = sys.stdin.read()
    payload = {
        "messages": [{"role": "user", "content": prompt}],
        "response_format": {"type": "json_object"},
        "temperature": 0,
        "top_p": 1,
        "seed": 42,
        "max_tokens": int(os.environ.get("LUNA_LLM_MAX_TOKENS", "2048")),
        "stream": False,
    }
    data = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        "http://127.0.0.1:8181/v1/chat/completions",
        data=data,
        headers={"Content-Type": "application/json; charset=utf-8"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        body = json.loads(response.read().decode("utf-8"))
    content = body["choices"][0]["message"]["content"]
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
