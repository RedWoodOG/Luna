# Luna Supersession Plan
## How Luna Absorbs and Surpasses Aura TCF Genesis

### Governing Law

Luna's existing invariants are non-negotiable. Every new capability must:

1. **Rebuild from the event log.** No derived state that cannot be replayed.
2. **Carry provenance.** Every surfaced fact traces to a source event.
3. **Pass replay audit.** Same log → same state, provably.
4. **Be mechanically gated.** Scenario, lint, type gate, or all three.

This means: Aura's geometric field cannot be imported as-is. It must be rebuilt as a **derived layer over the typed graph**, with every field position traceable to a `MemoryProvenance`.

---

## Phase A: The Cognitive Field Layer (new)

**Goal:** A 7D cognitive field that is computed deterministically from typed assertions — not hashed from raw text.

### A1 — Typed Field Dimensions

Define seven field dimensions, each anchored to a Luna domain:

| Dimension | Luna domain | Meaning |
|---|---|---|
| `identity` | `identity:*` | Self/other reference strength |
| `meaning` | `person:*`, `project:*`, `manuscript:*` | Semantic weight of stored claims |
| `goal` | `person:goal`, `project:deadline` | Directional intent |
| `trust` | `relationship:*` | Relational confidence |
| `attention` | working-memory activation score | What is salient right now |
| `context` | episode recency, turn position | Situational grounding |
| `skill` | `identity:profession`, `person:role` | Domain competence signal |

Each dimension is a `f32` computed from the current `MemoryState`. No hashing. No embedding. Every value is derived from inspectable typed claims.

**Proof:**
- Scenario: `cognitive_field_from_graph.json` — seed claims, verify field values, verify field values change after correction
- Replay audit: same graph → same field vector

### A2 — Field Vector as Provenance Carrying

```rust
pub struct CognitiveField {
    pub identity: f32,
    pub meaning: f32,
    pub goal: f32,
    pub trust: f32,
    pub attention: f32,
    pub context: f32,
    pub skill: f32,
    /// Every dimension's contributing claims, for inspect.
    pub provenance: FieldProvenance,
}

pub struct FieldProvenance {
    pub identity_sources: Vec<MemoryProvenance>,
    pub meaning_sources: Vec<MemoryProvenance>,
    // ... one per dimension
}
```

Inspect must answer: "Why is trust at 0.82?" by naming the relationship claims that contributed.

**Proof:**
- Scenario: `field_provenance_inspect.json` — verify every field dimension can be traced to source claims
- `luna runtime inspect --field` shows field + source claims

### A3 — Field Override in Episodes

When an episode carries a `StructuredAssertion`, the claim's contribution to the field is weighted by:
- Confidence tier (confirmed > unconfirmed)
- Lifecycle status (current > superseded > stale)
- Correction salience (recent corrections boost attention)

This replaces Aura's FNV-1a hash projection entirely. No random hashing. Every field value has a reason.

**Proof:**
- Scenario: `field_confidence_weighting.json` — confirmed claims contribute more than unconfirmed; superseded claims contribute less than current
- Unit test: `field_vector_from_state()` returns deterministic values

---

## Phase B: Relationship System (new)

**Goal:** Typed relationships between entities, built as graph edges with lifecycle and provenance.

### B1 — Relationship as First-Class Graph Edge

Aura models relationships as a geometric field. Luna models them as typed edges:

```rust
pub struct RelationshipEdge {
    pub source_entity: String,   // "self"
    pub target_entity: String,   // "Tom"
    pub kind: RelationshipKind,  // Friend, Colleague, Family
    pub trust: f32,              // 0.0–1.0
    pub intimacy: f32,           // 0.0–1.0
    pub recency: DateTime<Utc>,
    pub provenance: Vec<MemoryProvenance>,
    pub lifecycle_status: AssertionLifecycleStatus,
}
```

Relationships evolve through assertions (`"Tom is my colleague"`, `"Tom is a close friend"`). Corrections create supersession. Trust and intimacy are computed from the assertion history — not hashed, not embedded.

