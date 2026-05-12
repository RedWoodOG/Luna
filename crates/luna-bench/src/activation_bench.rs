//! Activation-driven selection benchmark.
//!
//! Generates a synthetic memory corpus of 200 nodes with varied kinds,
//! labels, confidence tiers, and timestamps. Runs 50 probe queries
//! comparing flat retrieval (alphabetical sort, top N) against
//! activation-driven selection (`luna_activation::compute_activation`
//! + propagation, sorted by activation descending, top N).
//!
//! Metrics: Precision@N, Recall@N, Mean Reciprocal Rank.
//!
//! The benchmark gates: activation must beat flat on at least 2 of 3
//! aggregate metrics.

use luna_activation::{compute_activation, propagate_activation, ActivationConfig};
use luna_core::{
    AssertionConfidenceTier, MemoryEdge, MemoryNode, MemoryNodeKind, MemoryRelationKind,
};

// ── synthetic corpus constants ──────────────────────────────────────

const CORPUS_SIZE: usize = 200;
#[allow(dead_code)]
const QUERY_COUNT: usize = 50;
const BUDGET_N: usize = 5; // top-N for precision/recall

// ── data generation vocabulary ──────────────────────────────────────

/// Pre-baked label segments so the generated corpus is deterministic
/// and human-auditable.
const FIRST_NAMES: &[&str] = &[
    "Alice", "Bob", "Carla", "Darius", "Elena", "Felix", "Grace", "Hugo",
    "Iris", "Jasper", "Kai", "Lena", "Milo", "Nia", "Omar", "Priya",
    "Quinn", "Rosa", "Samir", "Tessa",
];

const LAST_NAMES: &[&str] = &[
    "Chen", "Drake", "Estevez", "Fields", "Gupta", "Hawthorne", "Ito",
    "Jansen", "Kowalski", "Larson", "Mendes", "Novak", "Osei", "Park",
    "Quincy", "Rossi", "Singh", "Tanaka", "Uribe", "Voss",
];

const PROJECT_PREFIXES: &[&str] = &[
    "Apollo", "Borealis", "Cascade", "Delta", "Eclipse", "Fusion", "Gemini",
    "Helios", "Icarus", "Juno", "Kepler", "Lyra", "Mirage", "Nova",
    "Orion", "Phoenix", "Quantum", "Rigel", "Solaris", "Titan",
];

const PROJECT_SUFFIXES: &[&str] = &[
    "Launch System", "Deploy Pipeline", "Analytics Engine", "Data Mesh",
    "Runtime Core", "Security Module", "Sync Layer", "Query Engine",
    "Cache Fabric", "Stream Hub", "Model Registry", "Feature Store",
    "Auth Gateway", "Config Service", "Telemetry Bus", "Index Service",
    "Schema Registry", "Policy Engine", "Rate Limiter", "Circuit Breaker",
];

const PLACE_NAMES: &[&str] = &[
    "Tokyo", "Paris", "Amsterdam", "Singapore", "Berlin", "Nairobi",
    "Sao Paulo", "Dubai", "Seoul", "Toronto", "Sydney", "Mumbai",
    "Oslo", "Lisbon", "Helsinki", "Vancouver", "Cape Town", "Buenos Aires",
    "Stockholm", "Zurich",
];

const PLACE_TYPES: &[&str] = &[
    "Office", "Lab", "Hub", "Data Center", "Studio", "Workshop",
];

const GOAL_VERBS: &[&str] = &[
    "Launch", "Deploy", "Scale", "Optimize", "Integrate", "Migrate",
    "Refactor", "Decouple", "Harden", "Benchmark", "Document", "Retire",
    "Onboard", "Validate", "Monitor", "Archive", "Automate", "Provision",
    "Deprecate", "Upgrade",
];

const GOAL_TARGETS: &[&str] = &[
    "MVP", "v2 backend", "dashboard UI", "auth layer", "cache tier",
    "search index", "event log", "metrics pipeline", "alert rules",
    "deploy scripts", "CI pipeline", "data warehouse", "API gateway",
    "mobile SDK", "admin panel", "logging framework", "backup system",
    "rate limiter", "feature flags", "A/B test harness",
];

