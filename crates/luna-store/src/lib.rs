use luna_core::{Episode, RecallRecord, Result, StructuredAssertion};
use luna_events::{LunaEvent, StoredEvent};
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn rebuild_episodes(events: &[StoredEvent]) -> Result<Vec<Episode>> {
    let mut episodes: BTreeMap<Uuid, Episode> = BTreeMap::new();
    let mut assertion_index: BTreeMap<String, Uuid> = BTreeMap::new();

    for event in events {
        match &event.payload {
            LunaEvent::EpisodeCreated(payload) => {
                let episode_id = event.episode_id.unwrap_or_else(Uuid::new_v4);
                assertion_index.insert(payload.assertion.key(), episode_id);
                let contour = luna_tcf::contour_from_observation(&payload.observation);
                let confidence = event.confidence;
                let coherence_score = luna_tcf::coherence_score(&contour, confidence, 0.0);
                episodes.insert(
                    episode_id,
                    Episode {
                        id: episode_id,
                        created_at: event.timestamp,
                        updated_at: event.timestamp,
                        assertions: vec![payload.assertion.clone()],
                        contour,
                        recall_history: Vec::new(),
                        confidence,
                        coherence_score,
                        forgotten_risk: 0.0,
                    },
                );
            }
            LunaEvent::EpisodeReinforced(payload) => {
                let episode_id = event
                    .episode_id
                    .or_else(|| assertion_index.get(&payload.assertion.key()).copied());
                if let Some(episode) =
                    episode_id.and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    luna_tcf::reinforce_contour(&mut episode.contour, &payload.observation);
                    episode.confidence =
                        (episode.confidence + event.confidence * 0.18).clamp(0.0, 1.0);
                    episode.updated_at = event.timestamp;
                    episode.coherence_score = luna_tcf::coherence_score(
                        &episode.contour,
                        episode.confidence,
                        episode.forgotten_risk,
                    );
                    merge_assertion(
                        &mut episode.assertions,
                        payload.assertion.clone().reinforced(),
                    );
                }
            }
            LunaEvent::EpisodeRecalled(payload) => {
                if let Some(episode) = event
                    .episode_id
                    .and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    episode.recall_history.push(RecallRecord {
                        turn_id: event.turn_id.unwrap_or_else(Uuid::new_v4),
                        recalled_at: event.timestamp,
                        score: payload.score,
                        succeeded: None,
                    });
                    episode.updated_at = event.timestamp;
                }
            }
            LunaEvent::RecallSucceeded(_) => {
                if let Some(episode) = event
                    .episode_id
                    .and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    episode.contour.successful_recall_count += 1;
                    if let Some(record) = episode.recall_history.last_mut() {
                        record.succeeded = Some(true);
                    }
                    episode.coherence_score = luna_tcf::coherence_score(
                        &episode.contour,
                        episode.confidence,
                        episode.forgotten_risk,
                    );
                }
            }
            LunaEvent::RecallFailed(_) => {
                if let Some(episode) = event
                    .episode_id
                    .and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    episode.contour.failed_recall_count += 1;
                    if let Some(record) = episode.recall_history.last_mut() {
                        record.succeeded = Some(false);
                    }
                    episode.coherence_score = luna_tcf::coherence_score(
                        &episode.contour,
                        episode.confidence,
                        episode.forgotten_risk,
                    );
                }
            }
            LunaEvent::ContradictionDetected(payload) => {
                // Doctrine: the event carries its own transformation. Replay
                // applies the delta the producer wrote into the payload; it
                // does not compute one. Legacy events deserialize with the
                // historical default (see `legacy_contradiction_delta` in
                // luna-core) so old logs replay identically.
                if let Some(episode) = event
                    .episode_id
                    .and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    episode.contour.contradiction_count += 1;
                    episode.confidence =
                        (episode.confidence + payload.confidence_delta).clamp(0.0, 1.0);
                    episode.updated_at = event.timestamp;
                    episode.coherence_score = luna_tcf::coherence_score(
                        &episode.contour,
                        episode.confidence,
                        episode.forgotten_risk,
                    );
                }
            }
            LunaEvent::AssertionCorrected(payload) => {
                // See `ContradictionDetected` arm above. Same contract: the
                // payload carries the confidence delta; replay does not
                // synthesize one.
                if let Some(episode) = event
                    .episode_id
                    .and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    episode.contour.contradiction_count += 1;
                    episode.confidence =
                        (episode.confidence + payload.confidence_delta).clamp(0.0, 1.0);
                    episode.updated_at = event.timestamp;
                    episode.coherence_score = luna_tcf::coherence_score(
                        &episode.contour,
                        episode.confidence,
                        episode.forgotten_risk,
                    );
                }
            }
            LunaEvent::EpisodeDecayed(payload) => {
                if let Some(episode) = event
                    .episode_id
                    .and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    episode.forgotten_risk = payload.forgotten_risk.clamp(0.0, 1.0);
                    episode.coherence_score = luna_tcf::coherence_score(
                        &episode.contour,
                        episode.confidence,
                        episode.forgotten_risk,
                    );
                }
            }
            // Informational-only events: replay does not derive episode
            // state from them. `RawObservationCaptured` (R-003 closure) is
            // an audit record of pre-normalization extractor output; the
            // post-normalization assertions still produce
            // `AssertionExtracted` / `EpisodeCreated` / `EpisodeReinforced`
            // events, which remain the source of truth. `OrbTetherBound`
            // (pr-1.2) and `NodeMerged` (pr-1.2 / R-005 closure) are
            // audit records too — `NodeMerged` is rederived by
            // `MemoryState::from_episodes` itself, and `OrbTetherBound`
            // becomes consequential when pr-1.6 begins producing it
            // (pr-1.2 lands the vocabulary, not the producer).
            LunaEvent::TurnObserved(_)
            | LunaEvent::AssertionExtracted(_)
            | LunaEvent::RawObservationCaptured(_)
            | LunaEvent::OrbTetherBound(_)
            | LunaEvent::NodeMerged(_) => {}
        }
    }

    Ok(episodes.into_values().collect())
}