**Proof:**
- Scenario: `relationship_evolution.json` — disclose colleague → disclose friendship → verify trust/intimacy increase → verify old colleague status superseded
- Replay audit: same assertions → same relationship state

### B2 — Relationship-Biased Recall

When querying for facts about an entity, the relationship edge strength biases activation:

```
activation_score = base_match * (1.0 + trust * 0.3 + intimacy * 0.2)
```

A fact about a close friend activates more strongly than a fact about a stranger. This is the same effect Aura achieves with relationship field geometry, but implemented as a deterministic score function over typed edges.

**Proof:**
- Scenario: `relationship_biased_recall.json` — store facts about acquaintance Tom and close friend Maria → query "who works on what?" → Maria's facts surface before Tom's
- Activation report shows relationship contribution

### B3 — Relationship Decay

Relationships weaken without interaction. This is Aura's "forgotten_risk" done correctly:

```
decay_factor = 1.0 - (elapsed_days / 90.0).clamp(0.0, 1.0)
effective_trust = trust * decay_factor
```

Decay is inspectable. The decay curve is a parameter, not hidden state.

**Proof:**
- Scenario: `relationship_decay.json` — establish relationship → simulate 30 day gap → verify trust has decayed → verify inspect shows decay reason

---

## Phase C: Cognitive Merge Engine (improved)

**Goal:** Episode consolidation that is reversible, provenance-preserving, and replay-auditable. Fixes Aura's silent merge problem.

### C1 — Merge Receipts with Source Ancestry

Aura's `ShapeMergeEngine` merges episodes silently with no rollback. Luna's version:

```rust
pub struct MergeReceipt {
    pub merged_episode_ids: Vec<Uuid>,
    pub result_episode_id: Uuid,
    pub similarity_score: f32,
    pub merge_policy: MergePolicy,
    pub source_event_hashes: Vec<String>,  // events that contributed
    pub created_at: DateTime<Utc>,
    pub accepted: bool,
}
```

**Accepted** merges produce a receipt linked to source events. **Rejected** merges log why. Merges can be undone by removing the receipt and re-deriving from raw events.

**Proof:**
- Scenario: `merge_receipt_replay.json` — run 5 similar episodes → trigger merge → verify merge receipt has source hashes → replay from raw log → verify same merge receipt produced
- Scenario: `merge_receipt_rejection.json` — attempt merge below threshold → verify rejection logged

### C2 — Bounded Vault with Eviction Receipts

Luna inherits Aura's 128-episode ceiling but adds provenance:

```rust
pub struct EvictionReceipt {
    pub evicted_episode_id: Uuid,
    pub reason: EvictionReason,  // WeakScore, Tombstone, ManualPurge
    pub score_at_eviction: f32,
    pub evicted_at: DateTime<Utc>,
}
```

Eviction receipts are stored in the event log. A replay audit verifies that the vault never exceeded the ceiling and that evictions were justified by score.

**Proof:**
- Scenario: `vault_ceiling_eviction.json` — fill vault to 129 episodes → verify eviction receipt logged → verify vault at 128 → replay audit verifies ceiling enforced

### C3 — Corrected Decay Function

Aura's `forgotten_risk_for_memory()` bug: `created_at.saturating_sub(0)` always returns zero. Luna's version:

```
age_seconds = now.saturating_sub(episode.created_at)
decay = (age_seconds / MAX_AGE_SECONDS).clamp(0.0, 1.0)
forgotten_risk = decay * (1.0 - episode.recall_count as f32 / 10.0).clamp(0.0, 1.0)
```

Frequently recalled episodes resist decay. Never-recalled episodes decay faster. This is inspectable and parameterized — not hidden, not buggy.

**Proof:**
- Unit test: `decay_increases_with_age()` — older episodes have higher forgotten_risk
- Unit test: `decay_decreases_with_recall()` — frequently recalled episodes resist decay

---

## Phase D: Full Render Boundary (extended)

**Goal:** Extend Luna's `ResponsePlan` into a full render contract with integrity checks. This is Aura's `RendererCapsule` but provenance-traced.

