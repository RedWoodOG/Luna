# Luna Memory Milestone And Roadmap

This document records the current memory milestone, the proof boundary, the
known weak gates, the readiness ladder, and the backlog detail. It is meant to
keep each build day concrete: finish memory capabilities without drifting into
ordinary RAG, transcript stuffing, or impressive-sounding magic.

## Current Milestone

Luna now has a defensible product-track memory loop in deterministic runtime
scenarios:

```text
user turn
-> LLM or heuristic extraction
-> runtime fine capture
-> confidence-tiered assertions
-> append-only event log
-> rebuilt memory state
-> derived entity groups
-> derived memory map
-> bounded working memory
-> conversational reply or workbench output
```

What is real now:

- Runtime includes an LLM-backed extraction path; the release gate validates the
  deterministic heuristic scenario path plus a deterministic command-backed
  extractor smoke for schema validation, cache hits, and command/validation
  failure boundaries without external network access.
- `scripts/llm-ready.ps1` produces a reproducible command-backed extractor
  evaluation packet with corpus, model id, command path/args, timeout, cache,
  output hashes, and pass/fail summary.
- `scripts/local-runtime-trial.ps1 -Controlled` produces the first human
  controlled runtime-trial packet surface with source/prompt boundary,
  pre-answer question lock, scoring template, and regression backlog template.
- Runtime can run as a conversational terminal loop.
- Runtime stores memory as events and rebuilds state from those events.
- Assertions carry `Confirmed`, `Inferred`, or `Unconfirmed` tiers.
- Proof behavior and product behavior are separated.
- SystemKernel exists as inspectable behavioral substrate, not user memory.
- Working memory enforces a small active packet.
- Entity memory groups are derived for `self`, people, projects, and manuscript
  characters.
- The memory map includes entity nodes, not only flat claim nodes.
- Person- and project-specific recall can answer from bounded evidence.
- Dense runtime scenarios cover people, projects, corrections, manuscript
  aliases, scene chronology, open arcs, and a synthetic 10-turn conversation
  proxy.
- Memory-cluster formation receipts exist in the topology lane and are validated by
  milestone tests.
- Correction back to a previously superseded person-location value is now
  scenario-enforced by
  `scenarios/runtime/correction_back_to_superseded_value.json`.
- Runtime-to-topology bridge coverage now checks bridge node/tether artifacts,
  source event hashes, recall reasons, a persisted bridge-ref commit event, and
  SystemKernel leakage boundaries in
  `scenarios/runtime/council5_runtime_topology_bridge.json`. Runtime turns now
  persist a replayable topology ledger snapshot, runtime-derived memory-cluster
  receipt refs, and a canonical ledger-event hash in the bridge commit event;
  replay audit recomputes that evidence and quarantines missing or mismatched
  ledger snapshots.
- Compression receipts preserve raw event ancestry in the topology lane and
  reject forged, lossy, or under-proven compression.
- Accepted compression receipts can reduce bounded runtime working memory only
  in the focused runtime slice when source event ids and hashes are supplied by
  an external verified source index. Product runtime does not yet feed real
  compression receipts into recall.
- Replay auditing compares live and replayed topology snapshot hashes and
  quarantines divergence instead of silently repairing it.
- Runtime replay audit checks can now run over persisted runtime scenario logs
  and require clean replay with source-hashed topology evidence present.

What this milestone proves in CI:

- Luna is no longer only a flat pile of JSON claims.
- Luna can derive stable entity clusters from event-sourced assertions in
  deterministic fixture scenarios.
- Luna can keep quiet memory retrievable without dumping everything into each
  answer in deterministic fixture scenarios.
- Luna can be tested with ENCODE -> DISTRACT -> RETRIEVE runtime scenarios.
- The local gate runs workspace tests, clippy with `-D warnings`, workspace
  membership validation, doctrine lint, release CLI build, every manifest
  runtime scenario, and product smoke.

What this milestone does not prove yet:

- It does not prove long-horizon continuity.
- It does not prove the README v1.0 10-turn / 24-hour / 3-question acceptance
  test.
- It does not prove LLM-backed extraction quality.
- It does not prove full real-manuscript one-read memory, though the
  deterministic source-read / close / no-reread protocol is proof-gated.
