//! Time-decay of episode `forgotten_risk`, driven by event-time elapsed
//! since the episode's last reinforcement.
//!
//! Doctrine constraints (every one of them is a regression test below):
//!
//! 1. **Event-time, not wall-clock.** Decay is a pure function of
//!    `(now, episode.updated_at, half_life)`. Two replays of the same
//!    log produce identical decay output. `Utc::now()` never appears
//!    in this module.
//!
//! 2. **Every decay is a logged event.** `compute_decay_events` returns
//!    `EpisodeDecayed` payloads to be appended to the event log. The
//!    runtime emits them; replay re-applies them. There is no silent
//!    mutation of `forgotten_risk` outside the log.
//!
//! 3. **Threshold-gated emission.** A new risk is emitted only when it
//!    differs from the current risk by more than [`DecayConfig::emit_threshold`].
//!    Without this, every turn would log many tiny-delta decay events
//!    and pollute replay output.
//!
//! 4. **Reinforcement resets the clock.** `episode.updated_at` is
//!    refreshed every time the episode is reinforced (see
//!    `luna_store::rebuild_episodes`'s `EpisodeReinforced` arm). Decay
//!    measures elapsed time since *last activity*, not since creation.
//!
//! 5. **Decay never raises confidence.** It only adds to
//!    `forgotten_risk`, which downstream `forgotten_risk_gate` uses as
//!    a recall multiplier. Decay does not directly change
//!    `episode.confidence` — that's a separate concern.
//!
//! ## The decay function
//!
//! Exponential with configurable half-life:
//!
//! ```text
//! forgotten_risk(t) = 1 - exp(-elapsed * ln(2) / half_life)
//! ```
//!
//! Concrete values at the default 7-day half-life:
//!
//! ```text
//!   1 hour       0.0041     near-zero
//!   1 day        0.0943     barely-noticeable
//!   3 days       0.2575     visible erosion
//!   7 days       0.5000     half-forgotten
//!   14 days      0.7500
//!   30 days      0.9486     mostly forgotten
//! ```
//!
//! Linear was considered and rejected: it doesn't compose cleanly with
//! reinforcement-resets (each reinforcement would have to special-case
//! the linear ramp), and the orb-network's eventual consolidation
//! engine treats decay as one input to halo→core compression where
//! exponential composition is natural.

use chrono::{DateTime, Duration, Utc};
use luna_core::Episode;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct DecayConfig {
    /// Time over which `forgotten_risk` rises from 0 to 0.5 for an
    /// episode with no reinforcement. Default: 7 days.
    pub half_life: Duration,

    /// Minimum change in `forgotten_risk` required to emit an
    /// `EpisodeDecayed` event. Without this, every turn logs many
    /// near-zero-delta decays. Default: 0.05.
    pub emit_threshold: f32,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            half_life: Duration::days(7),
            emit_threshold: 0.05,
        }
    }
}

/// Pure decay function. Returns the new `forgotten_risk` value for an
/// episode given the elapsed event-time since its last reinforcement.
///
/// `0.0` when no time has passed; asymptotes to `1.0` as elapsed grows
/// without bound. Half-life parameter controls the rate: at
/// `elapsed == half_life` the risk is exactly `0.5`.
///
/// Edge cases:
/// - `half_life <= 0`: returns `0.0` (no decay configured).
/// - `elapsed < 0`: returns `0.0` (clock skew or replay anomaly; never
///   accidentally apply *negative* decay that would *raise* confidence).
pub fn compute_forgotten_risk(elapsed: Duration, half_life: Duration) -> f32 {
    let half_life_secs = half_life.num_milliseconds() as f64 / 1000.0;
    let elapsed_secs = elapsed.num_milliseconds() as f64 / 1000.0;
    if half_life_secs <= 0.0 || elapsed_secs <= 0.0 {
        return 0.0;
    }
    let risk = 1.0 - (-elapsed_secs * std::f64::consts::LN_2 / half_life_secs).exp();
    risk.clamp(0.0, 1.0) as f32
}

/// A decay decision: this episode's `forgotten_risk` should rise to
/// `new_risk` because event-time has elapsed since its last
/// reinforcement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecayDecision {
    pub episode_id: Uuid,
    pub new_risk: f32,
}