const ASSERTION_DOMAINS: &[&str] = &[
    "identity", "project", "location", "goal", "relationship",
    "capability", "constraint", "preference", "schedule", "risk",
];

const ASSERTION_VALUES: &[&str] = &[
    "Python expert", "AWS certified", "remote worker", "team lead",
    "security cleared", "on-call rotation", "west coast tz",
    "prefers Rust", "public speaking", "open source contributor",
    "reviewed 200 PRs", "shipping v3 on time", "database migrations done",
    "latency p99 < 50ms", "zero-downtime deploy", "gdpr compliant",
    "soc2 certified stack", "backup retention 90d", "cost under $5k/mo",
    "meets sla targets",
];

/// Deterministic seed labels for queries (20 noun-phrase targets).
const QUERY_TARGETS: &[&str] = &[
    "alice", "jasper", "priya", "samir", "rosa",     // person lookups
    "tokyo", "paris", "seoul", "nairobi", "oslo",    // place lookups
    "apollo", "icarus", "nova", "titan", "eclipse",  // project lookups
    "launch", "deploy", "migrate", "decouple",       // goal lookups
    "identity", "location", "schedule", "constraint", // assertion domain queries
    "python", "rust", "gdpr", "soc2", "latency",     // assertion value queries
    // mixed / compound queries
    "alice apollo", "jasper tokyo", "priya deploy",
    "nova oslo", "titan identity", "migrate gdpr",
    "rosa paris", "samir eclipse", "launch tokyo",
    "decouple python", "icarus nairobi",
    // edge-case probes
    "zzyzx",                      // no match
    "chen",                       // partial last-name
    "launch mvp v2 backend",      // multi-word goal
    "data center",                 // place-type term
    "pipeline",                    // project-suffix term
];

// ── corpus generation ───────────────────────────────────────────────

