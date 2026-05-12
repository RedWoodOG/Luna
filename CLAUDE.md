# CLAUDE.md

Orientation for Claude Code sessions in this repo. This file is deliberately
short: `AGENTS.md`, `docs/LUNA_BUILD_DOCTRINE.md`, and
`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md` are canonical.

## Canonical Sources

- Doctrine: `docs/LUNA_BUILD_DOCTRINE.md`
- Roadmap and proof boundary: `docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`
- Multi-agent orientation: `AGENTS.md`
- Current public status and CLI surface: `README.md`
- Current next artifact: `docs/LOCAL_RUNTIME_PRODUCTIZATION_PROTOCOL.md`

## Mechanical Gates

Every push and PR should stay compatible with:

1. `cargo test --workspace --all-features`
2. `bash scripts/doctrine_check.sh`
3. `cargo build -p luna-cli --release`
4. Every `scenarios/runtime/*.json` through `luna-cli runtime scenario`
5. `luna-cli runtime smoke`
6. PR doctrine body checks from `.github/workflows/doctrine.yml`

The local equivalent is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\gate.ps1
```

## Doctrine Lint

`scripts/doctrine_check.sh` currently blocks:

- production-code dispatch on known fixture names and literals
- empty or non-executable runtime scenario suites
- missing proof/protocol gate coverage for manuscript one-read behavior

PR doctrine-template completeness is enforced by `.github/workflows/doctrine.yml`
on pull requests, not by `scripts/doctrine_check.sh`.

Tests may assert fixture facts. Production code may not branch on them.

## Current Proof Boundary

Do not overclaim these areas:

- The 24-hour continuity fixture uses authored timestamps. It is not a real
  wall-clock trial.
- The manuscript one-read scenario is a protocol-eligible fixture. It is not a
  full real-manuscript trial.
- Runtime topology bridge checks prove persisted bridge refs and a
  scenario-local replayable topology projection. They do not yet prove a
  long-lived product topology/orb authority store.
- Runtime compression is only a focused slice unless verified source event
  hashes are supplied. Product runtime does not yet feed real compression
  receipts into recall.

## Current Next Work

Build the local runtime product loop first. `luna-cli runtime smoke` and
`scripts/testing-readiness.ps1` are the current product-loop surfaces. The
24-hour continuity trial and full-manuscript trial are marathon goals after Luna
can start, persist, reopen, inspect, audit, correct, and recall from bounded
memory with preserved evidence.

## Working Rule

Best idea wins, but it must be working, fixable, and explainable. If a claim
cannot be inspected, tested, or described plainly, it is not ready to become
architecture.