- It does not prove robust correction, contradiction, stale fact, and
  restoration handling across all lifecycle paths.
- It does not prove question quality.
- It does not prove the full documented activation formula.
- It does not prove baseline-vs-Luna superiority.
- It does not prove superiority over issue-driven cognitive runtime claims;
  [`LUNA_BUILD_PLAN.md`](LUNA_BUILD_PLAN.md) defines the required Luna-native
  build sequence and proof gates.

## Current Proof Boundary

Use this table when deciding whether a claim is allowed in docs, issues, PRs, or
release notes.

| Claim | Status |
|-------|--------|
| Workspace tests pass | CI/local gate proven |
| Clippy warning-free build | CI/local gate proven |
| Workspace member validation | CI/local gate proven |
| Doctrine fixture-hardcoding, manifest registration, nested-scenario, empty-scenario, and ignored-test lint runs | CI/local gate proven |
| Deterministic runtime scenarios pass | CI/local gate proven through `SCENARIO_MANIFEST.txt` |
| CLI runtime smoke/scenario commands execute | Rust integration-test and local-gate proven |
| Command-backed LLM extraction path | Rust integration-test proven for deterministic local command wrapper schema validation, cache hits, and command/validation failure boundaries; real LLM quality not proven |
| LLM Ready packet harness | Script and Rust integration-test proven for deterministic local command-backed corpus packets with output/cache hashes and pass/fail summary; real LLM quality depends on the reviewer corpus and supplied model command |
| Controlled human trial packet | Script/protocol present for reviewer-owned 5+ turn / 3+ question local trials; pass status requires reviewer scoring and regression capture |
| 24-hour continuity | Synthetic authorial timestamp gap only; real wall-clock trial not proven |
| One-read manuscript recall | Deterministic protocol-eligible fixture gated; full real-manuscript trial not proven |
| Memory cluster formation receipts | Milestone-test proven; runtime-derived memory-cluster refs are persisted in bridge commits and visible to bounded working memory |
| Cluster split/merge receipts | Milestone-test proven in topology lane, not yet driven by runtime sentinels or recall |
| Runtime memory bridge artifacts expose topology refs | Scenario proven |
| Runtime memory committed into durable topology/cluster ledger | Runtime-log bridge commits persist replayable topology ledger snapshots, canonical ledger hashes, and memory-cluster refs; long-lived external topology/cluster store not yet present |
| Manuscript one-read lockout | Proxy scenario proven after explicit close; full one-read trial not proven |
| Activation contract covers confidence/lifecycle/depth/filtering | Unit and scenario proven |
| Compression receipts preserve raw lineage | Milestone-test proven; runtime bounded-context slice requires verified source hashes; product path not wired |
| Replay auditor quarantines divergence | Milestone-test proven; persisted runtime scenario audit proven |
| PR doctrine template is complete | PR CI gate enforced |

## Known Weak Gates

These are the first repair targets. Do not treat a green build as stronger than
these gates allow.

1. **Correction lifecycle generalization is still partial.**
   `correction_back_to_superseded_value.json` proves the Chris Iowa -> Ohio ->
   Iowa person-location path, `identity_profession_correction.json` proves a
   non-location identity/profession correction, and
   `project_identity_correction.json` proves a project identity/description
   correction through the runtime scenario manifest. The remaining weak gate is
   broadening that guarantee across more correction slots while keeping
   lower-level lifecycle/unit proof beside the scenario witnesses.

2. **The 24-hour fixture is synthetic, not a wall-clock trial.**
   The v1.0 acceptance test requires a real 24-hour gap. The current scenario
   uses authored timestamps and only proves timestamp handling.
   `scripts/marathon-ready-packet.ps1` and
   [`MARATHON_READY_PROTOCOL.md`](MARATHON_READY_PROTOCOL.md) now provide the
   eligibility packet shape with start/reopen scripts, exact log path,
   reviewer-owned questions, and saved output requirements. The missing proof is
   the completed 24-hour reopen evidence and reviewed answers.

