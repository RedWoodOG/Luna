# Luna Build Plan
## A Receipt-Bound Cognitive Memory Runtime

### Status

**M1 landed.** `RuntimeTurnReceipt` events are in the event log with source hashes, replay audit covers them, and the full gate passes. The receipt infrastructure is the substrate for every system below.

**This document** is the canonical build plan. It describes what Luna becomes, not what any other system was. Where problems are referenced, they are described in Luna's terms — the solutions are native.

---

## Governing Law

Derived from `docs/LUNA_BUILD_DOCTRINE.md` and `docs/LUNA_MEMORY_STRUCTURE_CONTRACT.md`. These are inviolable:

1. **Event log is source truth.** No durable state exists outside append-only events.
2. **Derived memory must rebuild.** Every structure below must reconstruct identically from the event log.
3. **Provenance is binding, not commentary.** If a claim carries a source, the source event must exist and match.
4. **No hidden state.** No background threads, no unlogged mutations, no silent consolidation.
5. **Receipts for every transformation.** Coalescence, eviction pressure, constraint changes — all produce signed receipts in the event log.
6. **Mechanical enforcement.** Every rule with a detectable violation pattern has a lint, type gate, scenario, or CI check.

---

## Architecture Map

```
                    ┌──────────────────────────┐
                    │      EVENT LOG            │
                    │  (append-only, hashed,    │
                    │   replay-auditable)       │
                    └────────────┬─────────────┘
                                 │
            ┌────────────────────┼────────────────────┐
            ▼                    ▼                    ▼
   ┌─────────────────┐  ┌───────────────┐  ┌──────────────────┐
   │ Structured      │  │ Episode       │  │ RuntimeTurn      │
   │ Assertions      │  │ Formation     │  │ Receipt          │
   │ (domain:kind:   │  │ (grouped by   │  │ (M1: landed)     │
   │  value)         │  │  continuity)  │  │                   │
   └────────┬────────┘  └───────┬───────┘  └────────┬─────────┘
            │                   │                    │
            └───────────────────┼────────────────────┘
                                │
                    ┌───────────▼───────────┐
                    │    TYPED ENTITY       │
                    │    GRAPH              │
                    │  (nodes, edges,       │
                    │   lifecycle,          │
                    │   provenance)         │
                    └───────────┬───────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐     ┌─────────────────┐     ┌──────────────────┐
│ ATTENTION     │     │ ENTITY BOND     │     │ CONSTRAINT       │
│ LATTICE       │     │ GRAPH           │     │ GOVERNOR         │
│ (activation   │     │ (typed          │     │ (surface gating  │
│  propagation  │     │  relationships  │     │  from graph      │
│  over typed   │     │  with trust     │     │  state)          │
│  graph)       │     │  lifecycle)     │     │                  │
└───────┬───────┘     └────────┬────────┘     └────────┬─────────┘
        │                      │                       │
        └──────────────────────┼───────────────────────┘
                               │
                    ┌──────────▼──────────┐
                    │   BOUNDED WORKING   │
                    │   MEMORY            │
                    │ (activation-ranked, │
                    │  budget-constrained)│
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │   RESPONSE PLAN     │
                    │ (answer / refuse /  │
                    │  ask / suppress)    │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │   EVIDENCE CAPSULE  │
                    │ (render packet +    │
                    │  integrity manifest)│
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │   RENDERER          │
                    │ (LLM or geometric   │
                    │  word selector)     │
                    └─────────────────────┘
```

Every box is a system with its own crate, its own scenarios, its own doctrine gate. Nothing is a prompt. Nothing is an ad-hoc lookup. Every arrow is a typed interface with schema validation.

---

## Phase 1: Attention Lattice

**Problem solved:** Memory retrieval that understands what is relevant right now — not by hashing words into a vector, but by propagating activation through a typed entity graph where every node's activation has a reason.

**Why it is different:** There is no continuous field. No embedding layer. No FNV-1a projection. The Attention Lattice is a discrete propagation system. Claims are nodes. Edges are typed relations. Activation flows from query-matched nodes outward along edges, weighted by confidence, lifecycle, and bond strength. Every node in the lattice can explain why it activated: which query term matched, which edge carried the signal, which bond amplified it.

