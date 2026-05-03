# Luna

Luna is a local-first episodic memory framework that tests whether state-contour recall performs better than semantic retrieval across real multi-turn conversations.

The v0.1 target is deliberately boring:

- input: benchmark JSON conversations
- output: scored reports with concrete metrics
- engines: keyword baseline and Luna TCF baseline
- persistence: append-only JSONL events

Run:

```powershell
cargo run -p luna-cli -- bench run ./benchmarks --engine tcf
cargo run -p luna-cli -- bench run ./benchmarks --engine keyword
cargo run -p luna-cli -- bench compare ./runs/latest
cargo run -p luna-cli -- report ./runs/latest --format markdown
```

Luna stores events, not final memory truth. Episodes are rebuilt from those events.

## Boundary Rule

Legacy Aura code is quarantined outside this workspace. Luna must not import or reference it.
