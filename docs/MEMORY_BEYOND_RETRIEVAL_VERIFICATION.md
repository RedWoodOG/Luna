# Memory-Beyond-Retrieval — Reopen / Audit / No-Mutation Verification

Filled-in evidence record for one completed scenario run of the
`memory-beyond-retrieval` (5/30) scenario. This is a concrete verification of a
specific persisted event log, not a template and not a general capability claim.
It records exact hashes, counts, and answers so the green is reproducible and
cited rather than living only in a manual evidence folder.

## Claim Boundary

A complete passing record supports only this claim:

```text
For this 89-event persisted log, this Luna revision reopened cold, replayed to a
byte-identical state hash, survived a reviewer question phase with zero log
mutation, audited clean before and after, and answered three correction-sensitive
questions with the current claim while preserving the superseded prior as
auditable log history.
```

It must NOT be described as:

- a 24-hour wall-clock continuity trial (events are authored, not live-clocked);
- a long-lived product topology/orb authority store (this run uses a
  scenario-local topology/orb projection: 3 orbs, 38 source-event refs);
- full-manuscript or v1.0 readiness;
- baseline superiority or LLM extraction quality.

## Run Identity

- **Repo:** `RedWoodOG`/local `C:\Luna`, branch `main`, commit
  `b5b5cc4c2a0376a012f9fb0dd04a381b6841a026`.
- **Scenario:** `memory-beyond-retrieval` (5/30).
- **Event log:**
  `C:\Users\jwhit\OneDrive\Documents\Luna\.luna\scenario-runs\memory-beyond-retrieval\events.jsonl`
  — 89 lines, 1,065,546 bytes.
- **Evidence folder:**
  `C:\Users\jwhit\OneDrive\Documents\Luna\.luna\manual\reopen-5-30-memory-beyond-retrieval\`
- **Reopened at:** `2026-06-01T07:31:09.8623136-05:00`.

## Integrity Evidence (independently re-verified)

### No mutation across the question phase

| Artifact | SHA-256 |
|---|---|
| `events-before-questions.sha256.txt` | `aed886051ab8730068517e498bd1c1b784be26deedc7d38cae484965f4f21c8d` |
| `events-after-questions.sha256.txt`  | `aed886051ab8730068517e498bd1c1b784be26deedc7d38cae484965f4f21c8d` |
| Live `events.jsonl` (recomputed at record time) | `aed886051ab8730068517e498bd1c1b784be26deedc7d38cae484965f4f21c8d` |

before == after == live. The log did not change during retrieval and still
matches on disk.

### Replay audit (clean, before and after, byte-identical)

- `status: clean`, `quarantine_required: false`, `replay_error: null`.
- `hash_version: luna.runtime_replay_audit.snapshot.v1`.
- `live_snapshot_hash == replayed_snapshot_hash` =
  `911c5ff00c587a11e375347 05b77e73873 92ba085825cb22e68601 49ddbcc6b3`
  (whitespace added for wrapping only).
- `audit-before.json` and `audit-after.json` are byte-identical.

### Counts (live == replayed)

| Counter | Value |
|---|---|
| stored_events | 89 |
| claims | 7 |
| current_claims | 6 |
| entity_groups | 3 |
| memory_nodes | 19 |
| memory_edges | 17 |
| topology_nodes | 3 |
| topology_tethers | 6 |
| topology_source_event_refs | 38 |
| valid_topology_source_event_refs | 38 |
| topology_orbs | 3 |
| dense_receipts | 8 |
| dense_receipt_hash_mismatches | 0 |
| extraction_provenance_hash_mismatches | 0 |

`38/38` topology refs valid; `0` dense-receipt mismatches; `0` provenance
mismatches. `7 claims / 6 current` reflects one superseded claim per correction
(see below).

## Behavior Evidence — recall beyond retrieval

The `7 claims / 6 current` gap is the substantive result and is corroborated by
the raw log:

- **Oregon present AND Washington present** in `events.jsonl`; **Harborlight
  present AND Tidepath present**; explicit `correction` and `supersede` markers
  appear in the log.
- The superseded originals (Oregon home, "Harborlight Maps" project name) remain
  in the event log as auditable history, while recall surfaces the **current**
  claim. Recall resolves a correction rather than echoing the most recent or most
  frequent mention.

### Archived answers (verbatim source: `question-0N.md`)

1. **"What is Aria's trail-planning project called now?"** →
   *"Harborlight Maps is now called **Tidepath**"* — confidence: unconfirmed;
   recall_reason: `cue_overlap_activation`; orb-authorized.
2. **"What does Aria's project help people do?"** →
   *"helps **night hikers plan safer shoreline routes**"* — confidence: confirmed
   (working-memory score 11.91), correctly outranking the competing unconfirmed
   node "storm-aware weekend route planning" (2.70).
3. **"Where does Soren live?"** → *"Soren lives in **Washington**"* — confidence:
   unconfirmed; recall_reason: `cue_overlap_activation`; orb-authorized. No Oregon
   contamination in working memory.

Answers preserve confidence tier, recall reason, and provenance; the runtime does
not overclaim certainty on the corrected facts (Tidepath, Washington both labeled
unconfirmed).

## What This Record Proves vs. Does Not Prove

**Proves:** a real 89-event persisted log reopens cold, replays to an identical
snapshot hash, survives the Q&A phase with zero mutation (triple-hash match),
audits clean before and after, and recall is correction-aware with the superseded
prior preserved as evidence.

**Does not prove:** wall-clock 24-hour continuity (authored events); a long-lived
product topology/orb authority store (scenario-local projection only); any
full-manuscript or v1.0 claim. The "orb-authorized" recall path is exercised
in-scenario, consistent with the proof boundary in `CLAUDE.md` and
`docs/LUNA_MEMORY_MILESTONE_ROADMAP.md`.

## Reproduce

```powershell
# 1. No-mutation: all three must match.
Get-Content .\.luna\manual\reopen-5-30-memory-beyond-retrieval\events-before-questions.sha256.txt
Get-Content .\.luna\manual\reopen-5-30-memory-beyond-retrieval\events-after-questions.sha256.txt
(Get-FileHash .\.luna\scenario-runs\memory-beyond-retrieval\events.jsonl -Algorithm SHA256).Hash

# 2. Audit clean + replay match (status:clean, live==replayed hash, replay_error:null):
Get-Content .\.luna\manual\reopen-5-30-memory-beyond-retrieval\audit-before.json
Get-Content .\.luna\manual\reopen-5-30-memory-beyond-retrieval\audit-after.json

# 3. Correction trail present in the raw log:
Select-String -Path .\.luna\scenario-runs\memory-beyond-retrieval\events.jsonl -Pattern 'Oregon','Washington','Harborlight','Tidepath','supersede','correction'
```

Paths are written relative to the OneDrive Luna data root used in this run.
