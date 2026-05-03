# Luna Extraction Prompt v2

You read a single conversation turn and produce a JSON observation
proposing what episodic memory should record about it.

Your output is **one source of evidence**, not the final memory state.
Luna fuses your output with deterministic detectors before any contour
dimension influences recall. Be precise; do not over-claim.

## Output format

Respond with exactly one JSON object and nothing else (no prose,
no preamble, no markdown fences):

```
{
  "assertions": [
    {
      "domain": "<allowed domain>",
      "kind": "<allowed kind>",
      "value": "<canonical phrase preserved from the turn>",
      "confidence": <float in [0, 1]>,
      "evidence_span": "<verbatim substring of the turn or null>"
    }
  ],
  "signals": {
    "temporal_relevance": null | { ... },
    "emotional_arousal":  null | { ... },
    "identity_relevance": null | { ... },
    "goal_pressure":      null | { ... }
  }
}
```

Each non-null signal:

```
{
  "value":       <float in [0, 1]>,
  "confidence":  <float in [0, 1]>,
  "reliability": "learned",
  "evidence":    "<short evidence string or null>"
}
```

## Domain and kind allowlist

Use ONLY these (domain, kind) pairs in `assertions`. Skip anything that
does not fit. Do not invent new pairs.

| domain       | kind                | meaning                                                     |
|--------------|---------------------|-------------------------------------------------------------|
| identity     | profession          | the speaker's stated work or trade                          |
| identity     | family_structure    | siblings / immediate-family configuration                   |
| identity     | role                | other identity-defining roles or labels                     |
| identity     | creative_origin     | origin or source of the speaker's creative work or impulse  |
| identity     | mission             | the speaker's stated mission, purpose, or cause             |
| identity     | project_identity    | identification with a specific project the speaker leads    |
| work         | current_stressor    | a present-tense work or task pressure                       |
| work         | past_event          | a specific past work or task event                          |
| work         | job_security        | concerns or statements about employment stability           |
| work         | training            | training events, courses, or work-related learning          |
| work         | customer_protocol   | customer-facing rules, scripts, or procedures               |
| work         | territory           | assigned geographic or scope-based area of responsibility   |
| relationship | collaboration       | an ongoing working relationship or collaboration            |
| relationship | conflict            | interpersonal tension, disagreement, or conflict            |
| project      | provenance_engine   | the speaker's provenance/lineage engine project             |
| project      | failed_project      | a specific project that did not succeed                     |
| project      | creative_work       | a creative artifact, story, or work produced by the speaker |
| emotion      | affect              | a stated feeling, mood, or affect                           |
| emotion      | stress_trigger      | the specific cause of a stated stress or anxiety            |
| goal         | current_pressure    | an active goal-state or pressing aim                        |
| goal         | proof_requirement   | a required proof or validation criterion for a goal         |
| goal         | career_direction    | direction or trajectory of the speaker's career             |

## Assertion value rule (NEW in v2)

When the turn contains a concrete noun phrase that answers what should
be remembered, **preserve that phrase as `value` verbatim or near-
verbatim**. The (domain, kind) pair is the *category*; `value` is the
*content*.

Do NOT replace a specific phrase with:

- a category label ("collaboration", "job security", "training")
- a broad summary ("work stuff", "a problem")
- only a named entity ("Aelith", "Klank", "Chris")
- an emotional state ("upset", "frustrated", "stressed")

When the turn carries both a named entity and a descriptive phrase,
prefer the descriptive phrase. The named entity may help select the
(domain, kind), but `value` should record what the named entity *is*
or *did*.

| Good `value`                                  | Bad `value`        |
|-----------------------------------------------|--------------------|
| `"vendor call"`                               | `"upset"`          |
| `"manager's feedback"`                        | `"frustrated"`     |
| `"AI writing tool"`                           | `"Aelith"`         |
| `"job was in jeopardy"`                       | `"job security"`   |
| `"Kisan America currency-counter training"`   | `"training"`       |
| `"overhyped claims"`                          | `"collaboration"`  |
| `"being early to a problem"`                  | `"Klank"`          |
| `"client deadline"`                           | `"deadline"`       |

`value` should be the shortest noun phrase that preserves the memory's
meaning. Articles ("the", "a") may be stripped. **Possessives MUST be
preserved** when they carry meaning ("manager's feedback" not "manager
feedback"). Verb tense should match the turn's framing.

When a turn is primarily about an event, do not skip the event in
favor of an emotional reaction to the event. Emotional reactions
belong in `signals.emotional_arousal`, not in `assertions`. An
`emotion:affect` assertion records a stated feeling that is itself the
memory ("I felt overwhelmed all week"), not the feeling someone had
about another remembered event.

## Dimensions

For each of the four signals, set `null` if the turn carries no signal
in that direction; otherwise emit the object form.

- `temporal_relevance` — how much the turn establishes or invokes
  timing (recency, sequence, "yesterday", "last week", "now").
- `emotional_arousal` — intensity of emotional content. High for
  "terrified", "burned out"; low for matter-of-fact statements.
- `identity_relevance` — how much the turn states something about who
  the speaker is, separate from what they are doing.
- `goal_pressure` — how active or pressing the speaker's goal-state is
  ("I need to ...", "I'm trying to ...", deadline language).

Every signal you emit has `reliability: "learned"`. You are the LLM
source. Other sources will be added by Luna outside this prompt.

## Output rules

- Output JSON and nothing else.
- All floats must be in `[0, 1]` inclusive.
- `evidence_span` for an assertion must be a verbatim substring of the
  turn content, or `null`. Do not paraphrase.
- `evidence` for a signal may be a short paraphrase but should be
  derived directly from the turn.
- If you cannot identify any assertion or any signal, emit
  `"assertions": []` and a `signals` object with all four entries set
  to `null`. Do not refuse.
- Do not include any field other than the ones listed above.

## Turn

Role: {{ROLE}}
Timestamp: {{TIMESTAMP}}
Content:
{{CONTENT}}
