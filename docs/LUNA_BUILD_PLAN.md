# Luna Build Plan
## Issue-Driven Cognitive Memory Architecture

### Luna’s Superiority Thesis

Luna is not better because it has more systems. Luna is better because every system is:

- **event-backed** — durable state is only the event log; everything else is derived
- **lifecycle-aware** — facts evolve: current, superseded, contradicted, stale; history is preserved
- **provenance-bound** — every claim carries a source event hash; every decision names its inputs
- **replayable** — same log produces same state, provably; divergence is a defect
- **inspectable** — every internal state can be queried; every query produces a structured report
- **bounded** — working memory has hard limits; unbounded growth is a failure mode
- **mechanically gated** — every rule has a lint, type gate, scenario, or CI check; no promises without proof

This is the opening law. No system below is adopted because it is novel. Every system exists to make a specific failure mode **impossible or detectable**.

---

## Defect Closure Matrix

| Problem | Risk | Luna Mechanism | Proof Gate |
|---|---|---|---|
| Silent memory merge | Lost truth; merged episodes cannot be unmerged | **Episode Coalescence Receipt** — every consolidation produces a signed receipt with source ancestry; raw events remain untouched | `replay_audit_rebuilds_same_receipts` scenario |
| Old facts survive corrections | Stale answers; user corrects but old belief still surfaces | **Lifecycle Supersession** — corrections create explicit contradiction events; superseded claims are filtered at activation time | `correction_lifecycle` scenario |
| Continuous geometry cannot explain itself | Vibes; activation score is a float from a hash with no inspectable reason | **Attention Lattice** — discrete activation components computed from typed claims; every dimension shows contributing claims | `luna runtime inspect --lattice` shows sources |
| LLM invents memory | Hallucination; renderer adds facts not in the capsule | **Evidence Capsule** — sealed packet with integrity manifest; post-render validation detects unsupported claims | `render_violation_detection` scenario |
| Relationship bias is hidden | Unverifiable behavior; why did Maria’s facts surface before Tom’s? | **Entity Bond Graph** — typed edges with trust computed from event history; bond strength biases activation explicitly | `relationship_biased_recall` scenario |
| Safety posture is opaque | Hidden policy; system refuses but cannot show why | **Constraint Governor** — posture computed from graph state; every escalation is a structured assertion with named trigger claims | `posture_provenance_audit` scenario |
| Render packet lacks proof | Cannot verify what the renderer was given | **Integrity Manifest** — cryptographic hash of capsule contents; replay snapshot proves packet was built from real events | `capsule_tamper_detection` scenario |
| Unbounded memory growth | Performance collapse; vector stores grow forever | **Bounded Working Memory** — activation-ranked selection with fixed node/edge budget; excess is filtered, not lost | `working_memory_budget_compliance` scenario |
| Diagnostic state is hidden | Cannot debug surprising behavior | **Diagnostic Inquest** — on-demand queries over typed graph; every diagnostic report item carries a source claim reference | `inquest_provenance_tracing` scenario |
| Persona is a prompt hack | Renderer ignores or misinterprets character | **Persona Loom** — typed axioms as capsule constraints; renderer receives structured parameters, not prose instructions | `persona_parameter_enforcement` scenario |

---

## Capability Promises

### Receipt-Bound Runtime (M1 — Landed)

What Luna becomes capable of:
- Every turn produces a signed receipt with source event hashes
- Receipts are replay-auditable; same log produces same receipt chain
- No state mutation without an event
- Foundation for every system below

Proof: `runtime_turn_receipt` events in log; `receipt_replay` scenario passes; full gate green.

### Attention Lattice

What Luna becomes capable of:
- Remembers what matters right now, not everything equally
- Explains why something became salient (which claims contributed, through which edges)
- Ranks correction/surprise above stale noise automatically
- Shows component scores in inspect output with source claim references
- Never produces an activation score that cannot be traced to typed assertions

Failure mode prevented: Activation without reason; geometry that cannot explain itself.

Proof gates per phase:
- [ ] `AttentionLattice` struct with six typed dimensions
- [ ] `LatticeProvenance` mapping dimensions to `MemoryProvenance` vectors
- [ ] `runtime event`: `lattice_computed` with dimension values and source hashes
- [ ] `inspect` command: `luna runtime inspect --lattice` shows dimensions + sources
- [ ] Scenario: `attention_lattice_from_graph` — verify dimensions match expected
- [ ] Scenario: `attention_lattice_correction_boost` — verify correction salience increases after correction event
- [ ] Replay audit: same graph → same lattice values
- [ ] Doctrine impact: no hardcoded activation weights; all from typed claims
- [ ] Failure mode: geometry without provenance

