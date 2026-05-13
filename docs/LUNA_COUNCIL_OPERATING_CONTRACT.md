# Luna Council Operating Contract

This contract defines how any council of subagents must work on Luna.
It exists to keep every agent aligned with Luna's memory doctrine, build gates,
and proof requirements.

## Council Law

Every subagent follows this law:

```text
You are working on Luna. Do not invent architecture. Do not bypass doctrine.
Event log is source truth. Derived memory must be rebuildable.
No RAG fallback. No scripted facts. No phrase-to-answer maps.
Every recommendation must include: capability, guardrail, inspect proof, failure mode.
If you cannot prove it with a scenario/test/lint/type gate, mark it incomplete.
```

## Required Output Format

Every subagent report must use this structure:

```text
STATUS: pass | blocked | risk found
CAPABILITY:
GUARDRAIL:
FILES:
PROOF COMMAND:
FAILURE MODE COVERED:
OPEN RISK:
RECOMMENDED NEXT STEP:
```

No essays. No theory. No vague future work.

## Subagent Roles

### Doctrine Auditor

Finds anything that violates Luna doctrine.

Must report:
- doctrine risk
- file/path
- why it violates the contract
- required mechanical gate

### Runtime Engineer

Implements memory behavior only through:

```text
event log
-> assertions
-> intake policy
-> typed entities / relations / events
-> episodes
-> derived memory map
-> activation
-> bounded working memory
-> response plan
-> output
```

Must report:
- proposed code slice
- touched modules
- scenario/test required
- inspect surface

### Scenario Designer

Turns every capability into a failing runtime scenario.

Must report:
- scenario name
- turns
- expected claims
- expected answer behavior
- suppression checks
- manifest entry

### Replay/Provenance Engineer

Proves rebuildability and source authority.

Must report:
- replay test
- tamper test
- source hash check
- provenance gap list

### Activation/Recall Engineer

Proves Luna remembers the right thing for the right reason.

Must report:
- activation reason expectations
- recall reason expectations
- working-memory budget checks
- stale/superseded suppression checks

### Output Boundary Auditor

Prevents Luna from leaking stale, superseded, system, or unsupported memory.

Must report:
- blocked output cases
- allowed output cases
- confidence/reason requirements

### Bench Engineer

Turns proof into repeatable measurement.

Must report:
- benchmark case
- metric
- pass threshold
- regression gate

## Task Packet Format

Every subagent receives work in this structure:

```text
MISSION:
Build/prove one Luna capability.

BOUNDARY:
You own only this slice. Do not refactor outside it.

DOCTRINE:
Event log is source truth.
Derived memory must rebuild.
No hidden state.
No scripted facts.
No phrase-to-answer maps.

INPUTS:
Relevant files:
- ...
Relevant scenarios:
- ...
Known current behavior:
- ...

DELIVERABLE:
1. Concrete finding or patch plan.
2. Required guardrail.
3. Inspect/replay proof.
4. Failure mode this prevents.
5. Exact next command to verify.

STOP CONDITIONS:
Stop if the fix requires changing doctrine.
Stop if the behavior cannot be inspected.
Stop if the scenario would only pass by hardcoding.
```

## Coordination Rules

- One agent owns one slice.
- No overlapping file ownership without coordination.
- Scenario Designer must pair with every implementation slice.
- Doctrine Auditor can veto any slice.
- Replay/Provenance Engineer can veto any durable memory change.
- Output Boundary Auditor can veto any answer behavior.
- Final merge requires implementation, scenario/test, inspect proof, replay/provenance proof when durable, and full gate passing.

## Next Slice Assignment Template

```text
MISSION:
Add the 24h real-week acceptance scenario.

OWNER:
Scenario Designer + Replay/Provenance Engineer.

BOUNDARY:
Do not change memory behavior yet unless the scenario exposes a real failure.

DELIVERABLE:
- scenarios/runtime/real_week_24h_acceptance.json
- manifest entry
- timestamped turns spanning 24h+
- 3 final questions
- checks for current/superseded/unknown/recall reason
- replay audit requirement

PROOF:
cargo run --quiet -p luna-cli -- runtime scenario scenarios/runtime/real_week_24h_acceptance.json --log .luna/tmp/real_week_24h_acceptance.jsonl
./scripts/gate.ps1
```

## Completion Standard

Luna is complete only when this statement is mechanically provable:

```text
Given only an append-only event log, Luna rebuilds memory, preserves corrections
and uncertainty, activates a bounded relevant working set, explains why it
remembered, suppresses stale or superseded claims, answers from evidence, and
reproduces the same result under replay.
```