### 1.1 — Lattice Dimensions

Six activation dimensions, each computed from typed assertions:

| Dimension | Source domain(s) | Computed from |
|---|---|---|
| Identity weight | `identity:*` | Count and confidence of self-referential claims |
| Semantic density | `person:*`, `project:*`, `manuscript:*` | Depth of typed claims per entity |
| Goal pressure | `person:goal`, `project:deadline` | Recency + urgency of goal claims |
| Bond strength | bond edges | Trust × intimacy × recency for each bonded entity |
| Correction salience | correction events | Recency + count of supersession transitions |
| Temporal proximity | episode timestamps | How recently each entity was discussed |

Each dimension is a `f32` scored by a pure function over `MemoryState`. The scoring functions are deterministic, parameterized, and unit-tested in isolation.

**Scenarios:**
- `attention_lattice_from_graph.json` — seed claims across entities, compute lattice, verify each dimension value against expected
- `attention_lattice_correction_boost.json` — correct a fact → verify correction salience dimension increases → verify lattice reweights

### 1.2 — Lattice Provenance

```rust
pub struct AttentionLattice {
    pub dimensions: LatticeDimensions,
    pub activation_map: BTreeMap<String, NodeActivation>,
    pub provenance: LatticeProvenance,
}

pub struct LatticeProvenance {
    pub dimension_sources: BTreeMap<String, Vec<MemoryProvenance>>,
    pub activation_sources: BTreeMap<String, Vec<MemoryProvenance>>,
}
```

`luna runtime inspect --lattice` must show, for every activated node: which claims contributed, at what weight, through which edges.

**Scenarios:**
- `attention_lattice_provenance_inspect.json` — verify inspect output names source claims for every activated node
- Replay audit: same graph → same lattice → same inspect output

### 1.3 — Activation Propagation

Activation does not dump every claim. It propagates outward from query-matched seed nodes:

1. Entity-match: query term hits an entity label → that entity's claims activate at base weight
2. Bond-propagation: activation flows along bond edges to bonded entities, decaying with distance
3. Claim-propagation: within an entity, related claims activate (a `person:role` claim activates the `person:location` claim for the same person)
4. Lifecycle filter: superseded and stale claims are attenuated or excluded
5. Budget clamp: the top N nodes by activation score enter working memory

Propagation depth is configurable. Default: depth 2, decay 0.5 per hop.

**Scenarios:**
- `activation_propagation_depth.json` — set depth=1 → verify only direct matches activate → set depth=2 → verify bonded entities also activate
- `activation_propagation_lifecycle_filter.json` — seed superseded claim → verify it does not activate → verify current claim activates instead

---

## Phase 2: Entity Bond Graph

**Problem solved:** Memory that understands who matters to whom, and how that changes. Not a floating geometry — a typed, versioned graph of bonds between entities, with trust that decays correctly and corrections that leave a trace.

**Why it is different:** There is no relationship field. No geometric interpolation. Bonds are first-class typed edges with their own lifecycle. Every bond carries a chain of `BondEvent`s — disclosures, corrections, decays, reinforcements — all in the event log. A bond's trust is not a number in a hash; it is the sum of its event history, recomputable from the log.

### 2.1 — Bond as Typed Edge

```rust
pub struct EntityBond {
    pub source_entity: String,
    pub target_entity: String,
    pub bond_kind: BondKind,       // Colleague, Friend, Family, Acquaintance
    pub trust: f32,
    pub intimacy: f32,
    pub last_interaction: DateTime<Utc>,
    pub event_history: Vec<Uuid>,  // event IDs in the log
    pub lifecycle_status: AssertionLifecycleStatus,
    pub provenance: Vec<MemoryProvenance>,
}
```

Bonds form from assertions: `"Tom is my colleague"` creates a Colleague bond. `"Tom is a close friend"` supersedes it with a Friend bond. The old bond is not deleted. It is superseded, with a receipt linking old → new.

**Scenarios:**
- `bond_formation_from_assertions.json` — assert colleague → verify bond created → assert close friend → verify bond upgraded, old bond superseded
- `bond_correction_receipt.json` — verify correction event links old bond to new bond in inspect output

### 2.2 — Trust as Event Sum

