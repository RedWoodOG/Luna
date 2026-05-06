# Stage 7 findings — first probe of v1.0 acceptance

**Date.** `7c99ee9` (post-event-log-hardening, post-STATUS.md).
**Scope.** First attempt at executing Stage 7 of `LUNA_MEMORY_MILESTONE_ROADMAP.md` — the 10-turn real-conversation acceptance test described in the README.
**Result.** Both probe fixtures fail. The bottleneck is **extraction**, not memory architecture. This finding changes the priority order of the rebuild.

## What was tried

Two fixtures, paired to isolate one variable (prose style):

| Fixture | Style | Turn 1 assertions | Total assertions across 13 turns | Outcome |
|---|---|---:|---:|---|
| `stage7_dense_week.json` | natural conversational prose | 0 | 0 | FAIL: 6 of 6 `must_contain` checks missing |
| `stage7_dense_week_patterned.json` | rewritten in explicit "I am X" / "X is Y" templates | 1 | 1 | FAIL: 5 of 6 `must_contain` checks missing |

Both fixtures cover the same week:

- 10 conversational turns about a platform team (Joe → leads Mira on backend, Daniel on QA → Beacon project → session-bug found and fixed → Daniel takes vacation, back the 17th → Beacon ships)
- 3 question turns (when does Daniel return / what bug did Mira fix / who is the QA lead)

Compare to the existing `scenarios/runtime/joe_chris_francois.json` — three turns, 18 of 18 checks pass, 19 assertions extracted from turn 1 alone.

## What this tells us

The existing scenario passes not because Luna handles a 3-turn conversation well, but because **its first turn is a 19-fact data dump written in the heuristic extractor's exact pattern language.** The patterns the extractor recognises are narrow:

- `"I am ___"` — identity name / age / profession / interest
- `"[Name] lives in ___"` — person:location
- `"[Name] is ___ years old"` — person:age
- `"[Name] is a ___ fan"` — person:interest
- `"[Name] writes/runs/builds ___"` — person:profession
- `"[Name] wants to ___"` — person:goal

Outside that template set, extraction silently produces nothing. There is no "I tried but failed" event — turns just leave no trace in memory.

For a 10-turn natural conversation about a real week:
- "Mira on backend, Daniel on QA" → 0 extracted (compound role assignment, not a known template)
- "Daniel built the QA harness for Beacon" → 0 extracted (action verb, project reference)
- "Daniel will be back on the 17th" → 0 extracted (date, future tense)
- "Mira pushed back on the timeline today" → 0 extracted (verb phrase, no template match)
- "What bug did Mira fix?" → 0 extracted (a question)

This isn't a memory failure. The events were never extracted in the first place.

## Implications

### 1. Stage 7 with the heuristic extractor is not viable

`LUNA_MEMORY_MILESTONE_ROADMAP.md` lists "LLM-backed runtime is usable" as a Stage 7 prerequisite. This finding confirms it concretely: heuristic extraction alone cannot produce assertion coverage on natural conversational input. Without LLM-backed extraction, Stage 7 cannot be tested at all.

### 2. The orb-rebuild work is not the bottleneck

Everything in `memory_schema_v1/` (orb network, vector field, hybrid recall, consolidation engine) sits *on top of* extracted assertions. If extraction produces nothing, none of those features have anything to operate on. Building more memory architecture today does not move the system closer to Stage 7.

The honest reordering:

```
v1.0 critical path                         v2 architecture (later)
────────────────────────                   ────────────────────────
1. Reliable extraction on                  Orb network
   natural prose (LLM and/or               Vector field
   broader heuristic patterns)             Hybrid recall (memory brief)
2. Stage 7 dense-recall test               Consolidation engine
3. Time-decay process for                  Branching / merging
   the 24h gap portion                     Governance attestation
4. Stage 7 with 24h gap
```

The orb work is still good architecture. It is just **not the next thing**.

### 3. The CI gate is calibrated to a fixture that doesn't reflect natural language

`scenarios/runtime/joe_chris_francois.json` passes because it was written to the extractor's pattern language. It is a useful regression test — it locks the patterns the extractor *does* support — but it is not evidence that Luna can handle real conversation. Reading it as evidence of conversational capability has been a quiet overstatement in the project's self-assessment.

### 4. The 24h-gap portion of Stage 7 is currently unmeasurable

Independent of extraction: `forgotten_risk` exists as an episode field with a recall gate, but no runtime path drives it from elapsed time. `EpisodeDecayed` is only emitted manually. A 24h-gap fixture and a 0-second-gap fixture behave identically today. Even if extraction improves, the second half of Stage 7 cannot be measured without a time-decay process.

## What to do next

In strict priority order. Each gates the next.

### Priority 1 — Establish a working extraction path on natural prose

Two acceptable approaches:

- **(A) Wire LLM-backed extraction (`--extractor command`) to a local default and add scenario gates that exercise it.** The `command` extractor already exists; needs a defined backend, a determinism cache (the harness has one), and a CI strategy (probably skip in default CI, run in a dedicated workflow with cached fixtures).
- **(B) Substantially expand the heuristic extractor's pattern coverage** so a representative natural-prose fixture extracts coherent assertion sets. Honest version: this is a long road and probably not worth the investment if (A) is achievable.

(A) is the path the milestone roadmap already names as a Stage 7 prerequisite. It should land before further memory work.

### Priority 2 — Re-run the Stage 7 dense-recall fixtures with a working extractor

When (A) is in place, re-run `scenarios/exploratory/stage7_dense_week.json`. If it passes, graduate it to `scenarios/runtime/`. If it fails, the failures are the next set of findings. Either way, the result is meaningful.

### Priority 3 — Add the time-decay process

`EpisodeDecayed` events emitted from elapsed wall-clock time, with `forgotten_risk` driven through the recall gate. Once this exists, extend the scenario format to express simulated time gaps (per-turn `event_time` or cumulative `gap_seconds` — backward-compatible addition, ~30 lines of harness change). Then build the actual Stage 7 fixture with a 24h gap and run it.

### Priority 4 — Resume the orb-network rebuild

Only after Stages 7 closes. The orb work is correct architecture for v2; it is not v1.

## What this report does not claim

- It does not claim the heuristic extractor is "broken." It is what it is — a narrow, deterministic extractor calibrated to specific templates. That role is fine for a CI lint.
- It does not claim LLM extraction would necessarily pass the Stage 7 fixtures. That is unmeasured here. The path is to wire it and find out.
- It does not claim the orb-rebuild work landed so far (`memory_schema_v1/`, the audit, the event-log-hardening) was wasted. The substrate fixes (R-001, R-002, R-009 gate) tighten the foundation regardless. The schema is correct forward design. They are simply not on the v1.0 critical path.

## Status update for `docs/STATUS.md`

The "What needs to happen next" section should be revised. The current ordering names "Run the existing system against a v1.0 fixture" as the first move, and that's now done — the result is in this document. The updated ordering replaces it with the four priorities above, in order.

That update lands alongside this report.
