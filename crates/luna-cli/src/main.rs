use clap::{Parser, Subcommand, ValueEnum};
use luna_core::EngineKind;
use luna_extract::{
    CountingBackend, FileExtractionCache, FixtureBackend, LlmExtractor, LunaExtractor,
};
use luna_metrics::{BenchmarkReport, BenchmarkSubreport};
use std::{path::PathBuf, process::ExitCode, str::FromStr};

#[derive(Debug, Parser)]
#[command(name = "luna")]
#[command(about = "Luna episodic memory benchmark CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Bench {
        #[command(subcommand)]
        command: BenchCommand,
    },
    Report {
        run_dir: PathBuf,
        #[arg(long, value_enum, default_value = "markdown")]
        format: ReportFormat,
        #[arg(long, default_value = "tcf")]
        engine: String,
    },
}

#[derive(Debug, Subcommand)]
enum BenchCommand {
    Run {
        benchmarks: PathBuf,
        #[arg(long, default_value = "tcf")]
        engine: String,
        #[arg(long, default_value = "runs/latest")]
        out: PathBuf,
        #[arg(long)]
        explain: bool,
    },
    Compare {
        run_dir: PathBuf,
        /// Strict mode. Compare on the proof-eligible subreport only and
        /// exit with code 2 if any case in either engine's run is not
        /// proof-eligible. Auto-enables in a future PR when a non-draft
        /// `benchmarks/manifest.json` is present; for PR 0.1 only the
        /// explicit flag activates strict mode (no non-draft manifest
        /// exists yet).
        #[arg(long)]
        require_proof_eligible: bool,
    },
    /// Run the Stage 0 formation report. Proves benchmark cases can
    /// enter the recall path with valid, provenance-backed episodes
    /// without invoking any recall engine. Exits non-zero (code 3) if
    /// any case fails formation.
    Formation {
        benchmarks: PathBuf,
        /// Directory of pre-authored fixture extraction JSONs. Each
        /// fixture file is keyed by SHA-256 of the rendered prompt.
        /// Required until a real LLM backend is wired (PR 0.6 / 0.3b).
        #[arg(long)]
        fixtures: PathBuf,
        /// Cache root for the formation run. The second pass reads
        /// from here and a 100% hit rate is the formation gate.
        #[arg(long, default_value = ".luna/formation_cache")]
        cache: PathBuf,
        /// Output directory for the formation report JSON.
        #[arg(long, default_value = "runs/latest")]
        out: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReportFormat {
    Markdown,
    Json,
}

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Bench { command } => match command {
            BenchCommand::Run {
                benchmarks,
                engine,
                out,
                explain,
            } => {
                let engine = EngineKind::from_str(&engine)?;
                let run = luna_bench::run_benchmarks_with_options(
                    &benchmarks,
                    &out,
                    engine,
                    luna_bench::RunOptions { explain },
                )?;
                println!("{}", luna_metrics::markdown(&run.output.report));
                println!("Wrote {}", out.join(format!("{engine}.json")).display());
                if explain {
                    println!(
                        "Wrote {}",
                        out.join(format!("{engine}-explain.json")).display()
                    );
                    print_explanations(&run.explanations);
                }
            }
            BenchCommand::Formation {
                benchmarks,
                fixtures,
                cache,
                out,
            } => {
                let exit = run_formation_command(&benchmarks, &fixtures, &cache, &out)?;
                return Ok(exit);
            }
            BenchCommand::Compare {
                run_dir,
                require_proof_eligible,
            } => {
                let keyword = luna_bench::load_run(&run_dir, EngineKind::Keyword).ok();
                let tcf = luna_bench::load_run(&run_dir, EngineKind::Tcf).ok();
                match (keyword, tcf) {
                    (Some(keyword), Some(tcf)) => {
                        let exit =
                            print_compare(&keyword.report, &tcf.report, require_proof_eligible);
                        return Ok(exit);
                    }
                    _ => {
                        println!("Run both engines first:");
                        println!("luna bench run ./benchmarks --engine keyword");
                        println!("luna bench run ./benchmarks --engine tcf");
                    }
                }
            }
        },
        Command::Report {
            run_dir,
            format,
            engine,
        } => {
            let engine = EngineKind::from_str(&engine)?;
            let output = luna_bench::load_run(&run_dir, engine)?;
            match format {
                ReportFormat::Markdown => println!("{}", luna_metrics::markdown(&output.report)),
                ReportFormat::Json => println!("{}", serde_json::to_string_pretty(&output.report)?),
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn print_delta(label: &str, keyword: f32, tcf: f32) {
    println!(
        "| {label} | {:.2} | {:.2} | {:+.2} |",
        keyword,
        tcf,
        tcf - keyword
    );
}

/// Renders the compare table in either strict (proof-eligible only) or
/// lenient (all-cases totals) mode. Returns the exit code the CLI should
/// hand back to the shell: success in lenient mode, code 2 in strict mode
/// when either run contains ineligible cases or has no eligible subset.
fn print_compare(
    keyword: &BenchmarkReport,
    tcf: &BenchmarkReport,
    require_proof_eligible: bool,
) -> ExitCode {
    let keyword_ineligible = ineligible_count(keyword);
    let tcf_ineligible = ineligible_count(tcf);
    let any_ineligible = keyword_ineligible > 0 || tcf_ineligible > 0;

    if require_proof_eligible {
        let (k, t) = match (&keyword.eligible, &tcf.eligible) {
            (Some(k), Some(t)) => (k, t),
            _ => {
                eprintln!(
                    "--require-proof-eligible: at least one engine has no proof-eligible cases in this run."
                );
                return ExitCode::from(2);
            }
        };
        if any_ineligible {
            eprintln!(
                "--require-proof-eligible: {} keyword case(s) and {} TCF case(s) are not proof-eligible. Run aborted; results are non-publishable until all cases are eligible.",
                keyword_ineligible, tcf_ineligible
            );
            return ExitCode::from(2);
        }
        println!("# Luna Engine Compare (proof-eligible only)\n");
        print_compare_table(k, t);
        ExitCode::SUCCESS
    } else {
        if any_ineligible {
            eprintln!(
                "warning: {} keyword case(s) and {} TCF case(s) are not proof-eligible. \
                 Use --require-proof-eligible for strict comparison; numbers below \
                 are TOTALS over all cases and are NOT proof-counted.",
                keyword_ineligible, tcf_ineligible
            );
        }
        println!("# Luna Engine Compare\n");
        print_compare_table_total(keyword, tcf);
        ExitCode::SUCCESS
    }
}

fn print_compare_table(keyword: &BenchmarkSubreport, tcf: &BenchmarkSubreport) {
    println!("| Metric | Keyword | TCF | Delta |");
    println!("|---|---:|---:|---:|");
    print_delta(
        "Recall accuracy",
        keyword.recall_accuracy,
        tcf.recall_accuracy,
    );
    print_delta(
        "False memory rate",
        keyword.false_memory_rate,
        tcf.false_memory_rate,
    );
    print_delta("Overclaim rate", keyword.overclaim_rate, tcf.overclaim_rate);
    print_delta(
        "Mean latency ms",
        keyword.mean_latency_ms,
        tcf.mean_latency_ms,
    );
}

fn print_compare_table_total(keyword: &BenchmarkReport, tcf: &BenchmarkReport) {
    println!("| Metric | Keyword | TCF | Delta |");
    println!("|---|---:|---:|---:|");
    print_delta(
        "Recall accuracy",
        keyword.recall_accuracy,
        tcf.recall_accuracy,
    );
    print_delta(
        "False memory rate",
        keyword.false_memory_rate,
        tcf.false_memory_rate,
    );
    print_delta("Overclaim rate", keyword.overclaim_rate, tcf.overclaim_rate);
    print_delta(
        "Mean latency ms",
        keyword.mean_latency_ms,
        tcf.mean_latency_ms,
    );
}

fn ineligible_count(report: &BenchmarkReport) -> usize {
    let eligible = report
        .eligible
        .as_ref()
        .map(|sub| sub.total_cases)
        .unwrap_or(0);
    report.total_cases.saturating_sub(eligible)
}

/// Wires the FixtureBackend + cache + LunaExtractor and runs the
/// formation engine end-to-end. Returns the exit code the CLI hands
/// back to the shell: 0 on green, 3 on any case failing formation.
fn run_formation_command(
    benchmarks: &std::path::Path,
    fixtures: &std::path::Path,
    cache: &std::path::Path,
    out: &std::path::Path,
) -> anyhow::Result<ExitCode> {
    if !fixtures.exists() {
        anyhow::bail!(
            "fixture directory does not exist: {}\n\
             Authoring fixtures: each fixture is one JSON file at\n\
             <fixtures>/<sha256_of_rendered_prompt>.json containing the\n\
             would-be LLM response (an LlmObservation) for that turn.",
            fixtures.display()
        );
    }
    std::fs::create_dir_all(cache)?;
    std::fs::create_dir_all(out)?;

    let backend = CountingBackend::new(FixtureBackend::new(fixtures));
    let llm = LlmExtractor::new(backend, FileExtractionCache::new(cache));
    let extractor = LunaExtractor::with_default_v1_sources(llm);

    let report = luna_bench::run_formation(benchmarks, &extractor)?;

    println!("{}", luna_bench::formation_markdown(&report));

    let report_path = out.join("formation.json");
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    println!("Wrote {}", report_path.display());

    if report.formation_eligible < report.total_cases {
        eprintln!();
        eprintln!("{}", luna_bench::formation_failure_summary(&report));
        return Ok(ExitCode::from(3));
    }
    Ok(ExitCode::SUCCESS)
}

fn print_explanations(explanations: &[luna_bench::CaseExplanation]) {
    for explanation in explanations
        .iter()
        .filter(|explanation| !explanation.passed)
    {
        println!("\n=== {} [FAIL] ===", explanation.id);
        println!("Stored episodes:");
        if explanation.stored_episodes.is_empty() {
            println!("  (none)");
        }
        for episode in &explanation.stored_episodes {
            println!(
                "  {}: {}",
                episode.episode_id,
                episode.assertions.join(", ")
            );
            let contour = episode
                .contour
                .iter()
                .map(|dimension| match dimension.value {
                    Some(value) => format!(
                        "{}=Some({:.2}, conf={:.2}, sources={})",
                        dimension.name,
                        value,
                        dimension.confidence.unwrap_or_default(),
                        dimension.sources.unwrap_or_default()
                    ),
                    None => format!("{}=None", dimension.name),
                })
                .collect::<Vec<_>>()
                .join(", ");
            println!("        contour: {contour}");
        }

        println!(
            "Probe: {}",
            explanation.probe.as_deref().unwrap_or("(none)")
        );
        println!("Recall mode selected: {:?}", explanation.recall_mode);
        println!("Dimension weights: semantic=0.10, intent=0.13, assertion_fit=0.22, attention=0.10, goal=0.12, identity=0.14, trust=0.08, social=0.06, emotional_arousal=0.05, coherence=0.10");
        println!("Scoring:");
        if explanation.candidates.is_empty() {
            println!("  (no candidates)");
        }
        for candidate in &explanation.candidates {
            let contributions = candidate
                .breakdown
                .as_ref()
                .map(|breakdown| {
                    breakdown
                        .contributions
                        .iter()
                        .filter(|contribution| contribution.contribution > 0.0)
                        .map(|contribution| {
                            format!("{} {:.3}", contribution.name, contribution.contribution)
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            println!(
                "  {} -> {:.3} ({})",
                candidate.episode_id, candidate.score, contributions
            );
        }
        match explanation.top_candidate {
            Some(episode_id) => println!("Top candidate: {episode_id}"),
            None => println!("Top candidate: (none)"),
        }
        for needle in &explanation.must_recall_matches {
            println!(
                "must_recall match: {:?} -> {}",
                needle.needle,
                if needle.matched { "HIT" } else { "MISS" }
            );
        }
        for needle in &explanation.must_not_claim_matches {
            println!(
                "must_not_claim match: {:?} -> {}",
                needle.needle,
                if needle.matched { "HIT" } else { "MISS" }
            );
        }
        println!("Verdict: {}", explanation.verdict);
    }
}
