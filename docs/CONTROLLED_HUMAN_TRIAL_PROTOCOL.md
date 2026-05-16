# Controlled Human Trial Protocol

Controlled Human Trial is the first reviewer-owned testing step after Testing
Ready. It is deliberately smaller than the 24-hour marathon and smaller than a
full manuscript one-read trial.

## Claim Boundary

A completed packet may support this claim:

```text
For this reviewer-authored trial file and this Luna revision, Luna preserved a
local memory log, answered the archived reviewer questions from that log, and
saved enough evidence to score misses and create deterministic regressions.
```

It must not be described as:

- 24-hour continuity proof;
- full-manuscript one-read proof;
- LLM extraction quality proof;
- v1.0 readiness;
- general reliability.

## Required Input

Create a reviewer-owned JSON file before answer generation:

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
    "pass_rule": "Every miss is captured in review/regression_backlog.md."
  },
  "regression_capture": {
    "required": true,
    "backlog_path": "review/regression_backlog.md"
  },
  "turns": [
    "Turn 1...",
    "Turn 2...",
    "Turn 3...",
    "Turn 4...",
    "Turn 5..."
  ],
  "questions": [
    {
      "id": "q001",
      "category": "identity|project|relationship|correction|episodic|unknown",
      "question": "Question 1?",
      "expected_evidence": "The kind of memory evidence the reviewer will check.",
      "must_not_include": "Details that would indicate stale or leaked source use.",
      "notes": ""
    },
    {
      "id": "q002",
      "category": "identity|project|relationship|correction|episodic|unknown",
      "question": "Question 2?",
      "expected_evidence": "",
      "must_not_include": "",
      "notes": ""
    },
    {
      "id": "q003",
      "category": "identity|project|relationship|correction|episodic|unknown",
      "question": "Question 3?",
      "expected_evidence": "",
      "must_not_include": "",
      "notes": ""
    }
  ]
}
```

Minimums:

- at least 5 turns;
- at least 3 reviewer-owned questions;
- source and prompt boundary objects;
- scoring and regression-capture objects;
- no answers, gold answers, or expected final wording in the question prompts;
- no source text, hidden hints, or reread/search instructions in the question
  phase.

## Run Packet

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\controlled-human-trial.ps1 -Log .\.luna\controlled-human-trial\events.jsonl -TrialFile .\.luna\controlled-human-trial\trial.json
```

The script rejects a dirty checkout unless `-AllowDirty` is provided. If dirty
source is allowed, staged and unstaged patches are archived inside the packet.

## Packet Output

The packet writes:

- `manifest.json` and `manifest.md`;
- copied `trial.json`;
- exact log path and event-log hash;
- local runtime trial packet with command transcript, inspect/audit outputs,
  answers, copied event log, and hashes;
- nested `local-runtime-trial/questions-lock.json` and
  `local-runtime-trial/questions-lock.sha256.txt`;
- nested `local-runtime-trial/review/source-prompt-boundary.md`;
- `review/scoring.md`;
- `review/misses.md`;
- `review/regression_backlog.md`.

The initial status is `ready_for_review_not_passed`. The trial is not passed
until the reviewer scores the answers and every miss is turned into deterministic
regression work or an explicit deferred issue.

## Pass Criteria

- The packet was generated from a clean commit, or dirty diffs are archived and
  named.
- The trial JSON defines source boundary, prompt boundary, scoring, and
  regression capture before answer generation.
- `questions-lock.json` was written and hashed before Luna answered.
- The event log exists and hashes are preserved.
- Inspect and audit output are present before and after questions.
- Question prompts do not include source text, answer hints, or reread/search
  instructions.
- Answers include enough transcript evidence to judge recall, uncertainty, and
  unsupported claims.
- Every failed, stale, invented, or unsupported answer appears in
  `review/regression_backlog.md`.

## Failure Criteria

- Questions were written after seeing answers.
- Question prompts include answer hints, source text, or reread instructions.
- The event log is edited between turns and questions.
- Audit output is missing, failing, or not reproducible.
- Misses are summarized away instead of becoming regression work.
- The packet is used to claim marathon, manuscript, LLM, or v1.0 readiness.
