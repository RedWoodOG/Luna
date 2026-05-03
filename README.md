# Luna

> **Status: PR 0.1 — schema v1 only.** Run output is **NOT proof-eligible**
> until PR 0.4 closes Stage 0 (extractor formation gates). Numbers produced
> by `bench run` and `bench compare` reflect the current upstream-blocked
> state and are non-publishable. The five temporal cases ship as
> `proof_eligible: false` until PR 0.1b commits authorial timestamps and
> `benchmarks/temporal/RATIONALE.md`.

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
