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
//! [`luna_extract::validate_against_prompt_v3`] inside
//! [`luna_extract::LlmExtractor::extract`]; those cases surface here
//! as `ExtractionFailed`, which is why a separate
//! `ObservationInvalid` variant would be redundant.

use crate::{load_benchmark_cases, BenchmarkCase};
use luna_core::{ConversationTurn, Episode, LunaError, Result, Role, Signal, StructuredAssertion};
use luna_events::{
    AssertionExtracted, EpisodeCreated, EpisodeReinforced, EventEnvelope, EventSource, LunaEvent,
    StoredEvent, TurnObserved,
};
use luna_extract::{CountingBackend, ExtractionCache, LlmBackend, LunaExtractor};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    /// Per-needle diagnostics for any must_recall needle that did not
    /// match exactly one stored assertion value. Surfaces *why* the
    /// gate failed: did the LLM omit the content, paraphrase it,
    /// produce a broader category, or is the needle distributed
    /// across turns in a way the case wording doesn't allow a single
    /// assertion to capture? PR 0.8 introduces this; old run JSONs
    /// deserialize with the field absent.
    #[serde(default)]
    pub must_recall_diagnostics: Vec<MustRecallDiagnostic>,
}

/// One diagnostic record per must_recall needle that didn't match
/// exactly once. Pairs with a `MustRecallMissing` or
/// `MustRecallAmbiguous` entry in `failures` to classify the failure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MustRecallDiagnostic {
    pub needle: String,
    /// Classification picked by the lexical heuristic. Each variant
    /// suggests a different remediation lever (extractor prompt,
    /// case wording, formation-gate granularity); see
    /// [`MustRecallFailureKind`].
    pub kind: MustRecallFailureKind,
    /// True iff the needle (case-insensitive) appears as a substring
    /// in any user-role turn's content. False means the case wording
    /// itself doesn't include the canonical phrase the must_recall
    /// expects.
    pub appears_in_turn_text: bool,
    /// Index of the first user-role turn whose content contains the
    /// needle as a substring. `None` when `appears_in_turn_text` is
    /// false.
    pub appears_in_turn_index: Option<usize>,
    /// All needle words present somewhere across the user-role turns
    /// but no single turn contains all of them. Catches the
    /// cross-turn-distributed case (type E).
    pub distributed_across_turns: bool,
    /// Closest stored assertion value by Jaccard token overlap, if
    /// any assertion was stored at all. Lets a reviewer compare
    /// "what the case wanted" with "what the LLM produced".
    pub closest_match: Option<ClosestAssertion>,
    /// Number of stored assertion values across all rebuilt episodes
    /// for this case, for context when reading the diagnostic.
    pub stored_assertion_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosestAssertion {
    pub value: String,
    pub domain: String,
    pub kind: String,
    pub jaccard_similarity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MustRecallFailureKind {
    /// Type A. The needle appears in the case's user turns but no
    /// stored assertion value comes close (no related assertion at
    /// all, or only assertions with very low token overlap).
    /// Suggests the LLM did not extract the relevant content.
    Omitted,
    /// Types B and C combined. The needle appears in the case's user
    /// turns and a stored assertion has meaningful token overlap with
    /// the needle but is not an exact match. Either the LLM
    /// paraphrased the canonical noun phrase or extracted a broader
    /// category. Lexical similarity alone can't distinguish B from C
    /// reliably; both are "near miss" — inspect manually.
    Paraphrased,
    /// Type D. The needle does not appear as a substring in any
    /// user-role turn, and its tokens are not distributed across
    /// turns. Suggests the case's expected value is too strict or
    /// disconnected from the case wording.
    OverlyStrict,
    /// Type E. The needle does not appear in any single user-role
    /// turn but all of its tokens are present somewhere across the
    /// user turns. Suggests the formation gate's
    /// "value contained in single assertion" check is too rigid for
    /// content that's distributed across the conversation.
    CrossTurn,
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
    let proof_eligible_total = case_reports
        .iter()
        .filter(|case| case.proof_eligible)
        .count();
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
        .flat_map(|episode| {
            episode
                .assertions
                .iter()
                .filter(|assertion| assertion.confidence_tier.is_confirmed())
                .map(|assertion| assertion.value.clone())
        })
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

    let must_recall_diagnostics = build_must_recall_diagnostics(
        &case.expected.must_recall,
        &assertion_values,
        &collect_assertions(&episodes),
        &case.turns,
        &failures,
    );

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
        must_recall_diagnostics,
    }
}

