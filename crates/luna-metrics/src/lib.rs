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
}

pub fn summarize(cases: &[CaseScore]) -> BenchmarkReport {
    let total_cases = cases.len();
    if total_cases == 0 {
        return BenchmarkReport::default();
    }
    let passed = cases.iter().filter(|case| case.passed).count();
    let failed = total_cases - passed;
    let mut latencies = cases.iter().map(|case| case.latency_ms).collect::<Vec<_>>();
    latencies.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    BenchmarkReport {
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
    format!(
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
    )
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
