use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use luna_core::{AssertionLifecycleStatus, EngineKind};
use luna_extract::{
    CommandBackend, CountingBackend, FileExtractionCache, FixtureBackend, FusedExtractor,
    LlmBackend, LlmExtractor, LunaExtractor,
};
use luna_gauges::{calibrate_thresholds, GaugeReadingLog};
use luna_metrics::{BenchmarkReport, BenchmarkSubreport};
use luna_runtime::{
    audit_runtime_log, default_runtime_log_path, render_conversation_reply,
    render_runtime_markdown, LocalProductSmokePhrases, RuntimeExtractor, RuntimeSession,
};
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
        #[arg(long, default_value = "similarity")]
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
        /// Include the conversational reply with the runtime workbench output.
        #[arg(long)]
        include_reply: bool,
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
        /// Show only one entity group, for example `--entity Chris`.
        #[arg(long)]
        entity: Option<String>,
        /// Show only current claims.
        #[arg(long)]
        current: bool,
        /// Show only superseded claims.
        #[arg(long)]
        superseded: bool,
        /// Show why a specific term was not surfaced in memory.
        #[arg(long)]
        missing: Option<String>,
        /// Show the latest Attention Lattice derived from typed memory.
        #[arg(long)]
        lattice: bool,
        /// Output format.
        #[arg(long, value_enum, default_value = "markdown")]
        format: ReportFormat,
    },
    /// Replay-audit the persisted product runtime JSONL log.
    Audit {
        /// JSONL event log. Defaults to `.luna/runtime/events.jsonl`.
        #[arg(long)]
        log: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value = "markdown")]
        format: ReportFormat,
    },
    /// Local product-memory loop without scenario JSON: seed → audit → reopen → retrieve →
    /// correct → retrieve → audit. Dialogue defaults to `crates/luna-cli/smoke-dialog.json`
    /// (override with `--dialog`).
    Smoke {
        /// JSONL event log (default: temp file under %TEMP%).
        #[arg(long)]
        log: Option<PathBuf>,
        /// JSON file with [`LocalProductSmokePhrases`] fields (seed, retrieve lines, expectations).
        #[arg(long)]
        dialog: Option<PathBuf>,
        /// Delete `--log` before running when it already exists (recommended with a fixed path).
        #[arg(long)]
        reset: bool,
        /// Print JSON report to stdout after running.
        #[arg(long)]
        json: bool,
        /// Also write the JSON report to this path.
        #[arg(long)]
        report: Option<PathBuf>,
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
        /// Keep the freshly generated scenario event log after validation.
        #[arg(long)]
        keep_log: bool,
    },
}