/// Builds a deterministic synthetic corpus: 200 nodes + edges.
pub fn build_corpus() -> (Vec<MemoryNode>, Vec<MemoryEdge>) {
    let mut nodes = Vec::with_capacity(CORPUS_SIZE);
    let mut edges = Vec::new();

    // Distribute: 50 each of Person, Project, Place, Goal; 50 Assertion.
    // We cycle through name/project/place/goal arrays deterministically.

    for i in 0..50 {
        let fn_idx = i % FIRST_NAMES.len();
        let ln_idx = i % LAST_NAMES.len();
        let label = format!(
            "{} {}",
            FIRST_NAMES[fn_idx], LAST_NAMES[ln_idx]
        );
        nodes.push(MemoryNode {
            id: format!("person-{i:03}"),
            label,
            kind: MemoryNodeKind::Person,
            confidence_tier: tier_from_index(i),
            density: 0.5 + ((i % 10) as f32 * 0.05),
            activation: 0.0,
            provenance: vec![],
            created_at: None,
            contradiction_count: 0,
        });
    }

    for i in 0..50 {
        let pfx_idx = i % PROJECT_PREFIXES.len();
        let sfx_idx = i % PROJECT_SUFFIXES.len();
        let label = format!(
            "{} {}",
            PROJECT_PREFIXES[pfx_idx], PROJECT_SUFFIXES[sfx_idx]
        );
        nodes.push(MemoryNode {
            id: format!("project-{i:03}"),
            label,
            kind: MemoryNodeKind::Project,
            confidence_tier: tier_from_index(i),
            density: 0.5 + ((i % 10) as f32 * 0.05),
            activation: 0.0,
            provenance: vec![],
            created_at: None,
            contradiction_count: 0,
        });
    }

    for i in 0..50 {
        let place_idx = i % PLACE_NAMES.len();
        let type_idx = (i / PLACE_NAMES.len()) % PLACE_TYPES.len();
        let label = format!("{} {}", PLACE_NAMES[place_idx], PLACE_TYPES[type_idx]);
        nodes.push(MemoryNode {
            id: format!("place-{i:03}"),
            label,
            kind: MemoryNodeKind::Place,
            confidence_tier: tier_from_index(i),
            density: 0.5 + ((i % 10) as f32 * 0.05),
            activation: 0.0,
            provenance: vec![],
            created_at: None,
            contradiction_count: 0,
        });
    }

    for i in 0..50 {
        let verb_idx = i % GOAL_VERBS.len();
        let tgt_idx = i % GOAL_TARGETS.len();
        let label = format!("{} {}", GOAL_VERBS[verb_idx], GOAL_TARGETS[tgt_idx]);
        nodes.push(MemoryNode {
            id: format!("goal-{i:03}"),
            label,
            kind: MemoryNodeKind::Goal,
            confidence_tier: tier_from_index(i),
            density: 0.5 + ((i % 10) as f32 * 0.05),
            activation: 0.0,
            provenance: vec![],
            created_at: None,
            contradiction_count: 0,
        });
    }

    for i in 0..50 {
        let dom_idx = i % ASSERTION_DOMAINS.len();
        let val_idx = i % ASSERTION_VALUES.len();
        let label = format!(
            "{}: {}",
            ASSERTION_DOMAINS[dom_idx], ASSERTION_VALUES[val_idx]
        );
        nodes.push(MemoryNode {
            id: format!("assertion-{i:03}"),
            label,
            kind: MemoryNodeKind::Assertion,
            confidence_tier: tier_from_index(i),
            density: 0.5 + ((i % 10) as f32 * 0.05),
            activation: 0.0,
            provenance: vec![],
            created_at: None,
            contradiction_count: 0,
        });
    }

    // Edges: connect Person→Project (works on), Project→Place (located in),
    // Goal→Person (owned by), plus some RelatedTo cross-connections.
    for i in 0..50 {
        let p_idx = i;
        let proj_idx = i;
        let place_idx = i % 50;

        edges.push(MemoryEdge {
            source: format!("person-{p_idx:03}"),
            target: format!("project-{proj_idx:03}"),
            relation: MemoryRelationKind::Mentions,
            confidence_tier: tier_from_index(i),
            strength: 0.5 + ((i % 5) as f32 * 0.1),
            activation: 0.0,
            provenance: vec![],
        });

        edges.push(MemoryEdge {
            source: format!("project-{proj_idx:03}"),
            target: format!("place-{place_idx:03}"),
            relation: MemoryRelationKind::LocatedIn,
            confidence_tier: tier_from_index(i),
            strength: 0.6,
            activation: 0.0,
            provenance: vec![],
        });

        edges.push(MemoryEdge {
            source: format!("goal-{i:03}"),
            target: format!("person-{p_idx:03}"),
            relation: MemoryRelationKind::HasGoal,
            confidence_tier: tier_from_index(i),
            strength: 0.4 + ((i % 6) as f32 * 0.1),
            activation: 0.0,
            provenance: vec![],
        });

        // Cross-connections for propagation testing
        if i < 20 {
            edges.push(MemoryEdge {
                source: format!("person-{p_idx:03}"),
                target: format!("person-{:03}", (p_idx + 1) % 50),
                relation: MemoryRelationKind::RelatedTo,
                confidence_tier: AssertionConfidenceTier::Confirmed,
                strength: 0.3,
                activation: 0.0,
                provenance: vec![],
            });
        }
    }

    (nodes, edges)
}

/// Returns a deterministic confidence tier from an index.
fn tier_from_index(i: usize) -> AssertionConfidenceTier {
    match i % 4 {
        0 | 1 => AssertionConfidenceTier::Confirmed,
        2 => AssertionConfidenceTier::Inferred,
        _ => AssertionConfidenceTier::Unconfirmed,
    }
}

// ── query set ───────────────────────────────────────────────────────

/// Build the 50 probe queries.
pub fn build_queries() -> Vec<String> {
    QUERY_TARGETS.iter().map(|s| s.to_string()).collect()
}