3. **Cluster provenance is visible to bounded working memory, but sentinels still lag.**
   Runtime-derived memory-cluster provenance is now added to product working-memory
   nodes and replay-audited through persisted bridge commits. Sentinel scheduling
   and contradiction-pressure routing still do not consume active/retired cluster
   lineage.

4. **Topology bridge authority is runtime-log durable, not externally stored.**
   Runtime turns append a bridge commit with replayable ledger events, memory-cluster
   refs, and a canonical ledger-event hash. The remaining authority gap is a
   long-lived topology/cluster store and recall path that can use that store across
   larger sessions without replaying whole runtime-log snapshots.

## North Star

The final memory destination is one-read continuity:

```text
read manuscript once
-> close source
-> survive time and interference
-> answer questions about characters, plotlines, timelines, flashbacks,
   contradictions, and unresolved arcs
```

The near-term acceptance test remains:

```text
10-turn real conversation
-> close terminal
-> return 24 hours later
-> ask 3 questions
-> answer correctly with confidence, unknowns, ambiguity, and recall reasons
```

Every memory PR should either move Luna toward that test or protect something
already needed for it.

## Core Architecture Target

The memory stack should keep this shape:

```text
raw event log
-> extracted assertions
-> intake policy
-> typed entities / relations / events
-> episodes
-> derived memory map
-> relevance scoring
-> bounded working memory
-> response plan
-> model or tool output
-> event commit
```

The LLM may help extract, reason, and speak. It is not the memory authority.
The event log and derived memory structures are the authority.

## Readiness Ladder

Readiness labels are release-control labels, not marketing labels. A stage is
ready only when the listed evidence can be regenerated from the repo and
archived without relying on hidden local state. The immediate proof sequence is
Testing Ready, Controlled Human Trial, LLM Ready, Marathon Ready, Manuscript
Ready, and v1.0 Ready. Each is a packet/protocol pass, not a claim that LLM
quality, 24-hour continuity, or full-manuscript recall has already been
achieved. Later stages inherit earlier gates.

### Testing Ready

Meaning:

Luna is ready for human testers to run the deterministic local product loop and
archive reproducible evidence.

Already gated:

- Local gate runs workspace tests, clippy `-D warnings`, workspace-member
  validation, doctrine lint, runtime scenarios, CLI runtime integration tests,
  release CLI checks, and product smoke.
- `scripts/testing-readiness.ps1` runs the local gate first and archives commit,
  git status, toolchain versions, gate output, smoke JSON, event log, inspect
  output, audit output, repeat-audit output, event-log hash, deterministic LLM
  Ready packet, local runtime trial packet, exact commands, top-level artifact
  hashes, and dirty-worktree patches/untracked files when `-AllowDirty` is used.

Still required:

- Keep the readiness packet as the only archive-grade handoff path.
- Treat dirty packets as evidence snapshots, not release-quality proof, unless
  release notes name the exact diff being tested.
- Keep skipped/ignored tests and unregistered scenarios out of "green" claims.

Not proven by this stage:

- LLM extraction quality.
- Real 24-hour continuity.
- Full real-manuscript one-read recall.

Exit artifact:

- A reproducible Testing Ready evidence packet from a named commit or from a
  dirty packet that preserves the exact source delta. The packet must include
  local gate output, product smoke/repeat audit evidence, deterministic
  LLM-ready packet evidence, local-runtime-trial packet evidence, nested
  manifests, and packet-wide artifact hashes.

### Controlled Human Trial Ready

Meaning:

Luna is ready for the first reviewer-owned local memory test that is smaller
than a marathon and smaller than a manuscript trial.

Required evidence:

- Testing Ready packet for the exact code under trial.
- Reviewer-owned `trial.json` with at least 5 turns and 3 questions created
  before answer generation.
- Packet from `scripts/controlled-human-trial.ps1` with exact log path, copied
  trial file, local runtime trial evidence, scoring template, misses file, and
  regression backlog file.
- Reviewer scoring that turns every miss, stale answer, invented detail, or
  unsupported answer into deterministic regression work or an explicit deferred
  issue.

Not enough:

- An interactive chat transcript without the trial file and hashes.
- Questions written after seeing answers.
- A packet with `ready_for_review_not_passed` status described as passed.
- A controlled trial used to claim 24-hour, manuscript, LLM quality, or v1.0
  readiness.

