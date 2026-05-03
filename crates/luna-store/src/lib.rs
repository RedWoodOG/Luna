use chrono::Utc;
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
                    merge_assertion(&mut episode.assertions, payload.assertion.clone());
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
            LunaEvent::ContradictionDetected(_) | LunaEvent::AssertionCorrected(_) => {
                if let Some(episode) = event
                    .episode_id
                    .and_then(|episode_id| episodes.get_mut(&episode_id))
                {
                    episode.contour.contradiction_count += 1;
                    episode.confidence = (episode.confidence - 0.22).clamp(0.0, 1.0);
                    episode.updated_at = Utc::now();
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
            LunaEvent::TurnObserved(_) | LunaEvent::AssertionExtracted(_) => {}
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
    if !assertions
        .iter()
        .any(|existing| existing.key() == assertion.key())
    {
        assertions.push(assertion);
    }
}