/// Flattens the rebuilt episodes' assertions into one list, preserving
/// each assertion's domain/kind alongside its value. The diagnostic
/// pass uses this to surface the closest stored value's context.
fn collect_assertions(episodes: &[Episode]) -> Vec<StructuredAssertion> {
    episodes
        .iter()
        .flat_map(|episode| episode.assertions.iter().cloned())
        .filter(|assertion| assertion.confidence_tier.is_confirmed())
        .collect()
}

/// Builds one [`MustRecallDiagnostic`] per needle that surfaced as
/// `MustRecallMissing` or `MustRecallAmbiguous` in `failures`. No
/// behavior change — purely additive context for the human reviewer.
fn build_must_recall_diagnostics(
    must_recall: &[String],
    assertion_values: &[String],
    structured_assertions: &[StructuredAssertion],
    turns: &[ConversationTurn],
    failures: &[FormationFailure],
) -> Vec<MustRecallDiagnostic> {
    let needs_diagnostic: HashSet<&str> = failures
        .iter()
        .filter_map(|failure| match failure {
            FormationFailure::MustRecallMissing { needle }
            | FormationFailure::MustRecallAmbiguous { needle, .. } => Some(needle.as_str()),
            _ => None,
        })
        .collect();

    must_recall
        .iter()
        .filter(|needle| needs_diagnostic.contains(needle.as_str()))
        .map(|needle| diagnose_needle(needle, assertion_values, structured_assertions, turns))
        .collect()
}

fn diagnose_needle(
    needle: &str,
    assertion_values: &[String],
    structured_assertions: &[StructuredAssertion],
    turns: &[ConversationTurn],
) -> MustRecallDiagnostic {
    let needle_lower = needle.to_ascii_lowercase();

    let appears_in_turn_index = turns
        .iter()
        .enumerate()
        .find(|(_, turn)| {
            turn.role == Role::User && turn.content.to_ascii_lowercase().contains(&needle_lower)
        })
        .map(|(index, _)| index);
    let appears_in_turn_text = appears_in_turn_index.is_some();

    let distributed_across_turns =
        !appears_in_turn_text && needle_words_distributed_across_turns(&needle_lower, turns);

    let closest_match = closest_assertion(&needle_lower, assertion_values, structured_assertions);

    let kind = classify(
        appears_in_turn_text,
        distributed_across_turns,
        closest_match.as_ref(),
    );

    MustRecallDiagnostic {
        needle: needle.to_string(),
        kind,
        appears_in_turn_text,
        appears_in_turn_index,
        distributed_across_turns,
        closest_match,
        stored_assertion_count: assertion_values.len(),
    }
}

fn classify(
    appears_in_turn_text: bool,
    distributed_across_turns: bool,
    closest: Option<&ClosestAssertion>,
) -> MustRecallFailureKind {
    // Threshold for "the LLM said something near this" vs "the LLM
    // missed it entirely". Picked at 0.34 — a near-miss assertion
    // sharing one of two or three tokens crosses it; sharing zero
    // tokens does not. Adjust when accumulated diagnostics suggest a
    // sharper line.
    const NEAR_MISS_JACCARD: f32 = 0.34;

    if !appears_in_turn_text {
        return if distributed_across_turns {
            MustRecallFailureKind::CrossTurn
        } else {
            MustRecallFailureKind::OverlyStrict
        };
    }
    match closest {
        Some(match_) if match_.jaccard_similarity >= NEAR_MISS_JACCARD => {
            MustRecallFailureKind::Paraphrased
        }
        _ => MustRecallFailureKind::Omitted,
    }
}

fn closest_assertion(
    needle_lower: &str,
    assertion_values: &[String],
    structured_assertions: &[StructuredAssertion],
) -> Option<ClosestAssertion> {
    if assertion_values.is_empty() {
        return None;
    }
    let needle_tokens = tokenize(needle_lower);
    let mut best: Option<(usize, f32)> = None;
    for (index, value) in assertion_values.iter().enumerate() {
        let value_tokens = tokenize(&value.to_ascii_lowercase());
        let similarity = jaccard(&needle_tokens, &value_tokens);
        if best.map(|(_, score)| similarity > score).unwrap_or(true) {
            best = Some((index, similarity));
        }
    }
    let (index, similarity) = best?;
    // structured_assertions and assertion_values are constructed in
    // the same order, so index aligns. Defensive fallback to the
    // first assertion if alignment ever drifts.
    let domain_kind = structured_assertions
        .get(index)
        .or_else(|| structured_assertions.first());
    Some(ClosestAssertion {
        value: assertion_values[index].clone(),
        domain: domain_kind.map(|a| a.domain.clone()).unwrap_or_default(),
        kind: domain_kind.map(|a| a.kind.clone()).unwrap_or_default(),
        jaccard_similarity: similarity,
    })
}