Trust is not set directly. It is computed from bond events:

```
trust = clamp(base_trust + Σ event_weight, 0.0, 1.0)

Event weights:
  disclosure (personal fact shared)   +0.05
  correction (user corrected Luna)    +0.03 (shows engagement)
  contradiction (disagreement)        -0.08
  long_gap (>30 days no interaction)  decay by factor
```

Every event that affects trust is in the log. Replay reproduces the same trust value.

**Scenarios:**
- `trust_computation_from_events.json` — simulate bond events → verify trust value → replay → verify same value
- `trust_decay_over_gap.json` — establish bond → inject 45-day gap event → verify trust decayed → verify decay formula in inspect

### 2.3 — Bond-Biased Activation

The Attention Lattice uses bond strength to bias activation:

```
activation_bonus = base_activation * (trust * 0.3 + intimacy * 0.2)
```

Facts about closely bonded entities activate more strongly than facts about strangers. The bonus is inspectable: `luna runtime inspect --lattice` shows the bond contribution to each node's activation score.

**Scenarios:**
- `bond_biased_activation.json` — store identical facts about bonded entity (Tom, trust=0.9) and unbonded entity (stranger) → query for facts → verify Tom's facts activate first

---

## Phase 3: Episode Coalescence Receipts

**Problem solved:** Memory that consolidates similar experiences without losing evidence. Every merge is a signed receipt. Every receipt can be rejected. Raw events always remain.

**Why it is different:** There is no silent merge. No write-time consolidation that destroys source episodes. Coalescence is a derived operation that produces an accepted or rejected receipt. Accepted receipts become part of the derived state. Rejected receipts log why they failed. Raw episodes are never deleted — coalescence adds a layer, it does not replace.

### 3.1 — Coalescence Receipt Structure

```rust
pub struct CoalescenceReceipt {
    pub receipt_id: Uuid,
    pub source_episode_ids: Vec<Uuid>,
    pub coalesced_episode_id: Uuid,
    pub similarity_score: f32,
    pub policy: CoalescencePolicy,
    pub source_event_hashes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub accepted: bool,
    pub rejection_reason: Option<String>,
}
```

Accepted receipts link to source events. A replay audit verifies that coalescence was applied consistently. If a receipt is later found invalid (threshold change, policy change), it can be rejected retroactively and the system re-derives from raw episodes.

**Scenarios:**
- `coalescence_receipt_accepted.json` — 5 similar episodes → coalescence triggers → verify receipt links to all 5 sources → replay produces same receipt
- `coalescence_receipt_rejected.json` — episode below threshold → attempt coalescence → verify receipt rejected with reason logged

### 3.2 — Pressure-Driven Coalescence

Coalescence is not always-on. It activates under memory pressure:

```
pressure = active_episode_count / config.soft_ceiling
if pressure > 0.8:
    coalescence_threshold = 0.60  // aggressive
elif pressure > 0.5:
    coalescence_threshold = 0.75  // moderate
else:
    coalescence_threshold = 0.85  // conservative, rare coalescence
```

The soft ceiling is configurable. It is a pressure signal, not a hard eviction bound. Episodes are never deleted to stay under ceiling — coalescence reduces the active set, but raw events remain.

**Scenarios:**
- `coalescence_pressure_thresholds.json` — fill active set to 80% → verify threshold drops → coalescence triggers → verify active set shrinks → raw events unchanged
- `coalescence_replay_rebuild.json` — delete all coalescence receipts → replay from raw events → verify same receipts regenerated

### 3.3 — Correct Episodic Decay

Episodes weaken over time unless recalled:

```
age_days = (now - episode.created_at).days
recall_boost = min(episode.recall_count, 10) as f32 / 10.0
freshness = 1.0 - (age_days / 365.0).clamp(0.0, 0.95)
effective_strength = episode.base_strength * freshness * (0.3 + 0.7 * recall_boost)
```

Frequently recalled episodes stay strong. Never-recalled episodes decay. The decay formula is a pure function with unit-tested parameters.

**Unit tests:**
- `decay_increases_with_age()` — older → weaker
- `decay_resists_with_recall()` — recalled → resistant
- `decay_never_drops_below_floor()` — minimum strength = base × 0.05 × 0.3

