//! Formation: prove benchmark cases can enter the recall path with
//! valid, provenance-backed episodes — without invoking any recall
//! engine.
//!
//! Each case is run through the v0.1 pipeline (LunaExtractor + the
//! deterministic second sources from PR 0.4) and the resulting events
//! are replayed by [`luna_store::rebuild_episodes`] into the same
//! Episode shape recall would consume. The replayed episodes are then
//! checked against a fixed list of gates. No recall engine is called;
//! no scoring happens. Formation is the upstream of recall.
//!
//! Cache-hit-rate accounting requires the backend to be wrapped in
//! [`luna_extract::CountingBackend`] so [`run_formation`] can observe
//! how many backend calls the second pass consumed. This is a
//! structural requirement: every formation run reports the metric.
//!
//! Failure types intentionally mirror the spec the user committed to
//! before formation was implemented:
//!
//! ```text
//! NoExpectedEpisodeStored
//! MustRecallMissing
//! MustRecallAmbiguous
//! ForbiddenAssertionStored
//! TargetDimensionMissing
//! TargetDimensionSingleSource
//! NoProbeObserved
//! ExtractionFailed
//! ```
//!
//! Schema-violating LLM output is caught upstream by
//! [`luna_extract::validate_against_prompt_v1`] inside
//! [`luna_extract::LlmExtractor::extract`]; those cases surface here
//! as `ExtractionFailed`, which is why a separate
//! `ObservationInvalid` variant would be redundant.

