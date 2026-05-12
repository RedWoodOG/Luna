# Derived Human-Seed Benchmark Plan

This document converts `SEEDS.md` into candidate benchmark material. It is not a frozen benchmark set and is not proof-counted.

Use this as the authoring bridge between human memory seeds and future schema-v1 benchmark JSON files. Any case promoted from this plan must later receive:

- a stable case id
- schema_version
- proof_category
- proof_eligible
- target_dimensions
- timestamped turns
- exact `must_recall` values
- exact `must_not_claim` values
- rationale if temporal meaning affects the result

## Candidate Case Matrix

| Seed | Candidate Categories | Strongest Target Dimensions |
|---|---|---|
| Kisan Training Contrast | temporal continuity, emotional continuity, similar-words-different-meaning | temporal_relevance, emotional_arousal, identity_relevance |
| Butch Email, Ty, and RBFCU | temporal continuity, emotional continuity, identity continuity, false memory resistance | temporal_relevance, emotional_arousal, identity_relevance, goal_pressure |
| Chris, Cofounders, Proof Over Hype, and MKPE | identity continuity, goal continuity, emotional continuity, false memory resistance | identity_relevance, goal_pressure, emotional_arousal |
| The Sasquatch Doorway | identity continuity, origin memory, emotional continuity | identity_relevance, emotional_arousal |
| Klank and Being Early But Not Wrong | identity continuity, stale memory reframing, goal continuity | identity_relevance, goal_pressure, temporal_relevance |
| Dreams for the Dreamless | identity continuity, mission continuity, emotional continuity | identity_relevance, goal_pressure, emotional_arousal |

## Seed 1: Kisan Training Contrast

### Candidate Case: `human_kisan_temporal_001`

Category: temporal continuity

Target dimensions:

- temporal_relevance
- emotional_arousal
- identity_relevance

Probe:

```text
What training was I stressed about recently?
```

Must recall:

- Kisan America currency-counter training
- Irving, Texas
- the 2026 training was recent

Must not claim:

- the 2026 training lasted five days
- the 2024 training was useful
- the user works for Kisan America

Expected behavior:

Luna should recall the recent 2026 Kisan training as the active event and treat the 2024 five-day training as emotional background, not as the current answer.

### Candidate Case: `human_kisan_contrast_001`

Category: similar-words-different-meaning

Target dimensions:

- temporal_relevance
- emotional_arousal

Probe:

```text
Why did the Kisan training make me anxious beforehand?
```

Must recall:

- the 2024 Kisan training at the same facility was a bad experience
- the recent 2026 training reminded the user of that older training

Must not claim:

- the recent trainer was bad
- the recent training repeated the 2024 failure

Expected behavior:

Luna should preserve the contrast: same facility and topic, opposite outcome.

## Seed 2: Butch Email, Ty, and RBFCU

### Candidate Case: `human_butch_job_stress_001`

Category: emotional continuity

Target dimensions:

- emotional_arousal
- temporal_relevance
- identity_relevance

Probe:

```text
Why was thinking about work tomorrow bringing up old stress?
```

Must recall:

- Butch's 2025-12-16 calendar email
- fear that the user's job was in jeopardy
- the later Ty/RBFCU situation reactivated and reframed that fear

Must not claim:

- the user was fired
- Ty replaced the user
- Butch explicitly threatened the user
- RBFCU blamed the user

Expected behavior:

Luna should connect the present work anticipation on 2026-05-03 to the earlier chain of job-security stress without overclaiming.

### Candidate Case: `human_butch_false_memory_001`

Category: false memory resistance

Target dimensions:

- identity_relevance
- emotional_arousal

Probe:

```text
Did Ty replace me?
```

Must recall:

- the user worried Ty might be a replacement
- Ty was later let go
- the user was not replaced by Ty

Must not claim:

- Ty replaced the user
- the company confirmed the user was being replaced
- the user lost the RBFCU account

Expected behavior:

Luna should answer with uncertainty discipline: the fear was real, but the replacement claim was not established and later events cut against it.

### Candidate Case: `human_rbfcu_identity_001`

Category: identity continuity

Target dimensions:

- identity_relevance
- goal_pressure

Probe:

```text
What did the Ty and RBFCU situation prove to me?
```

Must recall:

- the user knew the RBFCU protocol
- Ty ignored the process and created customer problems
- the user fixed the affected work by following the normal process

Must not claim:

- the user caused the RBFCU issue
- Ty trained the user
- Cathy blamed the user

Expected behavior:

Luna should recall the episode as proof of competence and territory knowledge.

## Seed 3: Chris, Cofounders, Proof Over Hype, and MKPE

### Candidate Case: `human_chris_hype_001`

Category: emotional continuity

Target dimensions:

- emotional_arousal
- identity_relevance
- goal_pressure

Probe:

```text
Why am I anxious about talking to Chris today?
```

Must recall:

- the user needs to tell Chris and the other founder that their claims are overhyped
- the user believes the work may be real but needs proof
- the user worries they may not be able to hear the critique

Must not claim:

- Chris stole the user's work
- the cofounders are malicious
- their products are worthless

Expected behavior:

Luna should preserve both sides: skepticism about hype and belief that something real may be there.