---

## Phase 4: Evidence Capsule

**Problem solved:** A render boundary that proves what the renderer was given. The LLM or geometric selector receives a sealed capsule — assertions, bonds, lattice state — with an integrity manifest that can be verified independently.

**Why it is different:** This is not a "render contract" that depends on the renderer behaving. It is a cryptographic boundary. The capsule carries source event hashes. Post-render, the output is checked against the capsule's contents. Violations are detected, logged, and surfaced in inspect. The renderer cannot silently inject facts.

### 4.1 — Capsule Structure

```rust
pub struct EvidenceCapsule {
    pub response_plan: ResponsePlan,
    pub lattice_snapshot: AttentionLattice,
    pub active_bonds: Vec<EntityBond>,
    pub selected_claims: Vec<MemoryClaim>,
    pub integrity: CapsuleIntegrity,
}

pub struct CapsuleIntegrity {
    pub source_event_count: usize,
    pub source_event_hashes: Vec<String>,
    pub lattice_provenance_hash: String,
    pub replay_snapshot_hash: String,
    pub capsule_id: Uuid,
}
```

The capsule's integrity fields are hashed from the current replay snapshot. Any tampering with the capsule's contents produces a hash mismatch. The renderer cannot alter the capsule — it only translates it.

**Scenarios:**
- `evidence_capsule_production.json` — produce capsule → verify integrity hashes match replay → serialize → deserialize → verify hashes still match
- `evidence_capsule_tamper_detection.json` — alter a claim in the capsule → verify integrity check fails

### 4.2 — Post-Render Validation

After the renderer produces output, Luna checks:

| Check | What it detects |
|---|---|
| Fact injection | Renderer added a claim not in the capsule |
| Supersession leak | Renderer surfaced a superseded claim |
| Identity violation | Renderer added self-description not in axioms |
| Confidence flattening | Renderer presented speculation as certainty |
| Capsule bypass | Renderer ignored the capsule entirely |

Each violation is a `CapsuleViolation` event in the log. Repeated violations trigger the Constraint Governor.

**Scenarios:**
- `post_render_fact_injection.json` — simulate render output with unsupported claim → verify violation logged → verify output marked
- `post_render_supersession_leak.json` — simulate render output with superseded claim → verify violation detected

---

## Phase 5: Constraint Governor

**Problem solved:** A safety layer that gates what memory can surface and what plans can execute — not from a hardcoded state machine, but computed dynamically from the current graph state, with every decision logged as a structured assertion.

**Why it is different:** There is no safety FSM with hardcoded transitions. The governor is a pure function over `MemoryState` that produces a `ConstraintPosture`. Every posture change is an event in the log with the source claims that triggered it. Replay reproduces the same posture at every turn.

### 5.1 — Posture Computation

```
ConstraintPosture = evaluate(
    contradiction_density,      // claims in conflict / total claims
    correction_salience_max,    // most recent correction salience
    bond_trust_minimum,         // lowest trust across all active bonds
    capsule_violation_count,    // violations in last 10 turns
    unknown_preservation_rate   // fraction of queries answered "unknown"
)

Postures:
  Open       — all claims surfaceable, all plans available
  Careful    — unconfirmed claims require explicit marking
  Guarded    — only confirmed current claims surface; no speculative answers
```

The transition from `Open` to `Careful` happens when contradiction density exceeds a threshold. The trigger claims are named in the posture change event. Inspect shows: "Guarded because 4 contradictions in 8 turns: [claim keys]".

**Scenarios:**
- `constraint_governor_escalation.json` — seed contradictions → verify posture escalates → verify posture event names source claims → resolve contradictions → verify posture returns to Open
- Replay audit: same events → same posture at every turn

### 5.2 — Surface Gating

Each posture gates:

| Surface | Open | Careful | Guarded |
|---|---|---|---|
| Current confirmed claims | ✓ | ✓ | ✓ |
| Current unconfirmed claims | ✓ | marked | ✗ |
| Superseded claims | ✗ | ✗ | ✗ |
| Bond trust below 0.3 | ✓ | ✗ | ✗ |
| Speculative answers | ✓ | ✗ | ✗ |
| Capsule violation output | ✗ | ✗ | ✗ |

