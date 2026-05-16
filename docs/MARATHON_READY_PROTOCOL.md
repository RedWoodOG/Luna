# Marathon Ready Protocol

Marathon Ready means Luna is eligible to run the real 10-turn / 24-hour /
3-question continuity trial. It does not mean the trial has passed.

## Required Input

Before preparing this packet, complete a controlled human trial packet from
[`CONTROLLED_HUMAN_TRIAL_PROTOCOL.md`](CONTROLLED_HUMAN_TRIAL_PROTOCOL.md).
The controlled packet should have reviewer scoring and regression capture
completed; its existence does not prove 24-hour continuity, but it proves the
reviewer-owned question process before the real waiting period.

Prepare a reviewer-owned JSON file with at least 10 start turns and at least 3
reviewer questions:

```json
{
  "turns": [
    "Turn 1...",
    "Turn 2..."
  ],
  "questions": [
    "Question 1?",
    "Question 2?",
    "Question 3?"
  ]
}
```

## Prepare Packet

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\marathon-ready-packet.ps1 -Log .\.luna\marathon\events.jsonl -TrialFile .\.luna\marathon\trial.json
```

The packet writes:

- `manifest.json` and `manifest.md` with `eligible_to_run_not_passed` status.
- the exact marathon log path;
- reviewer questions;
- ready-packet start/close/reopen eligibility timestamps;
- `start-marathon.ps1`, which records actual start and close timestamps;
- `reopen-after-24h.ps1`, which refuses to run before the recorded 24-hour gap
  and verifies the event log hash before asking reviewer questions;
- an immediate local-runtime rehearsal packet proving the command path is
  replayable before the real waiting period.

This packet does not prove 24-hour continuity, manuscript memory, LLM quality,
or v1.0 readiness. It only proves that the trial inputs and command path are
ready to begin and can be archived before the waiting period.

## Actual Trial

1. Run `start-marathon.ps1 -ResetLog` from the prepared packet.
2. Close Luna and do not edit the event log.
3. Wait until `start-evidence/reopen-not-before-at.txt`.
4. Run `reopen-after-24h.ps1`.
5. Review `reopen-evidence/question-*.md`, `inspect-final.json`, and
   `audit-final.json`.

Only the completed reopen evidence can support a marathon-passed claim.