use crate::{load_benchmark_cases, BenchmarkCase};
use luna_core::{Episode, LunaError, Result, Role, Signal};
use luna_events::{
    AssertionExtracted, EpisodeCreated, EpisodeReinforced, EventEnvelope, EventSource, LunaEvent,
    StoredEvent, TurnObserved,
};
use luna_extract::{CountingBackend, ExtractionCache, LlmBackend, LunaExtractor};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormationReport {
    pub total_cases: usize,
    pub formation_eligible: usize,
    pub proof_eligible_total: usize,
    pub proof_eligible_passing_formation: usize,
    pub gates: GateCounts,
    pub second_run_cache_hit_rate: f32,
    pub backend_calls_first_run: usize,
    pub backend_calls_second_run: usize,
    pub total_turns: usize,
    pub cases: Vec<FormationCaseReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GateCounts {
    /// Cases where the expected episode (matching at least one
    /// must_recall needle) was created. `passes / total`.
    pub expected_episode_creation: usize,
    /// Cases where every must_recall needle matches exactly one
    /// stored assertion value. `passes / total`.
    pub must_recall_exact_value_coverage: usize,
    /// Cases where at least one stored assertion value matched a
    /// must_not_claim needle. `failures / total` (lower is better).
    pub forbidden_assertion_leakage: usize,
    /// Cases where every target dimension on the expected episode is
    /// populated (Some). `passes / total`.
    pub target_dimension_populated: usize,
    /// Cases where every populated target dimension's signal has
    /// source_count >= 2. `passes / total`.
    pub two_source_satisfaction: usize,
    /// Cases where no user-question probe turn was observed.
    /// `failures / total` (lower is better).
    pub no_probe_failures: usize,
    /// Cases where every turn's extraction succeeded.
    /// `passes / total`.
    pub observation_validation_passed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormationCaseReport {
    pub id: String,
    pub category: String,
    pub proof_category: String,
    pub proof_eligible: bool,
    pub passed: bool,
    pub failures: Vec<FormationFailure>,
    pub episodes_created: usize,
    pub assertion_values: Vec<String>,
    pub target_dimensions_status: Vec<TargetDimensionStatus>,
    pub probe_observed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetDimensionStatus {
    pub name: String,
    pub populated: bool,
    pub source_count: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum FormationFailure {
    NoExpectedEpisodeStored { needle: String },
    MustRecallMissing { needle: String },
    MustRecallAmbiguous { needle: String, matches: usize },
    ForbiddenAssertionStored { needle: String, value: String },
    TargetDimensionMissing { dimension: String },
    TargetDimensionSingleSource { dimension: String, source_count: u8 },
    NoProbeObserved,
    ExtractionFailed { turn_index: usize, error: String },
}

/// Run the full formation report against the cases under `input_dir`.
/// The extractor's backend MUST be a [`CountingBackend`] — the
/// second-pass cache hit rate gate cannot be computed without call
/// accounting.
pub fn run_formation<B, C>(
    input_dir: &Path,
    extractor: &LunaExtractor<CountingBackend<B>, C>,
) -> Result<FormationReport>
where
    B: LlmBackend,
    C: ExtractionCache,
{
    let cases = load_benchmark_cases(input_dir)?;
    run_formation_on_cases(&cases, extractor)
}

/// Same as [`run_formation`] but takes pre-loaded cases. Useful for
/// tests that construct cases programmatically.
pub fn run_formation_on_cases<B, C>(
    cases: &[BenchmarkCase],
    extractor: &LunaExtractor<CountingBackend<B>, C>,
) -> Result<FormationReport>
where
    B: LlmBackend,
    C: ExtractionCache,
{
    let backend = extractor.llm().backend();
    let pre_first = backend.count();

    let mut case_reports = Vec::new();
    for case in cases {
        case_reports.push(run_case_formation(case, extractor));
    }
    let after_first = backend.count();
    let backend_calls_first_run = after_first - pre_first;

    // Second pass — same cases, same extractor, fresh case reports
    // discarded. Successful first-pass cache writes plus deterministic
    // re-extraction give a 100% second-pass hit rate; any miss surfaces
    // as a non-zero second_run delta.
    let pre_second = backend.count();
    for case in cases {
        let _ = run_case_formation(case, extractor);
    }
    let after_second = backend.count();
    let backend_calls_second_run = after_second - pre_second;

    let total_turns: usize = cases.iter().map(|case| case.turns.len()).sum();
    let second_run_cache_hit_rate = if total_turns == 0 {
        1.0
    } else {
        1.0 - (backend_calls_second_run as f32 / total_turns as f32)
    };

    let gates = aggregate_gates(&case_reports);
    let formation_eligible = case_reports.iter().filter(|case| case.passed).count();
    let proof_eligible_total = case_reports.iter().filter(|case| case.proof_eligible).count();
    let proof_eligible_passing_formation = case_reports
        .iter()
        .filter(|case| case.proof_eligible && case.passed)
        .count();

    Ok(FormationReport {
        total_cases: cases.len(),
        formation_eligible,
        proof_eligible_total,
        proof_eligible_passing_formation,
        gates,
        second_run_cache_hit_rate,
        backend_calls_first_run,
        backend_calls_second_run,
        total_turns,
        cases: case_reports,
    })
}

fn run_case_formation<B, C>(
    case: &BenchmarkCase,
    extractor: &LunaExtractor<CountingBackend<B>, C>,
) -> FormationCaseReport
where
    B: LlmBackend,
    C: ExtractionCache,
{
    let mut events: Vec<StoredEvent> = Vec::new();
    let mut failures: Vec<FormationFailure> = Vec::new();
    let mut probe_observed = false;

    for (turn_index, turn) in case.turns.iter().enumerate() {
        if turn.role == Role::User && turn.content.contains('?') {
            probe_observed = true;
        }

        let observation = match extractor.extract(turn) {
            Ok(observation) => observation,
            Err(error) => {
                failures.push(FormationFailure::ExtractionFailed {
                    turn_index,
                    error: error.to_string(),
                });
                continue;
            }
        };

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
        events.push(turn_event);

        for assertion in &observation.assertions {
            events.push(
                EventEnvelope::new(
                    LunaEvent::AssertionExtracted(AssertionExtracted {
                        assertion: assertion.clone(),
                        observation: observation.clone(),
                    }),
                    EventSource::HeuristicExtractor,
                    1.0 - observation.uncertainty.value(),
                )
                .with_turn_id(observation.turn_id),
            );

            if let Some(episode_id) = luna_store::episode_id_for_assertion(&events, assertion) {
                events.push(
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
                );
            } else {
                let new_id = Uuid::new_v4();
                events.push(
                    EventEnvelope::new(
                        LunaEvent::EpisodeCreated(EpisodeCreated {
                            assertion: assertion.clone(),
                            observation: observation.clone(),
                        }),
                        EventSource::HeuristicExtractor,
                        1.0 - observation.uncertainty.value(),
                    )
                    .with_turn_id(observation.turn_id)
                    .with_episode_id(new_id),
                );
            }
        }
    }

    if !probe_observed {
        failures.push(FormationFailure::NoProbeObserved);
    }

    let episodes = luna_store::rebuild_episodes(&events).unwrap_or_default();
    let assertion_values: Vec<String> = episodes
        .iter()
        .flat_map(|episode| episode.assertions.iter().map(|a| a.value.clone()))
        .collect();

    // Gate: must_recall coverage (each needle exactly once).
    for needle in &case.expected.must_recall {
        let matches = count_value_matches(&assertion_values, needle);
        match matches {
            0 => failures.push(FormationFailure::MustRecallMissing {
                needle: needle.clone(),
            }),
            1 => {}
            n => failures.push(FormationFailure::MustRecallAmbiguous {
                needle: needle.clone(),
                matches: n,
            }),
        }
    }

    // Gate: must_not_claim leakage.
    for needle in &case.expected.must_not_claim {
        for value in &assertion_values {
            if value_contains(value, needle) {
                failures.push(FormationFailure::ForbiddenAssertionStored {
                    needle: needle.clone(),
                    value: value.clone(),
                });
            }
        }
    }

    // Gate: expected episode + target dimensions.
    let expected_episode = pick_expected_episode(&episodes, &case.expected.must_recall);
    if expected_episode.is_none() && !case.expected.must_recall.is_empty() {
        for needle in &case.expected.must_recall {
            failures.push(FormationFailure::NoExpectedEpisodeStored {
                needle: needle.clone(),
            });
        }
    }

    let mut target_dimensions_status = Vec::new();
    for dimension in &case.target_dimensions {
        let signal = expected_episode.and_then(|episode| dimension_signal(episode, dimension));
        let populated = signal.is_some();
        let source_count = signal.map(|signal| signal.source_count()).unwrap_or(0);
        target_dimensions_status.push(TargetDimensionStatus {
            name: dimension.clone(),
            populated,
            source_count,
        });
        // Only emit gate failures when there IS an expected episode to
        // check against. Otherwise NoExpectedEpisodeStored is the
        // primary failure and dimension checks would be redundant.
        if expected_episode.is_some() {
            if !populated {
                failures.push(FormationFailure::TargetDimensionMissing {
                    dimension: dimension.clone(),
                });
            } else if source_count < 2 {
                failures.push(FormationFailure::TargetDimensionSingleSource {
                    dimension: dimension.clone(),
                    source_count,
                });
            }
        }
    }

    let passed = failures.is_empty();

    FormationCaseReport {
        id: case.id.clone(),
        category: case.category.clone(),
        proof_category: case.proof_category.clone(),
        proof_eligible: case.proof_eligible,
        passed,
        failures,
        episodes_created: episodes.len(),
        assertion_values,
        target_dimensions_status,
        probe_observed,
    }
}

fn aggregate_gates(reports: &[FormationCaseReport]) -> GateCounts {
    let mut gates = GateCounts::default();
    for report in reports {
        let has_no_expected = report
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::NoExpectedEpisodeStored { .. }));
        let has_recall_miss_or_ambig = report.failures.iter().any(|f| {
            matches!(
                f,
                FormationFailure::MustRecallMissing { .. }
                    | FormationFailure::MustRecallAmbiguous { .. }
            )
        });
        let has_forbidden = report
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::ForbiddenAssertionStored { .. }));
        let has_dim_missing = report
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::TargetDimensionMissing { .. }));
        let has_single_source = report
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::TargetDimensionSingleSource { .. }));
        let has_no_probe = report
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::NoProbeObserved));
        let has_extract_fail = report
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::ExtractionFailed { .. }));

        if !has_no_expected {
            gates.expected_episode_creation += 1;
        }
        if !has_recall_miss_or_ambig {
            gates.must_recall_exact_value_coverage += 1;
        }
        if has_forbidden {
            gates.forbidden_assertion_leakage += 1;
        }
        if !has_dim_missing {
            gates.target_dimension_populated += 1;
        }
        if !has_single_source {
            gates.two_source_satisfaction += 1;
        }
        if has_no_probe {
            gates.no_probe_failures += 1;
        }
        if !has_extract_fail {
            gates.observation_validation_passed += 1;
        }
    }
    gates
}