Exit artifact:

- A completed controlled human trial packet. Until reviewer scoring and
  regression capture are complete, the packet is evidence-ready but not passed.

### LLM Ready

Meaning:

Luna is ready to evaluate an LLM-backed extraction path under repeatable test
conditions. This stage proves the evaluation harness exists; it does not by
itself prove broad LLM quality.

Required evidence:

- The command-backed LLM packet harness records corpus path, copied corpus
  hashes, model id, command path, command arguments, timeout, cache root,
  prompt/schema version, output hashes, cache file hashes, and pass/fail
  summary.
- A small reviewer-owned corpus that checks extraction, correction, uncertainty,
  stale-fact handling, and refusal to invent missing facts.
- Docs that separate deterministic CI proof from LLM-evaluation evidence.

Not enough:

- A present LLM code path with no reproducible run packet.
- Manual chat transcripts without command, model, prompt/schema, and output
  hashes.
- Claims that deterministic scenario success proves LLM extraction quality.

Exit artifact:

- A reproducible LLM evaluation packet with `manifest.json`, `config.json`,
  copied corpus, case stdout/stderr/logs, output hashes, cache hash manifest,
  and replay commands, tied to a Testing Ready packet when release claims require
  the full evidence chain.

### Marathon Ready

Meaning:

Luna is ready to begin the real 10-turn / 24-hour / 3-question continuity
trial. The label means "eligible to run," not "trial passed."

Required evidence:

- Testing Ready packet for the exact code under trial.
- First human controlled trial packet from
  [`CONTROLLED_HUMAN_TRIAL_PROTOCOL.md`](CONTROLLED_HUMAN_TRIAL_PROTOCOL.md),
  with reviewer-owned questions locked before answers, no source leakage in
  prompts, completed scoring, and regression rows for every miss.
- Marathon Ready packet from `scripts/marathon-ready-packet.ps1` with start
  command, close/reopen timestamp capture, exact log path, reviewer-owned
  questions, inspect output, audit output, and generated reopen commands.
- Protocol requirements from
  [`MARATHON_READY_PROTOCOL.md`](MARATHON_READY_PROTOCOL.md), including the
  `eligible_to_run_not_passed` manifest status before the real reopen evidence
  exists.
- A no-edit persisted event log across the real waiting period.
- A rule that every miss becomes a deterministic regression scenario after the
  trial.

Not enough:

- Synthetic timestamp fixtures.
- Scenario-only continuity.
- A readiness packet that lacks the actual trial transcript and final log.

Exit artifact:

- A complete marathon trial packet. The prepared packet status is
  `eligible_to_run_not_passed`; until the reopen evidence passes, Luna may be
  Marathon Ready but not marathon-proven.

### Manuscript Ready

Meaning:

Luna is ready to run a real one-read manuscript trial after local continuity is
auditable. Current deterministic manuscript fixtures are protocol proxies, not
proof of full-manuscript recall.

Required evidence:

- Testing Ready packet for the exact code under trial.
- First human controlled trial packet with completed scoring and regression
  capture, so the reviewer-owned question process has been proven before source
  scale increases.
- Locked source packet, reviewer-owned questions, command transcript, inspect
  output, audit output, event-log hash, and final answers.
- A completed
  [`MANUSCRIPT_ONE_READ_PROOF_PACKET.md`](MANUSCRIPT_ONE_READ_PROOF_PACKET.md)
  packet with manifest, source hash, reviewer questions, answer transcript,
  audit/inspect outputs, scoring, misses, and regression backlog.
- Evidence that recall covers characters, relationships, plot sequence,
  timeline, flashbacks, unknowns, and corrections without rereading the source.
- A rule that every miss becomes a deterministic regression scenario after the
  trial.

Not enough:

- The deterministic protocol fixture alone.
- A short slice described as a full-manuscript result.
- LLM-generated summaries without log-backed recall/audit evidence.

Exit artifact:

- A complete manuscript trial packet. Until that packet passes, Luna may be
  Manuscript Ready but not full-manuscript-proven. The canonical packet template
  is
  [`docs/MANUSCRIPT_ONE_READ_PROOF_PACKET.md`](MANUSCRIPT_ONE_READ_PROOF_PACKET.md).

