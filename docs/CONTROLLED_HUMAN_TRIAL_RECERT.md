# Controlled Human Trial — Re-certification on `82f49de`

Re-ran the controlled human trial harness (`scripts/controlled-human-trial.ps1`)
on current code with the reviewer-owned Mara/Northstar trial
(`docs/examples/first-human-controlled-trial.json`). Packet:
`.luna/controlled-human-trial/20260601-recert/` (data root, not git).

## Result: reviewed, **not passed** — regressions captured

Boundary intact: questions locked before answers, audit `status: clean`,
`replay_error: null`. Default extraction path (intake flag OFF). Agent-scored.

| Q | Reply | Result |
|---|---|---|
| What is Mara's project called now? | "Mara is the founder of Northstar Maps." | **fail** |
| What does Mara's project help people do? | (same) | **fail** |
| Who is Mara preparing the pilot with? | (same) | **fail** |

0/3 correct. **No fabrication** — the stated fact is true; the failures are
missing coverage + routing, not invention.

## Two root causes (captured as regression work)

- **R-1 cross-entity routing.** "Mara's project … called now?" routes to the
  *person* Mara and returns her facts; the stored project rename
  (`project:identity = Northstar Maps is now called Cleartrail`, confirmed in the
  packet's `inspect-final`) is never surfaced. Same class as the Echoes Fallen
  "vessel that recovers Jax" miss — a known, deferred recall-routing limitation.
- **R-2 native extractor coverage gaps.** Trial turns 2/3/5 ("X helps Y", "X is
  preparing Y", "Cleartrail's pilot focuses…") produced **zero** claims — the
  heuristic extractor has no pattern for them. Audit shows only 2 of 5 turns
  yielded claims. (The opt-in AURA intake captures some of these but is not yet
  lifecycle-safe; see `INTAKE_DEFAULT_READINESS.md`.)

## What this re-certification establishes (honestly)

- The **lifecycle layer is healthy** on current code: replay clean, the rename is
  correctly stored as a project claim.
- The **answer layer has a real, reproducible weakness** vs. the prior packet:
  cross-entity routing + extractor coverage. The trial did its job — it found
  regressions, which are now logged (R-1, R-2) rather than stamped green.

This is not a 24h / LLM / manuscript / v1.0 claim. It is a single agent-scored
controlled trial that re-certified current code and **correctly failed**,
surfacing the next work.
