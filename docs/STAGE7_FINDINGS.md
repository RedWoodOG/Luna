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

## Update — LLM extraction run (`glm-4.6:cloud`, 2026-05)

After the heuristic-extractor probes failed, the priority shifted to scaffolding the LLM-backed extraction path (see `docs/STAGE7_LLM_SETUP.md`). A user ran `scenarios/exploratory/stage7_dense_week.json` against `glm-4.6:cloud` via Ollama. The result is the architectural answer.

### What ran end-to-end

```text
13/13 turns processed                              no crashes, no parse failures
~21 assertions extracted across the run           real, on natural prose
4/6 must_contain checks pass                      Mira, Daniel, Beacon, session
0/7 must_not_contain violations                   no false positives
working memory bounded 3-5 nodes throughout       budget held
```

This is the first measurement of Luna's existing memory architecture handling natural-prose multi-turn input. **It works.** The pipeline — extraction → events → episodes → replay → recall → working set — is functional across 13 turns of natural conversation about a real week.

### What failed and why

The two failing checks are extraction-vocabulary issues, not memory issues.

**`identity:name=Joe`** — Turn 1 extracted 5 assertions covering Joe's role, his two leads, the project, and the deadline. Joe himself appears in the run's signals (`identity_relevance.evidence: "I'm Joe. Running a small platform team."`) but was not promoted to an assertion. The prompt's "bare name alone is low-value" guidance is correct in most cases but overcorrects for self-introduction. Confirmation: turn 11 (a question, "When is Daniel back?") did produce `person:name=Daniel` — bare-name-with-no-context still triggers extraction; bare-name-with-rich-context gets dominated by the richer fact.

**`17th`** — Turn 6 ("Daniel asked for next Monday off, actually he is out for a week. Back on the 17th") captured the date in signals (`temporal_relevance.evidence: "next Monday, a week, 17th"`) but produced only one assertion (`work:past_event=Daniel asked for next Monday off`). The date itself wasn't routed because the prompt's `domain:kind` allowlist has no slot for "X is back on Y date" / "person:availability" / "schedule:return". GLM correctly observed there was no place for the fact and dropped it.

Both failures fall on the extraction side of the pipeline, before memory ever sees the input. Memory cannot recall what extraction never produced.

### What this confirms

```text
✓ heuristic extractor    fails on natural prose                       (already known)
✓ LLM extraction         produces real assertions on natural prose    (new)
✓ memory architecture    handles 13-turn flow, bounded budget         (new)
✓ recall pipeline        working                                      (new)
✓ orb-network rebuild    confirmed v2, not a v1.0 prerequisite        (key call)
```

The orb-rebuild is correct architecture but not v1.0-blocking. Existing memory works given working extraction.

### What this opens

The v1.0 critical path is now extraction-side, not memory-side. Three options for closing the two failures:

1. **Iterate the prompt template.** Promote self-introduction names ("I am X" / "I'm X" / "this is X") to high-value `identity:name` assertions even when richer context exists. Add an allowlisted `domain:kind` for time-anchored personal facts (e.g. `person:availability`, `schedule:return_date`). This invalidates `prompt_v3_hash` and forces re-extraction across all cached cases — real prompt engineering. Probably the right v1.0 move because it expands what the system can capture.
2. **Loosen the fixture's checks.** Replace `identity:name=Joe` with substring `Joe`; drop `17th`. Honest about today's behavior but does not advance capability. This is the same anti-pattern as the original heuristic fixture (calibrating tests to extractor output).
3. **Both.** Tighten the fixture to what passes today AND iterate the prompt to expand what passes tomorrow.

Default to (1) for the v1.0 path; (2) only as a "minimum viable runtime gate for what already works."

### Promotion path for `stage7_dense_week.json`

When option (1) lands and the fixture passes 6/6 (or whatever the calibrated check set is), graduate from `scenarios/exploratory/` to `scenarios/runtime/` with an explicit note that it requires LLM-backed extraction (not the default heuristic CI path). CI strategy for LLM-backed scenarios is downstream of having one passing fixture.

### What this section does not claim

- Does not claim GLM-4.6 is the canonical extractor. Other models would produce different vocabularies. The result here is a single point on a curve.
- Does not claim Stage 7 is fully passed. The 24h-gap portion (the "→ 24h →" segment of the README's acceptance test) requires a time-decay process that does not exist yet (priority 3 in the post-result list).
- Does not validate any v2 architecture. The orb-network is unbuilt; the schemas in `memory_schema_v1/` remain forward design.

What it claims: **memory architecture survives a 10-turn natural-prose case once extraction works.** That is the call the rebuild order rests on.

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
