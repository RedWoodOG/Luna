use luna_core::{AssertionLifecycleStatus, Episode, RecallRecord, Result, StructuredAssertion};
use luna_events::{LunaEvent, StoredEvent};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const STALE_FORGOTTEN_RISK_THRESHOLD: f32 = 0.80;
const ARCHIVE_FORGOTTEN_RISK_THRESHOLD: f32 = 0.95;

pub fn rebuild_episodes(events: &[StoredEvent]) -> Result<Vec<Episode>> {
    let mut episodes: BTreeMap<Uuid, Episode> = BTreeMap::new();
    let mut assertion_index: BTreeMap<String, Uuid> = BTreeMap::new();
    let mut contradiction_pressure_seen = BTreeSet::new();

    for event in events {
        match &event.payload {
            LunaEvent::EpisodeCreated(payload) => {
                let episode_id = event.episode_id.unwrap_or_else(Uuid::new_v4);
                assertion_index.insert(payload.assertion.key(), episode_id);
                let profile = luna_match::profile_from_reading(&payload.observation);
                let confidence = event.confidence;
                let coherence_score = luna_match::coherence_score(&profile, confidence, 0.0);
                episodes.insert(
                    episode_id,
                    Episode {
                        id: episode_id,
                        created_at: event.timestamp,
                        updated_at: event.timestamp,
                        profile,
                        confidence,
                        coherence_score,
                        assertions: vec![payload.assertion.clone()],
                        recall_history: Vec::new(),
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
                    luna_match::reinforce_profile(&mut episode.profile, &payload.observation);
                    episode.confidence =
                        (episode.confidence + event.confidence * 0.18).clamp(0.0, 1.0);
                    episode.updated_at = event.timestamp;
                    episode.coherence_score = luna_match::coherence_score(
                        &episode.profile,
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
                    episode.profile.successful_recall_count += 1;
                    if let Some(record) = episode.recall_history.last_mut() {
                        record.succeeded = Some(true);
                    }
                    episode.coherence_score = luna_match::coherence_score(
                        &episode.profile,
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
                    episode.profile.failed_recall_count += 1;
                    if let Some(record) = episode.recall_history.last_mut() {
                        record.succeeded = Some(false);
                    }
                    episode.coherence_score = luna_match::coherence_score(
                        &episode.profile,
                        episode.confidence,
                        episode.forgotten_risk,
                    );
                }
            }
            LunaEvent::ContradictionDetected(payload) => {
                if let Some(episode) = event
                    .episode_id
                    .and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    episode.profile.contradiction_count += 1;
                    episode.confidence = (episode.confidence - 0.22).clamp(0.0, 1.0);
                    episode.updated_at = event.timestamp;
                    episode.coherence_score = luna_match::coherence_score(
                        &episode.profile,
                        episode.confidence,
                        episode.forgotten_risk,
                    );
                    contradiction_pressure_seen.insert(correction_pressure_key(
                        episode.id,
                        &payload.left,
                        &payload.right,
                    ));
                }
            }
            LunaEvent::AssertionCorrected(payload) => {
                if let Some(episode) = event
                    .episode_id
                    .and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    let pressure_key = correction_pressure_key(
                        episode.id,
                        &payload.old_assertion,
                        &payload.new_assertion,
                    );
                    if contradiction_pressure_seen.insert(pressure_key) {
                        episode.profile.contradiction_count += 1;
                    }
                    episode.confidence = event.confidence.max(episode.confidence - 0.10);
                    episode.updated_at = event.timestamp;
                    mark_assertion_lifecycle(
                        &mut episode.assertions,
                        &payload.old_assertion,
                        AssertionLifecycleStatus::Superseded,
                    );
                    merge_assertion(&mut episode.assertions, payload.new_assertion.clone());
                    assertion_index.insert(payload.new_assertion.key(), episode.id);
                    episode.coherence_score = luna_match::coherence_score(
                        &episode.profile,
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
                    episode.updated_at = event.timestamp;
                    if episode.forgotten_risk >= ARCHIVE_FORGOTTEN_RISK_THRESHOLD {
                        mark_recallable_assertions_archived(&mut episode.assertions);
                    } else if episode.forgotten_risk >= STALE_FORGOTTEN_RISK_THRESHOLD {
                        mark_current_assertions_stale(&mut episode.assertions);
                    }
                    episode.coherence_score = luna_match::coherence_score(
                        &episode.profile,
                        episode.confidence,
                        episode.forgotten_risk,
                    );
                }
            }
            LunaEvent::TurnObserved(_)
            | LunaEvent::MemoryIntakeDecided(_)
            | LunaEvent::AssertionExtracted(_)
            | LunaEvent::MemoryRepairRecorded(_)
            | LunaEvent::TopologyBridgeCommitted(_)
            | LunaEvent::RuntimeTurnReceipted(_)
            | LunaEvent::LatticeComputed(_) => {}
        }
    }

    Ok(episodes.into_values().collect())
}

fn correction_pressure_key(
    episode_id: Uuid,
    old_assertion: &StructuredAssertion,
    new_assertion: &StructuredAssertion,
) -> String {
    format!(
        "{episode_id}:{}->{}",
        old_assertion.key(),
        new_assertion.key()
    )
}

fn mark_assertion_lifecycle(
    assertions: &mut [StructuredAssertion],
    target: &StructuredAssertion,
    status: AssertionLifecycleStatus,
) {
    if let Some(assertion) = assertions
        .iter_mut()
        .find(|assertion| assertion.key() == target.key())
    {
        assertion.lifecycle_status = status;
    }
}

fn mark_current_assertions_stale(assertions: &mut [StructuredAssertion]) {
    for assertion in assertions {
        if assertion.lifecycle_status == AssertionLifecycleStatus::Current {
            assertion.lifecycle_status = AssertionLifecycleStatus::Stale;
        }
    }
}

fn mark_recallable_assertions_archived(assertions: &mut [StructuredAssertion]) {
    for assertion in assertions {
        if matches!(
            assertion.lifecycle_status,
            AssertionLifecycleStatus::Current | AssertionLifecycleStatus::Stale
        ) {
            assertion.lifecycle_status = AssertionLifecycleStatus::Archived;
        }
    }
}

pub fn episode_id_for_assertion(
    events: &[StoredEvent],
    assertion: &StructuredAssertion,
) -> Option<Uuid> {
    events.iter().find_map(|event| match &event.payload {
        LunaEvent::EpisodeCreated(payload) if payload.assertion.key() == assertion.key() => {
            event.episode_id
        }
        LunaEvent::AssertionCorrected(payload)
            if payload.new_assertion.key() == assertion.key() =>
        {
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
        existing.lifecycle_status = assertion.lifecycle_status;
    } else {
        assertions.push(assertion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use luna_core::{Signal, SignalReliability, TurnReading};
    use luna_events::{
        AssertionCorrected, ContradictionDetected, EpisodeCreated, EpisodeDecayed, EventEnvelope,
        EventSource,
    };

    #[test]
    fn contradiction_and_correction_rebuilds_are_deterministic() {
        let episode_id = Uuid::from_u128(1);
        let turn_id = Uuid::from_u128(2);
        let created_at = Utc.with_ymd_and_hms(2026, 5, 9, 10, 0, 0).unwrap();
        let contradicted_at = Utc.with_ymd_and_hms(2026, 5, 9, 10, 5, 0).unwrap();
        let corrected_at = Utc.with_ymd_and_hms(2026, 5, 9, 10, 10, 0).unwrap();

        let original = StructuredAssertion::inferred("identity", "location", "Chicago");
        let correction = StructuredAssertion::inferred("identity", "location", "Milwaukee");
        let observation = TurnReading {
            turn_id,
            semantic: Some(vec![0.1, 0.2]),
            intent: Some(vec![0.3, 0.4]),
            attention: Some(Signal::new(0.8, 0.9, SignalReliability::Heuristic)),
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: Some(Signal::new(0.7, 0.8, SignalReliability::Heuristic)),
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: Signal::new(0.2, 0.9, SignalReliability::Heuristic),
            cue_terms: vec!["location".to_string()],
            query_intents: Vec::new(),
            assertions: vec![original.clone()],
        };

        let events = vec![
            event_at(
                LunaEvent::EpisodeCreated(EpisodeCreated {
                    assertion: original.clone(),
                    observation,
                }),
                episode_id,
                created_at,
            ),
            event_at(
                LunaEvent::ContradictionDetected(ContradictionDetected {
                    left: original.clone(),
                    right: correction.clone(),
                }),
                episode_id,
                contradicted_at,
            ),
            event_at(
                LunaEvent::AssertionCorrected(AssertionCorrected {
                    old_assertion: original.clone(),
                    new_assertion: correction.clone(),
                }),
                episode_id,
                corrected_at,
            ),
        ];

        let first = rebuild_episodes(&events).unwrap();
        let second = rebuild_episodes(&events).unwrap();

        assert_eq!(first, second);
        assert_eq!(first[0].updated_at, corrected_at);
        assert_eq!(first[0].profile.contradiction_count, 1);
        assert!(first[0].assertions.iter().any(|assertion| {
            assertion.value == "Chicago"
                && assertion.lifecycle_status == AssertionLifecycleStatus::Superseded
        }));
        assert!(first[0].assertions.iter().any(|assertion| {
            assertion.value == "Milwaukee"
                && assertion.lifecycle_status == AssertionLifecycleStatus::Current
        }));
        assert_eq!(
            episode_id_for_assertion(&events, &correction),
            Some(episode_id)
        );
    }

    #[test]
    fn standalone_correction_event_still_counts_contradiction_pressure() {
        let episode_id = Uuid::from_u128(20);
        let turn_id = Uuid::from_u128(21);
        let created_at = Utc.with_ymd_and_hms(2026, 5, 9, 12, 0, 0).unwrap();
        let corrected_at = Utc.with_ymd_and_hms(2026, 5, 9, 12, 10, 0).unwrap();

        let original = StructuredAssertion::inferred("person", "location", "Chris lives in Iowa");
        let correction = StructuredAssertion::inferred("person", "location", "Chris lives in Ohio");
        let observation = TurnReading {
            turn_id,
            semantic: None,
            intent: None,
            attention: Some(Signal::new(0.8, 0.9, SignalReliability::Heuristic)),
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: Signal::new(0.2, 0.9, SignalReliability::Heuristic),
            cue_terms: vec!["location".to_string()],
            query_intents: Vec::new(),
            assertions: vec![original.clone()],
        };

        let events = vec![
            event_at(
                LunaEvent::EpisodeCreated(EpisodeCreated {
                    assertion: original.clone(),
                    observation,
                }),
                episode_id,
                created_at,
            ),
            event_at(
                LunaEvent::AssertionCorrected(AssertionCorrected {
                    old_assertion: original,
                    new_assertion: correction,
                }),
                episode_id,
                corrected_at,
            ),
        ];

        let episodes = rebuild_episodes(&events).unwrap();

        assert_eq!(episodes[0].profile.contradiction_count, 1);
    }

    #[test]
    fn high_decay_marks_current_assertions_stale() {
        let episode_id = Uuid::from_u128(10);
        let turn_id = Uuid::from_u128(11);
        let created_at = Utc.with_ymd_and_hms(2026, 5, 9, 11, 0, 0).unwrap();
        let decayed_at = Utc.with_ymd_and_hms(2026, 5, 9, 11, 30, 0).unwrap();
        let assertion = StructuredAssertion::inferred("identity", "location", "Chicago");
        let observation = TurnReading {
            turn_id,
            semantic: None,
            intent: None,
            attention: Some(Signal::new(0.8, 0.9, SignalReliability::Heuristic)),
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: Some(Signal::new(0.7, 0.8, SignalReliability::Heuristic)),
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: Signal::new(0.2, 0.9, SignalReliability::Heuristic),
            cue_terms: vec!["location".to_string()],
            query_intents: Vec::new(),
            assertions: vec![assertion.clone()],
        };

        let events = vec![
            event_at(
                LunaEvent::EpisodeCreated(EpisodeCreated {
                    assertion: assertion.clone(),
                    observation,
                }),
                episode_id,
                created_at,
            ),
            event_at(
                LunaEvent::EpisodeDecayed(EpisodeDecayed {
                    forgotten_risk: 0.86,
                }),
                episode_id,
                decayed_at,
            ),
        ];

        let episodes = rebuild_episodes(&events).unwrap();

        assert_eq!(episodes[0].updated_at, decayed_at);
        assert_eq!(
            episodes[0].assertions[0].lifecycle_status,
            AssertionLifecycleStatus::Stale
        );
    }

    #[test]
    fn archive_decay_moves_assertions_to_long_term_without_deleting() {
        let episode_id = Uuid::from_u128(20);
        let turn_id = Uuid::from_u128(21);
        let created_at = Utc.with_ymd_and_hms(2026, 5, 9, 11, 0, 0).unwrap();
        let decayed_at = Utc.with_ymd_and_hms(2026, 5, 9, 12, 0, 0).unwrap();
        let assertion = StructuredAssertion::inferred("person", "location", "Chris lives in Iowa");
        let observation = TurnReading {
            turn_id,
            semantic: None,
            intent: None,
            attention: Some(Signal::new(0.8, 0.9, SignalReliability::Heuristic)),
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            uncertainty: Signal::new(0.2, 0.9, SignalReliability::Heuristic),
            cue_terms: vec!["chris".to_string()],
            query_intents: Vec::new(),
            assertions: vec![assertion.clone()],
        };

        let events = vec![
            event_at(
                LunaEvent::EpisodeCreated(EpisodeCreated {
                    assertion: assertion.clone(),
                    observation,
                }),
                episode_id,
                created_at,
            ),
            event_at(
                LunaEvent::EpisodeDecayed(EpisodeDecayed {
                    forgotten_risk: 0.97,
                }),
                episode_id,
                decayed_at,
            ),
        ];

        let episodes = rebuild_episodes(&events).unwrap();

        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].assertions.len(), 1);
        assert_eq!(episodes[0].assertions[0].value, "Chris lives in Iowa");
        assert_eq!(
            episodes[0].assertions[0].lifecycle_status,
            AssertionLifecycleStatus::Archived
        );
    }

    fn event_at(
        payload: LunaEvent,
        episode_id: Uuid,
        timestamp: chrono::DateTime<Utc>,
    ) -> StoredEvent {
        EventEnvelope {
            event_id: Uuid::new_v4(),
            episode_id: Some(episode_id),
            turn_id: None,
            timestamp,
            source: EventSource::System,
            confidence: 0.9,
            event_hash: None,
            payload,
        }
    }
}
