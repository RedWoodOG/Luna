# Echoes Fallen / Tetherfall — 20-Turn Memory Test (Pre-Intake Baseline)

Frozen baseline result of the reviewer-owned 20-turn Echoes Fallen memory test run
against the **current** Luna runtime (commit `f63c1e3`, heuristic extraction, no
manuscript-intake layer). This is the locked "failing test" that the
manuscript/language intake build (track A) must move — per the safe-build-path
rule, the test is not rewritten to make Luna look better.

## Method (no human in the loop)

1. Fed all 20 prose turns as separate `luna-cli runtime turn` calls into a single
   persisted log (`.luna/echoes-fallen-test/events.jsonl`).
2. Each query runs as a **fresh CLI process** reopening the persisted log (forced
   retrieval cycle, not in-process context).
3. Asked all 25 reviewer questions; saved every reply
   (`.luna/echoes-fallen-test/answers/q01..q25.md`).
4. Scored honestly against the reviewer rubric.

## Stored memory after 20 turns (audit, replay-clean)

| Metric | Value |
|---|---|
| stored_events | 100 |
| **claims** | **0** |
| current_claims | 0 |
| entity_groups | 0 |
| topology_nodes / tethers / source_event_refs | 0 / 0 / 0 |
| topology_orbs | 0 |
| dense_receipts | 0 |
| memory_nodes / edges | 9 / 7 (SystemKernel scaffolding only — no story content) |
| audit status | clean (replay hashes match) |

Every turn was classified `IgnoreNoise` (`created=0`). The heuristic extractor
fires only on fixed phrasings (`X is my Y`) and explicit `MANUSCRIPT:` markers;
the test turns are natural prose, so **zero** story entities, relations, scene
shapes, or topology were captured.

## Question results

**Answered from memory: 0 / 25.** Every question returned
`response actions: AvoidAnswering, StateUncertainty`, `Recalled: (none)`,
"No prior memory was activated for this turn." Example saved replies:

- **Q12 "What is a tether?"** → no recall, AvoidAnswering.
- **Q13 "Why did T'Sari believe the contact was need, not attack?"** → no recall,
  AvoidAnswering.

## Scoring against the reviewer rubric

| Level | Result |
|---|---|
| Pass Level 1 — Surface (names/places/roles) | **FAIL** (0 stored) |
| Pass Level 2 — Relationship | **FAIL** |
| Pass Level 3 — Conceptual (story rules) | **FAIL** |
| Pass Level 4 — Inference / Continuity | **FAIL** |

### Failure conditions (the rubric's "Luna fails if she…")

Critically, **none** of the hallucination/confusion failure conditions were
triggered:

- Did NOT confuse Jax with Praxxus.
- Did NOT call the tether telepathy/resonance.
- Did NOT claim T'Sari believed Jax attacked her.
- Did NOT invent Earth as known to the Accord.
- Did NOT treat the Orin Threshold as mere attackers.
- Did NOT overstate facts — it **admitted it did not know** on all 25.

## Honest diagnosis

The failure is **100% in extraction**, not in recall or honesty. With nothing
stored, the (now-green) answer layer did the *right* thing: it refused to answer
rather than fabricate. So this run cleanly separates the two axes:

- **Recall from prose memory: 0/25** — the real, expected gap.
- **Hallucination / invention: 0** — memory hygiene holds; no false answers.

This is exactly the evidence that justifies track A (manuscript/language intake):
the missing capability is prose → structured candidate memory
(entities/aliases/relations/scene-shape/coref) with provenance. Once A populates
real claims + topology + receipts, the existing recall/answer path can surface
them. The hard-fail gates for A (nonzero claims, topology refs, dense receipts,
provenance-backed answers) all currently read **zero** — this baseline is what A
must move off zero without rewriting the test.

## Reproduce

```powershell
# turns + questions are committed under .luna/echoes-fallen-test/ inputs (data root)
# feed 20 turns -> audit -> ask 25 questions; see answers/q01..q25.md
```
