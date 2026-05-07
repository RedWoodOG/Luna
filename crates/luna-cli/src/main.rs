use clap::{Parser, Subcommand, ValueEnum};
use luna_core::EngineKind;
use luna_extract::{
    CommandBackend, CountingBackend, FileExtractionCache, FixtureBackend, FusedExtractor,
    LlmBackend, LlmExtractor, LunaExtractor,
};
use luna_gauges::{calibrate_thresholds, GaugeReadingLog};
use luna_metrics::{BenchmarkReport, BenchmarkSubreport};
use luna_runtime::{
    default_runtime_log_path, render_conversation_reply, render_runtime_markdown, RuntimeExtractor,
    RuntimeSession,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    str::FromStr,
    time::Duration,
};

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
    /// Product-track runtime commands. These are intentionally
    /// separate from `bench`: they let Luna learn from live turns and
    /// expose what it stored without making proof claims.
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    Gauges {
        #[command(subcommand)]
        command: GaugesCommand,
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
enum GaugesCommand {
    /// Suggest gauge thresholds from historical gauge reading JSON.
    Calibrate {
        /// JSONL file containing one GaugeReading record per line.
        input: PathBuf,
        /// Output JSON file for threshold suggestions.
        #[arg(long)]
        out: PathBuf,
        /// Multiplier applied to observed standard deviation.
        #[arg(long, default_value = "3.0")]
        multiplier: f64,
    },
}

#[derive(Debug, Subcommand)]
enum RuntimeCommand {
    /// Process one user statement through the live memory loop.
    Turn {
        content: String,
        /// JSONL event log. Defaults to `.luna/runtime/events.jsonl`.
        #[arg(long)]
        log: Option<PathBuf>,
        /// Extractor path for the live turn. `heuristic` is fully
        /// local and deterministic; `command` invokes an LLM wrapper
        /// through the same extraction path used by formation.
        #[arg(long, value_enum, default_value = "heuristic")]
        extractor: RuntimeExtractorChoice,
        /// Path to the external program when `--extractor command`.
        /// The program receives the rendered extraction prompt on
        /// stdin and must emit a JSON LlmObservation on stdout.
        #[arg(long)]
        command: Option<PathBuf>,
        /// Opaque determinism key for `--extractor command`. Encode
        /// every flag that affects output into this string so the
        /// extraction cache invalidates when those flags change.
        #[arg(long = "model-id")]
        model_id: Option<String>,
        /// Arguments passed verbatim to `--command`. Repeat the flag
        /// for each argument. Example:
        /// `--command-arg .\scripts\run_llama_server_extract.py`.
        #[arg(long = "command-arg")]
        command_args: Vec<String>,
        /// Per-call timeout in seconds for the command extractor.
        #[arg(long = "timeout-secs", default_value = "120")]
        timeout_secs: u64,
        /// Cache root for command-backed runtime extraction.
        #[arg(long, default_value = ".luna/runtime_cache")]
        cache: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value = "markdown")]
        format: ReportFormat,
    },
    /// Start an interactive runtime loop. Type `exit` or `quit` to stop.
    Chat {
        /// JSONL event log. Defaults to `.luna/runtime/events.jsonl`.
        #[arg(long)]
        log: Option<PathBuf>,
        /// Extractor path for each live turn.
        #[arg(long, value_enum, default_value = "heuristic")]
        extractor: RuntimeExtractorChoice,
        /// Path to the external program when `--extractor command`.
        #[arg(long)]
        command: Option<PathBuf>,
        /// Opaque determinism key for `--extractor command`.
        #[arg(long = "model-id")]
        model_id: Option<String>,
        /// Arguments passed verbatim to `--command`. Repeat the flag
        /// for each argument.
        #[arg(long = "command-arg")]
        command_args: Vec<String>,
        /// Per-call timeout in seconds for the command extractor.
        #[arg(long = "timeout-secs", default_value = "120")]
        timeout_secs: u64,
        /// Cache root for command-backed runtime extraction.
        #[arg(long, default_value = ".luna/runtime_cache")]
        cache: PathBuf,
        /// Output format for each turn.
        #[arg(long, value_enum, default_value = "markdown")]
        format: ReportFormat,
        /// Also print the full memory workbench after Luna's reply.
        #[arg(long)]
        workbench: bool,
    },
    /// Inspect the rebuilt memory state from the JSONL event log.
    Inspect {
        /// JSONL event log. Defaults to `.luna/runtime/events.jsonl`.
        #[arg(long)]
        log: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value = "markdown")]
        format: ReportFormat,
    },
    /// Run a scripted runtime memory scenario against a fresh log.
    Scenario {
        /// Scenario JSON file.
        scenario: PathBuf,
        /// JSONL event log. Defaults to `.luna/runtime/scenario/<name>.jsonl`.
        #[arg(long)]
        log: Option<PathBuf>,
        /// Extractor path for each scenario turn.
        #[arg(long, value_enum, default_value = "heuristic")]
        extractor: RuntimeExtractorChoice,
        /// Path to the external program when `--extractor command`.
        #[arg(long)]
        command: Option<PathBuf>,
        /// Opaque determinism key for `--extractor command`.
        #[arg(long = "model-id")]
        model_id: Option<String>,
        /// Arguments passed verbatim to `--command`. Repeat the flag
        /// for each argument.
        #[arg(long = "command-arg")]
        command_args: Vec<String>,
        /// Per-call timeout in seconds for the command extractor.
        #[arg(long = "timeout-secs", default_value = "120")]
        timeout_secs: u64,
        /// Cache root for command-backed runtime extraction.
        #[arg(long, default_value = ".luna/runtime_cache")]
        cache: PathBuf,
        /// Keep the scenario event log instead of deleting it first.
        #[arg(long)]
        keep_log: bool,
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
        /// Backend that produces LlmObservations. `fixture` reads
        /// pre-authored JSON files from `--fixtures`; `command`
        /// invokes an external process supplied by `--command`.
        #[arg(long, default_value = "fixture")]
        backend: String,
        /// Directory of pre-authored fixture extraction JSONs. Each
        /// fixture file is keyed by SHA-256 of the rendered prompt.
        /// Required when `--backend fixture`.
        #[arg(long)]
        fixtures: Option<PathBuf>,
        /// Path to the external program when `--backend command`. The
        /// program receives the prompt on stdin and must emit a JSON
        /// LlmObservation on stdout.
        #[arg(long)]
        command: Option<PathBuf>,
        /// Opaque determinism key for the command backend. Encode
        /// every flag that affects output into this string so the
        /// extraction cache invalidates when those flags change.
        /// Required when `--backend command`.
        #[arg(long = "model-id")]
        model_id: Option<String>,
        /// Arguments passed verbatim to `--command`. Repeat the flag
        /// for each argument. Example:
        /// `--command-arg --temp --command-arg 0`.
        #[arg(long = "command-arg")]
        command_args: Vec<String>,
        /// Per-call timeout in seconds for the command backend.
        #[arg(long = "timeout-secs", default_value = "120")]
        timeout_secs: u64,
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RuntimeExtractorChoice {
    Heuristic,
    Command,
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
                backend,
                fixtures,
                command,
                model_id,
                command_args,
                timeout_secs,
                cache,
                out,
            } => {
                let exit = run_formation_command(
                    &benchmarks,
                    &backend,
                    fixtures.as_deref(),
                    command.as_deref(),
                    model_id.as_deref(),
                    &command_args,
                    timeout_secs,
                    &cache,
                    &out,
                )?;
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
        Command::Runtime { command } => match command {
            RuntimeCommand::Turn {
                content,
                log,
                extractor,
                command,
                model_id,
                command_args,
                timeout_secs,
                cache,
                format,
            } => {
                let log = log.unwrap_or_else(|| default_runtime_log_path(&PathBuf::from(".")));
                match extractor {
                    RuntimeExtractorChoice::Heuristic => {
                        run_runtime_turn(&log, FusedExtractor::new(), content, format)?;
                    }
                    RuntimeExtractorChoice::Command => {
                        let extractor = build_command_runtime_extractor(
                            command,
                            model_id,
                            command_args,
                            timeout_secs,
                            cache,
                        )?;
                        run_runtime_turn(&log, extractor, content, format)?;
                    }
                }
                println!("Wrote event log {}", log.display());
            }
            RuntimeCommand::Chat {
                log,
                extractor,
                command,
                model_id,
                command_args,
                timeout_secs,
                cache,
                format,
                workbench,
            } => {
                let log = log.unwrap_or_else(|| default_runtime_log_path(&PathBuf::from(".")));
                match extractor {
                    RuntimeExtractorChoice::Heuristic => {
                        run_runtime_chat(&log, FusedExtractor::new(), format, workbench)?;
                    }
                    RuntimeExtractorChoice::Command => {
                        let extractor = build_command_runtime_extractor(
                            command,
                            model_id,
                            command_args,
                            timeout_secs,
                            cache,
                        )?;
                        run_runtime_chat(&log, extractor, format, workbench)?;
                    }
                }
            }
            RuntimeCommand::Inspect { log, format } => {
                let log = log.unwrap_or_else(|| default_runtime_log_path(&PathBuf::from(".")));
                let session = RuntimeSession::new(&log, FusedExtractor::new());
                let state = session.inspect()?;
                match format {
                    ReportFormat::Markdown => print_memory_state(&state),
                    ReportFormat::Json => println!("{}", serde_json::to_string_pretty(&state)?),
                }
            }
            RuntimeCommand::Scenario {
                scenario,
                log,
                extractor,
                command,
                model_id,
                command_args,
                timeout_secs,
                cache,
                keep_log,
            } => {
                let log = log
                    .unwrap_or_else(|| default_scenario_log_path(&PathBuf::from("."), &scenario));
                match extractor {
                    RuntimeExtractorChoice::Heuristic => {
                        run_runtime_scenario(&scenario, &log, FusedExtractor::new(), keep_log)?;
                    }
                    RuntimeExtractorChoice::Command => {
                        let extractor = build_command_runtime_extractor(
                            command,
                            model_id,
                            command_args,
                            timeout_secs,
                            cache,
                        )?;
                        run_runtime_scenario(&scenario, &log, extractor, keep_log)?;
                    }
                }
            }
        },
        Command::Gauges { command } => match command {
            GaugesCommand::Calibrate {
                input,
                out,
                multiplier,
            } => {
                let readings = GaugeReadingLog::load_jsonl(&input)?;
                let suggestions = calibrate_thresholds(&readings, multiplier);
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&out, serde_json::to_string_pretty(&suggestions)?)?;
                println!("Wrote gauge threshold suggestions {}", out.display());
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

fn run_runtime_turn<E: RuntimeExtractor>(
    log: &Path,
    extractor: E,
    content: String,
    format: ReportFormat,
) -> anyhow::Result<()> {
    let session = RuntimeSession::new(log, extractor);
    let result = session.process_user_turn(content)?;
    match format {
        ReportFormat::Markdown => println!("{}", render_runtime_markdown(&result)),
        ReportFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}

fn run_runtime_chat<E: RuntimeExtractor>(
    log: &Path,
    extractor: E,
    format: ReportFormat,
    workbench: bool,
) -> anyhow::Result<()> {
    let session = RuntimeSession::new(log, extractor);
    println!("# Luna Runtime Chat");
    println!("Event log: {}", log.display());
    println!("Type a turn and press Enter. Type `exit` or `quit` to stop.\n");

    let stdin = io::stdin();
    loop {
        print!("luna> ");
        io::stdout().flush()?;

        let mut content = String::new();
        let bytes = stdin.read_line(&mut content)?;
        if bytes == 0 {
            println!();
            break;
        }

        let content = content.trim();
        if content.eq_ignore_ascii_case("exit") || content.eq_ignore_ascii_case("quit") {
            break;
        }
        if content.is_empty() {
            continue;
        }

        let result = session.process_user_turn(content.to_string())?;
        match format {
            ReportFormat::Markdown => {
                println!("Luna: {}", render_conversation_reply(content, &result));
                if workbench {
                    println!("\n{}", render_runtime_markdown(&result));
                }
            }
            ReportFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
        }
    }

    println!("Wrote event log {}", log.display());
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RuntimeScenarioFile {
    #[serde(default)]
    name: Option<String>,
    turns: Vec<String>,
    #[serde(default)]
    checks: RuntimeScenarioChecks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct RuntimeScenarioChecks {
    #[serde(default)]
    must_contain: Vec<String>,
    #[serde(default)]
    must_not_contain: Vec<String>,
}

fn run_runtime_scenario<E: RuntimeExtractor>(
    scenario_path: &Path,
    log: &Path,
    extractor: E,
    keep_log: bool,
) -> anyhow::Result<()> {
    if !keep_log && log.exists() {
        fs::remove_file(log)?;
    }
    let text = fs::read_to_string(scenario_path)?;
    let scenario: RuntimeScenarioFile = serde_json::from_str(&text)?;
    let session = RuntimeSession::new(log, extractor);

    println!(
        "# Luna Runtime Scenario: {}\n",
        scenario.name.as_deref().unwrap_or("unnamed")
    );
    println!("Turns: {}", scenario.turns.len());
    println!("Event log: {}\n", log.display());

    for (index, turn) in scenario.turns.iter().enumerate() {
        let result = session.process_user_turn(turn.clone())?;
        println!(
            "{}. {} assertion(s), {} working node(s)",
            index + 1,
            result.observation.assertions.len(),
            result.working_memory.nodes.len()
        );
    }

    let state = session.inspect()?;
    let memory_text = scenario_memory_text(&state);
    let mut failures = Vec::new();
    for needle in &scenario.checks.must_contain {
        if !contains_ci(&memory_text, needle) {
            failures.push(format!("missing required memory: {needle}"));
        }
    }
    for needle in &scenario.checks.must_not_contain {
        if contains_ci(&memory_text, needle) {
            failures.push(format!("forbidden memory present: {needle}"));
        }
    }

    if failures.is_empty() {
        println!(
            "\nPASS: {} memory check(s)",
            scenario_check_count(&scenario)
        );
        Ok(())
    } else {
        println!("\nFAIL: {} issue(s)", failures.len());
        for failure in &failures {
            println!("- {failure}");
        }
        anyhow::bail!("runtime scenario failed")
    }
}

fn scenario_memory_text(state: &luna_runtime::MemoryState) -> String {
    state
        .claims
        .iter()
        .map(|claim| format!("{}:{}={}", claim.domain, claim.kind, claim.value))
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario_check_count(scenario: &RuntimeScenarioFile) -> usize {
    scenario.checks.must_contain.len() + scenario.checks.must_not_contain.len()
}

fn contains_ci(text: &str, needle: &str) -> bool {
    text.to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn default_scenario_log_path(root: &Path, scenario_path: &Path) -> PathBuf {
    let stem = scenario_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("scenario");
    root.join(".luna")
        .join("runtime")
        .join("scenario")
        .join(format!("{stem}.jsonl"))
}

fn build_command_runtime_extractor(
    command: Option<PathBuf>,
    model_id: Option<String>,
    command_args: Vec<String>,
    timeout_secs: u64,
    cache: PathBuf,
) -> anyhow::Result<LunaExtractor<CommandBackend, FileExtractionCache>> {
    let command =
        command.ok_or_else(|| anyhow::anyhow!("--extractor command requires --command <path>"))?;
    let model_id = model_id.ok_or_else(|| {
        anyhow::anyhow!(
            "--extractor command requires --model-id <id>; encode every\n\
             flag that affects determinism (sampling, seed, CPU vs GPU,\n\
             quantization) into this string so the cache invalidates\n\
             correctly when those change."
        )
    })?;
    let command_backend = CommandBackend::new(command, command_args, model_id)
        .with_timeout(Duration::from_secs(timeout_secs));
    let llm = LlmExtractor::new(command_backend, FileExtractionCache::new(cache));
    Ok(LunaExtractor::with_default_v1_sources(llm))
}

fn print_memory_state(state: &luna_runtime::MemoryState) {
    println!("# Luna Memory State\n");
    if state.claims.is_empty() {
        println!("(no claims yet)");
        return;
    }

    if !state.entity_groups.is_empty() {
        println!("## Entity Memory\n");
        for group in &state.entity_groups {
            println!("### {} ({})", group.label, group.kind);
            for claim in &group.claims {
                println!(
                    "- {:?}: {}:{} = {}",
                    claim.status, claim.domain, claim.kind, claim.value
                );
            }
            println!();
        }
    }

    println!("## Flat Claims\n");
    for claim in &state.claims {
        println!(
            "- {:?}: {}:{} = {}",
            claim.status, claim.domain, claim.kind, claim.value
        );
    }
    println!(
        "\nMemory map: {} node(s), {} edge(s)",
        state.map.nodes.len(),
        state.map.edges.len()
    );
    for edge in state.map.edges.iter().take(12) {
        println!(
            "- {:?}: {} -> {} ({:?}, strength {:.2})",
            edge.confidence_tier, edge.source, edge.target, edge.relation, edge.strength
        );
    }
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

/// Build the chosen backend (fixture or command), wrap it in
/// CountingBackend + cache + LunaExtractor, and run the formation
/// engine end-to-end. Returns 0 on green, 3 on any case failing
/// formation.
#[allow(clippy::too_many_arguments)]
fn run_formation_command(
    benchmarks: &std::path::Path,
    backend_choice: &str,
    fixtures: Option<&std::path::Path>,
    command: Option<&std::path::Path>,
    model_id: Option<&str>,
    command_args: &[String],
    timeout_secs: u64,
    cache: &std::path::Path,
    out: &std::path::Path,
) -> anyhow::Result<ExitCode> {
    let backend: Box<dyn LlmBackend> = match backend_choice {
        "fixture" => {
            let fixtures = fixtures
                .ok_or_else(|| anyhow::anyhow!("--backend fixture requires --fixtures <dir>"))?;
            if !fixtures.exists() {
                anyhow::bail!(
                    "fixture directory does not exist: {}\n\
                     Authoring fixtures: each fixture is one JSON file at\n\
                     <fixtures>/<sha256_of_rendered_prompt>.json containing the\n\
                     would-be LLM response (an LlmObservation) for that turn.",
                    fixtures.display()
                );
            }
            Box::new(FixtureBackend::new(fixtures))
        }
        "command" => {
            let command = command
                .ok_or_else(|| anyhow::anyhow!("--backend command requires --command <path>"))?;
            let model_id = model_id.ok_or_else(|| {
                anyhow::anyhow!(
                    "--backend command requires --model-id <id>; encode every\n\
                     flag that affects determinism (sampling, seed, CPU vs GPU,\n\
                     quantization) into this string so the cache invalidates\n\
                     correctly when those change."
                )
            })?;
            let command_backend = CommandBackend::new(command, command_args.to_vec(), model_id)
                .with_timeout(Duration::from_secs(timeout_secs));
            Box::new(command_backend)
        }
        other => {
            anyhow::bail!("unknown --backend '{other}' (expected 'fixture' or 'command')");
        }
    };

    std::fs::create_dir_all(cache)?;
    std::fs::create_dir_all(out)?;

    let counted = CountingBackend::new(backend);
    let llm = LlmExtractor::new(counted, FileExtractionCache::new(cache));
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
