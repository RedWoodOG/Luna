# Local Runtime Productization Protocol

This protocol is the Testing Ready product-loop milestone before any LLM quality
claim, 24-hour continuity trial, or full-manuscript marathon trial.

## Purpose

Prove Luna can run as a local memory product loop, not only as hand-authored
scenario fixtures. The output is a Testing Ready evidence packet, not proof of
LLM extraction quality, real 24-hour continuity, or full-manuscript one-read
recall.

## Required Loop

The product loop must support:

1. Start a local session with a predictable log path.
2. Enter live or scripted turns.
3. Stop the session.
4. Reopen the same persisted log.
5. Inspect memory state, including lifecycle status and confidence tier.
6. Ask recall questions from bounded working memory.
7. Correct a stored fact through the same runtime loop.
8. Audit the persisted log with strict event hashes.

## Command Contract

The smoke artifact should exercise these commands or their direct replacement:

```powershell
cargo run -p luna-cli -- runtime turn "Chris lives in Iowa." --log .\.luna\product_smoke\events.jsonl --format markdown
cargo run -p luna-cli -- runtime inspect --log .\.luna\product_smoke\events.jsonl
cargo run -p luna-cli -- runtime audit --log .\.luna\product_smoke\events.jsonl
cargo run -p luna-cli -- runtime turn "Correction: Chris lives in Ohio." --log .\.luna\product_smoke\events.jsonl --format markdown
cargo run -p luna-cli -- runtime inspect --log .\.luna\product_smoke\events.jsonl
cargo run -p luna-cli -- runtime audit --log .\.luna\product_smoke\events.jsonl
cargo run -p luna-cli -- runtime turn "Where does Chris live?" --log .\.luna\product_smoke\events.jsonl --format markdown
```

The second inspect/audit pair must run from a new CLI process after the
correction, proving the persisted log can be reopened cleanly before final
recall.

The current executable shortcut is:

```powershell
cargo run -p luna-cli -- runtime smoke --log .\.luna\product_smoke\events.jsonl --reset --json --report .\.luna\product_smoke\smoke-report.json
```

The current readiness packet builder is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\testing-readiness.ps1
```

It runs the local gate first, then reruns the product smoke with archived
inspect, audit, and repeat-audit outputs. It also includes a deterministic
command-backed LLM-ready packet and a local-runtime-trial packet so the first
testing handoff contains the extraction harness and reviewer-trial harness
evidence. The packet preserves nested manifests plus a top-level
`artifact-hashes.json` SHA256 inventory. It has no skip-gate mode. A dirty
checkout is rejected unless `-AllowDirty` is passed; in that case the packet
must preserve staged and unstaged patches plus untracked files/directories.

The current non-marathon local reviewer trial harness is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-runtime-trial.ps1 -Log .\.luna\local-trial\events.jsonl -ResetLog -TrialFile .\.luna\local-trial\trial.json
```