### v1.0 Ready

Meaning:

Luna can be described as v1.0-ready only after the product loop, evidence
packets, and claimed proof lanes are all repeatable from the repo.

Required evidence:

- Testing Ready packet from the release candidate.
- LLM Ready packet if the release claims LLM-backed extraction quality.
- Passing marathon trial packet if the release claims 24-hour continuity.
- Passing manuscript trial packet if the release claims full-manuscript
  one-read memory.
- Release notes that distinguish CI-proven, locally packet-proven,
  reviewer-trial-proven, present-but-not-gated, and not-yet-true capabilities.
- A completed [`V1_RELEASE_EVIDENCE_PACKET.md`](V1_RELEASE_EVIDENCE_PACKET.md)
  packet with a claim matrix that maps every public release claim to an
  evidence packet or marks it not claimed.

Not enough:

- A green deterministic scenario suite.
- Synthetic timestamp continuity.
- The manuscript protocol fixture.
- An LLM path with no reproducible evidence packet.
- A dirty readiness packet unless the release explicitly identifies the source
  diff and preserves the archived changes.

Exit artifact:

- A complete v1.0 release evidence packet. Until that packet passes, Luna may
  have v1.0 candidate work but not v1.0-ready evidence. The canonical packet
  template is
  [`docs/V1_RELEASE_EVIDENCE_PACKET.md`](V1_RELEASE_EVIDENCE_PACKET.md).

## Backlog Detail

The readiness ladder above controls sequencing. The sections below keep the
older technical work breakdown as backlog detail; do not use them to claim
Testing Ready, LLM Ready, Marathon Ready, Manuscript Ready, or v1.0 Ready
without the stage evidence above.

### 0. Local Runtime Productization

Goal:

Turn the protected scenario engine into a usable local runtime loop before any
24-hour or full-manuscript marathon is treated as an acceptance test.

Concrete artifact:

- `docs/LOCAL_RUNTIME_PRODUCTIZATION_PROTOCOL.md` defines the local run
  protocol, smoke-fixture expectations, command contract, saved evidence, and
  failure criteria.
- `luna-cli runtime smoke` is the executable local product loop. It seeds a
  persisted log, runs optional distract turns, audits, reopens the log, recalls,
  corrects through the same runtime path, recalls again, and audits again.
- `scripts/local-memory-loop.ps1` is the quick local wrapper.
- `scripts/testing-readiness.ps1` is the archival readiness packet builder. It
  runs the local gate and saves commit/status/toolchain evidence, gate output,
  smoke JSON, event log, inspect output, audit output, event-log hash, and the
  exact commands used. It has no skip-gate mode; dirty packets must preserve
  staged/unstaged diffs and untracked files/directories.
- `scripts/local-runtime-trial.ps1` is the non-marathon reviewer trial harness.
  It starts from a caller-owned log path, accepts scripted or live turns,
  reopens the same log for inspect/audit, asks reviewer-owned questions, and
  writes a replayable evidence packet. Its `-Controlled` mode is the required
  first-human-trial surface before 24-hour or manuscript marathon work because
  it requires a source/prompt boundary, locks questions before answers, rejects
  common source-leakage terms, and writes scoring/regression templates.

Required before marathon trials:

- A human can start a local session, enter live or scripted turns, stop the
  session, and reopen the persisted log.
- Inspect and audit commands show lifecycle status, confidence tier, source
  event id/hash, recall reason, bounded working memory, and replay status.
- Corrections and uncertain assertions are available through the same runtime
  loop, not only through hand-authored fixtures.
- A repeatable readiness packet can be generated without losing the gate log or
  product smoke evidence; clean checkouts are preferred, and dirty packets are
  source-archived.
- A repeatable local trial packet can be generated from reviewer-owned questions
  without waiting for the 24-hour marathon.
- A first human controlled trial packet can be generated from
  [`docs/examples/first-human-controlled-trial.json`](examples/first-human-controlled-trial.json)
  and then reviewer-scored before any marathon packet is treated as eligible.
