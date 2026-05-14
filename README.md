# Luna

> **Current status, 2026-05-10:** Luna is a `0.1.0` executable memory
> architecture prototype. The pushed `main` branch has deterministic runtime
> scenarios, doctrine lint, memory-cluster receipt validation, and a local gate, but
> it has **not** passed the v1.0 10-turn / 24-hour / 3-question continuity
> trial. Treat current runtime scenarios as regression proxies unless a run is
> explicitly marked eligible for a specific proof protocol.

[![License](https://img.shields.io/badge/license-PolyForm%20Noncommercial-blue)](LICENSE)

Luna is a local-first episodic memory framework that tests whether governed,
event-sourced memory can preserve continuity across real multi-turn
conversations better than ordinary transcript stuffing or semantic retrieval.

## Current Proof Boundary

CI and the local gate currently prove:

- workspace Rust tests pass with all features;
- clippy passes with `-D warnings`;
- every `crates/*` crate is a Cargo workspace member;
- `scripts/doctrine_check.sh` blocks known fixture-hardcoding,
  unregistered/nested/empty scenario, and ignored Rust test/doc-example failure
  modes;
- deterministic runtime scenarios registered in
  `scenarios/runtime/SCENARIO_MANIFEST.txt` pass through
  `luna-cli runtime scenario`;
- CLI-level product smoke and scenario commands are exercised by Rust
  integration tests and by the local gate;
- command-backed runtime extraction is exercised by a deterministic local CLI
  smoke that proves schema validation, extraction-cache hits, and command or
  validation failure boundaries without external network access;
- `scripts/llm-ready.ps1` can build a reproducible command-backed extractor
  evaluation packet with corpus, model id, command path/args, timeout, cache,
  output hashes, and pass/fail summary;
- the manuscript one-read protocol fixture reports protocol eligibility for the
  deterministic source-read / close / no-reread retrieval contract;
- the person-location correction path can return to a previously superseded
  value in the correction-return runtime scenario;
- the Council 5 runtime topology bridge scenario checks node/tether refs,
  source event hashes, recall reasons, accepted memory-cluster refs, SystemKernel leakage boundaries, and a
  replayable topology ledger snapshot with runtime-derived memory-cluster receipts
  that matches the persisted bridge commit event;
- runtime replay audits reject missing, empty, or hashless logs and can compare
  persisted runtime logs against rebuilt memory state, topology evidence, and
  durable ledger-event evidence;
- the release CLI builds;
- local product-loop smoke passes.

They do not yet prove:

- LLM-backed extraction quality;
- a real 24-hour continuity gap;
- real-manuscript one-read recall outside the deterministic protocol fixture;
- long-lived external topology/cluster storage beyond the persisted runtime-log
  ledger snapshots;
- superiority over baseline RAG, graph retrieval, or prior Luna versions;
- superiority over Aura TCF-style cognitive runtime claims until the
  Luna-over-Aura build plan has implementation proof, not only design parity;
- that every documented activation component is mechanically enforced;
- preserved gate artifacts from CI; use `scripts/testing-readiness.ps1` when an
  archive-grade evidence packet is required.

Known weak gates are tracked in
[`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`](docs/LUNA_MEMORY_MILESTONE_ROADMAP.md).
Fix those before expanding benchmark volume.

## Document Map

Read these in order when deciding what Luna is today and what should happen
next:

1. [`README.md`](README.md) - current status, proof boundary, CLI surface, and
   v1.0 acceptance test.
2. [`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`](docs/LUNA_MEMORY_MILESTONE_ROADMAP.md)
   - current milestone, proof boundary, readiness ladder, and backlog detail.
3. [`docs/LOCAL_RUNTIME_PRODUCTIZATION_PROTOCOL.md`](docs/LOCAL_RUNTIME_PRODUCTIZATION_PROTOCOL.md)
   - Testing Ready product-loop protocol before 24-hour or full-manuscript
     trials.
4. [`docs/LUNA_DENSE_RELIABLE_MEMORY_PLAN.md`](docs/LUNA_DENSE_RELIABLE_MEMORY_PLAN.md)
   - active build track for surprise-gated intake, replay review,
     lineage-preserving consolidation, specific activation, and miss-to-regression
     memory.
5. [`docs/CONTROLLED_HUMAN_TRIAL_PROTOCOL.md`](docs/CONTROLLED_HUMAN_TRIAL_PROTOCOL.md)
   - First reviewer-owned controlled trial before marathon or manuscript
     claims.
6. [`docs/MARATHON_READY_PROTOCOL.md`](docs/MARATHON_READY_PROTOCOL.md)
   - Marathon Ready packet protocol for the real 10-turn / 24-hour /
     3-question trial; eligibility only, not a passed trial.
7. [`docs/MANUSCRIPT_ONE_READ_PROOF_PACKET.md`](docs/MANUSCRIPT_ONE_READ_PROOF_PACKET.md)
   - Manuscript Ready packet template for a real one-read trial.
8. [`docs/V1_RELEASE_EVIDENCE_PACKET.md`](docs/V1_RELEASE_EVIDENCE_PACKET.md)
   - v1.0 Ready packet template and claim matrix.
9. [`docs/LUNA_BUILD_DOCTRINE.md`](docs/LUNA_BUILD_DOCTRINE.md) - rules that
   prevent memory from becoming transcript stuffing or scripted answers.
10. [`docs/LUNA_MEMORY_STRUCTURE_CONTRACT.md`](docs/LUNA_MEMORY_STRUCTURE_CONTRACT.md)
   - binding contract for event log -> intake -> typed graph -> lifecycle ->
     bounded working memory -> provenance -> replay audit.
11. [`docs/LUNA_OVER_AURA_BUILD_PLAN.md`](docs/LUNA_OVER_AURA_BUILD_PLAN.md)
   - engineering plan for making Luna stronger than Aura-style TCF systems
     without weakening the memory contract.
12. [`LUNA_ENGINEERING_CONSTRAINTS.md`](LUNA_ENGINEERING_CONSTRAINTS.md) -
   engineering guardrails and enforcement status.
13. [`luna-memory/docs/06_ROADMAP.md`](luna-memory/docs/06_ROADMAP.md) -
   topology-engine roadmap, including the runtime-to-topology bridge.

## Destination

Luna's long-term target is one-read continuity over a complex manuscript:

1. Read a manuscript once.
2. Close the source.
3. Later answer questions about characters, plotlines, timelines, flashbacks,
   contradictions, and unresolved threads.

Pass:

- Luna answers from its event-sourced memory, not by rereading or searching the
  manuscript.
- Luna separates scene order from story chronology when flashbacks or nonlinear
  timelines appear.
- Luna preserves character relationships, plot state, and unresolved arcs across
  interference.
- Luna marks what is confirmed, inferred, unconfirmed, and unknown.
- Luna explains why each relevant memory was recalled.

The smaller multi-turn conversation test below is the first proving ground for
the same shape: encode active truth, survive distraction, then retrieve without
flattening uncertainty.

## Build Doctrine

Luna's implementation rules are captured in
[`docs/LUNA_BUILD_DOCTRINE.md`](docs/LUNA_BUILD_DOCTRINE.md).
The binding memory-structure contract is captured in
[`docs/LUNA_MEMORY_STRUCTURE_CONTRACT.md`](docs/LUNA_MEMORY_STRUCTURE_CONTRACT.md).
The current memory milestone and forward roadmap are tracked in
[`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`](docs/LUNA_MEMORY_MILESTONE_ROADMAP.md).

The short version:

- Luna must not become ordinary JSON memory plus retrieval.
- Every memory change must preserve the event log -> intake -> typed graph ->
  lifecycle -> bounded working memory -> provenance -> replay audit chain.
- No scripted user facts or scripted final answers.
- Memory behavior must come from reusable intake, ontology, graph, activation,
  working-set, and response-planning mechanisms.
- Root memory defines behavior and semantic grammar; it does not store user
  facts.
- The event log remains the source of truth.
- Every new memory behavior should be protected by an ENCODE -> DISTRACT ->
  RETRIEVE runtime scenario.

## v1.0 Acceptance Test

Luna v1.0 passes when it can preserve project and personal continuity across a
real gap in time:

1. Talk to Luna for 10 turns about a real week.
2. Close the terminal.
3. Come back at least 24 hours later.
4. Ask Luna three questions about what was said.

Pass:

- Luna answers all three questions correctly from its event-sourced memory.
- Each answer marks what is confirmed, inferred, and still unknown.
- Luna does not flatten ambiguity into certainty.
- Luna can explain why the relevant memory was recalled.

Until this test has passed once, defer work that does not directly move Luna
toward passing it.

## Current CLI Surface

The current executable surfaces are:

```powershell
cargo run -p luna-cli -- runtime turn "Chris lives in Iowa." --log .\memory.jsonl --format markdown
cargo run -p luna-cli -- runtime chat --log .\memory.jsonl
cargo run -p luna-cli -- runtime inspect --log .\memory.jsonl
cargo run -p luna-cli -- runtime audit --log .\memory.jsonl
cargo run -p luna-cli -- runtime smoke
cargo run -p luna-cli -- runtime smoke --log .\smoke.jsonl --reset
cargo run -p luna-cli -- runtime smoke --log .\smoke.jsonl --reset --json --report .\smoke-report.json
powershell -ExecutionPolicy Bypass -File .\scripts\local-memory-loop.ps1
cargo run -p luna-cli -- runtime scenario .\scenarios\runtime\real_conversation_gate.json --keep-log
cargo run -p luna-cli -- runtime scenario .\scenarios\runtime\correction_back_to_superseded_value.json
cargo run -p luna-cli -- runtime scenario .\scenarios\runtime\council5_runtime_topology_bridge.json
powershell -ExecutionPolicy Bypass -File .\scripts\gate.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\testing-readiness.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\local-runtime-trial.ps1 -Log .\.luna\local-trial\events.jsonl -ResetLog -TrialFile .\.luna\local-trial\trial.json
powershell -ExecutionPolicy Bypass -File .\scripts\llm-ready.ps1 -Corpus .\scenarios\runtime -ModelId "local-model@deterministic-v1" -ExtractorCommand .\scripts\run_llama_server_extract.ps1 -Cache .\.luna\llm-ready\cache -OutDir .\.luna\llm-ready\packet
powershell -ExecutionPolicy Bypass -File .\scripts\controlled-human-trial.ps1 -Log .\.luna\controlled-human-trial\events.jsonl -TrialFile .\.luna\controlled-human-trial\trial.json
powershell -ExecutionPolicy Bypass -File .\scripts\marathon-ready-packet.ps1 -Log .\.luna\marathon\events.jsonl -TrialFile .\.luna\marathon\trial.json
```

These commands are product-track memory surfaces, not publication proof. They
exercise event-sourced storage, rebuilt memory state, entity groups, bounded
working memory, response planning, and scenario checks.

`runtime smoke` runs the repeatable **local product loop** without scenario JSON:
seed memory on disk, process optional distract turns, replay audit, open a new
session (same log path), retrieve, apply a correction turn, retrieve again, and
run a final replay audit. Dialogue defaults to `crates/luna-cli/smoke-dialog.json`
(`--dialog` to override). Omit `--log` to use a fresh temp JSONL path each run;
use `--log .\smoke.jsonl --reset` when you want a fixed path and a clean file
each time. `--json` prints a small report object; `--report <path>` writes the
same machine-readable report for evidence packets.

`scripts/testing-readiness.ps1` runs the local gate, then creates a
`.luna/testing-readiness/<timestamp>/` evidence packet with the commit, git
status, toolchain versions, gate log, product smoke event log, smoke JSON,
inspect output, audit output, repeat-audit evidence, event-log hash,
deterministic LLM-ready packet, local-runtime-trial packet, nested manifests,
packet-wide artifact hashes, and exact commands used. This is the handoff
artifact before human-led controlled, 24-hour, or full-manuscript trials. It has
no skip-gate mode; if `-AllowDirty` is used, staged/unstaged patches and
untracked files are archived with the packet.

`scripts/local-runtime-trial.ps1` is the smaller non-marathon reviewer trial
harness. It starts from a caller-supplied `-Log`, runs scripted turns from
`-TrialFile`, `-Turn`, or `-TurnsFile` (or prompts with `-Live`), reopens the
same log for inspect/audit, asks reviewer-owned questions from `-TrialFile`,
`-Question`, or `-QuestionsFile`, then writes a replayable packet under
`.luna/local-runtime-trial/<timestamp>/`.

`scripts/llm-ready.ps1` is the command-backed extractor evaluation packet
harness. It runs runtime scenario JSON from `-Corpus` through
`runtime scenario --extractor command`, records the model id, command path,
arguments, timeout, cache root, copied corpus, output hashes, cache file hashes,
and pass/fail summary, then writes `manifest.json` under the requested packet
directory. The harness itself has no network dependency; any network behavior
would come from the caller-supplied extractor command.

`scripts/controlled-human-trial.ps1` wraps a reviewer-owned 5+ turn / 3+
question trial into a review packet before marathon or manuscript trials. It
requires source/prompt boundary, scoring, and regression-capture fields in the
trial JSON, runs the local runtime trial harness in `-Controlled` mode, archives
the exact event log and locked questions, and creates scoring, misses, and
regression-backlog templates. Its status is `ready_for_review_not_passed` until
the reviewer scores the answers and turns every miss into regression work.

`scripts/marathon-ready-packet.ps1` prepares the real 10-turn / 24-hour /
3-question trial packet from a reviewer-owned trial JSON. It records the exact
log path, reviewer questions, ready-packet timestamps, and generated
start/reopen scripts. Its manifest status is `eligible_to_run_not_passed` until
the 24-hour reopen evidence exists.

The historical benchmark baseline remains available:

```powershell
cargo run -p luna-cli -- bench run .\benchmarks --engine similarity
cargo run -p luna-cli -- bench run .\benchmarks --engine keyword
cargo run -p luna-cli -- bench compare .\runs\latest
cargo run -p luna-cli -- bench compare .\runs\latest --require-proof-eligible
cargo run -p luna-cli -- report .\runs\latest --format markdown
```

Use plain `bench compare` for exploratory reports. Use
`--require-proof-eligible` for publishable proof comparisons. Benchmark output
is not a substitute for the v1.0 continuity trial.

## Memory Working Set

Luna memory may grow large, but the active memory shown to a model or user must
stay small, relevant, and explainable.

The event log remains the source of truth. The memory map can contain many nodes
and edges. Each turn should activate only a bounded working set:

```text
event log
-> assertions and episodes
-> memory map
-> activation
-> filtered working memory packet
```

Rules:

- Store durable memory without forcing it into every turn.
- Recompute activation per turn instead of leaving memories permanently on.
- Surface only the highest-value nodes, edges, and unknowns within a fixed
  budget.
- Keep quiet memories retrievable without letting them become constant
  roadblocks.
- Track why memory was surfaced and what was filtered out.
- Fail safe: if filtering breaks, preserve the raw event log and avoid
  overclaiming.

This is the anti-scope-creep rule for memory growth: Luna can build a rich
internal map, but product behavior must be governed by a compact working set.

## SystemKernel

Luna starts with a small `SystemKernel`: a transparent boot substrate for memory
behavior.

SystemKernel is not user memory and not a hardcoded factual trap. It defines starting
invariants such as:

- Luna is a local-first memory runtime, not a conscious being.
- The event log is the source of truth.
- Memory must distinguish confirmed, inferred, unconfirmed, and unknown.
- Ambiguity must not be flattened into certainty.
- Only a bounded working set should enter the current turn.
- Surfaced memory should carry provenance and recall reasons.
- Proof behavior and product behavior stay separate.

Rules:

- SystemKernel provenance is marked as `system_root`.
- SystemKernel is versioned.
- SystemKernel is inspectable.
- SystemKernel is quiet unless Luna's identity, memory behavior, proof boundary, or
  certainty rules are relevant.
- SystemKernel must not override event-sourced user evidence.
- SystemKernel may evolve by version or configuration; it is a starting point, not a
  cage.