Gating is applied at activation time and at capsule assembly. The governor does not modify memory — it gates what reaches the renderer.

**Scenarios:**
- `constraint_governor_surface_gating.json` — set Guarded posture → verify unconfirmed claims excluded from capsule → verify speculative answer blocked

---

## Phase 6: Diagnostic Inquest

**Problem solved:** When Luna's memory produces a surprising answer, you can dissect why — without reading raw code or grepping logs. Every diagnostic is a named query over the typed graph that produces a structured report with source claims.

**Why it is different:** There are no background observers. No threads that accumulate state outside the event log. Inquests are deterministic, on-demand queries that run at inspect time. They read the current `MemoryState` and produce reports. Every report item carries a source claim reference.

### 6.1 — Inquest Commands

```
luna runtime inquest contradiction-density
luna runtime inquest bond-health --entity Tom
luna runtime inquest lattice-drift --since 2026-05-01
luna runtime inquest capsule-integrity --last 20
luna runtime inquest stale-accumulation
luna runtime inquest correction-chain --claim <key>
```

Each inquest is a pure function: `fn(MemoryState) -> InquestReport`. No side effects. No state mutation. Reports are JSON-serializable with schema validation.

**Scenarios:**
- `inquest_contradiction_density.json` — seed contradictory claims → run inquest → verify report shows density, source claims, recommended action
- `inquest_correction_chain.json` — correct a fact twice → run inquest → verify report shows full chain: original → corrected → re-corrected

### 6.2 — Between-Turn Reflection

Optionally, Luna can run reflection inquests between turns and write findings as system events:

```
domain: system
kind: reflection
value: "bond_trust_Tom_decaying: last_interaction_14_days_ago"
```

Reflection events enter the event log with system provenance. They influence the next turn's activation but never override user-sourced evidence. Reflection is configurable — it can be disabled, run every N turns, or triggered by pressure signals.

**Scenarios:**
- `between_turn_reflection.json` — seed decaying bond → trigger reflection → verify reflection event in log → verify next turn activation reflects finding

---

## Phase 7: Persona Loom

**Problem solved:** Luna's voice — warm, precise, funny, natural — is not a prompt. It is a set of typed constraints woven into the Evidence Capsule that constrain how the renderer translates evidence into language.

**Why it is different:** There is no "character prompt." No hidden system instruction. The Persona Loom is an entity group in the typed graph: `domain:persona`, with claims for tone, formality, humor, verbosity, and self-disclosure. Persona changes create correction events. Persona state is inspectable. The loom's parameters are passed as capsule constraints, not as text the renderer can ignore.

### 7.1 — Persona Definition

```
domain: persona
kind: axiom
value: "speaks warmly, with wit that emerges from understanding, never forced"
lifecycle_status: current
provenance: system_root:persona_v1
```

A persona is a named entity group with typed claims. Multiple personas can exist (professional, casual, terse). Switching personas creates a `persona:activation` event. The previous persona's claims are not deleted — they remain with `lifecycle_status: inactive`.

**Scenarios:**
- `persona_definition_lifecycle.json` — define persona → modify tone → verify old axiom superseded → verify new axiom current
- `persona_activation_switch.json` — switch from professional to casual → verify activation event logged → verify capsule constraints reflect casual parameters

### 7.2 — Loom Parameters as Capsule Constraints

The loom translates persona claims into capsule constraint parameters:

```rust
pub struct LoomParameters {
    pub warmth: f32,
    pub formality: f32,
    pub humor: f32,
    pub verbosity: f32,
    pub self_disclosure: f32,
}
```

These are not floats in a prompt. They are typed fields in the Evidence Capsule that the renderer must respect. A geometric word selector uses them to bias trajectory planning. An LLM renderer receives them as structured constraints, not as prose it can reinterpret.

**Scenarios:**
- `loom_parameters_capsule_constraints.json` — set warmth=0.9, formality=0.2 → verify capsule carries these values → verify render output respects them
- Unit test: `loom_from_persona_claims()` — persona claims with "warm, casual" produce warmth=0.85, formality=0.25

---

## Phase 8: Receipt-Bound Runtime

**Goal:** The complete Luna turn pipeline. Every step is a pure function or a logged event. Every output carries provenance. Replay reproduces everything.