- Command-backed extraction schema/cache/failure boundaries are separately
  checked from deterministic fixture extraction; real LLM extraction quality
  still needs a reproducible evaluation packet.
- Working memory stays bounded in real runtime logs.

Only after this is true:

- Run the 10-turn / 24-hour / 3-question continuity trial.
- Run the full-manuscript one-read trial with reviewer-owned questions.
- Turn every miss from those trials into a deterministic scenario.

### 1. Repair Weak Gates

Goal:

Make the green gate mean what the docs say it means.

Landed and protected gates:

- Schema examples deserialize into Rust and pass Rust validation.
- `recorded_at` tampering is rejected or explicitly tested as non-identity
  metadata.
- Activation component tests cover current/stale/superseded/confidence and
  graph-distance behavior.
- PR doctrine-template completeness is checked on pull requests.
- Scenario coverage is manifest-registered; unlisted, missing, duplicate, and
  nested runtime scenario JSON files fail the local gate/doctrine check.
- The local gate includes clippy, workspace membership validation, manifest
  scenarios, and the product smoke loop on Windows PowerShell.
- CLI product smoke and scenario commands have Rust integration tests.
- Benchmark fixture directories must exist and contain JSON; fixture tests no
  longer return early when coverage disappears.
- Readiness packets cannot skip the local gate, and dirty packets archive
  staged/unstaged diffs plus untracked files/directories.

Still weak:

- Correction back to a superseded value is scenario-covered for the current
  person-location slice, and identity/profession correction has a separate
  runtime scenario plus lifecycle unit coverage. Project identity/description
  correction now has its own runtime scenario plus lifecycle unit coverage too;
  broader correction slot generalization still needs more cases before it can
  be claimed as universal.
- One logical correction must keep proving it is not accidentally counted
  twice as the correction schema grows.

### 2. Bridge Runtime Memory To Topology Orbs

Goal:

Connect product-track runtime memory to the topology/cluster lane so conversational
recall benefits from replayable provenance instead of living beside it.

Implemented bridge slice:

- Adapter from runtime assertions/entities/relations into topology ledger
  artifact refs.
- Runtime scenario checks for node refs, tether refs, source event hashes,
  recall reasons, and SystemKernel leakage boundaries.
- Runtime bridge commits persist replayable topology ledger events, canonical
  ledger-event hashes, accepted/rejected memory-cluster refs, and source-hashed
  node/tether evidence.
- Runtime replay audit recomputes every persisted bridge commit and quarantines
  missing or mismatched durable ledger evidence.
- Bounded working-memory nodes inherit active runtime memory-cluster provenance from
  the replayed cluster registry.

Still needed:

- Long-lived topology/cluster storage outside the runtime log.
- Recall search that can query the topology/cluster store directly instead of using
  runtime recall first and topology evidence second.
- Context packets that cite durable ledger node id, tether path, source event
  id/hash, recall reason, and memory-cluster id together.

Scenario gates:

- `council5_runtime_topology_bridge.json` proves bridge artifacts from runtime
  event logs, verifies the persisted bridge commit event, and replays the
  persisted topology ledger snapshot, not only runtime memory-map structures.
- Joe/Chris/Francois, project, correction, and manuscript scenarios remain
  green.
- Surfaced memory can point to source event id/hash, node id, tether path, and
  recall reason.
- `real_conversation_gate.json` now requires persisted matching topology bridge
  evidence. The remaining product gate is direct recall over durable
  topology/cluster authority.

### 3. Compression Without Lineage Loss

Goal:

Compact dense memory only when lineage survives.

Implemented receipt slice:

- Compression receipts that preserve raw event ancestry.
- Rejection paths for forged, lossy, or under-proven compression.
- Focused runtime slice that can use accepted compression receipts in bounded
  working memory while retaining raw source event citations, but only when a
  verified source id/hash index is supplied.

Still needed:

- Separate metrics for context-size reduction and answer fidelity.
- A product scenario that receives real runtime compression receipts rather than
  only exercising the focused runtime slice.

Unit and milestone gates:

- Compression receipts preserve raw source event refs through replay.
- A deliberately bad compression receipt is rejected before append.
- Focused runtime tests prove compressed context packets still cite raw source
  events while reducing active context size when source hashes are externally
  verified.