#[derive(Debug, Subcommand)]
enum BenchCommand {
    Run {
        benchmarks: PathBuf,
        #[arg(long, default_value = "similarity")]
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
                let similarity = luna_bench::load_run(&run_dir, EngineKind::Similarity).ok();
                match (keyword, similarity) {
                    (Some(keyword), Some(similarity)) => {
                        let exit = print_compare(
                            &keyword.report,
                            &similarity.report,
                            require_proof_eligible,
                        );
                        return Ok(exit);
                    }
                    _ => {
                        println!("Run both engines first:");
                        println!("luna bench run ./benchmarks --engine keyword");
                        println!("luna bench run ./benchmarks --engine similarity");
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
                include_reply,
            } => {
                let log = log.unwrap_or_else(|| default_runtime_log_path(&PathBuf::from(".")));
                match extractor {
                    RuntimeExtractorChoice::Heuristic => {
                        run_runtime_turn(
                            &log,
                            FusedExtractor::new(),
                            content,
                            format,
                            include_reply,
                        )?;
                    }
                    RuntimeExtractorChoice::Command => {
                        let extractor = build_command_runtime_extractor(
                            command,
                            model_id,
                            command_args,
                            timeout_secs,
                            cache,
                        )?;
                        run_runtime_turn(&log, extractor, content, format, include_reply)?;
                    }
                }
                if matches!(format, ReportFormat::Markdown) {
                    println!("Wrote event log {}", log.display());
                }
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
            RuntimeCommand::Inspect {
                log,
                entity,
                current,
                superseded,
                missing,
                lattice,
                format,
            } => {
                let log = log.unwrap_or_else(|| default_runtime_log_path(&PathBuf::from(".")));
                let session = RuntimeSession::new(&log, FusedExtractor::new());
                let state = session.inspect()?;
                let latest_lattice = if lattice {
                    session.latest_attention_lattice()?
                } else {
                    None
                };
                let filters = InspectFilters {
                    entity,
                    current,
                    superseded,
                    missing,
                };
                match format {
                    ReportFormat::Markdown => {
                        print_memory_state(&state, &filters);
                        if lattice {
                            print_attention_lattice(latest_lattice.as_ref());
                        }
                    }
                    ReportFormat::Json => {
                        if lattice {
                            println!("{}", serde_json::to_string_pretty(&latest_lattice)?);
                        } else {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&filtered_claims(&state, &filters))?
                            );
                        }
                    }
                }
            }
            RuntimeCommand::Audit { log, format } => {
                let log = log.unwrap_or_else(|| default_runtime_log_path(&PathBuf::from(".")));
                let report = audit_runtime_log(&log)?;
                match format {
                    ReportFormat::Markdown => print_runtime_replay_audit(&log, &report),
                    ReportFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
                }
                if report.is_quarantined() {
                    return Ok(ExitCode::from(4));
                }
            }
            RuntimeCommand::Smoke {
                log,
                dialog,
                reset,
                json,
                report,
            } => {
                run_runtime_smoke(log, dialog, reset, json, report)?;
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

const DEFAULT_SMOKE_DIALOG: &str = include_str!("../smoke-dialog.json");

fn run_runtime_smoke(
    log: Option<PathBuf>,
    dialog: Option<PathBuf>,
    reset: bool,
    json: bool,
    report_path: Option<PathBuf>,
) -> anyhow::Result<()> {
    let log_path = log.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("luna_runtime_smoke_{}.jsonl", uuid::Uuid::new_v4()))
    });
    if reset {
        match fs::remove_file(&log_path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to reset smoke log {}", log_path.display()));
            }
        }
    }
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let dialog_json = match dialog {
        Some(path) => fs::read_to_string(&path)?,
        None => DEFAULT_SMOKE_DIALOG.to_string(),
    };
    let phrases: LocalProductSmokePhrases = serde_json::from_str(&dialog_json)?;
    let report = luna_runtime::run_local_product_memory_smoke(&log_path, &phrases)?;
    let payload = serde_json::json!({
        "log": log_path.to_string_lossy(),
        "success": report.is_success(),
        "report": {
            "distract_turn_count": report.distract_turn_count,
            "audit_clean_after_seed": report.audit_clean_after_seed,
            "recall_hit_after_reopen": report.recall_hit_after_reopen,
            "reply_contains_expect_before": report.reply_contains_expect_before,
            "recall_hit_after_correction": report.recall_hit_after_correction,
            "answer_evidence_before_correction": report.answer_evidence_before_correction,
            "answer_evidence_after_correction": report.answer_evidence_after_correction,
            "current_evidence_after_correction": report.current_evidence_after_correction,
            "audit_clean_after_full_loop": report.audit_clean_after_full_loop,
            "reply_contains_expect_after": report.reply_contains_expect_after,
            "reply_excludes_rejected_after": report.reply_excludes_rejected_after,
        },
        "evidence": {
            "reply_before_correction": report.reply_before_correction,
            "reply_after_correction": report.reply_after_correction,
        },
    });
    if let Some(path) = report_path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_string_pretty(&payload)?)?;
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("Local product memory smoke (no scenario harness)");
        println!("  log: {}", log_path.display());
        println!("  distract_turn_count: {}", report.distract_turn_count);
        println!(
            "  audit_clean_after_seed: {}",
            report.audit_clean_after_seed
        );
        println!(
            "  recall_hit_after_reopen: {}",
            report.recall_hit_after_reopen
        );
        println!(
            "  reply_contains_expect_before: {}",
            report.reply_contains_expect_before
        );
        println!(
            "  recall_hit_after_correction: {}",
            report.recall_hit_after_correction
        );
        println!(
            "  answer_evidence_before_correction: {}",
            report.answer_evidence_before_correction
        );
        println!(
            "  answer_evidence_after_correction: {}",
            report.answer_evidence_after_correction
        );
        println!(
            "  current_evidence_after_correction: {}",
            report.current_evidence_after_correction
        );
        println!(
            "  audit_clean_after_full_loop: {}",
            report.audit_clean_after_full_loop
        );
        println!(
            "  reply_contains_expect_after: {}",
            report.reply_contains_expect_after
        );
        println!(
            "  reply_excludes_rejected_after: {}",
            report.reply_excludes_rejected_after
        );
        println!("  success: {}", report.is_success());
    }
    if !report.is_success() {
        anyhow::bail!(
            "smoke checks failed — inspect replies or audit; event log at {}",
            log_path.display()
        );
    }
    if !json {
        println!("Wrote event log {}", log_path.display());
    }
    Ok(())
}