pub fn episode_id_for_assertion(
    events: &[StoredEvent],
    assertion: &StructuredAssertion,
) -> Option<Uuid> {
    events.iter().find_map(|event| match &event.payload {
        LunaEvent::EpisodeCreated(payload) if payload.assertion.key() == assertion.key() => {
            event.episode_id
        }
        _ => None,
    })
}

fn merge_assertion(assertions: &mut Vec<StructuredAssertion>, assertion: StructuredAssertion) {
    if let Some(existing) = assertions
        .iter_mut()
        .find(|existing| existing.key() == assertion.key())
    {
        existing.source_count = existing.source_count.max(assertion.source_count);
        existing.reinforcement_count = existing
            .reinforcement_count
            .max(assertion.reinforcement_count);
        existing.confidence_tier = existing.confidence_tier.max(assertion.confidence_tier);
    } else {
        assertions.push(assertion);
    }
}

#[cfg(test)]
mod tests {
    //! Replay determinism gates landed in `pr-1.0a/event-log-hardening`.
    //!
    //! These tests exist to keep three properties true forever:
    //!
    //! 1. `rebuild_episodes` is byte-for-byte deterministic for a given log
    //!    (R-009 gate). Any future PR that introduces non-determinism in
    //!    replay will fail `rebuild_is_deterministic_byte_for_byte`.
    //!
    //! 2. Confidence deltas live in the event payload (R-001). Replay applies
    //!    the carried delta; it does not synthesize one. The legacy `-0.22`
    //!    constant survives only as a serde default for events emitted before
    //!    pr-1.0a, so old logs replay identically.
    //!
    //! 3. `episode.updated_at` reflects event-time, not processing-time
    //!    (R-002). Replays separated by wall-clock time produce identical
    //!    episode state.
    use super::*;
    use chrono::{TimeZone, Utc};
    use luna_core::{
        CognitiveObservation, ContradictionDetected, EpisodeCreated, EpisodeReinforced,
        EventEnvelope, EventSource, LunaEvent, Signal, SignalReliability,
    };

    fn observation() -> CognitiveObservation {
        CognitiveObservation {
            turn_id: Uuid::nil(),
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
            uncertainty: Signal::new(0.5, 0.5, SignalReliability::Heuristic),
            cue_terms: Vec::new(),
            query_intents: Vec::new(),
            assertions: Vec::new(),
        }
    }

    /// Build a stored event with a fixed event-time timestamp. We override
    /// the envelope's default `Utc::now()` to keep tests deterministic.
    fn envelope_at(seconds: i64, episode_id: Uuid, payload: LunaEvent, confidence: f32) -> StoredEvent {
        let mut env = EventEnvelope::new(payload, EventSource::User, confidence)
            .with_episode_id(episode_id);
        env.timestamp = Utc.timestamp_opt(seconds, 0).unwrap();
        env
    }

    fn create_event(seconds: i64, episode_id: Uuid, key: (&str, &str, &str)) -> StoredEvent {
        let payload = LunaEvent::EpisodeCreated(EpisodeCreated {
            assertion: StructuredAssertion::new(key.0, key.1, key.2),
            observation: observation(),
        });
        envelope_at(seconds, episode_id, payload, 0.7)
    }

    fn reinforce_event(seconds: i64, episode_id: Uuid, key: (&str, &str, &str)) -> StoredEvent {
        let payload = LunaEvent::EpisodeReinforced(EpisodeReinforced {
            assertion: StructuredAssertion::new(key.0, key.1, key.2),
            observation: observation(),
        });
        envelope_at(seconds, episode_id, payload, 0.5)
    }