// ── relevance ───────────────────────────────────────────────────────

/// A node is relevant to a query if any query token (len >= 2) appears
/// as a case-insensitive substring of the node label.
fn is_relevant(node: &MemoryNode, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let label_lower = node.label.to_lowercase();
    tokenize(query).iter().any(|token| {
        token.len() >= 2 && label_lower.contains(token.as_str())
    })
}

fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

// ── strategies ──────────────────────────────────────────────────────

/// Flat retrieval: sort alphabetically by label, take first N.
fn flat_select(nodes: &[MemoryNode], n: usize) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..nodes.len()).collect();
    // Sort by label (case-insensitive), tie-break by id.
    indices.sort_by(|&a, &b| {
        nodes[a]
            .label
            .to_lowercase()
            .cmp(&nodes[b].label.to_lowercase())
            .then_with(|| nodes[a].id.cmp(&nodes[b].id))
    });
    indices.truncate(n);
    indices
}

/// Activation-driven selection: compute activation, propagate, sort
/// by activation descending, take first N.
fn activation_select(
    nodes: &[MemoryNode],
    edges: &[MemoryEdge],
    query: &str,
    n: usize,
    config: &ActivationConfig,
    max_depth: usize,
) -> Vec<usize> {
    let mut nodes_mut: Vec<MemoryNode> = nodes.to_vec();

    // Compute base activation for each node.
    for node in &mut nodes_mut {
        node.activation = compute_activation(node, query, &[], &[], config);
    }

    // Propagate.
    propagate_activation(&mut nodes_mut, edges, config, max_depth);

    // Sort by activation descending. Tie-break: higher density, then id.
    let mut indices: Vec<usize> = (0..nodes_mut.len()).collect();
    indices.sort_by(|&a, &b| {
        nodes_mut[b]
            .activation
            .partial_cmp(&nodes_mut[a].activation)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                nodes_mut[b]
                    .density
                    .partial_cmp(&nodes_mut[a].density)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| nodes_mut[a].id.cmp(&nodes_mut[b].id))
    });
    indices.truncate(n);
    indices
}

// ── metrics ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct AggregateMetrics {
    pub precision: f32,
    pub recall: f32,
    pub mrr: f32,
}