The first reviewer-owned controlled human trial must use the controlled mode:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\local-runtime-trial.ps1 -Controlled -Log .\.luna\first-human-trial\events.jsonl -ResetLog -TrialFile .\.luna\first-human-trial\trial.json -OutDir .\.luna\first-human-trial\packet
```

The trial file is reviewer-authored JSON:

```json
{
  "protocol": "first-human-controlled-runtime-trial-v1",
  "source_boundary": {
    "source_scope": "Only turns[] are source material for this trial.",
    "locked_before_questions": true,
    "forbidden_question_terms": [
      "according to the source",
      "open the source",
      "reread",
      "search the source"
    ]
  },
  "prompt_boundary": {
    "questions_created_before_answers": true,
    "no_source_text_in_questions": true,
    "no_answer_hints_in_questions": true,
    "no_retrieval_time_reread_or_search": true
  },
  "scoring": {
    "reviewer": "",
    "scale": "pass|partial|fail|justified_unknown|boundary_violation",
    "pass_rule": "All misses are captured in review/regression_backlog.md."
  },
  "regression_capture": {
    "required": true,
    "backlog_path": "review/regression_backlog.md"
  },
  "turns": [
    "Taylor lives in Vermont.",
    "Taylor is planning a quiet Sunday grocery run."
  ],
  "questions": [
    {
      "id": "q001",
      "category": "location",
      "question": "Where does Taylor live?",
      "expected_evidence": "The answer should come from the persisted memory log.",
      "must_not_include": "",
      "notes": ""
    }
  ]
}
```

Use `-TurnsFile` and `-QuestionsFile` for separate repeatable reviewer-authored
fixtures, or `-Live` to enter turns and questions interactively. The script must
reopen the same persisted log for inspect/audit before reviewer questions, run
each question as a separate runtime turn, and write a replayable packet
containing the copied log, command transcript, inputs, inspect/audit outputs,
question transcripts, hashes, and manifest.

In `-Controlled` mode, the script requires all turns and questions to come from
`-TrialFile`, rejects common answer-leak fields and forbidden source/prompt
terms, writes `questions-lock.json` before answer generation, and creates
`review/source-prompt-boundary.md`, `review/scoring.md`, and
`review/regression_backlog.md`. See
[`CONTROLLED_HUMAN_TRIAL_PROTOCOL.md`](CONTROLLED_HUMAN_TRIAL_PROTOCOL.md)
for the full packet contract and exact review procedure.

The Marathon Ready packet builder is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\marathon-ready-packet.ps1 -Log .\.luna\marathon\events.jsonl -TrialFile .\.luna\marathon\trial.json
```

It wraps the local runtime trial as an immediate rehearsal, then writes
`start-marathon.ps1` and `reopen-after-24h.ps1` for the real 24-hour trial. Its
manifest status is `eligible_to_run_not_passed`; the packet is not a passed
marathon result until the generated reopen script has been run after the real
gap and the final audit/question evidence has been reviewed.

The Controlled Human Trial packet builder is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\controlled-human-trial.ps1 -Log .\.luna\controlled-human-trial\events.jsonl -TrialFile .\.luna\controlled-human-trial\trial.json
```

It wraps a reviewer-owned 5+ turn / 3+ question local trial, runs the local
runtime trial harness, and writes scoring, misses, and regression-backlog
templates. Its manifest status is `ready_for_review_not_passed` until the
reviewer scores the answers and every miss is converted into regression work.

## Saved Evidence

The smoke run must save:

- the event log;
- inspect output;
- audit output;
- final recall output;
- the exact commands used.
- the commit hash, dirty/clean git status, toolchain versions, smoke report
  JSON, and event-log SHA256 when building a readiness packet.
- staged/unstaged diff patches and untracked source copies when
  `-AllowDirty` is used.
- reviewer-owned questions, question transcripts, final inspect/audit JSON, and
  the copied event log when building a local runtime trial packet.
- controlled human-trial packets must also save the source/prompt boundary,
  locked question hash, scoring sheet, and regression backlog template before
  any pass claim.
- deterministic LLM-ready packet manifest and local-runtime-trial packet
  manifest when building a Testing Ready packet.
- copied LLM-ready corpus, command transcript, cache hash manifest, case output
  hashes, and top-level `artifact-hashes.json` when building a Testing Ready
  packet.
- scoring, misses, and regression-backlog templates when building a controlled
  human trial packet.

## Pass Criteria

- The log is non-empty and every event has a valid persisted `event_hash`.
- `runtime audit` exits clean for the smoke log.
- repeat `runtime audit --format json` for the smoke log is byte-stable against
  the first captured audit JSON.
- Inspect output shows current and superseded lifecycle state after correction.
- Final recall answers from the corrected current fact, includes confidence and
  recall reason, and does not surface the superseded value as current.
- Working memory remains within the configured budget.
- Optional distract turns do not prevent later recall from the persisted log.

## Fail Criteria

- Missing, empty, or hashless logs.
- Recall succeeds only because a fixture answer or stale log was reused.
- The correction is not inspectable.
- Audit passes after event tampering.
- The smoke run cannot be repeated from a clean checkout.
- A readiness packet claims marathon eligibility without preserving the gate
  log, event log, inspect output, audit output, and exact command transcript.
- A human-trial packet uses reviewer questions that contain source text, answer
  hints, reread/search instructions, or no regression-capture path.
