//! Three-way recall benchmark: Keyword vs Vector vs Geometric.
//!
//! Generates 200 synthetic episodes with known ground truth and runs
//! probe queries through all three recall engines.
//!
//! Gate: Geometric must beat at least one baseline on at least one metric.

use chrono::Utc;
use luna_core::{
    Episode, EpisodeProfile, RecallMode, Signal, SignalReliability, StructuredAssertion,
    TurnReading,
};
use luna_recall::{GeometricRecallEngine, KeywordRecallEngine, RecallEngine, VectorRecallEngine};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

// ── helpers ──

fn sig(value: f32, reliability: SignalReliability) -> Option<Signal> {
    Some(Signal::new(value, 0.75, reliability).with_source_count(2))
}

fn make_ep(assertions: Vec<StructuredAssertion>, profile: EpisodeProfile) -> Episode {
    Episode {
        id: Uuid::new_v4(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        assertions,
        profile,
        recall_history: Vec::new(),
        confidence: 0.8,
        coherence_score: 0.7,
        forgotten_risk: 0.0,
    }
}

fn probe(
    semantic: Option<Vec<f32>>,
    goal: Option<Signal>,
    identity: Option<Signal>,
    cue_terms: Vec<String>,
    query_intents: Vec<String>,
) -> TurnReading {
    TurnReading {
        turn_id: Uuid::new_v4(),
        semantic,
        intent: None,
        attention: None,
        goal_pressure: goal,
        emotional_valence: None,
        emotional_arousal: None,
        identity_relevance: identity,
        trust_relevance: None,
        social_frame: None,
        temporal_relevance: None,
        uncertainty: Signal::new(0.0, 1.0, SignalReliability::Heuristic),
        cue_terms,
        query_intents,
        assertions: Vec::new(),
    }
}

// ── corpus ──

struct Corpus {
    episodes: Vec<Episode>,
    ground_truth: Vec<HashSet<usize>>,
    probes: Vec<TurnReading>,
}

fn build_corpus() -> Corpus {
    let mut episodes = Vec::new();
    let mut ground_truth: Vec<HashSet<usize>> = Vec::new();
    let mut probes = Vec::new();

    // ── Group A (idx 0-49): profession episodes — geometric should dominate ──
    for i in 0..50 {
        let a = StructuredAssertion::inferred("identity", "profession", format!("engineer_{i}"))
            .with_source_count(2);
        let profile = EpisodeProfile {
            semantic: None,
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: sig(0.88, SignalReliability::Learned),
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            reinforcement_count: 3,
            contradiction_count: 0,
            successful_recall_count: 2,
            failed_recall_count: 0,
        };
        episodes.push(make_ep(vec![a], profile));
    }

    // ── Group B (idx 50-99): keyword-only episodes (project names) ──
    for i in 0..50 {
        let a = StructuredAssertion::inferred("project", "name", format!("zircon_project_{i}"))
            .with_source_count(2);
        let profile = EpisodeProfile {
            semantic: None,
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: sig(0.1, SignalReliability::Heuristic),
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            reinforcement_count: 1,
            contradiction_count: 0,
            successful_recall_count: 0,
            failed_recall_count: 0,
        };
        episodes.push(make_ep(vec![a], profile));
    }

    // ── Group C (idx 100-149): vector episodes ──
    let base_vec: Vec<f32> = (0..16).map(|i| (i + 1) as f32 * 0.05).collect();
    for i in 0..50 {
        let mut v = base_vec.clone();
        v[0] += i as f32 * 0.003;
        let a = StructuredAssertion::inferred("fact", "vector", format!("vec_data_{i}"))
            .with_source_count(2);
        let profile = EpisodeProfile {
            semantic: Some(v),
            intent: None,
            attention: None,
            goal_pressure: None,
            emotional_valence: None,
            emotional_arousal: None,
            identity_relevance: None,
            trust_relevance: None,
            social_frame: None,
            temporal_relevance: None,
            reinforcement_count: 1,
            contradiction_count: 0,
            successful_recall_count: 0,
            failed_recall_count: 0,
        };
        episodes.push(make_ep(vec![a], profile));
    }

    // ── Group D (idx 150-199): distractors ──
    for i in 0..50 {
        let a = StructuredAssertion::inferred("noise", "distractor", format!("zzz_noise_{i}"))
            .with_source_count(2);
        let profile = EpisodeProfile {
            semantic: Some(vec![0.0; 16]),
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
        };
        episodes.push(make_ep(vec![a], profile));
    }

    // ── probes ──

    // Probe 0: profession query → geometric wins (assertion_fit + identity signal)
    {
        let mut gt = HashSet::new();
        gt.extend(0..50);
        ground_truth.push(gt);
        probes.push(probe(
            None,
            None,
            sig(0.9, SignalReliability::Learned),
            vec!["engineer".to_string()],
            vec!["identity.profession.query".to_string()],
        ));
    }

    // Probe 1: profession query with weak identity signal → geometric still hits
    {
        let mut gt = HashSet::new();
        gt.extend(0..50);
        ground_truth.push(gt);
        probes.push(probe(
            None,
            None,
            sig(0.5, SignalReliability::Heuristic),
            vec!["profession".to_string()],
            vec!["identity.profession.query".to_string()],
        ));
    }

    // Probe 2: "what is my job" variant
    {
        let mut gt = HashSet::new();
        gt.extend(0..50);
        ground_truth.push(gt);
        probes.push(probe(
            None,
            None,
            sig(0.7, SignalReliability::Learned),
            vec!["job".to_string(), "work".to_string()],
            vec!["identity.profession.query".to_string()],
        ));
    }

    // Probe 3: keyword query → keyword engine wins
    {
        let mut gt = HashSet::new();
        gt.extend(50..100);
        ground_truth.push(gt);
        probes.push(probe(
            None,
            None,
            None,
            vec!["zircon".to_string()],
            Vec::new(),
        ));
    }

    // Probe 4: specific keyword
    {
        let mut gt = HashSet::new();
        gt.extend(50..100);
        ground_truth.push(gt);
        probes.push(probe(
            None,
            None,
            None,
            vec!["zircon_project_7".to_string()],
            Vec::new(),
        ));
    }

    // Probe 5: project keyword
    {
        let mut gt = HashSet::new();
        gt.extend(50..100);
        ground_truth.push(gt);
        probes.push(probe(
            None,
            None,
            None,
            vec!["project".to_string(), "zircon".to_string()],
            Vec::new(),
        ));
    }

    // Probe 6: vector query → vector engine wins
    {
        let mut gt = HashSet::new();
        gt.extend(100..150);
        ground_truth.push(gt);
        probes.push(probe(
            Some(base_vec.clone()),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ));
    }

    // Probe 7: vector variant
    {
        let mut v = base_vec.clone();
        v[0] += 0.01;
        let mut gt = HashSet::new();
        gt.extend(100..150);
        ground_truth.push(gt);
        probes.push(probe(Some(v), None, None, Vec::new(), Vec::new()));
    }

    // Probe 8: mixed — profession + keyword
    {
        let mut gt = HashSet::new();
        gt.extend(0..50);
        gt.extend(50..100);
        ground_truth.push(gt);
        probes.push(probe(
            None,
            None,
            sig(0.55, SignalReliability::Heuristic),
            vec!["engineer".to_string(), "zircon".to_string()],
            vec!["identity.profession.query".to_string()],
        ));
    }

    // Probe 9: mixed with vector
    {
        let mut gt = HashSet::new();
        gt.extend(0..50);
        gt.extend(100..150);
        ground_truth.push(gt);
        probes.push(probe(
            Some(base_vec.clone()),
            None,
            sig(0.7, SignalReliability::Learned),
            vec!["engineer".to_string()],
            vec!["identity.profession.query".to_string()],
        ));
    }

    Corpus {
        episodes,
        ground_truth,
        probes,
    }
}

// ── metrics ──

struct EngineMetrics {
    recall_at_5: f32,
    precision_at_5: f32,
    mrr: f32,
}

fn evaluate(engine: &dyn RecallEngine, corpus: &Corpus) -> EngineMetrics {
    let n = corpus.probes.len() as f32;
    let mut total_r = 0.0;
    let mut total_p = 0.0;
    let mut total_m = 0.0;

    let id_to_idx: BTreeMap<Uuid, usize> = corpus
        .episodes
        .iter()
        .enumerate()
        .map(|(i, ep)| (ep.id, i))
        .collect();

    for (pi, probe) in corpus.probes.iter().enumerate() {
        let gt = &corpus.ground_truth[pi];
        if gt.is_empty() {
            continue;
        }
        let result = engine
            .recall(probe, &corpus.episodes, RecallMode::Factual)
            .unwrap();

        let top: Vec<usize> = result
            .hits
            .iter()
            .filter_map(|h| id_to_idx.get(&h.episode_id).copied())
            .take(5)
            .collect();

        let found: HashSet<usize> = top.iter().copied().collect();
        let rel = found.intersection(gt).count();

        total_r += rel as f32 / gt.len() as f32;
        total_p += if top.is_empty() {
            0.0
        } else {
            rel as f32 / top.len() as f32
        };

        let mut mrr = 0.0;
        for (rank, idx) in top.iter().enumerate() {
            if gt.contains(idx) {
                mrr = 1.0 / (rank as f32 + 1.0);
                break;
            }
        }
        total_m += mrr;
    }

    EngineMetrics {
        recall_at_5: total_r / n,
        precision_at_5: total_p / n,
        mrr: total_m / n,
    }
}

// ── benchmark ──

#[test]
fn three_way_recall_benchmark() {
    let corpus = build_corpus();

    let kw = evaluate(&KeywordRecallEngine, &corpus);
    let vc = evaluate(&VectorRecallEngine, &corpus);
    let gm = evaluate(&GeometricRecallEngine, &corpus);

    println!();
    println!("| Metric      | Keyword | Vector | Geometric |");
    println!("|-------------+---------+--------+-----------|");
    println!(
        "| Recall@5    |  {:.4} |  {:.4} |   {:.4}  |",
        kw.recall_at_5, vc.recall_at_5, gm.recall_at_5
    );
    println!(
        "| Precision@5 |  {:.4} |  {:.4} |   {:.4}  |",
        kw.precision_at_5, vc.precision_at_5, gm.precision_at_5
    );
    println!(
        "| MRR         |  {:.4} |  {:.4} |   {:.4}  |",
        kw.mrr, vc.mrr, gm.mrr
    );
    println!();

    let beats_kw =
        gm.recall_at_5 > kw.recall_at_5 || gm.precision_at_5 > kw.precision_at_5 || gm.mrr > kw.mrr;
    let beats_vc =
        gm.recall_at_5 > vc.recall_at_5 || gm.precision_at_5 > vc.precision_at_5 || gm.mrr > vc.mrr;

    println!("Geometric beats keyword: {}", beats_kw);
    println!("Geometric beats vector:  {}", beats_vc);

    if beats_kw || beats_vc {
        println!("PASS: geometric beats at least one baseline.");
    } else {
        panic!(
            "FAIL: geometric lost to keyword (R={:.4} P={:.4} M={:.4}) \
             and vector (R={:.4} P={:.4} M={:.4}).",
            kw.recall_at_5, kw.precision_at_5, kw.mrr, vc.recall_at_5, vc.precision_at_5, vc.mrr,
        );
    }
}

#[test]
fn empty_episodes_all_return_empty() {
    let p = probe(None, None, None, vec!["test".to_string()], Vec::new());
    for engine in [
        &KeywordRecallEngine as &dyn RecallEngine,
        &VectorRecallEngine,
        &GeometricRecallEngine,
    ] {
        let r = engine.recall(&p, &[], RecallMode::Factual).unwrap();
        assert!(r.hits.is_empty());
    }
}

#[test]
fn no_signal_probe_geometric_still_may_hit_via_assertion_fit() {
    // If assertion_fit matches via query_intents, score can be > threshold
    let p = probe(
        None,
        None,
        None,
        Vec::new(),
        vec!["identity.profession.query".to_string()],
    );
    let profile = EpisodeProfile {
        semantic: None,
        intent: None,
        attention: None,
        goal_pressure: None,
        emotional_valence: None,
        emotional_arousal: None,
        identity_relevance: sig(0.9, SignalReliability::Learned),
        trust_relevance: None,
        social_frame: None,
        temporal_relevance: None,
        reinforcement_count: 1,
        contradiction_count: 0,
        successful_recall_count: 0,
        failed_recall_count: 0,
    };
    let ep = make_ep(
        vec![StructuredAssertion::inferred("identity", "profession", "pilot").with_source_count(2)],
        profile,
    );
    let r = GeometricRecallEngine
        .recall(&p, &[ep], RecallMode::Factual)
        .unwrap();
    // Should hit because assertion_fit matches
    assert_eq!(r.hits.len(), 1);
}
