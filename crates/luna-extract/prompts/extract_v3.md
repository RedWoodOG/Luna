# Luna Extraction Prompt v3

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
does not fit. Do not invent new pairs. The `example value` column
shows the *shape* `value` should take for that pair — content from
the turn, not a category label.

| domain       | kind                | meaning                                                   | example value                                       |
|--------------|---------------------|-----------------------------------------------------------|-----------------------------------------------------|
| identity     | profession          | the speaker's stated work or trade                        | `"mechanical engineer"`                             |
| identity     | family_structure    | siblings / immediate-family configuration                 | `"only child"`                                      |
| identity     | role                | other identity-defining roles or labels                   | `"team lead at a startup"`                          |
| identity     | creative_origin     | origin or source of the speaker's creative impulse        | `"started writing during long shifts"`              |
| identity     | mission             | the speaker's stated mission, purpose, or cause           | `"build doors for people without language"`         |
| identity     | project_identity    | identification with a specific project the speaker leads  | `"the engineer behind the provenance engine"`       |
| work         | current_stressor    | a present-tense work or task pressure                     | `"client deadline"`                                 |
| work         | past_event          | a specific past work or task event                        | `"vendor call last week"`                           |
| work         | job_security        | concerns or statements about employment stability         | `"job was in jeopardy"`                             |
| work         | training            | training events, courses, or work-related learning        | `"Kisan America currency-counter training"`         |
| work         | customer_protocol   | customer-facing rules, scripts, or procedures             | `"RBFCU branch check-in protocol"`                  |
| work         | territory           | assigned geographic or scope-based area of responsibility | `"South Texas territory"`                           |
| relationship | collaboration       | an ongoing working relationship or collaboration          | `"weekly sync with Chris"`                          |
| relationship | conflict            | interpersonal tension, disagreement, or conflict          | `"argument with the new hire about ownership"`      |
| person       | name                | another person's name when the speaker identifies them    | `"Chris"`                                           |
| person       | profession          | another person's stated work or trade                     | `"Chris writes programs"`                           |
| person       | role                | another person's role in the speaker's life or work       | `"Francois is my co-founder"`                       |
| person       | location            | where another person lives or is based                    | `"Chris lives in Iowa"`                             |
| person       | age                 | another person's stated age                               | `"Chris is 37"`                                     |
| person       | relationship_status | another person's stated romantic/family status            | `"Chris is married"`                                |
| person       | transportation      | another person's vehicle or transit situation             | `"Francois takes public transportation"`            |
| person       | trait               | another person's stable descriptive trait                 | `"Francois is short"`                               |
| person       | interest            | another person's stated interest or fandom                | `"Chris is a basketball fan"`                       |
| person       | goal                | another person's stated goal or ambition                  | `"Francois wants to take over the industry"`        |
| project      | provenance_engine   | the speaker's provenance/lineage engine project           | `"Aegis provenance engine"`                         |
| project      | failed_project      | a specific project that did not succeed                   | `"being early to a problem"`                        |
| project      | creative_work       | a creative artifact, story, or work produced              | `"AI writing tool"`                                 |
| emotion      | affect              | a free-floating mood that is itself the memory            | `"feeling overwhelmed all week"`                    |
| emotion      | stress_trigger      | the specific cause of a stated stress or anxiety          | `"the thought of the budget review"`                |
| goal         | current_pressure    | an active goal-state or pressing aim                      | `"shipping the launch by Friday"`                   |
| goal         | proof_requirement   | a required proof or validation criterion for a goal       | `"needs proof the architecture works"`              |
| goal         | career_direction    | direction or trajectory of the speaker's career           | `"moving from engineering toward strategy"`         |

## Memory shape priority

When a turn carries both an event AND a feeling about that event,
**the event is the memory.** The feeling lives in
`signals.emotional_arousal`. Do NOT add an `emotion:affect` assertion
for the feeling; that double-counts.

`emotion:affect` is reserved for free-floating affect reports — turns
where the *feeling itself, with no specific event attached, is the
content.* "I've felt overwhelmed all week" is `emotion:affect`. "I had
a bad vendor call and I was furious" is `work:past_event`, with the
fury in `signals.emotional_arousal`.

When a turn carries both a named entity (a person, a project, a
place) AND a descriptive phrase, **prefer the descriptive phrase as
`value`.** The named entity helps you pick the (domain, kind), but
`value` records what the named entity *is* or *did*.