### 8.1 — Turn Pipeline

```
 1. intake_normalize        — sanitize text, detect language
 2. intake_classify         — statement, question, correction, noise, greeting
 3. entity_sieve            — extract typed assertions from text
 4. correction_target       — if correction cue: find target claim, create supersession
 5. lifecycle_apply         — supersede, contradict, or reinforce existing claims
 6. bond_update             — update bond event history from assertions
 7. lattice_compute         — compute Attention Lattice from current state
 8. governor_evaluate       — compute ConstraintPosture
 9. recall_resonate         — query-matched + bond-propagated + lifecycle-filtered
10. activation_select       — top N nodes by score enter working memory
11. coalescence_pressure    — if pressure > threshold: attempt episode coalescence
12. response_plan           — select action: answer, ask, suppress, unknown
13. capsule_assemble        — build Evidence Capsule with integrity manifest
14. renderer_invoke         — pass capsule to renderer (LLM or geometric selector)
15. capsule_validate        — check render output against capsule contents
16. reflection_optional     — if configured: run diagnostic inquests
17. commit                  — write turn receipt to event log
```

Every step that mutates durable state writes an event. Steps 1–13 are deterministic pure functions over `(event_log, turn_text)`. Steps 14–15 bridge to the renderer. Steps 16–17 close the loop.

### 8.2 — Luna Is Complete When

```
Given only an append-only event log, Luna:
 1. Rebuilds typed memory with full lifecycle
 2. Computes an Attention Lattice traced to source claims
 3. Models bonds as typed edges with trust computed from event history
 4. Coalesces episodes with signed, replayable receipts — raw events preserved
 5. Computes constraint posture from graph state with named trigger claims
 6. Activates a bounded working set with bond-biased propagation
 7. Produces an Evidence Capsule with cryptographic integrity manifest
 8. Validates render output against capsule contents
 9. Answers from evidence with recall reasons
10. Suppresses stale, superseded, and unconfirmed claims per posture
11. Reproduces the same result under replay audit
12. Passes every scenario, benchmark, and doctrine gate mechanically
13. Supports diagnostic inquests that trace every decision to source events
```

---

## Execution Order

Proceed in this exact order. Each phase builds on the previous.

| # | Phase | Depends on |
|---|---|---|
| 1 | Attention Lattice (1.1–1.3) | Existing typed graph + M1 receipts |
| 2 | Entity Bond Graph (2.1–2.3) | Attention Lattice (bond-biased activation) |
| 3 | Episode Coalescence Receipts (3.1–3.3) | M1 receipts infrastructure |
| 4 | Evidence Capsule (4.1–4.2) | Lattice + Bonds + ResponsePlan |
| 5 | Constraint Governor (5.1–5.2) | Capsule (posture gates capsule contents) |
| 6 | Diagnostic Inquest (6.1–6.2) | Full graph state available |
| 7 | Persona Loom (7.1–7.2) | Capsule (loom parameters are capsule constraints) |
| 8 | Receipt-Bound Runtime (8.1–8.2) | All of the above |

### Build Law

Every slice ships with:
- One capability
- One failing-then-passing guardrail (scenario or unit test)
- Inspect output proving why it worked
- Full local gate pass
- Green GitHub checks

No capability merges without a scenario. No scenario without `SCENARIO_MANIFEST.txt` registration. No doctrine relaxation without a documented reason.

---

## Appendix: Historical Context

Luna was built to solve a specific class of memory problems that existing systems handled with continuous geometry, hash projections, and silent consolidation. Those systems demonstrated that personal, persistent memory is valuable — but their architectures had structural gaps:

- **Supersession:** When facts change, old beliefs must be marked, not silently accumulated.
- **Provenance:** Every surfaced claim must trace to a source event.
- **Replay audit:** Memory correctness must be mechanically verifiable.
- **Receipt-bound transformations:** Every consolidation, eviction, or constraint change must leave a signed receipt.

Luna's architecture — typed graph, lifecycle, provenance, receipts, replay audit — was designed to close those gaps. The systems described in this plan extend that architecture into cognition, relationships, safety, and voice — without importing the architectural choices that created the gaps.

Luna does not replicate. It supersedes.
