//! Integration test: activation benchmark gate.
//!
//! Runs the full synthetic-corpus benchmark and asserts that
//! activation-driven selection beats flat retrieval on at least
//! 2 of 3 aggregate metrics (precision@N, recall@N, MRR).
//!
//! If activation loses, the test fails — this is the gate.

use luna_bench::activation_bench::run_benchmark;

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