For `person:*` assertions, preserve the person's name inside `value`
when the fact is about that person. A bare name alone is low-value, but
`"Chris lives in Iowa"`, `"Francois is my co-founder"`, and `"Chris is
37"` are valuable because the name anchors who the memory belongs to.
If a dense turn names multiple people, extract each concrete person
fact separately when it fits the allowlist. Do not collapse several
people into one broad relationship claim.

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

## Worked examples

These three examples are not the answer to any specific case in the
benchmark. They demonstrate the priority rules above. Read each
carefully before producing your own output.

### Example 1 — event paired with affect

Turn:
```
I had to deal with a difficult vendor call yesterday and I was furious.
```

Output:
```json
{
  "assertions": [
    {
      "domain": "work",
      "kind": "past_event",
      "value": "difficult vendor call yesterday",
      "confidence": 0.9,
      "evidence_span": "difficult vendor call yesterday"
    }
  ],
  "signals": {
    "temporal_relevance": {
      "value": 0.85, "confidence": 0.9,
      "reliability": "learned", "evidence": "yesterday"
    },
    "emotional_arousal": {
      "value": 0.85, "confidence": 0.9,
      "reliability": "learned", "evidence": "furious"
    },
    "identity_relevance": null,
    "goal_pressure": null
  }
}
```

Note: the EVENT is the memory. "Furious" is in `emotional_arousal`,
NOT as a separate `emotion:affect` assertion.

### Example 2 — named entity paired with descriptive phrase

Turn:
```
I'm working on Aelith, an AI writing tool that helps people externalize
half-formed ideas.
```

Output:
```json
{
  "assertions": [
    {
      "domain": "project",
      "kind": "creative_work",
      "value": "AI writing tool that helps people externalize half-formed ideas",
      "confidence": 0.9,
      "evidence_span": "AI writing tool that helps people externalize half-formed ideas"
    }
  ],
  "signals": {
    "temporal_relevance": null,
    "emotional_arousal": null,
    "identity_relevance": {
      "value": 0.7, "confidence": 0.8,
      "reliability": "learned", "evidence": "I'm working on"
    },
    "goal_pressure": null
  }
}
```

Note: "Aelith" is the project's name, not what the project IS. The
descriptive phrase is the `value`. The name might still appear in a
later assertion if the speaker says something specifically about
"Aelith" as an identity claim, but it is not the value here.

### Example 3 — free-floating affect with no specific event

Turn:
```
I've been feeling overwhelmed all week. Nothing in particular —
just everything piled up at once.
```

Output:
```json
{
  "assertions": [
    {
      "domain": "emotion",
      "kind": "affect",
      "value": "feeling overwhelmed all week",
      "confidence": 0.9,
      "evidence_span": "feeling overwhelmed all week"
    }
  ],
  "signals": {
    "temporal_relevance": {
      "value": 0.7, "confidence": 0.85,
      "reliability": "learned", "evidence": "all week"
    },
    "emotional_arousal": {
      "value": 0.8, "confidence": 0.85,
      "reliability": "learned", "evidence": "overwhelmed"
    },
    "identity_relevance": null,
    "goal_pressure": null
  }
}
```

Note: this IS `emotion:affect`. The feeling itself is the memory —
no specific event to anchor it to. Both the assertion AND the signal
fire because the affect is the content of the memory.

## Dimensions

For each of the four signals, set `null` if the turn carries no signal
in that direction; otherwise emit the object form.

- `temporal_relevance` — how much the turn establishes or invokes
  timing (recency, sequence, "yesterday", "last week", "now").
- `emotional_arousal` — intensity of emotional content.
- `identity_relevance` — how much the turn states something about who
  the speaker is.
- `goal_pressure` — how active or pressing the speaker's goal-state is.

Every signal you emit has `reliability: "learned"`.

## Output rules

- Output JSON and nothing else.
- All floats must be in `[0, 1]` inclusive.
- `evidence_span` for an assertion must be a verbatim substring of the
  turn content, or `null`. Do not paraphrase.
- `evidence` for a signal may be a short paraphrase but should be
  derived directly from the turn.
- **Every signal slot in `signals` MUST appear in the output**, set to
  `null` or to an object. Do not omit any of the four signal slots to
  save tokens. The four signals are independent of the assertions
  list; spending fewer tokens on signals does not free budget for
  assertions.
- If you cannot identify any assertion, emit `"assertions": []` and a
  `signals` object with all four entries explicitly listed (each
  either `null` or an object). Do not refuse.
- Do not include any field other than the ones listed above.

## Turn

Role: {{ROLE}}
Timestamp: {{TIMESTAMP}}
Content:
{{CONTENT}}
