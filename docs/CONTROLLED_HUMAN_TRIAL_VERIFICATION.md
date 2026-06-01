# Controlled Human Trial — Scored / Reviewed Record

Record of completing the reviewer scoring for the first controlled human trial
packet, moving it off `ready_for_review_not_passed`. This is the ladder rung
**before** Marathon Ready (the 10-turn / 24-hour / 3-question trial). It is not a
24-hour, LLM-quality, manuscript, or v1.0 claim.

## Packet

- **Location:**
  `.luna/controlled-human-trial/20260514-093857/` (OneDrive data root, not git).
- **Packet kind:** `luna.controlled_human_trial.v1`.
- **Commit under trial:** `9d2cd26b86583950e2b8bd60a2bd668a976e2922` (dirty packet).
- **Status:** `ready_for_review_not_passed` → **`reviewed_passed_with_regressions`**.
- **Reviewer:** agent reviewer (Claude), on the operator's behalf. Agent-scored —
  weaker independence than a third-party human; all judgments inspectable in
  `review/scoring.md` and re-gradable.

## Boundary integrity (verified)

- Questions locked before answers: `questions-lock.json`,
  `questions_locked_before_answers: true`.
- Source/prompt boundary respected: no source text, answer hints, or
  reread/search terms in prompts.
- Event log unchanged: packet `events.jsonl` recomputes to
  `a867d94f…349c0a6f`, matching the stored hash.
- Final replay audit: `status: clean`, `replay_error: null`,
  `live_snapshot_hash == replayed_snapshot_hash` (`891d8e15…298fc3`), 46 events,
  24/24 valid topology refs.

## Trial and result

Reviewer-owned turns (the only source material): Mara founds **Northstar Maps**
(trail-route planning for hikers) → preparing a spring pilot with three local
hiking clubs → **Correction: now called Cleartrail** → Cleartrail's pilot focuses
on safer weekend planning.

| Q | Question | Result |
|---|---|---|
| q001 | What is the project called now? | **pass** — "now called Cleartrail"; Northstar not presented as current. |
| q002 | What does the project help people do? | **pass (documented defect)** — purpose correctly recalled; used the **retired** name "Northstar Maps" → miss M-1. |
| q003 | Who is the pilot with? | **pass** — "three local hiking clubs." |

Pass rule met (all required questions pass on their stated criteria; every miss
captured as regression work). **Process pass**, not a claim of fully general
correction handling.

## What the trial surfaced (the point of it)

A real **correction-propagation gap**: the project rename was captured (q001) but
did not re-key the project's *other* claims, so a purpose query surfaced the old
name (q002). Root cause in the audit: `claims: 5, current_claims: 5` — **zero
supersessions** (contrast the memory-beyond-retrieval run's `7 / 6`, a real
person-location supersession). Captured as:

- **R-1** (deterministic regression): `project_rename_propagates_to_dependent_claims`
  — rename must supersede/re-key dependent project claims and recall must surface
  the current name. Ties to roadmap §Known Weak Gates #1.
- **R-2** (deferred policy issue): confidence tier for a reviewer-stated
  correction target.

See `review/scoring.md`, `review/misses.md`, `review/regression_backlog.md` in the
packet.

## Ladder position after this record

`Testing Ready → **Controlled Human Trial (scored, passed-with-regressions)** →
Marathon Ready (24h, still eligible-not-passed) → Manuscript → v1.0.`

To attach a controlled-trial pass to **current** code (not commit `9d2cd26`),
regenerate the packet against `main` HEAD via
`scripts/controlled-human-trial.ps1` and re-score. The R-1 regression should land
as a deterministic scenario before that re-run is treated as a clean pass.
