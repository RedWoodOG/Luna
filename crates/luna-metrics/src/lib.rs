use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BenchmarkReport {
    pub total_cases: usize,
    pub passed: usize,
    pub failed: usize,
    pub recall_accuracy: f32,
    pub false_memory_rate: f32,
    pub paraphrase_stability: f32,
    pub contradiction_accuracy: f32,
    pub stale_memory_suppression: f32,
    pub overclaim_rate: f32,
    pub uncertainty_accuracy: f32,
    pub mean_latency_ms: f32,
    pub p95_latency_ms: f32,
    /// Proof-eligible subset of the run.
    ///
    /// Computed only over cases with `proof_eligible == true`. `None` when no
    /// case in the run is proof-eligible. Run JSONs produced before PR 0.1
    /// deserialize with this field absent (treated as `None`).
    #[serde(default)]
    pub eligible: Option<BenchmarkSubreport>,
}

/// Mirror of [`BenchmarkReport`]'s scalar fields, computed over the
/// proof-eligible subset of cases. Kept as a separate type so the headline
/// totals and the proof-eligible numbers cannot drift out of sync through
/// callsite mistakes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BenchmarkSubreport {
    pub total_cases: usize,
    pub passed: usize,
    pub failed: usize,
    pub recall_accuracy: f32,
    pub false_memory_rate: f32,
    pub paraphrase_stability: f32,
    pub contradiction_accuracy: f32,
    pub stale_memory_suppression: f32,
    pub overclaim_rate: f32,
    pub uncertainty_accuracy: f32,
    pub mean_latency_ms: f32,
    pub p95_latency_ms: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseScore {
    pub id: String,
    pub category: String,
    pub passed: bool,
    pub recalled_expected: bool,
    pub false_memory: bool,
    pub overclaimed: bool,
    pub uncertainty_correct: bool,
    pub latency_ms: f32,
    pub claims: Vec<String>,
    /// Whether this case counts toward proof metrics in the manifest.
    /// Run JSONs produced before PR 0.1 deserialize with this field absent;
    /// the default of `false` correctly excludes pre-schema runs from any
    /// proof-eligible subreport.
    #[serde(default)]
    pub proof_eligible: bool,
}

pub fn summarize(cases: &[CaseScore]) -> BenchmarkReport {
    let totals = subreport(cases);
    let eligible_subset: Vec<CaseScore> = cases
        .iter()
        .filter(|case| case.proof_eligible)
        .cloned()
        .collect();
    let eligible = if eligible_subset.is_empty() {
        None
    } else {
        Some(subreport(&eligible_subset))
    };
    BenchmarkReport {
        total_cases: totals.total_cases,
        passed: totals.passed,
        failed: totals.failed,
        recall_accuracy: totals.recall_accuracy,
        false_memory_rate: totals.false_memory_rate,
        paraphrase_stability: totals.paraphrase_stability,
        contradiction_accuracy: totals.contradiction_accuracy,
        stale_memory_suppression: totals.stale_memory_suppression,
        overclaim_rate: totals.overclaim_rate,
        uncertainty_accuracy: totals.uncertainty_accuracy,
        mean_latency_ms: totals.mean_latency_ms,
        p95_latency_ms: totals.p95_latency_ms,
        eligible,
    }
}

fn subreport(cases: &[CaseScore]) -> BenchmarkSubreport {
    let total_cases = cases.len();
    if total_cases == 0 {
        return BenchmarkSubreport::default();
    }
    let passed = cases.iter().filter(|case| case.passed).count();
    let failed = total_cases - passed;
    let mut latencies = cases.iter().map(|case| case.latency_ms).collect::<Vec<_>>();
    latencies.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    BenchmarkSubreport {
        total_cases,
        passed,
        failed,
        recall_accuracy: ratio(
            cases.iter().filter(|case| case.recalled_expected).count(),
            total_cases,
        ),
        false_memory_rate: ratio(
            cases.iter().filter(|case| case.false_memory).count(),
            total_cases,
        ),
        paraphrase_stability: category_ratio(cases, "paraphrase_invariance", |case| {
            case.recalled_expected
        }),
        contradiction_accuracy: category_ratio(cases, "contradiction_handling", |case| case.passed),
        stale_memory_suppression: category_ratio(cases, "stale_memory_decay", |case| case.passed),
        overclaim_rate: ratio(
            cases.iter().filter(|case| case.overclaimed).count(),
            total_cases,
        ),
        uncertainty_accuracy: ratio(
            cases.iter().filter(|case| case.uncertainty_correct).count(),
            total_cases,
        ),
        mean_latency_ms: latencies.iter().sum::<f32>() / latencies.len() as f32,
        p95_latency_ms: percentile(&latencies, 0.95),
    }
}