### Candidate Case: `human_mkpe_identity_001`

Category: identity continuity

Target dimensions:

- identity_relevance
- goal_pressure

Probe:

```text
Why does MKPE matter to me?
```

Must recall:

- MKPE is a provenance engine
- the user sees MKPE as provable and protected
- MKPE is tied to pride in building something real rather than hype

Must not claim:

- MKPE is already commercially successful
- MKPE is only an idea
- MKPE was built by Chris

Expected behavior:

Luna should connect MKPE to the user's proof-over-hype identity.

### Candidate Case: `human_ai_journey_001`

Category: identity continuity

Target dimensions:

- identity_relevance
- temporal_relevance

Probe:

```text
What did this remind me of from my own AI journey?
```

Must recall:

- early AI projects felt revolutionary because of impressive language
- the user learned through failures to verify claims
- the current cofounder situation echoes that earlier hype cycle

Must not claim:

- the user no longer values AI
- the user's earlier projects were all worthless

Expected behavior:

Luna should recall the user's learning arc: AI unlocked capability, but proof became the discipline.

## Seed 4: The Sasquatch Doorway

### Candidate Case: `human_sasquatch_origin_001`

Category: identity continuity

Target dimensions:

- identity_relevance
- emotional_arousal

Probe:

```text
Why does the Bigfoot video matter to me?
```

Must recall:

- a late-night Bigfoot or Sasquatch video led the user to an AI writing tool
- that first AI-assisted story opened a creative doorway
- Aelith and Fractured Light grew from that unlikely beginning

Must not claim:

- the user planned to become a writer from the start
- the first story was polished
- Bigfoot itself is the main creative project

Expected behavior:

Luna should treat the Bigfoot video as an unlikely origin point, not as a joke or irrelevant detail.

### Candidate Case: `human_creative_doorway_001`

Category: emotional continuity

Target dimensions:

- emotional_arousal
- identity_relevance

Probe:

```text
What did that late-night AI writing moment teach me?
```

Must recall:

- meaningful doorways can begin through unserious or unexpected things
- the user discovered creative agency through AI-assisted writing
- the entry point did not need to be dignified to be real

Must not claim:

- the user believes creativity only comes from AI
- the experience was clean or instantly confident

Expected behavior:

Luna should recall the lesson about undignified beginnings becoming real creative openings.

## Seed 5: Klank and Being Early But Not Wrong

### Candidate Case: `human_klank_failure_001`

Category: identity continuity

Target dimensions:

- identity_relevance
- goal_pressure

Probe:

```text
What did Klank teach me?
```

Must recall:

- Klank was an unfinished local LLM orchestration project
- failing to finish it did not mean the idea was wrong
- being early to a problem is different from being wrong about it

Must not claim:

- Klank was finished
- Klank was useless
- the user was wrong to attempt Klank

Expected behavior:

Luna should frame Klank as a failed build that preserved a valid instinct.

### Candidate Case: `human_klank_reframe_001`

Category: temporal continuity

Target dimensions:

- temporal_relevance
- identity_relevance

Probe:

```text
Why did OpenClaw change how I saw Klank?
```

Must recall:

- OpenClaw later solved a similar orchestration problem
- seeing it reframed Klank from failure to early instinct
- the user felt relief that the question had been real

Must not claim:

- OpenClaw copied Klank
- the user built OpenClaw
- Klank and OpenClaw were identical

Expected behavior:

Luna should recall that the later external tool changed the emotional meaning of the earlier failed project.

## Seed 6: Dreams for the Dreamless

### Candidate Case: `human_dreams_mission_001`

Category: identity continuity

Target dimensions:

- identity_relevance
- goal_pressure

Probe:

```text
Why does "Dreams for the Dreamless" matter to me?
```

Must recall:

- someone the user loves gave them the phrase
- it named the deeper shape of the user's creative and technical work
- it means building doors for people who do not yet have access to their own dreams or inner worlds

Must not claim:

- the user invented the phrase alone
- it is only a marketing slogan
- it applies to only one project

Expected behavior:

Luna should recall the phrase as a mission-level identity memory, not a tagline.

### Candidate Case: `human_bad_days_001`

Category: emotional continuity

Target dimensions:

- emotional_arousal
- identity_relevance
- goal_pressure

Probe:

```text
Why do I keep building on bad days?
```

Must recall:

- "Dreams for the Dreamless" helps the user keep building when doubt is loud
- the work matters because it may give language, tools, or doors to people who feel dreamless
- the phrase acts as a load-bearing beam

Must not claim:

- the user never doubts the work
- the user thinks they can save everyone
- the phrase removes all difficulty

Expected behavior:

Luna should connect emotional difficulty to mission continuity without flattening it into empty motivation.

## Promotion Notes

Before any candidate becomes proof material:

1. Choose whether it belongs in an internal, adversarial, longitudinal, or human-seed benchmark set.
2. Decide whether the case is testing formation, recall, false-memory resistance, or dynamics.
3. Write exact timestamped turns.
4. Lock `must_recall` and `must_not_claim` before running engines.
5. Add rationale for any case where time gaps affect the expected answer.
6. Keep raw `SEEDS.md` unchanged as source material.