fn run_runtime_turn<E: RuntimeExtractor>(
    log: &Path,
    extractor: E,
    content: String,
    format: ReportFormat,
    include_reply: bool,
) -> anyhow::Result<()> {
    let session = RuntimeSession::new(log, extractor);
    let result = session.process_user_turn(content.clone())?;
    match format {
        ReportFormat::Markdown => {
            if include_reply {
                println!(
                    "# Luna Conversation Reply\n\n{}\n\n---\n\n{}",
                    render_conversation_reply(&content, &result),
                    render_runtime_markdown(&result)
                );
            } else {
                println!("{}", render_runtime_markdown(&result));
            }
        }
        ReportFormat::Json => {
            if include_reply {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "conversation_reply": render_conversation_reply(&content, &result),
                        "result": result,
                    }))?
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        }
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

fn run_runtime_scenario<E: RuntimeExtractor>(
    scenario_path: &Path,
    log: &Path,
    extractor: E,
    keep_log: bool,
) -> anyhow::Result<()> {
    let report =
        luna_runtime::scenario::run_runtime_scenario(scenario_path, log, extractor, keep_log)?;

    println!("# Luna Runtime Scenario: {}\n", report.name);
    println!("Turns: {}", report.turn_count);
    if report.log_retained {
        println!("Event log: {}\n", report.log_path);
    } else {
        println!(
            "Event log: {} (removed after validation; rerun with --keep-log to retain it)\n",
            report.log_path
        );
    }
    let proof_status = if report.proof_eligibility_applicable {
        if report.proof_eligible {
            "yes"
        } else {
            "no"
        }
    } else {
        "n/a"
    };
    println!("Proof eligible: {proof_status}");
    if !report.proof_eligibility_failures.is_empty() {
        println!("Proof eligibility blockers:");
        for failure in &report.proof_eligibility_failures {
            println!("- {failure}");
        }
    }
    println!();

    for (index, turn) in report.turn_summaries.iter().enumerate() {
        println!(
            "{}. {} assertion(s), {} working node(s)",
            index + 1,
            turn.assertion_count,
            turn.working_node_count
        );
    }

    if report.is_pass() {
        println!("\nPASS: {} memory check(s)", report.check_count);
        Ok(())
    } else {
        println!("\nFAIL: {} issue(s)", report.failures.len());
        for failure in &report.failures {
            println!("- {failure}");
        }
        anyhow::bail!("runtime scenario failed")
    }
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

#[derive(Debug, Clone, Default)]
struct InspectFilters {
    entity: Option<String>,
    current: bool,
    superseded: bool,
    missing: Option<String>,
}

fn print_memory_state(state: &luna_runtime::MemoryState, filters: &InspectFilters) {
    if let Some(query) = &filters.missing {
        println!("# Why Not Remembered: \"{}\"\n", query);
        let lower_q = query.to_ascii_lowercase();
        let matching: Vec<&luna_runtime::MemoryClaim> = state
            .claims
            .iter()
            .filter(|c| c.value.to_ascii_lowercase().contains(&lower_q))
            .collect();
        if matching.is_empty() {
            println!(
                "No claims found containing \"{}\" in memory state.\n",
                query
            );
            println!("The term was never stored as an assertion, or the phrase was not captured by the entity sieve.");
        } else {
            for claim in &matching {
                let reason = match claim.lifecycle_status {
                    luna_core::AssertionLifecycleStatus::Current => {
                        if claim.status == luna_core::AssertionConfidenceTier::Unconfirmed {
                            "would be suppressed: unconfirmed (requires repeated evidence to become confirmed)"
                        } else {
                            "would be surfaced: current and confirmed"
                        }
                    }
                    luna_core::AssertionLifecycleStatus::Superseded => {
                        "would be suppressed: superseded by a newer correction"
                    }
                    luna_core::AssertionLifecycleStatus::Stale => {
                        "would be suppressed: marked stale (decayed or unused)"
                    }
                    luna_core::AssertionLifecycleStatus::Contradicted => {
                        "would be suppressed: contradicted by other evidence"
                    }
                };
                println!(
                    "- {:?}/{:?}: {}:{} = {}",
                    claim.status, claim.lifecycle_status, claim.domain, claim.kind, claim.value
                );
                println!("  Reason: {}", reason);
            }
        }
        return;
    }
    println!("# Luna Memory State\n");
    let claims = filtered_claims(state, filters);
    if claims.is_empty() && state.map.nodes.is_empty() && state.map.edges.is_empty() {
        println!("(no claims yet)");
        return;
    }

    if !state.entity_groups.is_empty() {
        println!("## Entity Memory\n");
        for group in state
            .entity_groups
            .iter()
            .filter(|group| group_matches(group, filters))
        {
            let group_claims = group
                .claims
                .iter()
                .filter(|claim| claim_matches(claim, filters))
                .collect::<Vec<_>>();
            if group_claims.is_empty() {
                continue;
            }
            println!("### {} ({})", group.label, group.kind);
            for claim in group_claims {
                println!(
                    "- {:?}/{:?}: {}:{} = {}",
                    claim.status, claim.lifecycle_status, claim.domain, claim.kind, claim.value
                );
            }
            println!();
        }
    }

    println!("## Flat Claims\n");
    for claim in &claims {
        println!(
            "- {:?}/{:?}: {}:{} = {}",
            claim.status, claim.lifecycle_status, claim.domain, claim.kind, claim.value
        );
    }
    if filters.has_filters() {
        println!(
            "\nMemory map: full derived map omitted in filtered inspect view ({} node(s), {} edge(s) total)",
            state.map.nodes.len(),
            state.map.edges.len()
        );
        return;
    }
    println!(
        "\nMemory map: {} node(s), {} edge(s)",
        state.map.nodes.len(),
        state.map.edges.len()
    );
    if !state.map.nodes.is_empty() {
        println!("\n## Memory Map Nodes");
        for node in state.map.nodes.iter().take(12) {
            let assertion_keys = node
                .provenance
                .iter()
                .filter_map(|provenance| provenance.assertion_key.as_deref())
                .collect::<Vec<_>>()
                .join(", ");
            let system_roots = node
                .provenance
                .iter()
                .filter_map(|provenance| provenance.system_root.as_deref())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "- {:?}: {} ({:?}, activation {:.2}; assertions: {}; system roots: {})",
                node.kind,
                node.label,
                node.confidence_tier,
                node.activation,
                if assertion_keys.is_empty() {
                    "none"
                } else {
                    assertion_keys.as_str()
                },
                if system_roots.is_empty() {
                    "none"
                } else {
                    system_roots.as_str()
                }
            );
        }
    }
    if !state.map.edges.is_empty() {
        println!("\n## Memory Map Edges");
    }
    for edge in state.map.edges.iter().take(12) {
        println!(
            "- {:?}: {} -> {} ({:?}, strength {:.2})",
            edge.confidence_tier, edge.source, edge.target, edge.relation, edge.strength
        );
    }
}