### D1 — Render Packet with Source Binding

```rust
pub struct RenderPacket {
    pub response_plan: ResponsePlan,
    pub cognitive_field: CognitiveField,
    pub selected_anchors: Vec<MemoryAnchor>,
    pub relationship_context: Vec<RelationshipEdge>,
    pub integrity_manifest: IntegrityManifest,
}

pub struct IntegrityManifest {
    pub source_event_count: usize,
    pub source_event_hashes: Vec<String>,
    pub field_provenance_hash: String,
    pub replay_snapshot_hash: String,
}
```

The renderer (LLM or Voice Cortex) receives a packet with everything it needs to produce natural language — and an integrity manifest proving the packet was built from real events.

**Proof:**
- Scenario: `render_packet_integrity.json` — produce render packet → verify integrity manifest hashes match replay snapshot → tamper with packet → verify integrity check fails

### D2 — Render Violation Detection

Post-render, check that the LLM's output respects the packet's constraints:

- Did the renderer introduce facts not in the packet? → violation
- Did the renderer contradict a current claim? → violation
- Did the renderer surface a superseded claim? → violation
- Did the renderer add identity claims (e.g. "as an AI...")? → violation

Violations are scored and logged. Repeated violations escalate to the safety system.

**Proof:**
- Scenario: `render_violation_detection.json` — inject a render output that adds unsupported claims → verify violation logged → verify output marked as unreliable

---

## Phase E: Safety System (new)

**Goal:** A safety posture that gates what memory can surface and what plans can execute. Luna's version is mechanically verified — every safety decision has a provenance trail.

### E1 — Safety Posture from Graph State

Safety posture is not a hardcoded state machine. It is computed from the current memory state:

```
safety_posture = evaluate(
    recent_correction_salience,
    contradiction_count,
    unknown_preservation_rate,
    relationship_trust_min,
    render_violation_count
)
```

Postures: `normal`, `elevated`, `restricted`.

Each posture gates:
- What claims can enter working memory (restricted: only confirmed, current)
- What response actions are available (restricted: no speculation)
- What the renderer is allowed to add (restricted: no paraphrasing)

**Proof:**
- Scenario: `safety_posture_escalation.json` — accumulate contradictions → posture elevates → verify restricted output → resolve contradictions → posture returns to normal
- Replay audit: same events → same safety posture at each turn

### E2 — Safety Decision Provenance

Every safety decision is a `StructuredAssertion` in the event log:

```
domain: safety
kind: posture_change
value: "elevated: 3 contradictions in 5 turns"
```

The inspect command shows the full chain: which contradictions caused the escalation, which corrections resolved it.

**Proof:**
- Scenario: `safety_provenance_audit.json` — trigger safety escalation → inspect shows source events → replay produces same chain

---

## Phase F: Observer & Diagnostic Layer (new)

**Goal:** Aura's observer council, rebuilt as a diagnostic layer over Luna's inspect surface. No hidden observers — every observation is a query over the typed graph.

### F1 — Diagnostic Queries as Typed Assertions

Observers are not background threads. They are named diagnostic queries that run at inspect time:

```
luna runtime inspect --diagnostic contradiction-density
luna runtime inspect --diagnostic correction-salience
luna runtime inspect --diagnostic relationship-health
luna runtime inspect --diagnostic vault-pressure
```

Each diagnostic produces a structured report with source claims. This replaces Aura's `ObserversCouncil` with a deterministic, on-demand system.

**Proof:**
- Scenario: `diagnostic_contradiction_density.json` — seed contradictory claims → run diagnostic → verify report shows density, source claims, recommended action
- Golden snapshot: diagnostic output matches expected for known scenarios

### F2 — Between-Turn Reflection

Between turns, Luna can run reflection diagnostics and write findings as system events:

```
domain: system
kind: reflection
value: "trust_with_Tom_decaying: last_interaction_12_days_ago"
```

These reflection events enter the event log with system provenance. They influence the next turn's activation but never override user-sourced evidence.