- Remaining runtime scenario gate: product-derived compressed cluster answers
  preserve correctness under real retrieval pressure.

### 4. Recognition Sparks For Real Recall

Goal:

Make query/turn signals activate relevant topology, not flat keyword matches or
parallel memory maps.

Implemented activation slice:

- Activation reports that expose entity match, relation match, cue match,
  confidence, filtered-out memory, and graph-depth effects.
- Current, confirmed memory should beat stale, superseded, contradicted, or
  unconfirmed memory when query match is otherwise equal.
- Quiet old memory should remain retrievable when directly cued.

Still needed:

- Direct topology lineage and conflict details inside runtime activation
  reports once runtime recall commits through topology/cluster receipts.

Scenario gates:

- "What do you know about Chris?" activates Chris first and excludes
  Francois-only facts.
- "What did I say about MKPE?" activates project memory without dragging in
  personal/location facts.
- Activation reasons identify signals and filtered memory.

### 5. Auditor Deep Replay

Goal:

Replay must catch drift before runtime memory expands further.

Implemented auditor slice:

- Auditor replays a ledger window and compares live snapshot hash with replayed
  snapshot hash.
- Divergence is quarantined and reported, not silently repaired.
- Cluster lineage can be inspected after consolidation/evolution replay.
- `luna-cli runtime audit --log` audits persisted runtime logs and scenario
  checks can require clean replay with source-hashed topology evidence.

Still needed:

- Scheduled/background runtime auditor integration over long-lived product logs.

Scenario gates:

- A forced divergence fixture is detected.
- A valid cluster-backed topology log replays cleanly.
- Runtime scenario logs replay cleanly after runtime topology commits.
- Remaining runtime gate: long-lived product logs replay cleanly after
  compression is product-integrated.

### 6. Splinter And Merge Mechanics

Status:

- First topology-lane receipt mechanics are implemented in `luna-cluster` and
  covered by `crates/luna-tests/tests/milestone_4.rs`.
- Split receipts retire the parent cluster, create child clusters, and preserve parent
  source event ancestry plus evolution event lineage.
- Merge receipts retire compatible parents, create one child cluster, and preserve
  the union of parent source event ancestry.
- Forged split receipts reject before registry mutation.
- Split evolution now requires contradiction-pressure or splinter-pressure
  evidence before retiring a parent cluster.
- Focused runtime slice filters retired-cluster provenance so retired memory is not
  surfaced as current recall when an explicit cluster activation state is supplied.
- One split path is appended through the topology ledger and replayed back into
  equivalent state.
- Not yet proven: per-child split membership partitions, typed merge
  compatibility thresholds, sentinel-triggered splits, end-to-end product
  runtime recall after split/merge, or product memory using these receipts.

Goal:

Let memory grow sanely by splitting unstable orbs and merging compatible ones
without losing ancestry.

Needed:

- Typed merge compatibility thresholds that decide when merge receipts are
  allowed rather than only proving the receipt mechanics.
- Sentinel-triggered split execution in the product runtime path.
- Recall after split prefers current, coherent memory while preserving
  superseded facts.

Scenario gates:

- Topology lane: split and merge receipts preserve source event ancestry and
  replayable evolution lineage.
- Topology lane: forged split receipts are rejected before mutation.
- Topology lane: contradiction/splinter pressure is required before split
  mutation.
- Runtime slice: retired-cluster provenance is filtered from current recall.
- Remaining runtime gate: sentinels can trigger splits and recall after
  split/merge remains bounded and provenance-backed.

### 7. Marathon Trial Readiness

Goal:

Prepare the real 10-turn / 24-hour / 3-question and full-manuscript trials only
after the local runtime productization gate exists. The Marathon Ready protocol
and packet script exist now; they make the run eligible and reproducible, not
passed.

Required before marathon trials:

- Runtime memory is topology-node/tether backed in deterministic scenarios;
  memory-cluster product authority remains to be integrated.
- A human can start a local session, enter live or scripted turns, stop the
  session, and reopen the persisted log.
- Inspect and audit commands show lifecycle status, confidence tier, source
  event id/hash, recall reason, bounded working memory, and replay status.
