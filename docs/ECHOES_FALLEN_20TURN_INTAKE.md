# Echoes Fallen 20-Turn Test — With Heuristic Intake (flag ON)

Measurement of the same locked 20-turn Echoes Fallen benchmark
([`ECHOES_FALLEN_20TURN_BASELINE.md`](ECHOES_FALLEN_20TURN_BASELINE.md)) with the
imported heuristic intake layer enabled (`LUNA_INTAKE_HEURISTICS=1`, commit
`8bd8878`). The benchmark itself is unchanged.

## What the intake layer is

- **Disclosure extractor** — ported from AURA `aura-input/assertions.rs`,
  rewritten to Luna conventions (`disclosure_extract.rs`). First-person only.
- **Narrative extractor** — Luna-original third-person prose extractor
  (`narrative_extract.rs`): `<Proper Subject> is/are/was <predicate>` facts and
  `<Name> also known as / called <Alias>` aliases → `manuscript:character_*`
  claims, case preserved, flowing through Luna's existing character-entity
  derivation and entity-group recall.
- Both compose into `entity_sieve_assertions` only when the flag is set. Default
  OFF: 111 luna-runtime tests + gate clippy stay green unchanged.

## Stored memory after 20 turns (audit, replay-clean)

| Metric | Baseline (OFF) | Intake (ON) |
|---|---|---|
| claims | 0 | **7** |
| entity_groups | 0 | **6** |
| topology_nodes | 0 | **6** |
| topology_orbs | 0 | **6** |
| topology_source_event_refs | 0 | **44** |
| memory_nodes / edges | 9 / 7 | 23 / 21 |

Entities surfaced: **Jax, T'Sari, Primarch Viserys, Ren, The Crimson Fold, The
Inherence**. Stored facts include the alias `Jax → Jackson Renn`, `Primarch
Viserys is T'Sari's father`, `The Crimson Fold is forbidden space`, `The
Inherence is an Accord vessel that recovers a containment pod`.

## Question results: 0/25 → 10/25 activated

| | Baseline | Intake |
|---|---|---|
| Questions activating memory | 0 / 25 | **10 / 25** |
| Genuinely correct answers | 0 | ~3–4 |

- **Correct:** q01 (the Inherence is the recovering vessel), q07 (Viserys is
  T'Sari's father); alias and entities stored and inspectable.
- **Activated but imprecise:** several questions (q03 species, q11/q13 tether/why)
  surfaced a *related* entity fact rather than the specific answer — entity-group
  recall matched on entity terms loosely.

## Honest gaps this run exposed (next work)

1. **Pronoun coreference.** "He is human" (after "…is Jax") was skipped — the
   species fact never stored. Resolving `He/She/They` to the last proper subject
   would capture it, but must stay high-precision to avoid misattribution
   (the rubric counts inventing/confusing as failure).
2. **Common-noun concept subjects.** "A tether is load-sharing…" has a lowercase
   subject, so concept definitions (q11/q12) are not captured. Needs a concept
   path distinct from proper-noun entities.
3. **Recall precision.** Entity-group recall returns any current fact for a
   matched entity, not the attribute the question asked for. This is a recall-
   layer refinement, separate from extraction.

## Boundary

This is a measurement of an opt-in extraction layer, not a passed trial. It is
not a one-read manuscript proof and not a recall-quality claim. The win is
specific and honest: third-person prose now produces real, auditable,
provenance-backed structured memory (0 → 7 claims, 0 → 6 entities) where the
baseline produced nothing — with the next gaps named, not hidden.
