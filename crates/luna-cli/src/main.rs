use clap::{Parser, Subcommand, ValueEnum};
use luna_core::EngineKind;
use std::{path::PathBuf, str::FromStr};

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
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReportFormat {
    Markdown,
    Json,
}

fn main() -> anyhow::Result<()> {
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
            BenchCommand::Compare { run_dir } => {
                let keyword = luna_bench::load_run(&run_dir, EngineKind::Keyword).ok();
                let tcf = luna_bench::load_run(&run_dir, EngineKind::Tcf).ok();
                match (keyword, tcf) {
                    (Some(keyword), Some(tcf)) => {
                        println!("# Luna Engine Compare\n");
                        println!("| Metric | Keyword | TCF | Delta |");
                        println!("|---|---:|---:|---:|");
                        print_delta(
                            "Recall accuracy",
                            keyword.report.recall_accuracy,
                            tcf.report.recall_accuracy,
                        );
                        print_delta(
                            "False memory rate",
                            keyword.report.false_memory_rate,
                            tcf.report.false_memory_rate,
                        );
                        print_delta(
                            "Overclaim rate",
                            keyword.report.overclaim_rate,
                            tcf.report.overclaim_rate,
                        );
                        print_delta(
                            "Mean latency ms",
                            keyword.report.mean_latency_ms,
                            tcf.report.mean_latency_ms,
                        );
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
    Ok(())
}

fn print_delta(label: &str, keyword: f32, tcf: f32) {
    println!(
        "| {label} | {:.2} | {:.2} | {:+.2} |",
        keyword,
        tcf,
        tcf - keyword
    );
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