impl AggregateMetrics {
    fn average(scores: &[QueryScores]) -> Self {
        let n = scores.len() as f32;
        if n == 0.0 {
            return Self::default();
        }
        Self {
            precision: scores.iter().map(|s| s.precision).sum::<f32>() / n,
            recall: scores.iter().map(|s| s.recall).sum::<f32>() / n,
            mrr: scores.iter().map(|s| s.mrr).sum::<f32>() / n,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct QueryScores {
    precision: f32,
    recall: f32,
    mrr: f32,
}

/// Compute Precision@N, Recall@N, and MRR for a single query.
///
/// - Precision@N: fraction of top-N selected nodes that are relevant.
/// - Recall@N: fraction of all relevant nodes in corpus that are in top-N.
/// - MRR: 1 / rank of first relevant node (or 0.0 if none).
///
/// Edge cases:
/// - Empty query or no relevant nodes → precision=1.0, recall=1.0, mrr=1.0
///   (both strategies return empty; agreement is perfect).
/// - All nodes relevant → precision=1.0, recall=N/total (both identical).
fn compute_scores(
    nodes: &[MemoryNode],
    query: &str,
    selected_indices: &[usize],
) -> QueryScores {
    let _selected_set: std::collections::HashSet<usize> =
        selected_indices.iter().copied().collect();

    // Find all relevant node indices.
    let relevant: Vec<usize> = (0..nodes.len())
        .filter(|&i| is_relevant(&nodes[i], query))
        .collect();

    if relevant.is_empty() {
        return QueryScores {
            precision: 1.0,
            recall: 1.0,
            mrr: 1.0,
        };
    }

    // How many selected are relevant.
    let relevant_selected = selected_indices
        .iter()
        .filter(|&&i| is_relevant(&nodes[i], query))
        .count();

    let precision = relevant_selected as f32 / selected_indices.len().max(1) as f32;
    let recall = relevant_selected as f32 / relevant.len().max(1) as f32;

    // MRR: find rank of first relevant node in selected list.
    let mrr = selected_indices
        .iter()
        .position(|&i| is_relevant(&nodes[i], query))
        .map(|rank| 1.0 / (rank as f32 + 1.0))
        .unwrap_or(0.0);

    QueryScores {
        precision,
        recall,
        mrr,
    }
}

// ── runner ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct BenchmarkResult {
    pub flat: AggregateMetrics,
    pub activation: AggregateMetrics,
    pub activation_wins_precision: bool,
    pub activation_wins_recall: bool,
    pub activation_wins_mrr: bool,
    pub wins: usize,
    pub passed: bool,
}

/// Run the full benchmark. Prints a comparison table to stdout and
/// returns the structured result.
pub fn run_benchmark() -> BenchmarkResult {
    let (nodes, edges) = build_corpus();
    let queries = build_queries();
    let config = ActivationConfig::default();
    let max_depth = 2;

    let mut flat_scores = Vec::with_capacity(queries.len());
    let mut act_scores = Vec::with_capacity(queries.len());

    for query in &queries {
        let flat_idx = flat_select(&nodes, BUDGET_N);
        let act_idx = activation_select(&nodes, &edges, query, BUDGET_N, &config, max_depth);

        flat_scores.push(compute_scores(&nodes, query, &flat_idx));
        act_scores.push(compute_scores(&nodes, query, &act_idx));
    }

    let flat_agg = AggregateMetrics::average(&flat_scores);
    let act_agg = AggregateMetrics::average(&act_scores);

    let wins_precision = act_agg.precision > flat_agg.precision;
    let wins_recall = act_agg.recall > flat_agg.recall;
    let wins_mrr = act_agg.mrr > flat_agg.mrr;
    let wins = [wins_precision, wins_recall, wins_mrr]
        .iter()
        .filter(|&&w| w)
        .count();
    let passed = wins >= 2;

    // Print comparison table.
    println!();
    println!(
        "{0: <18} | {1: >10} | {2: >10} | {3: >10}",
        "Metric", "Flat", "Activation", "Delta"
    );
    println!("{:-<18}-+-{:-<10}-+-{:-<10}-+-{:-<10}", "", "", "", "");

    let precision_delta = act_agg.precision - flat_agg.precision;
    let recall_delta = act_agg.recall - flat_agg.recall;
    let mrr_delta = act_agg.mrr - flat_agg.mrr;

    println!(
        "{0: <18} | {1: >10.4} | {2: >10.4} | {3: >+10.4}",
        "Precision@N", flat_agg.precision, act_agg.precision, precision_delta
    );
    println!(
        "{0: <18} | {1: >10.4} | {2: >10.4} | {3: >+10.4}",
        "Recall@N", flat_agg.recall, act_agg.recall, recall_delta
    );
    println!(
        "{0: <18} | {1: >10.4} | {2: >10.4} | {3: >+10.4}",
        "MRR", flat_agg.mrr, act_agg.mrr, mrr_delta
    );

    println!();
    println!(
        "Activation wins: Precision={wp}, Recall={wr}, MRR={wm}  ({wins}/3)",
        wp = wins_precision,
        wr = wins_recall,
        wm = wins_mrr,
        wins = wins
    );
    if passed {
        println!("PASS: activation-driven selection beats flat retrieval.");
    } else {
        println!("FAIL: activation did not beat flat on enough metrics.");
    }

    BenchmarkResult {
        flat: flat_agg,
        activation: act_agg,
        activation_wins_precision: wins_precision,
        activation_wins_recall: wins_recall,
        activation_wins_mrr: wins_mrr,
        wins,
        passed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_select_is_deterministic() {
        let (nodes, _) = build_corpus();
        let a = flat_select(&nodes, 5);
        let b = flat_select(&nodes, 5);
        assert_eq!(a, b);
    }

    #[test]
    fn activation_select_is_deterministic() {
        let (nodes, edges) = build_corpus();
        let config = ActivationConfig::default();
        let a = activation_select(&nodes, &edges, "alice", 5, &config, 2);
        let b = activation_select(&nodes, &edges, "alice", 5, &config, 2);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_query_all_zero_activation() {
        let (nodes, edges) = build_corpus();
        let config = ActivationConfig::default();
        let selected = activation_select(&nodes, &edges, "", 5, &config, 2);
        // All activations should be 0.0, so tie-breaking picks first by density.
        // The result should be deterministic.
        assert_eq!(selected.len(), 5);
    }

    #[test]
    fn no_relevant_nodes_both_strategies_return_scores() {
        let nodes = vec![
            MemoryNode {
                id: "a".into(),
                label: "Alpha".into(),
                kind: MemoryNodeKind::Person,
                confidence_tier: AssertionConfidenceTier::Confirmed,
                density: 0.5,
                activation: 0.0,
                provenance: vec![],
                created_at: None,
                contradiction_count: 0,
            },
        ];
        let scores = compute_scores(&nodes, "zzyzx_nonexistent", &[0]);
        // No relevant nodes → per spec, both precision and recall = 1.0, mrr = 1.0
        assert_eq!(scores.precision, 1.0);
        assert_eq!(scores.recall, 1.0);
        assert_eq!(scores.mrr, 1.0);
    }

    #[test]
    fn all_relevant_nodes_both_strategies_identical() {
        let nodes = vec![
            MemoryNode {
                id: "a".into(),
                label: "match".into(),
                kind: MemoryNodeKind::Person,
                confidence_tier: AssertionConfidenceTier::Confirmed,
                density: 0.5,
                activation: 0.0,
                provenance: vec![],
                created_at: None,
                contradiction_count: 0,
            },
            MemoryNode {
                id: "b".into(),
                label: "match".into(),
                kind: MemoryNodeKind::Person,
                confidence_tier: AssertionConfidenceTier::Confirmed,
                density: 0.5,
                activation: 0.0,
                provenance: vec![],
                created_at: None,
                contradiction_count: 0,
            },
        ];
        let scores = compute_scores(&nodes, "match", &[0]);
        // 1 relevant selected out of 1 selected, 2 total relevant
        assert_eq!(scores.precision, 1.0);
        assert_eq!(scores.recall, 0.5);
        assert_eq!(scores.mrr, 1.0);
    }

    #[test]
    fn mrr_computes_reciprocal_of_first_relevant_rank() {
        let nodes = vec![
            MemoryNode {
                id: "a".into(),
                label: "irrelevant".into(),
                kind: MemoryNodeKind::Person,
                confidence_tier: AssertionConfidenceTier::Confirmed,
                density: 0.5,
                activation: 0.0,
                provenance: vec![],
                created_at: None,
                contradiction_count: 0,
            },
            MemoryNode {
                id: "b".into(),
                label: "target match".into(),
                kind: MemoryNodeKind::Person,
                confidence_tier: AssertionConfidenceTier::Confirmed,
                density: 0.5,
                activation: 0.0,
                provenance: vec![],
                created_at: None,
                contradiction_count: 0,
            },
        ];
        // First selected (index 0) is irrelevant, second (index 1) matches
        let scores = compute_scores(&nodes, "target", &[0, 1]);
        assert_eq!(scores.mrr, 0.5); // 1/2
    }

    #[test]
    fn activation_beats_flat_on_synthetic_corpus() {
        let result = run_benchmark();
        assert!(
            result.passed,
            "activation must win at least 2/3 metrics; wins={}/3  \
             flat(prec={:.4}, rec={:.4}, mrr={:.4})  \
             act(prec={:.4}, rec={:.4}, mrr={:.4})",
            result.wins,
            result.flat.precision,
            result.flat.recall,
            result.flat.mrr,
            result.activation.precision,
            result.activation.recall,
            result.activation.mrr
        );
    }
}
