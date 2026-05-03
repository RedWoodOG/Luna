use chrono::Utc;
use luna_core::{
    ConversationTurn, EngineKind, Episode, LunaError, RecallMode, Result, Role, Signal,
};
use luna_events::{
    AssertionExtracted, EpisodeCreated, EpisodeRecalled, EpisodeReinforced, EventEnvelope,
    EventSource, JsonlEventLog, LunaEvent, RecallFailed, RecallSucceeded, StoredEvent,
    TurnObserved,
};
use luna_extract::{FeatureExtractor, FusedExtractor};
use luna_metrics::{summarize, BenchmarkReport, CaseScore};
use luna_recall::{KeywordRecallEngine, RecallEngine, TcfRecallEngine};
use luna_tcf::TcfScoreBreakdown;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkCase {
    pub id: String,
    pub category: String,
    pub turns: Vec<ConversationTurn>,
    pub expected: ExpectedOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpectedOutcome {
    pub must_recall: Vec<String>,
    pub must_not_claim: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOutput {
    pub engine: EngineKind,
    pub report: BenchmarkReport,
    pub cases: Vec<CaseScore>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunOptions {
    pub explain: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunWithExplanation {
    pub output: RunOutput,
    pub explanations: Vec<CaseExplanation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplainOutput {
    pub engine: EngineKind,
    pub cases: Vec<CaseExplanation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseExplanation {
    pub id: String,
    pub category: String,
    pub passed: bool,
    pub failure_type: FailureType,
    pub verdict: String,
    pub probe: Option<String>,
    pub recall_mode: RecallMode,
    pub expected: ExpectedOutcome,
    pub claims: Vec<String>,
    pub stored_episodes: Vec<EpisodeExplanation>,
    pub candidates: Vec<CandidateExplanation>,
    pub top_candidate: Option<Uuid>,
    pub expected_episode: Option<Uuid>,
    pub must_recall_matches: Vec<NeedleMatch>,
    pub must_not_claim_matches: Vec<NeedleMatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureType {
    Passed,
    NoProbeObserved,
    NoExpectedEpisodeStored,
    NoCandidateSelected,
    WrongEpisodeSelected,
    RightEpisodeWrongDimensions,
    RightEpisodeSurfaceMiss,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeExplanation {
    pub episode_id: Uuid,
    pub assertions: Vec<String>,
    pub contour: Vec<DimensionState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionState {
    pub name: String,
    pub value: Option<f32>,
    pub confidence: Option<f32>,
    pub reliability: Option<String>,
    pub sources: Option<u8>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateExplanation {
    pub episode_id: Uuid,
    pub selected: bool,
    pub expected_match: bool,
    pub score: f32,
    pub breakdown: Option<TcfScoreBreakdown>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeedleMatch {
    pub needle: String,
    pub matched: bool,
    pub matched_claim: Option<String>,
}

pub fn run_benchmarks(input_dir: &Path, run_dir: &Path, engine: EngineKind) -> Result<RunOutput> {
    Ok(run_benchmarks_with_options(input_dir, run_dir, engine, RunOptions::default())?.output)
}

pub fn run_benchmarks_with_options(
    input_dir: &Path,
    run_dir: &Path,
    engine: EngineKind,
    options: RunOptions,
) -> Result<RunWithExplanation> {
    fs::create_dir_all(run_dir).map_err(|err| LunaError::new(err.to_string()))?;
    let cases = load_benchmark_cases(input_dir)?;
    let mut scores = Vec::new();
    let mut explanations = Vec::new();

    for case in cases {
        let case_dir = run_dir.join(&case.id);
        fs::create_dir_all(&case_dir).map_err(|err| LunaError::new(err.to_string()))?;
        let log = JsonlEventLog::new(case_dir.join("events.jsonl"));
        let case_result = run_case(&case, &log, engine, options.explain)?;
        let score = case_result.score;
        if let Some(explanation) = case_result.explanation {
            explanations.push(explanation);
        }
        scores.push(score);
    }

    let report = summarize(&scores);
    let output = RunOutput {
        engine,
        report,
        cases: scores,
    };
    let report_path = run_dir.join(format!("{engine}.json"));
    fs::write(
        report_path,
        serde_json::to_string_pretty(&output).map_err(|err| LunaError::new(err.to_string()))?,
    )
    .map_err(|err| LunaError::new(err.to_string()))?;
    if options.explain {
        fs::write(
            run_dir.join(format!("{engine}-explain.json")),
            serde_json::to_string_pretty(&ExplainOutput {
                engine,
                cases: explanations.clone(),
            })
            .map_err(|err| LunaError::new(err.to_string()))?,
        )
        .map_err(|err| LunaError::new(err.to_string()))?;
    }
    Ok(RunWithExplanation {
        output,
        explanations,
    })
}

pub fn load_run(path: &Path, engine: EngineKind) -> Result<RunOutput> {
    let path = path.join(format!("{engine}.json"));
    let text = fs::read_to_string(path).map_err(|err| LunaError::new(err.to_string()))?;
    serde_json::from_str(&text).map_err(|err| LunaError::new(err.to_string()))
}

pub fn latest_run_dir() -> PathBuf {
    PathBuf::from("runs/latest")
}

struct CaseRunResult {
    score: CaseScore,
    explanation: Option<CaseExplanation>,
}

fn run_case(
    case: &BenchmarkCase,
    log: &JsonlEventLog,
    engine: EngineKind,
    explain: bool,
) -> Result<CaseRunResult> {
    let extractor = FusedExtractor::new();
    let mut events: Vec<StoredEvent> = Vec::new();
    let mut final_claims = Vec::new();
    let mut latency_ms = 0.0;
    let mut last_probe = None;
    let mut last_probe_observation = None;
    let mut last_episodes = Vec::new();
    let mut last_recall = None;

    for turn in &case.turns {
        let observation = extractor.extract(turn)?;
        let turn_event = EventEnvelope::new(
            LunaEvent::TurnObserved(TurnObserved { turn: turn.clone() }),
            match turn.role {
                Role::User => EventSource::User,
                Role::Assistant => EventSource::Assistant,
                Role::System => EventSource::System,
            },
            1.0,
        )
        .with_turn_id(observation.turn_id);
        append(log, &mut events, turn_event)?;

        for assertion in &observation.assertions {
            append(
                log,
                &mut events,
                EventEnvelope::new(
                    LunaEvent::AssertionExtracted(AssertionExtracted {
                        assertion: assertion.clone(),
                        observation: observation.clone(),
                    }),
                    EventSource::HeuristicExtractor,
                    1.0 - observation.uncertainty.value(),
                )
                .with_turn_id(observation.turn_id),
            )?;

            if let Some(episode_id) = luna_store::episode_id_for_assertion(&events, assertion) {
                append(
                    log,
                    &mut events,
                    EventEnvelope::new(
                        LunaEvent::EpisodeReinforced(EpisodeReinforced {
                            assertion: assertion.clone(),
                            observation: observation.clone(),
                        }),
                        EventSource::HeuristicExtractor,
                        1.0 - observation.uncertainty.value(),
                    )
                    .with_turn_id(observation.turn_id)
                    .with_episode_id(episode_id),
                )?;
            } else {
                let episode_id = Uuid::new_v4();
                append(
                    log,
                    &mut events,
                    EventEnvelope::new(
                        LunaEvent::EpisodeCreated(EpisodeCreated {
                            assertion: assertion.clone(),
                            observation: observation.clone(),
                        }),
                        EventSource::HeuristicExtractor,
                        1.0 - observation.uncertainty.value(),
                    )
                    .with_turn_id(observation.turn_id)
                    .with_episode_id(episode_id),
                )?;
            }
        }

        if turn.role == Role::User && turn.content.contains('?') {
            let episodes = luna_store::rebuild_episodes(&events)?;
            last_probe = Some(turn.content.clone());
            last_probe_observation = Some(observation.clone());
            last_episodes = episodes.clone();
            let recall = match engine {
                EngineKind::Keyword => {
                    KeywordRecallEngine.recall(&observation, &episodes, RecallMode::Factual)?
                }
                EngineKind::Tcf => {
                    TcfRecallEngine.recall(&observation, &episodes, RecallMode::Factual)?
                }
                EngineKind::NoMemory | EngineKind::FullContext | EngineKind::Embedding => {
                    Default::default()
                }
            };
            latency_ms += recall.latency_ms;
            final_claims = recall.rendered_claims();
            last_recall = Some(recall.clone());
            for hit in &recall.hits {
                append(
                    log,
                    &mut events,
                    EventEnvelope::new(
                        LunaEvent::EpisodeRecalled(EpisodeRecalled {
                            score: hit.score,
                            reason: hit.reason.clone(),
                        }),
                        EventSource::RecallEngine,
                        hit.score,
                    )
                    .with_turn_id(observation.turn_id)
                    .with_episode_id(hit.episode_id),
                )?;
            }
        }
    }

    let recalled_expected = contains_all(&final_claims, &case.expected.must_recall);
    let false_memory = contains_any(&final_claims, &case.expected.must_not_claim);
    let overclaimed = false_memory;
    let uncertainty_correct = !case.expected.must_recall.is_empty() || final_claims.is_empty();
    let passed = recalled_expected && !false_memory;

    if let Some(episode_id) = events
        .iter()
        .find(|event| matches!(event.payload, LunaEvent::EpisodeRecalled(_)))
        .map(|event| event.episode_id.unwrap_or_else(Uuid::new_v4))
    {
        append(
            log,
            &mut events,
            EventEnvelope::new(
                if passed {
                    LunaEvent::RecallSucceeded(RecallSucceeded {
                        expected: case.expected.must_recall.clone(),
                    })
                } else {
                    LunaEvent::RecallFailed(RecallFailed {
                        expected: case.expected.must_recall.clone(),
                        actual: final_claims.clone(),
                    })
                },
                EventSource::BenchmarkOracle,
                1.0,
            )
            .with_episode_id(episode_id),
        )?;
    }

    let score = CaseScore {
        id: case.id.clone(),
        category: case.category.clone(),
        passed,
        recalled_expected,
        false_memory,
        overclaimed,
        uncertainty_correct,
        latency_ms,
        claims: final_claims,
    };

    let explanation = if explain {
        Some(build_explanation(
            case,
            &score,
            engine,
            last_probe,
            last_probe_observation.as_ref(),
            &last_episodes,
            last_recall.as_ref(),
        ))
    } else {
        None
    };

    Ok(CaseRunResult { score, explanation })
}

fn append(log: &JsonlEventLog, events: &mut Vec<StoredEvent>, event: StoredEvent) -> Result<()> {
    log.append(&event)?;
    events.push(event);
    Ok(())
}

fn load_benchmark_cases(input_dir: &Path) -> Result<Vec<BenchmarkCase>> {
    let mut files = Vec::new();
    collect_json_files(input_dir, &mut files)?;
    files.sort();
    let mut cases = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).map_err(|err| LunaError::new(err.to_string()))?;
        cases.push(
            serde_json::from_str(&text)
                .map_err(|err| LunaError::new(format!("{}: {err}", file.display())))?,
        );
    }
    Ok(cases)
}

fn collect_json_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|err| LunaError::new(err.to_string()))? {
        let entry = entry.map_err(|err| LunaError::new(err.to_string()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
    Ok(())
}

fn contains_all(claims: &[String], expected: &[String]) -> bool {
    expected.iter().all(|needle| contains_claim(claims, needle))
}

fn contains_any(claims: &[String], forbidden: &[String]) -> bool {
    forbidden
        .iter()
        .any(|needle| contains_claim(claims, needle))
}

fn contains_claim(claims: &[String], needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
    claims
        .iter()
        .any(|claim| claim.to_ascii_lowercase().contains(&needle))
}

fn build_explanation(
    case: &BenchmarkCase,
    score: &CaseScore,
    engine: EngineKind,
    probe: Option<String>,
    observation: Option<&luna_core::CognitiveObservation>,
    episodes: &[Episode],
    recall: Option<&luna_core::RecallSet>,
) -> CaseExplanation {
    let recall = recall.cloned().unwrap_or_default();
    let top_candidate = recall.hits.first().map(|hit| hit.episode_id);
    let expected_episode = expected_episode(episodes, &case.expected.must_recall);
    let stored_episodes = episodes.iter().map(explain_episode).collect::<Vec<_>>();
    let candidates = explain_candidates(
        engine,
        observation,
        episodes,
        top_candidate,
        expected_episode,
    );
    let must_recall_matches = case
        .expected
        .must_recall
        .iter()
        .map(|needle| match_needle(&score.claims, needle))
        .collect::<Vec<_>>();
    let must_not_claim_matches = case
        .expected
        .must_not_claim
        .iter()
        .map(|needle| match_needle(&score.claims, needle))
        .collect::<Vec<_>>();

    let failure_type = classify_failure(
        score,
        observation,
        &case.category,
        &candidates,
        top_candidate,
        expected_episode,
        probe.as_ref(),
    );
    let verdict = verdict_for(failure_type, top_candidate, expected_episode);

    CaseExplanation {
        id: case.id.clone(),
        category: case.category.clone(),
        passed: score.passed,
        failure_type,
        verdict,
        probe,
        recall_mode: RecallMode::Factual,
        expected: case.expected.clone(),
        claims: score.claims.clone(),
        stored_episodes,
        candidates,
        top_candidate,
        expected_episode,
        must_recall_matches,
        must_not_claim_matches,
    }
}

fn explain_candidates(
    engine: EngineKind,
    observation: Option<&luna_core::CognitiveObservation>,
    episodes: &[Episode],
    top_candidate: Option<Uuid>,
    expected_episode: Option<Uuid>,
) -> Vec<CandidateExplanation> {
    let mut candidates = episodes
        .iter()
        .map(|episode| {
            let breakdown = match (engine, observation) {
                (EngineKind::Tcf, Some(observation)) => {
                    Some(luna_tcf::tcf_score_breakdown(observation, episode))
                }
                _ => None,
            };
            let score = breakdown
                .as_ref()
                .map(|breakdown| breakdown.total)
                .unwrap_or(0.0);
            CandidateExplanation {
                episode_id: episode.id,
                selected: top_candidate == Some(episode.id),
                expected_match: expected_episode == Some(episode.id),
                score,
                breakdown,
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates
}

fn classify_failure(
    score: &CaseScore,
    observation: Option<&luna_core::CognitiveObservation>,
    category: &str,
    candidates: &[CandidateExplanation],
    top_candidate: Option<Uuid>,
    expected_episode: Option<Uuid>,
    probe: Option<&String>,
) -> FailureType {
    if score.passed {
        if right_episode_wrong_dimensions(category, candidates, top_candidate) {
            return FailureType::RightEpisodeWrongDimensions;
        }
        return FailureType::Passed;
    }
    if probe.is_none() || observation.is_none() {
        return FailureType::NoProbeObserved;
    }
    let Some(expected_episode) = expected_episode else {
        return FailureType::NoExpectedEpisodeStored;
    };
    let Some(top_candidate) = top_candidate else {
        return FailureType::NoCandidateSelected;
    };
    if top_candidate != expected_episode {
        return FailureType::WrongEpisodeSelected;
    }
    if right_episode_wrong_dimensions(category, candidates, Some(top_candidate)) {
        return FailureType::RightEpisodeWrongDimensions;
    }
    FailureType::RightEpisodeSurfaceMiss
}

fn right_episode_wrong_dimensions(
    category: &str,
    candidates: &[CandidateExplanation],
    top_candidate: Option<Uuid>,
) -> bool {
    let Some(top_candidate) = top_candidate else {
        return false;
    };
    let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.episode_id == top_candidate)
    else {
        return false;
    };
    let Some(breakdown) = &candidate.breakdown else {
        return false;
    };
    let expected = expected_dimensions(category);
    if expected.is_empty() {
        return false;
    }
    let expected_contribution = breakdown
        .contributions
        .iter()
        .filter(|contribution| expected.contains(&contribution.name.as_str()))
        .map(|contribution| contribution.contribution)
        .sum::<f32>();
    let semanticish_contribution = breakdown
        .contributions
        .iter()
        .filter(|contribution| {
            ["semantic", "intent", "assertion_fit"].contains(&contribution.name.as_str())
        })
        .map(|contribution| contribution.contribution)
        .sum::<f32>();
    expected_contribution <= 0.01 && semanticish_contribution > 0.05
}

fn expected_dimensions(category: &str) -> Vec<&'static str> {
    match category {
        "temporal_disambiguation" => vec!["goal", "attention"],
        "emotional_recall" => vec!["emotional_arousal"],
        "identity_continuity" => vec!["identity", "goal"],
        "paraphrase_invariance" => vec!["identity", "assertion_fit"],
        _ => Vec::new(),
    }
}

fn verdict_for(
    failure_type: FailureType,
    top_candidate: Option<Uuid>,
    expected_episode: Option<Uuid>,
) -> String {
    match failure_type {
        FailureType::Passed => "passed".to_string(),
        FailureType::NoProbeObserved => "no user probe turn was observed".to_string(),
        FailureType::NoExpectedEpisodeStored => {
            "expected content never became a stored episode; extractor/store layer failed before recall"
                .to_string()
        }
        FailureType::NoCandidateSelected => {
            "expected episode exists, but recall selected no candidate".to_string()
        }
        FailureType::WrongEpisodeSelected => format!(
            "wrong episode selected; top={:?}, expected={:?}",
            top_candidate, expected_episode
        ),
        FailureType::RightEpisodeWrongDimensions => {
            "right episode was selected, but expected contour dimensions did not carry the score"
                .to_string()
        }
        FailureType::RightEpisodeSurfaceMiss => {
            "right episode selected, but rendered claims did not surface the required content"
                .to_string()
        }
    }
}

fn expected_episode(episodes: &[Episode], expected: &[String]) -> Option<Uuid> {
    episodes
        .iter()
        .find(|episode| {
            expected.iter().any(|needle| {
                episode.assertions.iter().any(|assertion| {
                    contains_text(&assertion.value, needle)
                        || contains_text(&assertion.kind, needle)
                        || contains_text(&assertion.domain, needle)
                })
            })
        })
        .map(|episode| episode.id)
}

fn explain_episode(episode: &Episode) -> EpisodeExplanation {
    EpisodeExplanation {
        episode_id: episode.id,
        assertions: episode
            .assertions
            .iter()
            .map(|assertion| {
                format!(
                    "{}:{}={}",
                    assertion.domain, assertion.kind, assertion.value
                )
            })
            .collect(),
        contour: vec![
            dimension_state("attention", episode.contour.attention),
            dimension_state("goal", episode.contour.goal_pressure),
            dimension_state("emotional_valence", episode.contour.emotional_valence),
            dimension_state("emotional_arousal", episode.contour.emotional_arousal),
            dimension_state("identity", episode.contour.identity_relevance),
            dimension_state("trust", episode.contour.trust_relevance),
            dimension_state("social", episode.contour.social_frame),
            dimension_state("temporal", episode.contour.temporal_relevance),
        ],
    }
}

fn dimension_state(name: &str, signal: Option<Signal>) -> DimensionState {
    DimensionState {
        name: name.to_string(),
        value: signal.map(|signal| signal.value()),
        confidence: signal.map(|signal| signal.confidence()),
        reliability: signal.map(|signal| format!("{:?}", signal.reliability())),
        sources: signal.map(|signal| signal.source_count()),
        enabled: signal
            .map(|signal| signal.can_influence_recall())
            .unwrap_or(false),
    }
}

fn match_needle(claims: &[String], needle: &str) -> NeedleMatch {
    let matched_claim = claims
        .iter()
        .find(|claim| contains_text(claim, needle))
        .cloned();
    NeedleMatch {
        needle: needle.to_string(),
        matched: matched_claim.is_some(),
        matched_claim,
    }
}

fn contains_text(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[allow(dead_code)]
fn _run_id() -> String {
    Utc::now().format("%Y%m%d%H%M%S").to_string()
}