- Corrections and uncertain assertions are available through the same runtime
  loop, not only through hand-authored fixtures.
- Command-backed extraction schema/cache/failure boundaries are separately
  checked from deterministic fixture extraction; real LLM extraction quality
  still needs a reproducible evaluation packet.
- Working memory stays bounded in real runtime logs.

Scenario gates:

- Existing deterministic scenarios remain green.
- The product-loop CLI smoke fixture runs through the same persistence,
  inspect, recall, correction, distract-turn, and audit path a user would use
  locally, and `scripts/testing-readiness.ps1` archives the proof packet.
- Runtime audit exits non-zero on quarantined replay drift and remains green on
  the smoke log.
- `docs/MARATHON_READY_PROTOCOL.md` defines the required 24-hour trial packet,
  and `scripts/marathon-ready-packet.ps1` prepares start/reopen scripts with
  `eligible_to_run_not_passed` status until the real reopen evidence exists.

Only after this is true:

- Ten-turn conversation.
- Runtime closed.
- Return after a real delay, ideally 24 hours.
- Three questions answered correctly.
- Answers include confidence tier, unknowns/ambiguity, recall reason, and
  bounded working memory.
- Full-manuscript one-read trial with reviewer-owned questions.
- Every miss becomes a new deterministic scenario.

### 8. Manuscript Memory Track

Goal:

Expand one-read manuscript memory after conversation memory has passed at least
once.

Needed:

- Scene/event ingestion.
- Character entity graph.
- Relationship graph.
- Timeline graph.
- Flashback handling.
- Plot-state and open-loop tracking.
- One-read lockout: no rereading during retrieval.

Scenario gates:

- Character identity survives aliases and nicknames. Implemented by
  `scenarios/runtime/manuscript_character_alias.json`.
- Scene order and story chronology are separated. Implemented by
  `scenarios/runtime/manuscript_scene_chronology_flashback.json`.
- Flashback facts do not overwrite present-time facts incorrectly. Implemented
  by `scenarios/runtime/manuscript_scene_chronology_flashback.json`.
- Unresolved arcs remain open until resolved. First executable slice implemented
  by `scenarios/runtime/manuscript_open_arc.json`; resolution/closure remains
  the next plot-state extension.
- One-read lockout proxy is implemented by
  `scenarios/runtime/manuscript_one_read_lockout_proxy.json`: after an explicit
  close marker, later `MANUSCRIPT:` source text is not ingested as new
  manuscript memory.
- One-read protocol gating is implemented by
  `scenarios/runtime/manuscript_one_read_protocol.json`: it records the source
  read, requires an explicit close, blocks retrieval-time source reread/search
  markers, rejects new manuscript assertions on retrieval turns, and reports
  protocol eligibility from the runtime scenario report.
- Remaining proof gate: a full real-manuscript one-read trial still needs a
  larger source, reviewer-owned questions, and a run log that is not authored as
  a deterministic fixture. The packet template for that trial is
  [`docs/MANUSCRIPT_ONE_READ_PROOF_PACKET.md`](MANUSCRIPT_ONE_READ_PROOF_PACKET.md).

## Daily Build Rule

Each day should finish at least one concrete memory capability and one concrete
guardrail.

Capability examples:

- topology-backed runtime recall
- correction handling
- compression with lineage
- activation scoring
- question priority
- response planning

Guardrail examples:

- runtime scenario
- inspect output
- provenance check
- confidence-tier check
- working-memory budget check
- doctrine/gate check

If a change cannot be inspected, tested, or explained, it is not ready to become
architecture.

## Anti-Magic Checks

Before accepting a memory change, ask:

1. Is this a reusable mechanism or a scripted outcome?
2. Can it handle new names, projects, and values without another special case?
3. Is the event log still the source of truth?
4. Can the derived structure be rebuilt?
5. Does it preserve ambiguity?
6. Does it avoid dumping too much memory into context?
7. Does it explain why memory surfaced?
8. Does it have a scenario that can fail?
9. Does it help the 10-turn acceptance test?
10. Does it help the manuscript one-read target?

If the answer is no, the idea may still be useful, but it belongs in backlog
until it can be made concrete.
