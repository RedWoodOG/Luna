import json
import os
import sys
import urllib.request


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
    sys.stdout.buffer.write(content.encode("utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