fn pick_expected_episode<'a>(episodes: &'a [Episode], must_recall: &[String]) -> Option<&'a Episode> {
    if must_recall.is_empty() {
        return None;
    }
    episodes.iter().find(|episode| {
        must_recall.iter().any(|needle| {
            episode
                .assertions
                .iter()
                .any(|a| value_contains(&a.value, needle))
        })
    })
}

fn count_value_matches(values: &[String], needle: &str) -> usize {
    values
        .iter()
        .filter(|value| value_contains(value, needle))
        .count()
}

fn value_contains(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn dimension_signal(episode: &Episode, name: &str) -> Option<Signal> {
    match name {
        "temporal_relevance" => episode.contour.temporal_relevance,
        "emotional_arousal" => episode.contour.emotional_arousal,
        "identity_relevance" => episode.contour.identity_relevance,
        "goal_pressure" => episode.contour.goal_pressure,
        "attention" => episode.contour.attention,
        "emotional_valence" => episode.contour.emotional_valence,
        "trust_relevance" => episode.contour.trust_relevance,
        "social_frame" => episode.contour.social_frame,
        _ => None,
    }
}

/// Render a [`FormationReport`] as the human-readable summary the user
/// committed to before implementation: total counts on top, gate-by-
/// gate counts, second-run cache hit rate, then the proof-eligible
/// breakdown.
pub fn formation_markdown(report: &FormationReport) -> String {
    let total = report.total_cases;
    let pct = |numerator: usize, denominator: usize| -> String {
        if denominator == 0 {
            "n/a".to_string()
        } else {
            format!(
                "{}/{} ({:.0}%)",
                numerator,
                denominator,
                100.0 * numerator as f32 / denominator as f32
            )
        }
    };

    format!(
        "# Luna Formation Report\n\n\
         Cases: {}\n\
         Formation eligible: {}\n\
         Proof-eligible cases: {}\n\
         Proof-eligible passing formation: {}\n\n\
         | Gate | Result |\n\
         |---|---:|\n\
         | Expected episode creation | {} |\n\
         | Must-recall exact value coverage | {} |\n\
         | Forbidden assertion leakage | {} |\n\
         | Target dimension populated | {} |\n\
         | Two-source satisfaction | {} |\n\
         | No-probe failures | {} |\n\
         | Observation validation passed | {} |\n\
         | Cache hit rate, second run | {:.0}% |\n\
         | Backend calls (first run) | {} |\n\
         | Backend calls (second run) | {} |\n\
         | Total turns | {} |\n\n\
         _Formation does not call any recall engine. Formation green is\n\
         a precondition for proof runs; formation red means fix\n\
         extraction or case wording, never TCF scoring._\n",
        total,
        report.formation_eligible,
        report.proof_eligible_total,
        pct(
            report.proof_eligible_passing_formation,
            report.proof_eligible_total
        ),
        pct(report.gates.expected_episode_creation, total),
        pct(report.gates.must_recall_exact_value_coverage, total),
        pct(report.gates.forbidden_assertion_leakage, total),
        pct(report.gates.target_dimension_populated, total),
        pct(report.gates.two_source_satisfaction, total),
        pct(report.gates.no_probe_failures, total),
        pct(report.gates.observation_validation_passed, total),
        report.second_run_cache_hit_rate * 100.0,
        report.backend_calls_first_run,
        report.backend_calls_second_run,
        report.total_turns,
    )
}

/// Returns a [`LunaError`] holding the markdown summary plus any failed
/// case ids and their failure types — useful when a CLI wants to fail
/// loudly with formation-quality context.
pub fn formation_failure_summary(report: &FormationReport) -> String {
    let failed: Vec<_> = report.cases.iter().filter(|c| !c.passed).collect();
    if failed.is_empty() {
        return "Formation: all cases passed.".to_string();
    }
    let mut out = format!(
        "Formation: {}/{} cases failed:\n",
        failed.len(),
        report.total_cases
    );
    for case in failed {
        out.push_str(&format!("  {} ({}):\n", case.id, case.category));
        for failure in &case.failures {
            out.push_str(&format!("    - {failure:?}\n"));
        }
    }
    out
}

#[allow(dead_code)]
fn _shape_compile_check() -> Result<()> {
    Err(LunaError::new("never called"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExpectedOutcome;
    use luna_core::ConversationTurn;
    use luna_extract::{
        FileExtractionCache, LlmAssertion, LlmExtractor, LlmObservation, LlmSignal,
        LunaExtractor, RecordingFakeBackend,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("luna_formation_{}", uuid::Uuid::new_v4()))
    }

    fn case(
        id: &str,
        category: &str,
        target_dimensions: Vec<&str>,
        proof_eligible: bool,
        turns: Vec<ConversationTurn>,
        must_recall: Vec<&str>,
        must_not_claim: Vec<&str>,
    ) -> BenchmarkCase {
        BenchmarkCase {
            schema_version: 1,
            id: id.to_string(),
            proof_category: "test".to_string(),
            proof_eligible,
            category: category.to_string(),
            target_dimensions: target_dimensions.into_iter().map(String::from).collect(),
            timestamp_origin: Some("test".to_string()),
            turns,
            expected: ExpectedOutcome {
                must_recall: must_recall.into_iter().map(String::from).collect(),
                must_not_claim: must_not_claim.into_iter().map(String::from).collect(),
            },
        }
    }

    fn empty_signals() -> BTreeMap<String, Option<LlmSignal>> {
        let mut s = BTreeMap::new();
        s.insert("temporal_relevance".to_string(), None);
        s.insert("emotional_arousal".to_string(), None);
        s.insert("identity_relevance".to_string(), None);
        s.insert("goal_pressure".to_string(), None);
        s
    }

    fn signal(value: f32) -> LlmSignal {
        LlmSignal {
            value,
            confidence: 0.8,
            reliability: "learned".to_string(),
            evidence: None,
        }
    }

    fn observation(
        assertions: Vec<(&str, &str, &str)>,
        signals: Vec<(&str, f32)>,
    ) -> LlmObservation {
        let mut sigs = empty_signals();
        for (name, value) in signals {
            sigs.insert(name.to_string(), Some(signal(value)));
        }
        LlmObservation {
            assertions: assertions
                .into_iter()
                .map(|(domain, kind, value)| LlmAssertion {
                    domain: domain.to_string(),
                    kind: kind.to_string(),
                    value: value.to_string(),
                    confidence: 0.9,
                    evidence_span: None,
                })
                .collect(),
            signals: sigs,
        }
    }

    fn fake() -> RecordingFakeBackend {
        RecordingFakeBackend::new("test-model@v1")
    }

    fn extractor(
        backend: RecordingFakeBackend,
        root: &PathBuf,
    ) -> LunaExtractor<CountingBackend<RecordingFakeBackend>, FileExtractionCache> {
        let counted = CountingBackend::new(backend);
        let llm = LlmExtractor::new(counted, FileExtractionCache::new(root));
        LunaExtractor::with_default_v1_sources(llm)
    }

    fn user_at(content: &str, ts: &str) -> ConversationTurn {
        let mut turn = ConversationTurn::user(content);
        turn.timestamp = Some(ts.parse().unwrap());
        turn
    }

    #[test]
    fn passing_case_satisfies_all_gates() {
        let root = temp_root();
        let backend = fake();
        let disclosure = observation(
            vec![("identity", "profession", "mechanical engineer")],
            vec![("identity_relevance", 0.9)],
        );
        let probe = observation(vec![], vec![("identity_relevance", 0.8)]);
        backend.expect("mechanical engineer", &serde_json::to_string(&disclosure).unwrap());
        backend.expect("for a living", &serde_json::to_string(&probe).unwrap());

        let extractor = extractor(backend, &root);
        let cases = vec![case(
            "case_1",
            "paraphrase_invariance",
            vec!["identity_relevance"],
            true,
            vec![
                user_at(
                    "I work as a mechanical engineer.",
                    "2026-05-03T10:00:00Z",
                ),
                user_at("What do I do for a living?", "2026-05-03T10:01:00Z"),
            ],
            vec!["mechanical engineer"],
            vec!["software engineer"],
        )];

        let report = run_formation_on_cases(&cases, &extractor).unwrap();
        assert_eq!(report.formation_eligible, 1);
        assert_eq!(report.gates.expected_episode_creation, 1);
        assert_eq!(report.gates.must_recall_exact_value_coverage, 1);
        assert_eq!(report.gates.forbidden_assertion_leakage, 0);
        assert_eq!(report.gates.target_dimension_populated, 1);
        assert_eq!(report.gates.two_source_satisfaction, 1);
        assert_eq!(report.gates.no_probe_failures, 0);
        assert_eq!(report.gates.observation_validation_passed, 1);
        assert!(report.cases[0].passed);
        assert!(report.cases[0].probe_observed);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_must_recall_surfaces_must_recall_missing_failure() {
        let root = temp_root();
        let backend = fake();
        // LLM never produces the must_recall content.
        let empty = observation(vec![], vec![("identity_relevance", 0.8)]);
        backend.expect("mechanical engineer", &serde_json::to_string(&empty).unwrap());
        backend.expect("for a living", &serde_json::to_string(&empty).unwrap());

        let extractor = extractor(backend, &root);
        let cases = vec![case(
            "case_missing",
            "paraphrase_invariance",
            vec!["identity_relevance"],
            true,
            vec![
                user_at("I work as a mechanical engineer.", "2026-05-03T10:00:00Z"),
                user_at("What do I do for a living?", "2026-05-03T10:01:00Z"),
            ],
            vec!["mechanical engineer"],
            vec![],
        )];

        let report = run_formation_on_cases(&cases, &extractor).unwrap();
        assert!(!report.cases[0].passed);
        assert!(report.cases[0]
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::MustRecallMissing { .. })));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn forbidden_assertion_surfaces_leakage_failure() {
        let root = temp_root();
        let backend = fake();
        // LLM extracts both the right answer AND a forbidden one.
        let disclosure = observation(
            vec![
                ("identity", "profession", "mechanical engineer"),
                ("identity", "profession", "software engineer"),
            ],
            vec![("identity_relevance", 0.9)],
        );
        let probe = observation(vec![], vec![("identity_relevance", 0.8)]);
        backend.expect("disclosure", &serde_json::to_string(&disclosure).unwrap());
        backend.expect("question", &serde_json::to_string(&probe).unwrap());

        let extractor = extractor(backend, &root);
        let cases = vec![case(
            "case_leak",
            "paraphrase_invariance",
            vec!["identity_relevance"],
            true,
            vec![
                user_at("disclosure", "2026-05-03T10:00:00Z"),
                user_at("question?", "2026-05-03T10:01:00Z"),
            ],
            vec!["mechanical engineer"],
            vec!["software engineer"],
        )];

        let report = run_formation_on_cases(&cases, &extractor).unwrap();
        assert!(!report.cases[0].passed);
        assert!(report.cases[0]
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::ForbiddenAssertionStored { .. })));
        assert_eq!(report.gates.forbidden_assertion_leakage, 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn target_dimension_unpopulated_surfaces_missing_failure() {
        let root = temp_root();
        let backend = fake();
        // LLM produces the assertion but no signal for the target dimension.
        let disclosure = observation(
            vec![("identity", "profession", "mechanical engineer")],
            vec![], // no signals
        );
        let probe = observation(vec![], vec![]);
        backend.expect("mechanical engineer", &serde_json::to_string(&disclosure).unwrap());
        backend.expect("for a living", &serde_json::to_string(&probe).unwrap());

        let extractor = extractor(backend, &root);
        let cases = vec![case(
            "case_dim_missing",
            "paraphrase_invariance",
            vec!["identity_relevance"],
            true,
            vec![
                user_at("I work as a mechanical engineer.", "2026-05-03T10:00:00Z"),
                user_at("What do I do for a living?", "2026-05-03T10:01:00Z"),
            ],
            vec!["mechanical engineer"],
            vec![],
        )];

        let report = run_formation_on_cases(&cases, &extractor).unwrap();
        assert!(!report.cases[0].passed);
        assert!(report.cases[0]
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::TargetDimensionMissing { .. })));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_question_in_any_turn_surfaces_no_probe_failure() {
        let root = temp_root();
        let backend = fake();
        let disclosure = observation(
            vec![("identity", "profession", "mechanical engineer")],
            vec![("identity_relevance", 0.9)],
        );
        backend.expect("mechanical engineer", &serde_json::to_string(&disclosure).unwrap());

        let extractor = extractor(backend, &root);
        let cases = vec![case(
            "case_no_probe",
            "paraphrase_invariance",
            vec!["identity_relevance"],
            true,
            vec![user_at(
                "I work as a mechanical engineer.",
                "2026-05-03T10:00:00Z",
            )],
            vec!["mechanical engineer"],
            vec![],
        )];

        let report = run_formation_on_cases(&cases, &extractor).unwrap();
        assert_eq!(report.gates.no_probe_failures, 1);
        assert!(report.cases[0]
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::NoProbeObserved)));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn extraction_error_surfaces_extraction_failed_and_blocks_other_gates_for_that_turn() {
        let root = temp_root();
        let backend = fake();
        // No prescription registered for the disclosure marker — causes
        // a backend error which becomes ExtractionFailed.
        let probe = observation(vec![], vec![("identity_relevance", 0.8)]);
        backend.expect("for a living", &serde_json::to_string(&probe).unwrap());

        let extractor = extractor(backend, &root);
        let cases = vec![case(
            "case_extract_fail",
            "paraphrase_invariance",
            vec!["identity_relevance"],
            true,
            vec![
                user_at("I work as a mechanical engineer.", "2026-05-03T10:00:00Z"),
                user_at("What do I do for a living?", "2026-05-03T10:01:00Z"),
            ],
            vec!["mechanical engineer"],
            vec![],
        )];

        let report = run_formation_on_cases(&cases, &extractor).unwrap();
        assert!(!report.cases[0].passed);
        assert!(report.cases[0]
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::ExtractionFailed { .. })));
        assert_eq!(report.gates.observation_validation_passed, 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn second_run_cache_hit_rate_is_one_when_extraction_succeeds() {
        let root = temp_root();
        let backend = fake();
        let disclosure = observation(
            vec![("identity", "profession", "mechanical engineer")],
            vec![("identity_relevance", 0.9)],
        );
        let probe = observation(vec![], vec![("identity_relevance", 0.8)]);
        backend.expect("mechanical engineer", &serde_json::to_string(&disclosure).unwrap());
        backend.expect("for a living", &serde_json::to_string(&probe).unwrap());

        let extractor = extractor(backend, &root);
        let cases = vec![case(
            "case_cache",
            "paraphrase_invariance",
            vec!["identity_relevance"],
            true,
            vec![
                user_at("I work as a mechanical engineer.", "2026-05-03T10:00:00Z"),
                user_at("What do I do for a living?", "2026-05-03T10:01:00Z"),
            ],
            vec!["mechanical engineer"],
            vec![],
        )];

        let report = run_formation_on_cases(&cases, &extractor).unwrap();
        assert_eq!(report.backend_calls_second_run, 0);
        assert!((report.second_run_cache_hit_rate - 1.0).abs() < 1e-6);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ambiguous_must_recall_surfaces_ambiguity_failure() {
        let root = temp_root();
        let backend = fake();
        // Two assertions that both contain the must_recall needle.
        // Wrap them across two turns to dodge the merge_assertions
        // de-dup that fires when a single observation produces twin
        // assertions with identical keys.
        let disclosure_one = observation(
            vec![("identity", "profession", "mechanical engineer")],
            vec![("identity_relevance", 0.9)],
        );
        let disclosure_two = observation(
            vec![("identity", "role", "mechanical engineer team lead")],
            vec![("identity_relevance", 0.9)],
        );
        let probe = observation(vec![], vec![("identity_relevance", 0.8)]);
        backend.expect("first ", &serde_json::to_string(&disclosure_one).unwrap());
        backend.expect("second ", &serde_json::to_string(&disclosure_two).unwrap());
        backend.expect("for a living", &serde_json::to_string(&probe).unwrap());

        let extractor = extractor(backend, &root);
        let cases = vec![case(
            "case_ambig",
            "paraphrase_invariance",
            vec!["identity_relevance"],
            true,
            vec![
                user_at("first turn says it.", "2026-05-03T10:00:00Z"),
                user_at("second turn also says it.", "2026-05-03T10:01:00Z"),
                user_at("for a living?", "2026-05-03T10:02:00Z"),
            ],
            vec!["mechanical engineer"],
            vec![],
        )];

        let report = run_formation_on_cases(&cases, &extractor).unwrap();
        assert!(report.cases[0]
            .failures
            .iter()
            .any(|f| matches!(f, FormationFailure::MustRecallAmbiguous { .. })));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn proof_eligible_breakdown_counts_correctly() {
        let root = temp_root();
        let backend = fake();
        let disclosure = observation(
            vec![("identity", "profession", "mechanical engineer")],
            vec![("identity_relevance", 0.9)],
        );
        let probe = observation(vec![], vec![("identity_relevance", 0.8)]);
        backend.expect("mechanical engineer", &serde_json::to_string(&disclosure).unwrap());
        backend.expect("for a living", &serde_json::to_string(&probe).unwrap());

        let extractor = extractor(backend, &root);
        let mk = |id: &str, eligible: bool| -> BenchmarkCase {
            case(
                id,
                "paraphrase_invariance",
                vec!["identity_relevance"],
                eligible,
                vec![
                    user_at("I work as a mechanical engineer.", "2026-05-03T10:00:00Z"),
                    user_at("What do I do for a living?", "2026-05-03T10:01:00Z"),
                ],
                vec!["mechanical engineer"],
                vec![],
            )
        };
        let cases = vec![mk("a", true), mk("b", true), mk("c", false)];
        let report = run_formation_on_cases(&cases, &extractor).unwrap();
        assert_eq!(report.proof_eligible_total, 2);
        assert_eq!(report.proof_eligible_passing_formation, 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn markdown_renders_without_panicking() {
        let report = FormationReport {
            total_cases: 0,
            formation_eligible: 0,
            proof_eligible_total: 0,
            proof_eligible_passing_formation: 0,
            gates: GateCounts::default(),
            second_run_cache_hit_rate: 1.0,
            backend_calls_first_run: 0,
            backend_calls_second_run: 0,
            total_turns: 0,
            cases: vec![],
        };
        let md = formation_markdown(&report);
        assert!(md.contains("Formation Report"));
    }
}