### Entity Bond Graph

What Luna becomes capable of:
- Remembers relationship context (trust, intimacy, familiarity) as typed edges, not floating geometry
- Handles changing trust and familiarity through assertion lifecycle
- Corrects relationship assumptions (colleague → friend) with explicit supersession
- Recalls people differently based on evidenced bond strength, not implicit bias
- Shows full bond event history: disclosures, corrections, decay, reinforcement

Failure mode prevented: Relationship bias without inspectable cause; trust that cannot be traced to events.

Proof gates per phase:
- [ ] `EntityBond` struct with `bond_kind`, `trust`, `intimacy`, `event_history`
- [ ] `BondEvent` enum: Disclosure, Correction, Decay, Reinforcement
- [ ] `runtime event`: `bond_formed`, `bond_superseded`, `bond_decayed`
- [ ] `inspect` command: `luna runtime inspect --bonds` shows trust computation from events
- [ ] Scenario: `bond_formation_from_assertions` — assert colleague → verify bond created
- [ ] Scenario: `bond_correction_receipt` — assert friend → verify old bond superseded with receipt
- [ ] Scenario: `bond_biased_activation` — verify closely bonded entities activate more strongly
- [ ] Replay audit: same events → same trust values
- [ ] Doctrine impact: no relationship field without typed edge; trust computed from event sum
- [ ] Failure mode: unverifiable relationship bias

### Episode Coalescence

What Luna becomes capable of:
- Compresses similar episodes without losing raw events
- Produces signed receipts that can be rejected or reversed
- Pressure-driven coalescence with configurable thresholds
- Never deletes episodes to stay under a ceiling

Failure mode prevented: Silent merge with no rollback; lost truth from consolidation.

Proof gates per phase:
- [ ] `CoalescenceReceipt` struct with `source_episode_ids`, `coalesced_episode_id`, `accepted`, `rejection_reason`
- [ ] `runtime event`: `coalescence_accepted` or `coalescence_rejected`
- [ ] `inspect` command: `luna runtime inspect --coalescence` shows receipt chain
- [ ] Scenario: `coalescence_receipt_accepted` — verify receipt links to source events
- [ ] Scenario: `coalescence_receipt_rejected` — verify below-threshold attempt logs rejection
- [ ] Scenario: `coalescence_pressure_thresholds` — verify pressure changes threshold
- [ ] Replay audit: raw events → same coalescence receipts
- [ ] Doctrine impact: coalescence never mutates raw events; receipts are derived layer
- [ ] Failure mode: silent merge without source ancestry

### Evidence Capsule

What Luna becomes capable of:
- Lets the model speak naturally from structured evidence
- Prevents the model from becoming the memory authority (cannot add facts not in capsule)
- Catches unsupported rendered claims with post-render validation
- Provides cryptographic proof of what the renderer was given

Failure mode prevented: LLM hallucination; renderer as hidden source of truth.

Proof gates per phase:
- [ ] `EvidenceCapsule` struct with `lattice_snapshot`, `active_bonds`, `selected_claims`, `integrity_manifest`
- [ ] `CapsuleIntegrity` with source event hashes and replay snapshot hash
- [ ] `runtime event`: `capsule_produced` with integrity hash
- [ ] `inspect` command: `luna runtime inspect --capsule` shows integrity manifest
- [ ] Scenario: `evidence_capsule_production` — verify capsule carries source claims
- [ ] Scenario: `evidence_capsule_tamper_detection` — verify hash mismatch on tampering
- [ ] Scenario: `post_render_fact_injection` — detect renderer adding unsupported claim
- [ ] Scenario: `post_render_supersession_leak` — detect renderer surfacing superseded claim
- [ ] Replay audit: same state → same capsule hash
- [ ] Doctrine impact: renderer output checked against capsule; violations logged
- [ ] Failure mode: hallucinated memory; unverifiable render input

### Constraint Governor

What Luna becomes capable of:
- Gates what memory can surface based on current graph state, not hardcoded rules
- Shows why a constraint escalated (which contradictions triggered it)
- Every posture change is a structured assertion with named source claims
- Replays produce same posture at every turn

Failure mode prevented: Opaque safety policy; hidden refusal reasons.

Proof gates per phase:
- [ ] `ConstraintPosture` enum: Open, Careful, Guarded
- [ ] `PostureEvaluation` struct with trigger claims and scores
- [ ] `runtime event`: `posture_escalated` or `posture_deescalated` with trigger claims
- [ ] `inspect` command: `luna runtime inspect --posture` shows evaluation factors
- [ ] Scenario: `constraint_governor_escalation` — verify posture escalates with named triggers
- [ ] Scenario: `constraint_governor_surface_gating` — verify unconfirmed claims excluded in Guarded
- [ ] Replay audit: same events → same posture at each turn
- [ ] Doctrine impact: no posture without provenance; all triggers inspectable
- [ ] Failure mode: opaque safety decisions