    /// R-009: replay must be a pure function from log to state. Two rebuilds
    /// of the same fixture must serialize byte-for-byte identical.
    #[test]
    fn rebuild_is_deterministic_byte_for_byte() {
        let episode_id = Uuid::new_v4();
        let events = vec![
            create_event(1_000_000_000, episode_id, ("identity", "name", "merrow")),
            reinforce_event(1_000_000_010, episode_id, ("identity", "name", "merrow")),
            envelope_at(
                1_000_000_020,
                episode_id,
                LunaEvent::ContradictionDetected(ContradictionDetected {
                    left: StructuredAssertion::new("identity", "name", "merrow"),
                    right: StructuredAssertion::new("identity", "name", "morrow"),
                    confidence_delta: -0.30,
                }),
                1.0,
            ),
        ];

        let first = rebuild_episodes(&events).expect("first rebuild");
        // Sleep a beat so any rogue Utc::now() drift would be observable.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let second = rebuild_episodes(&events).expect("second rebuild");

        let first_bytes = serde_json::to_vec(&first).expect("serialize first");
        let second_bytes = serde_json::to_vec(&second).expect("serialize second");

        assert_eq!(
            first_bytes, second_bytes,
            "rebuild_episodes must be byte-for-byte deterministic"
        );
        assert_eq!(first, second, "PartialEq must agree with byte equality");
    }

    /// R-001: ContradictionDetected carries its own delta. Replay applies the
    /// payload value, not a hardcoded constant.
    #[test]
    fn contradiction_uses_payload_delta_not_constant() {
        let episode_id = Uuid::new_v4();
        let create = create_event(1_000_000_000, episode_id, ("identity", "name", "merrow"));
        let reinforce = reinforce_event(1_000_000_010, episode_id, ("identity", "name", "merrow"));
        let contradiction = envelope_at(
            1_000_000_020,
            episode_id,
            LunaEvent::ContradictionDetected(ContradictionDetected {
                left: StructuredAssertion::new("identity", "name", "merrow"),
                right: StructuredAssertion::new("identity", "name", "morrow"),
                confidence_delta: -0.50,
            }),
            1.0,
        );

        let pre = rebuild_episodes(&[create.clone(), reinforce.clone()]).expect("pre rebuild");
        let pre_conf = pre[0].confidence;

        let post = rebuild_episodes(&[create, reinforce, contradiction]).expect("post rebuild");
        let post_conf = post[0].confidence;

        let applied = post_conf - pre_conf;
        assert!(
            (applied - (-0.50)).abs() < 1e-5,
            "expected payload delta -0.50; got applied={applied} (legacy default would be -0.22)"
        );
    }

    /// Backward compatibility: events serialized before pr-1.0a do not carry
    /// `confidence_delta`. They must still deserialize and replay using the
    /// historical penalty so existing logs are byte-identical pre/post.
    #[test]
    fn legacy_contradiction_event_replays_with_default_delta() {
        let episode_id = Uuid::new_v4();
        let create = create_event(1_000_000_000, episode_id, ("identity", "name", "merrow"));

        // Hand-built JSON for a legacy ContradictionDetected event payload
        // without `confidence_delta`. This is the format prior to pr-1.0a.
        let legacy_json = serde_json::json!({
            "event_id": Uuid::new_v4(),
            "episode_id": episode_id,
            "turn_id": null,
            "timestamp": "2024-01-01T00:00:00Z",
            "source": "user",
            "confidence": 1.0,
            "payload": {
                "type": "contradiction_detected",
                "data": {
                    "left": {
                        "domain": "identity",
                        "kind": "name",
                        "value": "merrow"
                    },
                    "right": {
                        "domain": "identity",
                        "kind": "name",
                        "value": "morrow"
                    }
                }
            }
        });
        let legacy: StoredEvent =
            serde_json::from_value(legacy_json).expect("legacy event must deserialize");

        let pre = rebuild_episodes(&[create.clone()]).expect("pre rebuild");
        let pre_conf = pre[0].confidence;

        let post = rebuild_episodes(&[create, legacy]).expect("post rebuild");
        let post_conf = post[0].confidence;

        let applied = post_conf - pre_conf;
        assert!(
            (applied - (-0.22)).abs() < 1e-5,
            "legacy event without confidence_delta must default to -0.22; got applied={applied}"
        );
    }

    /// R-002: replay uses event.timestamp, never `Utc::now()`. Replays
    /// separated by wall-clock time produce identical `updated_at`.
    #[test]
    fn updated_at_uses_event_timestamp_not_now() {
        let episode_id = Uuid::new_v4();
        let event_time = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let create = create_event(1_000_000_000, episode_id, ("identity", "name", "merrow"));
        let mut contradiction = envelope_at(
            1_000_000_020,
            episode_id,
            LunaEvent::ContradictionDetected(ContradictionDetected {
                left: StructuredAssertion::new("identity", "name", "merrow"),
                right: StructuredAssertion::new("identity", "name", "morrow"),
                confidence_delta: -0.10,
            }),
            1.0,
        );
        contradiction.timestamp = event_time;

        let first = rebuild_episodes(&[create.clone(), contradiction.clone()])
            .expect("first rebuild");
        std::thread::sleep(std::time::Duration::from_millis(50));
        let second = rebuild_episodes(&[create, contradiction]).expect("second rebuild");

        assert_eq!(
            first[0].updated_at, event_time,
            "updated_at must equal event.timestamp"
        );
        assert_eq!(
            first[0].updated_at, second[0].updated_at,
            "updated_at must not vary with wall-clock time across replays"
        );
    }
}