fn needle_words_distributed_across_turns(needle_lower: &str, turns: &[ConversationTurn]) -> bool {
    let needle_tokens: HashSet<String> = tokenize(needle_lower).into_iter().collect();
    if needle_tokens.is_empty() {
        return false;
    }
    let user_turn_token_sets: Vec<HashSet<String>> = turns
        .iter()
        .filter(|t| t.role == Role::User)
        .map(|t| {
            tokenize(&t.content.to_ascii_lowercase())
                .into_iter()
                .collect()
        })
        .collect();
    let union: HashSet<&String> = user_turn_token_sets.iter().flatten().collect();
    let all_present = needle_tokens.iter().all(|word| union.contains(word));
    if !all_present {
        return false;
    }
    let any_single_has_all = user_turn_token_sets
        .iter()
        .any(|turn_words| needle_tokens.iter().all(|w| turn_words.contains(w)));
    !any_single_has_all
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_lowercase())
        .collect()
}

fn jaccard(left: &[String], right: &[String]) -> f32 {
    if left.is_empty() && right.is_empty() {
        return 0.0;
    }
    let left_set: HashSet<&String> = left.iter().collect();
    let right_set: HashSet<&String> = right.iter().collect();
    let intersection = left_set.intersection(&right_set).count();
    let union = left_set.union(&right_set).count();
    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
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

fn pick_expected_episode<'a>(
    episodes: &'a [Episode],
    must_recall: &[String],
) -> Option<&'a Episode> {
    if must_recall.is_empty() {
        return None;
    }
    episodes.iter().find(|episode| {
        must_recall.iter().any(|needle| {
            episode
                .assertions
                .iter()
                .filter(|assertion| assertion.confidence_tier.is_confirmed())
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
        "temporal_relevance" => episode.profile.temporal_relevance,
        "emotional_arousal" => episode.profile.emotional_arousal,
        "identity_relevance" => episode.profile.identity_relevance,
        "goal_pressure" => episode.profile.goal_pressure,
        "attention" => episode.profile.attention,
        "emotional_valence" => episode.profile.emotional_valence,
        "trust_relevance" => episode.profile.trust_relevance,
        "social_frame" => episode.profile.social_frame,
        _ => None,
    }
}

/// Tally of [`MustRecallFailureKind`] classifications across all
/// per-needle diagnostics in a report. Surfaces in the markdown
/// summary so reviewers can see the dominant failure mode at a
/// glance — different modes call for different fixes (prompt,
/// case wording, gate granularity).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MustRecallDiagnosticCounts {
    pub omitted: usize,
    pub paraphrased: usize,
    pub overly_strict: usize,
    pub cross_turn: usize,
    pub total: usize,
}

pub fn diagnostic_counts(report: &FormationReport) -> MustRecallDiagnosticCounts {
    let mut counts = MustRecallDiagnosticCounts::default();
    for case in &report.cases {
        for diagnostic in &case.must_recall_diagnostics {
            counts.total += 1;
            match diagnostic.kind {
                MustRecallFailureKind::Omitted => counts.omitted += 1,
                MustRecallFailureKind::Paraphrased => counts.paraphrased += 1,
                MustRecallFailureKind::OverlyStrict => counts.overly_strict += 1,
                MustRecallFailureKind::CrossTurn => counts.cross_turn += 1,
            }
        }
    }
    counts
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

    let counts = diagnostic_counts(report);
    let diagnostics_block = if counts.total == 0 {
        String::new()
    } else {
        format!(
            "\n## Must-recall diagnostics (PR 0.8)\n\n\
             {} per-needle diagnostic record(s) across all failing cases:\n\n\
             | Classification | Count | Suggested lever |\n\
             |---|---:|---|\n\
             | Omitted (A) | {} | LLM extraction prompt — content not extracted |\n\
             | Paraphrased (B/C) | {} | Prompt instruction to preserve canonical noun phrases |\n\
             | Overly strict (D) | {} | Case wording — needle not in turn text |\n\
             | Cross-turn (E) | {} | Formation gate granularity — needle distributed across turns |\n",
            counts.total,
            counts.omitted,
            counts.paraphrased,
            counts.overly_strict,
            counts.cross_turn,
        )
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
         | Total turns | {} |\n{}\n\
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
        diagnostics_block,
    )
}

/// Returns a textual summary of failed cases with their failure types
/// and PR-0.8 must_recall diagnostics. Useful when a CLI wants to fail
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
        for diagnostic in &case.must_recall_diagnostics {
            out.push_str(&format!(
                "    [diag {:?}] needle={:?} in_turn={} closest={}\n",
                diagnostic.kind,
                diagnostic.needle,
                diagnostic
                    .appears_in_turn_index
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                diagnostic
                    .closest_match
                    .as_ref()
                    .map(|m| format!(
                        "{:?} (jaccard={:.2}, {}:{})",
                        m.value, m.jaccard_similarity, m.domain, m.kind
                    ))
                    .unwrap_or_else(|| "none".to_string()),
            ));
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
        FileExtractionCache, LlmAssertion, LlmExtractor, LlmObservation, LlmSignal, LunaExtractor,
        RecordingFakeBackend,
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
        backend.expect(
            "mechanical engineer",
            &serde_json::to_string(&disclosure).unwrap(),
        );
        backend.expect("for a living", &serde_json::to_string(&probe).unwrap());

        let extractor = extractor(backend, &root);
        let cases = vec![case(
            "case_1",
            "paraphrase_invariance",
            vec!["identity_relevance"],
            true,
            vec![
                user_at("I work as a mechanical engineer.", "2026-05-03T10:00:00Z"),
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
        backend.expect(
            "mechanical engineer",
            &serde_json::to_string(&empty).unwrap(),
        );
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
                user_at(
                    "disclosure: I work as a mechanical engineer.",
                    "2026-05-03T10:00:00Z",
                ),
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
        backend.expect(
            "mechanical engineer",
            &serde_json::to_string(&disclosure).unwrap(),
        );
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
        backend.expect(
            "mechanical engineer",
            &serde_json::to_string(&disclosure).unwrap(),
        );

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
        backend.expect(
            "mechanical engineer",
            &serde_json::to_string(&disclosure).unwrap(),
        );
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
                user_at(
                    "first turn says I work as a mechanical engineer.",
                    "2026-05-03T10:00:00Z",
                ),
                user_at(
                    "second turn says I am a mechanical engineer team lead.",
                    "2026-05-03T10:01:00Z",
                ),
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
        backend.expect(
            "mechanical engineer",
            &serde_json::to_string(&disclosure).unwrap(),
        );
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

    // PR 0.8 must_recall diagnostic classification tests. Each test
    // sets up a controlled situation where exactly one classification
    // should fire, then asserts on the kind.

    fn user(content: &str) -> ConversationTurn {
        ConversationTurn::user(content)
    }

    fn structured(domain: &str, kind: &str, value: &str) -> StructuredAssertion {
        StructuredAssertion::inferred(domain, kind, value)
    }

    #[test]
    fn diagnostic_classifies_omitted_when_needle_in_turn_but_no_related_assertion() {
        let needle = "client deadline";
        let assertion_values = vec!["mechanical engineer".to_string()];
        let structured_assertions =
            vec![structured("identity", "profession", "mechanical engineer")];
        let turns = vec![user("This morning the client deadline had me tense.")];
        let diag = diagnose_needle(needle, &assertion_values, &structured_assertions, &turns);
        assert_eq!(diag.kind, MustRecallFailureKind::Omitted);
        assert!(diag.appears_in_turn_text);
        assert_eq!(diag.appears_in_turn_index, Some(0));
        // Closest is recorded for diagnostic context but its similarity is below the
        // near-miss threshold, which is what flips the classification to Omitted.
        let closest = diag
            .closest_match
            .expect("a closest match should be recorded");
        assert!(
            closest.jaccard_similarity < 0.34,
            "expected jaccard below threshold for Omitted classification, got {}",
            closest.jaccard_similarity
        );
    }

    #[test]
    fn diagnostic_classifies_paraphrased_when_similar_assertion_exists() {
        let needle = "client deadline";
        let assertion_values = vec!["client deadline pressure".to_string()];
        let structured_assertions = vec![structured(
            "work",
            "current_stressor",
            "client deadline pressure",
        )];
        let turns = vec![user("This morning the client deadline had me tense.")];
        let diag = diagnose_needle(needle, &assertion_values, &structured_assertions, &turns);
        assert_eq!(diag.kind, MustRecallFailureKind::Paraphrased);
        let closest = diag.closest_match.unwrap();
        assert!(closest.jaccard_similarity >= 0.34);
        assert_eq!(closest.domain, "work");
        assert_eq!(closest.kind, "current_stressor");
    }

    #[test]
    fn diagnostic_classifies_overly_strict_when_needle_absent_from_turns() {
        let needle = "phantom phrase";
        let assertion_values = vec!["mechanical engineer".to_string()];
        let structured_assertions =
            vec![structured("identity", "profession", "mechanical engineer")];
        let turns = vec![user("I work as a mechanical engineer.")];
        let diag = diagnose_needle(needle, &assertion_values, &structured_assertions, &turns);
        assert_eq!(diag.kind, MustRecallFailureKind::OverlyStrict);
        assert!(!diag.appears_in_turn_text);
        assert!(!diag.distributed_across_turns);
    }

    #[test]
    fn diagnostic_classifies_cross_turn_when_needle_words_split_across_turns() {
        let needle = "client deadline";
        let assertion_values = vec!["work pressure".to_string()];
        let structured_assertions = vec![structured("work", "current_stressor", "work pressure")];
        let turns = vec![
            user("The client meeting is tomorrow."),
            user("That deadline is going to wreck me."),
        ];
        let diag = diagnose_needle(needle, &assertion_values, &structured_assertions, &turns);
        assert_eq!(diag.kind, MustRecallFailureKind::CrossTurn);
        assert!(!diag.appears_in_turn_text);
        assert!(diag.distributed_across_turns);
    }

    #[test]
    fn diagnostic_records_no_closest_match_when_no_assertions_stored() {
        let needle = "anything";
        let assertion_values: Vec<String> = vec![];
        let structured_assertions: Vec<StructuredAssertion> = vec![];
        let turns = vec![user("I had a strange day with anything happening.")];
        let diag = diagnose_needle(needle, &assertion_values, &structured_assertions, &turns);
        assert!(diag.closest_match.is_none());
        // Needle in turn text + no closest match -> classified Omitted.
        assert_eq!(diag.kind, MustRecallFailureKind::Omitted);
    }

    #[test]
    fn diagnostic_counts_aggregate_across_cases() {
        let report = FormationReport {
            total_cases: 2,
            formation_eligible: 0,
            proof_eligible_total: 0,
            proof_eligible_passing_formation: 0,
            gates: GateCounts::default(),
            second_run_cache_hit_rate: 0.0,
            backend_calls_first_run: 0,
            backend_calls_second_run: 0,
            total_turns: 0,
            cases: vec![
                FormationCaseReport {
                    id: "a".to_string(),
                    category: "x".to_string(),
                    proof_category: "p".to_string(),
                    proof_eligible: true,
                    passed: false,
                    failures: vec![],
                    episodes_created: 0,
                    assertion_values: vec![],
                    target_dimensions_status: vec![],
                    probe_observed: false,
                    must_recall_diagnostics: vec![
                        MustRecallDiagnostic {
                            needle: "x".to_string(),
                            kind: MustRecallFailureKind::Omitted,
                            appears_in_turn_text: true,
                            appears_in_turn_index: Some(0),
                            distributed_across_turns: false,
                            closest_match: None,
                            stored_assertion_count: 0,
                        },
                        MustRecallDiagnostic {
                            needle: "y".to_string(),
                            kind: MustRecallFailureKind::Paraphrased,
                            appears_in_turn_text: true,
                            appears_in_turn_index: Some(0),
                            distributed_across_turns: false,
                            closest_match: None,
                            stored_assertion_count: 0,
                        },
                    ],
                },
                FormationCaseReport {
                    id: "b".to_string(),
                    category: "x".to_string(),
                    proof_category: "p".to_string(),
                    proof_eligible: true,
                    passed: false,
                    failures: vec![],
                    episodes_created: 0,
                    assertion_values: vec![],
                    target_dimensions_status: vec![],
                    probe_observed: false,
                    must_recall_diagnostics: vec![MustRecallDiagnostic {
                        needle: "z".to_string(),
                        kind: MustRecallFailureKind::CrossTurn,
                        appears_in_turn_text: false,
                        appears_in_turn_index: None,
                        distributed_across_turns: true,
                        closest_match: None,
                        stored_assertion_count: 0,
                    }],
                },
            ],
        };
        let counts = diagnostic_counts(&report);
        assert_eq!(counts.total, 3);
        assert_eq!(counts.omitted, 1);
        assert_eq!(counts.paraphrased, 1);
        assert_eq!(counts.cross_turn, 1);
        assert_eq!(counts.overly_strict, 0);
    }
}