pub fn markdown(report: &BenchmarkReport) -> String {
    let mut out = format!(
        "# Luna Benchmark Report\n\n\
         Total cases: {}\n\n\
         | Metric | Value |\n\
         |---|---:|\n\
         | Passed | {} |\n\
         | Failed | {} |\n\
         | Recall accuracy | {:.1}% |\n\
         | False memory rate | {:.1}% |\n\
         | Paraphrase stability | {:.1}% |\n\
         | Contradiction handling | {:.1}% |\n\
         | Stale memory suppression | {:.1}% |\n\
         | Overclaim rate | {:.1}% |\n\
         | Uncertainty correctness | {:.1}% |\n\
         | Average recall latency | {:.2}ms |\n\
         | P95 recall latency | {:.2}ms |\n",
        report.total_cases,
        report.passed,
        report.failed,
        pct(report.recall_accuracy),
        pct(report.false_memory_rate),
        pct(report.paraphrase_stability),
        pct(report.contradiction_accuracy),
        pct(report.stale_memory_suppression),
        pct(report.overclaim_rate),
        pct(report.uncertainty_accuracy),
        report.mean_latency_ms,
        report.p95_latency_ms,
    );
    match &report.eligible {
        Some(eligible) => {
            out.push_str(&format!(
                "\n## Proof-eligible subset\n\nProof-eligible cases: {}\n\n\
                 | Metric | Value |\n\
                 |---|---:|\n\
                 | Passed | {} |\n\
                 | Failed | {} |\n\
                 | Recall accuracy | {:.1}% |\n\
                 | False memory rate | {:.1}% |\n\
                 | Paraphrase stability | {:.1}% |\n\
                 | Contradiction handling | {:.1}% |\n\
                 | Stale memory suppression | {:.1}% |\n\
                 | Overclaim rate | {:.1}% |\n\
                 | Uncertainty correctness | {:.1}% |\n\
                 | Average recall latency | {:.2}ms |\n\
                 | P95 recall latency | {:.2}ms |\n",
                eligible.total_cases,
                eligible.passed,
                eligible.failed,
                pct(eligible.recall_accuracy),
                pct(eligible.false_memory_rate),
                pct(eligible.paraphrase_stability),
                pct(eligible.contradiction_accuracy),
                pct(eligible.stale_memory_suppression),
                pct(eligible.overclaim_rate),
                pct(eligible.uncertainty_accuracy),
                eligible.mean_latency_ms,
                eligible.p95_latency_ms,
            ));
            if eligible.total_cases < report.total_cases {
                out.push_str(&format!(
                    "\n_NOT PROOF-COUNTED: {} case(s) excluded from the proof-eligible subset._\n",
                    report.total_cases - eligible.total_cases
                ));
            }
        }
        None => {
            out.push_str(
                "\n## Proof-eligible subset\n\n\
                 NOT PROOF-COUNTED: no proof-eligible cases in this run.\n",
            );
        }
    }
    out
}

fn category_ratio(cases: &[CaseScore], category: &str, pred: impl Fn(&CaseScore) -> bool) -> f32 {
    let matching = cases
        .iter()
        .filter(|case| case.category == category)
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return 0.0;
    }
    ratio(
        matching.iter().filter(|case| pred(case)).count(),
        matching.len(),
    )
}

fn ratio(count: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        count as f32 / total as f32
    }
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f32 * p).ceil() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn pct(value: f32) -> f32 {
    value * 100.0
}