fn filtered_claims(
    state: &luna_runtime::MemoryState,
    filters: &InspectFilters,
) -> Vec<luna_runtime::MemoryClaim> {
    let entity_keys = filters.entity.as_ref().map(|_| {
        state
            .entity_groups
            .iter()
            .filter(|group| group_matches(group, filters))
            .flat_map(|group| group.claims.iter().map(|claim| claim.key.clone()))
            .collect::<std::collections::BTreeSet<_>>()
    });
    state
        .claims
        .iter()
        .filter(|claim| claim_matches(claim, filters))
        .filter(|claim| {
            entity_keys
                .as_ref()
                .map(|keys| keys.contains(&claim.key) || claim_value_matches_entity(claim, filters))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

fn group_matches(group: &luna_runtime::EntityMemoryGroup, filters: &InspectFilters) -> bool {
    let Some(entity) = &filters.entity else {
        return true;
    };
    let entity = entity.to_ascii_lowercase();
    group.label.to_ascii_lowercase().contains(&entity)
        || group.id.to_ascii_lowercase().contains(&entity)
}

fn claim_matches(claim: &luna_runtime::MemoryClaim, filters: &InspectFilters) -> bool {
    if filters.current && claim.lifecycle_status != AssertionLifecycleStatus::Current {
        return false;
    }
    if filters.superseded && claim.lifecycle_status != AssertionLifecycleStatus::Superseded {
        return false;
    }
    true
}

fn claim_value_matches_entity(claim: &luna_runtime::MemoryClaim, filters: &InspectFilters) -> bool {
    let Some(entity) = &filters.entity else {
        return true;
    };
    claim
        .value
        .to_ascii_lowercase()
        .contains(&entity.to_ascii_lowercase())
}

impl InspectFilters {
    fn has_filters(&self) -> bool {
        self.entity.is_some() || self.current || self.superseded || self.missing.is_some()
    }
}

fn print_attention_lattice(lattice: Option<&luna_core::AttentionLattice>) {
    println!("\n## Attention Lattice\n");
    let Some(lattice) = lattice else {
        println!("(no lattice computed yet)");
        return;
    };
    print_lattice_dimension("identity", &lattice.identity);
    print_lattice_dimension("relationship", &lattice.relationship);
    print_lattice_dimension("goal", &lattice.goal);
    print_lattice_dimension("correction", &lattice.correction);
    print_lattice_dimension("context", &lattice.context);
    print_lattice_dimension("confidence", &lattice.confidence);
}

fn print_lattice_dimension(name: &str, dimension: &luna_core::LatticeDimension) {
    let sources = dimension
        .provenance
        .iter()
        .filter_map(|provenance| provenance.assertion_key.as_deref())
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "- {name}: {:.2} - {} (sources: {})",
        dimension.score,
        dimension.reason,
        if sources.is_empty() {
            "none"
        } else {
            sources.as_str()
        }
    );
}

fn print_runtime_replay_audit(log: &Path, report: &luna_runtime::RuntimeReplayAuditReport) {
    println!("# Luna Runtime Replay Audit\n");
    println!("Event log: {}", log.display());
    println!("Status: {:?}", report.status);
    println!("Hash version: {}", report.hash_version);
    println!("Stored events: {}", report.replayed_counts.stored_events);
    println!("Current claims: {}", report.replayed_counts.current_claims);
    println!(
        "Topology: {} node(s), {} tether(s), {} source event ref(s), {} valid source event ref(s), {} accepted orb ref(s)",
        report.replayed_counts.topology_nodes,
        report.replayed_counts.topology_tethers,
        report.replayed_counts.topology_source_event_refs,
        report.replayed_counts.valid_topology_source_event_refs,
        report.replayed_counts.topology_orbs
    );
    println!("Live snapshot: {}", report.live_snapshot_hash);
    println!("Replayed snapshot: {}", report.replayed_snapshot_hash);
    if let Some(error) = &report.replay_error {
        println!("Replay error: {error}");
    }
}

fn print_delta(label: &str, keyword: f32, similarity: f32) {
    println!(
        "| {label} | {:.2} | {:.2} | {:+.2} |",
        keyword,
        similarity,
        similarity - keyword
    );
}

/// Renders the compare table in either strict (proof-eligible only) or
/// lenient (all-cases totals) mode. Returns the exit code the CLI should
/// hand back to the shell: success in lenient mode, code 2 in strict mode
/// when either run contains ineligible cases or has no eligible subset.
fn print_compare(
    keyword: &BenchmarkReport,
    similarity: &BenchmarkReport,
    require_proof_eligible: bool,
) -> ExitCode {
    let keyword_ineligible = ineligible_count(keyword);
    let similarity_ineligible = ineligible_count(similarity);
    let any_ineligible = keyword_ineligible > 0 || similarity_ineligible > 0;

    if require_proof_eligible {
        let (k, s) = match (&keyword.eligible, &similarity.eligible) {
            (Some(k), Some(s)) => (k, s),
            _ => {
                eprintln!(
                    "--require-proof-eligible: at least one engine has no proof-eligible cases in this run."
                );
                return ExitCode::from(2);
            }
        };
        if any_ineligible {
            eprintln!(
                "--require-proof-eligible: {} keyword case(s) and {} Similarity case(s) are not proof-eligible. Run aborted; results are non-publishable until all cases are eligible.",
                keyword_ineligible, similarity_ineligible
            );
            return ExitCode::from(2);
        }
        println!("# Luna Engine Compare (proof-eligible only)\n");
        print_compare_table(k, s);
        ExitCode::SUCCESS
    } else {
        if any_ineligible {
            eprintln!(
                "warning: {} keyword case(s) and {} Similarity case(s) are not proof-eligible. \
                 Use --require-proof-eligible for strict comparison; numbers below \
                 are TOTALS over all cases and are NOT proof-counted.",
                keyword_ineligible, similarity_ineligible
            );
        }
        println!("# Luna Engine Compare\n");
        print_compare_table_total(keyword, similarity);
        ExitCode::SUCCESS
    }
}

fn print_compare_table(keyword: &BenchmarkSubreport, similarity: &BenchmarkSubreport) {
    println!("| Metric | Keyword | Similarity | Delta |");
    println!("|---|---:|---:|---:|");
    print_delta(
        "Recall accuracy",
        keyword.recall_accuracy,
        similarity.recall_accuracy,
    );
    print_delta(
        "False memory rate",
        keyword.false_memory_rate,
        similarity.false_memory_rate,
    );
    print_delta(
        "Overclaim rate",
        keyword.overclaim_rate,
        similarity.overclaim_rate,
    );
    print_delta(
        "Mean latency ms",
        keyword.mean_latency_ms,
        similarity.mean_latency_ms,
    );
}

fn print_compare_table_total(keyword: &BenchmarkReport, similarity: &BenchmarkReport) {
    println!("| Metric | Keyword | Similarity | Delta |");
    println!("|---|---:|---:|---:|");
    print_delta(
        "Recall accuracy",
        keyword.recall_accuracy,
        similarity.recall_accuracy,
    );
    print_delta(
        "False memory rate",
        keyword.false_memory_rate,
        similarity.false_memory_rate,
    );
    print_delta(
        "Overclaim rate",
        keyword.overclaim_rate,
        similarity.overclaim_rate,
    );
    print_delta(
        "Mean latency ms",
        keyword.mean_latency_ms,
        similarity.mean_latency_ms,
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
            let profile = episode
                .profile
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
            println!("        profile: {profile}");
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