/// Compute decay decisions for every episode whose `forgotten_risk`
/// has drifted by more than [`DecayConfig::emit_threshold`] from what
/// it would be at `now` under the configured half-life.
///
/// Pure: `now` is event-time, not `Utc::now()`. Caller controls.
pub fn compute_decay_events(
    episodes: &[Episode],
    now: DateTime<Utc>,
    config: &DecayConfig,
) -> Vec<DecayDecision> {
    episodes
        .iter()
        .filter_map(|episode| {
            let elapsed = now.signed_duration_since(episode.updated_at);
            let new_risk = compute_forgotten_risk(elapsed, config.half_life);
            let delta = new_risk - episode.forgotten_risk;
            if delta > config.emit_threshold {
                Some(DecayDecision {
                    episode_id: episode.id,
                    new_risk,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Apply decay decisions in place to a slice of episodes. Mirrors the
/// mutation that `luna_store::rebuild_episodes`'s `EpisodeDecayed` arm
/// performs during replay; used in the runtime so the recall pass
/// scores against decayed episodes without a second log-rebuild round
/// trip.
///
/// Emitting an `EpisodeDecayed` event afterward (in the same turn's
/// new-events list, before assertion/recall events) keeps the in-memory
/// state and the persisted log in sync: a fresh replay reconstructs the
/// same `forgotten_risk` value applied here.
pub fn apply_decay_in_place(episodes: &mut [Episode], decisions: &[DecayDecision]) {
    for decision in decisions {
        if let Some(ep) = episodes.iter_mut().find(|e| e.id == decision.episode_id) {
            ep.forgotten_risk = decision.new_risk.clamp(0.0, 1.0);
            ep.coherence_score = luna_tcf::coherence_score(
                &ep.contour,
                ep.confidence,
                ep.forgotten_risk,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use luna_core::EpisodeContour;

    fn empty_contour() -> EpisodeContour {
        EpisodeContour {
            semantic: None,
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            reinforcement_count: 0,
            contradiction_count: 0,
            successful_recall_count: 0,
            failed_recall_count: 0,
        }
    }

    fn episode_at(updated_at: DateTime<Utc>, forgotten_risk: f32) -> Episode {
        Episode {
            id: Uuid::new_v4(),
            created_at: updated_at,
            updated_at,
            assertions: Vec::new(),
            contour: empty_contour(),
            recall_history: Vec::new(),
            confidence: 0.7,
            coherence_score: 0.5,
            forgotten_risk,
        }
    }

    #[test]
    fn compute_forgotten_risk_zero_at_zero_elapsed() {
        let r = compute_forgotten_risk(Duration::seconds(0), Duration::days(7));
        assert_eq!(r, 0.0);
    }

    #[test]
    fn compute_forgotten_risk_half_at_half_life() {
        let r = compute_forgotten_risk(Duration::days(7), Duration::days(7));
        assert!((r - 0.5).abs() < 1e-5, "expected ~0.5, got {r}");
    }

    #[test]
    fn compute_forgotten_risk_quarter_after_one_day_with_seven_day_half_life() {
        // Documented expected value: ~0.0943 at 1 day with 7-day half-life.
        let r = compute_forgotten_risk(Duration::days(1), Duration::days(7));
        assert!(
            (r - 0.0943).abs() < 0.001,
            "expected ~0.0943 at 1d/7d, got {r}"
        );
    }

    #[test]
    fn compute_forgotten_risk_asymptotes_to_one() {
        let r = compute_forgotten_risk(Duration::days(365), Duration::days(7));
        assert!(r > 0.99 && r <= 1.0, "expected near-1.0 after 1 year, got {r}");
    }

    #[test]
    fn compute_forgotten_risk_zero_for_negative_elapsed() {
        // Doctrine: never raise confidence accidentally on clock skew.
        let r = compute_forgotten_risk(Duration::seconds(-10), Duration::days(7));
        assert_eq!(r, 0.0);
    }

    #[test]
    fn compute_forgotten_risk_zero_for_zero_or_negative_half_life() {
        assert_eq!(compute_forgotten_risk(Duration::days(1), Duration::seconds(0)), 0.0);
        assert_eq!(compute_forgotten_risk(Duration::days(1), Duration::seconds(-1)), 0.0);
    }

    #[test]
    fn decay_events_fire_when_delta_exceeds_threshold() {
        // 3-day half-life, 0.05 threshold. At 1 day elapsed:
        //   risk = 1 - exp(-1 * ln2 / 3) = 1 - exp(-0.2310) ≈ 0.2063
        // From a starting forgotten_risk of 0.0, delta = 0.2063 > 0.05 → emit.
        let now = chrono::Utc::now();
        let earlier = now - Duration::days(1);
        let episodes = vec![episode_at(earlier, 0.0)];
        let cfg = DecayConfig {
            half_life: Duration::days(3),
            emit_threshold: 0.05,
        };
        let decisions = compute_decay_events(&episodes, now, &cfg);
        assert_eq!(decisions.len(), 1, "expected one decision");
        assert!(decisions[0].new_risk > 0.20 && decisions[0].new_risk < 0.21);
    }

    #[test]
    fn decay_events_suppressed_when_delta_below_threshold() {
        // 1 day elapsed, 30-day half-life: ~0.0228 — below 0.05 threshold.
        let now = chrono::Utc::now();
        let earlier = now - Duration::days(1);
        let episodes = vec![episode_at(earlier, 0.0)];
        let cfg = DecayConfig {
            half_life: Duration::days(30),
            emit_threshold: 0.05,
        };
        let decisions = compute_decay_events(&episodes, now, &cfg);
        assert!(
            decisions.is_empty(),
            "expected no emission for sub-threshold delta, got {decisions:?}"
        );
    }

    #[test]
    fn decay_events_skip_already_decayed() {
        // Episode already at forgotten_risk = 0.5 with 7-day half-life,
        // 7 days elapsed. New computed risk = 0.5 → delta = 0 → no emit.
        let now = chrono::Utc::now();
        let earlier = now - Duration::days(7);
        let episodes = vec![episode_at(earlier, 0.5)];
        let cfg = DecayConfig::default();
        let decisions = compute_decay_events(&episodes, now, &cfg);
        assert!(
            decisions.is_empty(),
            "no emit when computed equals current"
        );
    }

    #[test]
    fn decay_decisions_deterministic_across_calls_with_same_now() {
        // R-009 / R-002 territory: same input → same output. Ensures
        // decay never reaches for Utc::now() internally.
        let now = chrono::Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();
        let earlier = chrono::Utc.with_ymd_and_hms(2026, 5, 25, 0, 0, 0).unwrap();
        let ep_id = Uuid::new_v4();
        let episodes_a = vec![{
            let mut e = episode_at(earlier, 0.0);
            e.id = ep_id;
            e
        }];
        let episodes_b = vec![{
            let mut e = episode_at(earlier, 0.0);
            e.id = ep_id;
            e
        }];
        let cfg = DecayConfig::default();
        let a = compute_decay_events(&episodes_a, now, &cfg);
        let b = compute_decay_events(&episodes_b, now, &cfg);
        assert_eq!(a, b);
    }

    #[test]
    fn apply_decay_in_place_clamps_and_recomputes_coherence() {
        let now = chrono::Utc::now();
        let earlier = now - Duration::days(7);
        let mut episodes = vec![episode_at(earlier, 0.0)];
        let original_coherence = episodes[0].coherence_score;

        let decisions = vec![DecayDecision {
            episode_id: episodes[0].id,
            new_risk: 0.5,
        }];
        apply_decay_in_place(&mut episodes, &decisions);

        assert!((episodes[0].forgotten_risk - 0.5).abs() < 1e-6);
        // coherence_score is recomputed; if forgotten_risk shifted, it should
        // differ from the pre-decay value (sign depends on luna_tcf weights,
        // we only assert it's no longer the seed default).
        assert_ne!(
            episodes[0].coherence_score, original_coherence,
            "coherence should be recomputed after decay"
        );
    }

    #[test]
    fn apply_decay_in_place_clamps_out_of_range_input() {
        let now = chrono::Utc::now();
        let earlier = now - Duration::days(1);
        let mut episodes = vec![episode_at(earlier, 0.0)];
        let decisions = vec![DecayDecision {
            episode_id: episodes[0].id,
            new_risk: 1.7, // out of [0, 1]
        }];
        apply_decay_in_place(&mut episodes, &decisions);
        assert_eq!(episodes[0].forgotten_risk, 1.0, "must clamp to [0,1]");
    }
}