**Proof:**
- Scenario: `between_turn_reflection.json` — seed memory → trigger reflection → verify reflection event in log → verify next turn activation reflects finding

---

## Phase G: Character & Voice System (new)

**Goal:** Character profiles and voice personality as typed graph entities with lifecycle. This is Aura's `aura-character` system, made inspectable.

### G1 — Character as Entity Group

A character is an entity group with typed claims:

```
domain: character
kind: definition
value: "Aura is warm, intelligent, funny, self-aware"
lifecycle_status: current
provenance: system_root:character_v1
```

Character state is inspectable. Character changes create correction events. Character profiles can be scoped (professional mode, casual mode).

**Proof:**
- Scenario: `character_definition_lifecycle.json` — define character → change tone → verify old definition superseded → verify new definition current

### G2 — Voice Personality Parameters

Voice personality is a set of typed constraints on the render packet:

```rust
pub struct VoicePersonality {
    pub warmth: f32,        // 0.0–1.0
    pub formality: f32,     // 0.0–1.0
    pub humor: f32,         // 0.0–1.0
    pub verbosity: f32,     // 0.0–1.0
    pub self_disclosure: f32,
}
```

These are not prompt hacks. They are parameters passed to the render packet that constrain the trajectory planner in the Voice Cortex.

**Proof:**
- Unit test: `voice_parameters_constrain_render_packet()` — different warmth values produce different render packet constraints
- Scenario: `voice_personality_switch.json` — switch from professional to casual → verify render packet parameters change

---

## Phase H: Full Integration — The Supersession Runtime

**Goal:** The complete Luna runtime that does everything Aura does, but better.

### H1 — Unified Turn Pipeline

```
1. input_normalization     — normalize text, detect language
2. intake_policy           — decide: store, ignore, ask for clarification
3. entity_sieve            — extract typed assertions from text
4. correction_detection    — detect "actually/correction" cues, find targets
5. lifecycle_update        — supersede old claims, create correction events
6. relationship_update     — update relationship edges from assertions
7. cognitive_field_compute — compute 7D field from current state
8. safety_evaluate         — compute safety posture
9. recall_resonance        — geometric proximity + relationship bias + lifecycle filter
10. activation             — select bounded working memory
11. compression_gate       — merge similar episodes if vault pressure high
12. response_plan          — decide: answer, refuse, ask, suppress
13. render_packet_assemble — build RenderPacket with integrity manifest
14. render_integrity_check — validate render output against packet
15. commit                 — write event to log, produce receipts
```

Every step is inspectable. Every step produces provenance. Every step is replay-auditable.

### H2 — Luna Is Complete When

```
Given only an append-only event log, Luna:
1. Rebuilds typed memory with lifecycle
2. Computes a 7D cognitive field traced to source claims
3. Models relationships as typed edges with decay
4. Merges similar episodes with reversible receipts
5. Evaluates safety posture from graph state
6. Activates a bounded working set with relationship-biased recall
7. Produces a render packet with integrity manifest
8. Answers from evidence with recall reasons
9. Suppresses stale, superseded, and unsafe claims
10. Reproduces the same result under replay audit
11. Passes every scenario, benchmark, and doctrine gate mechanically
```

---

## Execution Order

Proceed in this exact order. Each phase builds on the previous.

1. **Phase A1–A3:** Cognitive field layer over typed graph
2. **Phase B1–B3:** Relationship system as typed edges
3. **Phase C1–C3:** Merge engine with receipts (fixes Aura's bugs)
4. **Phase D1–D2:** Render boundary with integrity checks
5. **Phase E1–E2:** Safety system from graph state
6. **Phase F1–F2:** Diagnostic layer over inspect
7. **Phase G1–G2:** Character and voice system
8. **Phase H1–H2:** Full integration and acceptance gate

### Build Law (unchanged)

Every slice ships with:
- One capability
- One failing-then-passing guardrail
- Inspect output proving why it worked
- Full local gate
- Green GitHub checks

No capability merges without a scenario. No scenario without manifest registration.