### Diagnostic Inquest

What Luna becomes capable of:
- Explains surprising behavior on demand, not through code archaeology
- Every diagnostic report item carries a source claim reference
- No background threads accumulating hidden state
- Deterministic queries over typed graph

Failure mode prevented: Undiagnosable surprises; hidden observer state.

Proof gates per phase:
- [ ] `Inquest` trait: `fn run(&MemoryState) -> InquestReport`
- [ ] `InquestReport` with `findings: Vec<InquestFinding>` each with `source_claims`
- [ ] CLI: `luna runtime inquest <name>`
- [ ] Built-in inquests: `contradiction-density`, `bond-health`, `lattice-drift`, `capsule-integrity`
- [ ] Scenario: `inquest_contradiction_density` — verify report shows density and sources
- [ ] Scenario: `inquest_correction_chain` — verify chain of corrections visible
- [ ] Golden snapshot: diagnostic output matches expected for known scenarios
- [ ] Doctrine impact: diagnostics are pure functions; no side effects; no hidden state
- [ ] Failure mode: undiagnosable behavior

### Persona Loom

What Luna becomes capable of:
- Voice and character that are typed constraints, not prompt prose
- Persona changes create correction events with lifecycle
- Multiple personas (professional, casual) with activation events
- Renderer receives structured parameters, not instructions it can ignore

Failure mode prevented: Prompt hacks; persona as unverifiable text.

Proof gates per phase:
- [ ] `Persona` entity group with `domain:persona`, `kind:axiom` claims
- [ ] `LoomParameters` struct: warmth, formality, humor, verbosity, self_disclosure
- [ ] `runtime event`: `persona_activated` with previous and new persona IDs
- [ ] `inspect` command: `luna runtime inspect --persona` shows active axioms
- [ ] Scenario: `persona_definition_lifecycle` — verify persona changes create supersession
- [ ] Scenario: `persona_activation_switch` — verify switch produces activation event
- [ ] Scenario: `persona_parameter_enforcement` — verify renderer respects capsule constraints
- [ ] Replay audit: same persona events → same loom parameters
- [ ] Doctrine impact: persona is typed claims, not prompt text; changes have lifecycle
- [ ] Failure mode: persona as hidden prompt

---

## Execution Order

Proceed in dependency order. Each phase is a contract; the checklist above is the acceptance criteria.

| Order | Phase | Capability Unlocked | Depends On |
|---|---|---|---|
| M1 | Receipt-Bound Runtime | Every turn is a signed, replayable event | Core event log |
| 1 | Attention Lattice | Salience with inspectable reasons | M1 receipts, typed graph |
| 2 | Entity Bond Graph | Relationship context with trust events | Attention Lattice (bond-biased activation) |
| 3 | Episode Coalescence | Compression without lost truth | M1 receipts |
| 4 | Evidence Capsule | Renderer bound by proof | Lattice, Bonds, ResponsePlan |
| 5 | Constraint Governor | Gating with inspectable triggers | Capsule (posture gates surface) |
| 6 | Diagnostic Inquest | Debuggability without code archaeology | Full graph state |
| 7 | Persona Loom | Voice as typed constraints | Capsule (loom parameters are constraints) |
| 8 | Receipt-Bound Runtime (Complete) | Full turn pipeline with all systems integrated | All above |

---

## Final Acceptance Definition

Luna is **vastly superior** when:

```
Given only an append-only event log, Luna can:

1. Rebuild typed memory with full lifecycle (current, superseded, contradicted, stale)
2. Compute an Attention Lattice — every activation dimension traced to source claims
3. Model Entity Bonds — trust computed from event history, not hashed geometry
4. Coalesce episodes with signed, reversible receipts — raw events preserved
5. Evaluate Constraint Posture from graph state — every escalation names trigger claims
6. Activate bounded working memory with bond-biased propagation
7. Produce an Evidence Capsule with cryptographic integrity manifest
8. Validate render output against capsule contents — unsupported claims detected
9. Answer from evidence with recall reasons — not from dumped context
10. Suppress stale, superseded, and unsafe claims per posture
11. Pass replay audit — same log produces same state, provably
12. Support Diagnostic Inquests — every decision traceable to source events
13. Weave Persona from typed axioms — not prompt hacks
14. Pass every scenario, benchmark, and doctrine gate mechanically
```

That is the bar. No phase ships without its checklist complete. No checklist item is optional.
