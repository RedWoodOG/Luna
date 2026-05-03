//! Integration test: every committed benchmark JSON parses cleanly under
//! schema v1, validates without violations, and survives a canonical
//! serialize/parse roundtrip byte-for-byte.
//!
//! This is the mechanical guard for PR 0.1. Recall behavior, scoring, and
//! extractor work are all out of scope here. The only invariant under
//! test is that the case files themselves are structurally honest.

use luna_bench::{validate_case, BenchmarkCase, BENCHMARK_SCHEMA_VERSION};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn benchmarks_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("benchmarks")
}

fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_json(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            out.push(path);
        }
    }
}

/// Recursively rewrites a JSON value with object keys in BTreeMap (sorted)
/// order. Pairs with [`serde_json::to_string_pretty`] to give a canonical
/// rendering that's stable across producers.
fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted: BTreeMap<String, Value> = BTreeMap::new();
            for (key, val) in map {
                sorted.insert(key, canonicalize(val));
            }
            let mut out = Map::new();
            for (key, val) in sorted {
                out.insert(key, val);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize).collect()),
        other => other,
    }
}

fn canonical_string(text: &str) -> String {
    let value: Value = serde_json::from_str(text).expect("benchmark file is valid JSON");
    let canonical = canonicalize(value);
    let mut rendered = serde_json::to_string_pretty(&canonical).unwrap();
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}

#[test]
fn every_benchmark_file_parses_validates_and_roundtrips() {
    let dir = benchmarks_dir();
    let mut files = Vec::new();
    collect_json(&dir, &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "expected benchmark files under {}",
        dir.display()
    );

    let mut violations: Vec<String> = Vec::new();

    for path in &files {
        let original = std::fs::read_to_string(path).unwrap();

        // Parse must succeed under the current Rust schema.
        let case: BenchmarkCase = match serde_json::from_str(&original) {
            Ok(case) => case,
            Err(err) => {
                violations.push(format!("{}: parse failed: {err}", path.display()));
                continue;
            }
        };

        // schema_version pinned to 1 by validate_case; assert here too so
        // a missing or coerced field that somehow slipped through is
        // surfaced loudly.
        assert_eq!(
            case.schema_version,
            BENCHMARK_SCHEMA_VERSION,
            "{}: schema_version must be {}",
            path.display(),
            BENCHMARK_SCHEMA_VERSION
        );

        // Validation must pass with no violations on a frozen case.
        let case_violations = validate_case(&case, path);
        if !case_violations.is_empty() {
            violations.extend(case_violations);
            continue;
        }

        // Canonical roundtrip: file -> canonical -> Rust -> canonical
        // must be byte-identical to file -> canonical.
        let original_canonical = canonical_string(&original);
        let rust_serialized = serde_json::to_string_pretty(&case).unwrap();
        let rust_canonical = canonical_string(&rust_serialized);

        if original_canonical != rust_canonical {
            violations.push(format!(
                "{}: canonical roundtrip mismatch\n--- on disk (canonical) ---\n{}\n--- after Rust roundtrip (canonical) ---\n{}",
                path.display(),
                original_canonical,
                rust_canonical
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "{} benchmark schema violation(s):\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
}

#[test]
fn every_temporal_case_is_proof_ineligible_until_pr_0_1b() {
    // Discipline guard: PR 0.1 commits to mechanical timestamps for the
    // five temporal cases and flags them ineligible. PR 0.1b is the
    // authorial pass that flips this to true alongside RATIONALE.md. If
    // someone flips proof_eligible without committing rationale, this
    // test fails until rationale lands.
    let dir = benchmarks_dir().join("temporal");
    if !dir.exists() {
        return;
    }
    let rationale = benchmarks_dir().join("temporal").join("RATIONALE.md");
    let mut files = Vec::new();
    collect_json(&dir, &mut files);
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let case: BenchmarkCase = serde_json::from_str(&text).unwrap();
        if case.proof_eligible {
            assert!(
                rationale.exists(),
                "{}: temporal case is proof_eligible=true but {} is missing; PR 0.1b must commit rationale before flipping eligibility",
                path.display(),
                rationale.display()
            );
        }
    }
}

#[test]
fn every_non_temporal_case_is_proof_eligible() {
    let dir = benchmarks_dir();
    let mut files = Vec::new();
    collect_json(&dir, &mut files);
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let case: BenchmarkCase = serde_json::from_str(&text).unwrap();
        if case.category != "temporal_disambiguation" {
            assert!(
                case.proof_eligible,
                "{}: non-temporal case must ship proof_eligible=true",
                path.display()
            );
        }
    }
}
