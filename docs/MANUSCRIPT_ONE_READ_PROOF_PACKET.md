# Manuscript One-Read Proof Packet

This is the packet template for a real Manuscript Ready trial. It is not a
deterministic fixture, not a summary exercise, and not proof of all future
manuscripts. It proves only the named source, named code revision, named run,
and reviewer-owned questions archived in the packet.

## Claim Boundary

A complete passing packet may support this claim:

```text
For this source packet, this Luna revision answered the archived reviewer-owned
questions after a single source-read phase, with no retrieval-time reread, and
with replayable log/audit evidence.
```

It must not be described as:

- general full-manuscript quality;
- LLM extraction quality;
- 24-hour continuity;
- baseline superiority;
- v1.0 readiness by itself.

## Packet Layout

Use this directory shape:

```text
.luna/manuscript-one-read/<timestamp>/
  MANIFEST.json
  README.md
  commands.ps1
  source/
    source.txt
    source.sha256
    source_metadata.json
  questions/
    questions.json
    reviewer_notes.md
  logs/
    event_log.jsonl
    event_log.sha256
    stdout.log
    stderr.log
  outputs/
    inspect_before_questions.json
    audit_before_questions.json
    answers.json
    inspect_after_questions.json
    audit_after_questions.json
  review/
    scoring.md
    misses.md
    regression_backlog.md
```

The packet may include additional files, but the files above are the minimum
archive set.

## Manifest Template

`MANIFEST.json` must be filled in before any pass claim:

```json
{
  "packet_type": "manuscript-one-read",
  "packet_version": 1,
  "created_at_utc": "",
  "repo": {
    "branch": "",
    "commit": "",
    "git_status_short": "",
    "allow_dirty": false,
    "dirty_diff_files": []
  },
  "toolchain": {
    "rustc": "",
    "cargo": "",
    "powershell": "",
    "os": ""
  },
  "source": {
    "title_or_slug": "",
    "word_count": 0,
    "sha256": "",
    "locked_before_run": true,
    "source_read_started_at_utc": "",
    "source_read_closed_at_utc": ""
  },
  "questions": {
    "reviewer": "",
    "count": 0,
    "created_before_answers": true,
    "sha256": ""
  },
  "run": {
    "event_log": "logs/event_log.jsonl",
    "event_log_sha256": "",
    "commands_file": "commands.ps1",
    "retrieval_started_at_utc": "",
    "retrieval_completed_at_utc": ""
  },
  "outputs": {
    "inspect_before_questions": "outputs/inspect_before_questions.json",
    "audit_before_questions": "outputs/audit_before_questions.json",
    "answers": "outputs/answers.json",
    "inspect_after_questions": "outputs/inspect_after_questions.json",
    "audit_after_questions": "outputs/audit_after_questions.json"
  },
  "result": {
    "status": "not-reviewed",
    "passed_question_ids": [],
    "failed_question_ids": [],
    "unknown_or_ambiguous_question_ids": [],
    "regressions_opened": []
  }
}
```

## Question Template

`questions/questions.json` must be reviewer-owned and created before answer
generation.

```json
{
  "reviewer": "",
  "created_at_utc": "",
  "questions": [
    {
      "id": "q001",
      "category": "character|relationship|plot|timeline|flashback|open_thread|unknown",
      "question": "",
      "expected_evidence": "",
      "must_not_include": "",
      "notes": ""
    }
  ]
}
```

Do not include answers in this file. Expected evidence should identify the kind
of source-backed fact the reviewer expects, not feed Luna a final response.

## Answer Template

`outputs/answers.json` must preserve every answer exactly as produced.

```json
{
  "answers": [
    {
      "question_id": "q001",
      "question": "",
      "answer": "",
      "confidence_tier": "",
      "recall_reason": "",
      "unknowns_or_ambiguity": "",
      "supporting_memory_refs": [],
      "command": "",
      "started_at_utc": "",
      "completed_at_utc": ""
    }
  ]
}
```

## Required Procedure

1. Start from a Testing Ready packet for the exact revision under trial.
2. Lock `source/source.txt` and record `source/source.sha256`.
3. Create reviewer-owned `questions/questions.json` before answer generation.
4. Run a source-read phase once.
5. Close the source explicitly.
6. Reopen or continue only from the persisted event log.
7. Capture inspect and audit output before questions.
8. Ask every reviewer question without source text, reread requests, search
   requests, or answer hints in the prompt.
9. Capture final answers exactly as produced.
10. Capture inspect and audit output after questions.
11. Score the run in `review/scoring.md`.
12. Add every miss, invented detail, stale fact, or unsupported answer to
    `review/regression_backlog.md`.

## Pass Criteria

A packet can pass only if:

- the source hash is stable from lock through final audit;
- the questions were created before answer generation;
- retrieval prompts do not contain source text or reread/search instructions;
- audit output is green before and after questions;
- answers preserve confidence tier, recall reason, unknowns or ambiguity, and
  supporting memory references where available;
- reviewer scoring marks every required question as pass or justified unknown;
- every failed or ambiguous answer is captured as regression work.

## Failure Criteria

The packet fails if:

- source text is introduced during retrieval;
- the event log is edited between source read and questions;
- audit fails or cannot be reproduced;
- questions are written after seeing answers;
- answers rely on unsupported source claims;
- misses are not turned into regression backlog items;
- the packet claims full-manuscript or v1.0 proof beyond this run.
