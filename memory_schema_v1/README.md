# memory_schema_v1

The receipt format for memory changing itself.

## The rule

> Every transformation is itself a logged event.

- No memory claim exists without event lineage.
- No orb mutation happens outside the event log.
- No core is rewritten silently.

## Operating principles

```
compression for cognition
provenance for trust
branching for resilience
merging for coherence
```

## What is in this directory

| File | Status | Role |
|------|--------|------|
| `consolidation_event.schema.json` | detailed v1 | The keystone. The auditable receipt emitted whenever an orb's state changes through consolidation, branching, or merging. |
| `memory_event.schema.json` | skeleton | Base envelope for any event appended to the memory log. Detailed in `pr-1.0`. |
| `orb.schema.json` | skeleton | The MemoryOrb (core + halo + tethers + lineage). Detailed in `pr-1.0`. |
| `memory_brief.schema.json` | skeleton | The synthesized recall artifact a model receives instead of raw chunks. Detailed in `pr-1.4`. |
| `fixtures/consolidation_event.example.json` | minimal | A valid example for the `consolidate` operation. Branch and merge fixtures land alongside their phases. |

The detailed schemas land iteratively. `consolidation_event` lands first because every other operation in the system either produces or replays one.

## Guarantees the schema enforces

- Every transformation references the input events and orbs it derived from (`input_events`, `input_orbs`).
- Every change to a core is a versioned, replayable delta (`delta`, `previous_versions`).
- No claim is added or retired without evidence event ids (`claims_added[].evidence`, `claims_retired[].reason`).
- Halo events that "decay" are explicitly listed in `halo_events_dropped` — they are never silently forgotten. The underlying events remain in the event log.
- Privileged orbs require an `attestation` to compress (enforced at runtime; schema makes the field first-class).
- Every consolidation produces a `replay_bundle_hash` so the transformation can be verified and, if needed, rolled back.

## Architecture context

```
event log -> vector field -> orb network -> bridge / ui
              ^                  |
              |                  v
        consolidation engine / governance layer
```

The event log is the source of truth. The vector field is an index, rebuildable from the log. The orb network is the cognition layer, derived from the log via consolidation events. The bridge surfaces operational state to operators.

A `ConsolidationEvent` is the join between cognition and governance: it is the auditable record that says "memory changed itself; here is what changed; here is the lineage; here is who attested."

## Versioning

This is `memory_schema_v1`. Breaking changes ship as `memory_schema_v2` with a migration document. Existing consolidation events are immutable; they remain valid under v1 even after v2 ships.

## Validation

Quick parse and conformance check:

```bash
python3 -m json.tool memory_schema_v1/consolidation_event.schema.json > /dev/null
python3 -m json.tool memory_schema_v1/fixtures/consolidation_event.example.json > /dev/null
# optional, requires jsonschema:
python3 -c "import json, jsonschema; \
  s=json.load(open('memory_schema_v1/consolidation_event.schema.json')); \
  d=json.load(open('memory_schema_v1/fixtures/consolidation_event.example.json')); \
  jsonschema.Draft202012Validator(s).validate(d); print('ok')"
```

## What happens next

This schema is the contract. The next PR (`pr-1.0`) introduces the `luna-orbs` crate with types only, referencing this schema. The crate after that (`luna-consolidate`, `pr-1.6`) implements the engine that emits these events. Until both ship, no Luna code may rewrite an orb core in any way that does not produce a `ConsolidationEvent` matching this schema.
