use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn luna_bin() -> &'static str {
    env!("CARGO_BIN_EXE_luna")
}

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("luna_cli_{name}_{nanos}"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn assert_success(mut command: Command) -> String {
    let output = command.output().expect("command should execute");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "command failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout,
        stderr
    );
    stdout
}

fn assert_failure(mut command: Command) -> (String, String) {
    let output = command.output().expect("command should execute");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    (stdout, stderr)
}

fn command_args(program: PathBuf, args: Vec<String>) -> (PathBuf, Vec<String>) {
    (program, args)
}

#[cfg(windows)]
fn write_command_extractor_fixture(
    root: &Path,
    mode: &str,
    call_log: &Path,
) -> (PathBuf, Vec<String>) {
    let script = root.join(format!("command-extractor-{mode}.ps1"));
    fs::write(
        &script,
        r#"
param(
    [Parameter(Mandatory=$true)][string]$Mode,
    [Parameter(Mandatory=$true)][string]$CallLog
)
$null = [Console]::In.ReadToEnd()
Add-Content -LiteralPath $CallLog -Value $Mode -Encoding UTF8
if ($Mode -eq "fail") {
    Write-Error "forced command extractor failure"
    exit 7
}
if ($Mode -eq "invalid_schema") {
@'
{"assertions":[{"domain":"vibe","kind":"location","value":"Chris lives in Iowa","confidence":0.92,"evidence_span":"Chris lives in Iowa"}],"signals":{"emotional_arousal":null,"goal_pressure":null,"identity_relevance":null,"temporal_relevance":null}}
'@
    exit 0
}
if ($Mode -eq "formation_valid") {
@'
{"assertions":[{"domain":"identity","kind":"profession","value":"mechanical engineer","confidence":0.92,"evidence_span":"mechanical engineer"}],"signals":{"emotional_arousal":null,"goal_pressure":null,"identity_relevance":{"value":0.88,"confidence":0.84,"reliability":"learned","evidence":"I work as a mechanical engineer"},"temporal_relevance":null}}
'@
    exit 0
}
@'
{"assertions":[{"domain":"person","kind":"location","value":"Chris lives in Iowa","confidence":0.92,"evidence_span":"Chris lives in Iowa"}],"signals":{"emotional_arousal":null,"goal_pressure":null,"identity_relevance":null,"temporal_relevance":null}}
'@
"#,
    )
    .unwrap();
    command_args(
        PathBuf::from("powershell"),
        vec![
            "-NoProfile".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
            "-File".to_string(),
            script.to_string_lossy().to_string(),
            "-Mode".to_string(),
            mode.to_string(),
            "-CallLog".to_string(),
            call_log.to_string_lossy().to_string(),
        ],
    )
}

#[cfg(not(windows))]
fn write_command_extractor_fixture(
    root: &Path,
    mode: &str,
    call_log: &Path,
) -> (PathBuf, Vec<String>) {
    let script = root.join(format!("command-extractor-{mode}.sh"));
    fs::write(
        &script,
        r#"#!/usr/bin/env sh
mode="$1"
call_log="$2"
cat >/dev/null
printf '%s\n' "$mode" >> "$call_log"
if [ "$mode" = "fail" ]; then
  echo "forced command extractor failure" >&2
  exit 7
fi
if [ "$mode" = "invalid_schema" ]; then
  printf '%s\n' '{"assertions":[{"domain":"vibe","kind":"location","value":"Chris lives in Iowa","confidence":0.92,"evidence_span":"Chris lives in Iowa"}],"signals":{"emotional_arousal":null,"goal_pressure":null,"identity_relevance":null,"temporal_relevance":null}}'
  exit 0
fi
if [ "$mode" = "formation_valid" ]; then
  printf '%s\n' '{"assertions":[{"domain":"identity","kind":"profession","value":"mechanical engineer","confidence":0.92,"evidence_span":"mechanical engineer"}],"signals":{"emotional_arousal":null,"goal_pressure":null,"identity_relevance":{"value":0.88,"confidence":0.84,"reliability":"learned","evidence":"I work as a mechanical engineer"},"temporal_relevance":null}}'
  exit 0
fi
printf '%s\n' '{"assertions":[{"domain":"person","kind":"location","value":"Chris lives in Iowa","confidence":0.92,"evidence_span":"Chris lives in Iowa"}],"signals":{"emotional_arousal":null,"goal_pressure":null,"identity_relevance":null,"temporal_relevance":null}}'
"#,
    )
    .unwrap();
    command_args(
        PathBuf::from("sh"),
        vec![
            script.to_string_lossy().to_string(),
            mode.to_string(),
            call_log.to_string_lossy().to_string(),
        ],
    )
}

fn write_command_extractor_scenario(root: &Path) -> PathBuf {
    let scenario = root.join("command-extractor-scenario.json");
    fs::write(
        &scenario,
        r#"{
  "name": "command_extractor_schema_cache_gate",
  "turns": [
    {
      "content": "Chris lives in Iowa.",
      "timestamp": "2026-05-10T12:00:00Z"
    }
  ],
  "checks": {
    "claims": {
      "must_include": [
        {
          "domain": "person",
          "kind": "location",
          "value": "Chris lives in Iowa",
          "lifecycle_status": "current"
        }
      ]
    }
  }
}
"#,
    )
    .unwrap();
    scenario
}

fn add_command_extractor_args(
    command: &mut Command,
    scenario: &Path,
    log: &Path,
    cache: &Path,
    model_id: &str,
    program: &Path,
    args: &[String],
) {
    command
        .args(["runtime", "scenario"])
        .arg(scenario)
        .args(["--log"])
        .arg(log)
        .args(["--extractor", "command", "--command"])
        .arg(program)
        .args(["--model-id", model_id, "--cache"])
        .arg(cache)
        .args(["--timeout-secs", "10"]);
    for arg in args {
        command.arg(format!("--command-arg={arg}"));
    }
}

fn write_formation_benchmark_case(root: &Path) -> PathBuf {
    let benchmarks = root.join("benchmarks");
    fs::create_dir_all(&benchmarks).unwrap();
    let case = benchmarks.join("command_backend_case.json");
    fs::write(
        &case,
        r#"{
  "schema_version": 1,
  "id": "command_backend_case",
  "proof_category": "cli_backend_selection",
  "proof_eligible": true,
  "category": "cli_backend_selection",
  "target_dimensions": ["identity_relevance"],
  "timestamp_origin": "cli_test",
  "turns": [
    {
      "content": "I work as a mechanical engineer?",
      "role": "user",
      "timestamp": "2026-05-10T12:00:00Z"
    }
  ],
  "expected": {
    "must_recall": ["mechanical engineer"],
    "must_not_claim": ["software engineer"]
  }
}
"#,
    )
    .unwrap();
    benchmarks
}

fn add_formation_command_backend_args(
    command: &mut Command,
    benchmarks: &Path,
    cache: &Path,
    out: &Path,
    model_id: &str,
    program: &Path,
    args: &[String],
) {
    command
        .args(["bench", "formation"])
        .arg(benchmarks)
        .args(["--backend", "command", "--command"])
        .arg(program)
        .args(["--model-id", model_id, "--cache"])
        .arg(cache)
        .args(["--out"])
        .arg(out)
        .args(["--timeout-secs", "10"]);
    for arg in args {
        command.arg(format!("--command-arg={arg}"));
    }
}

fn call_count(path: &Path) -> usize {
    fs::read_to_string(path)
        .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count())
        .unwrap_or(0)
}

fn cache_file_count(path: &Path) -> usize {
    fn count_dir(path: &Path) -> usize {
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        entries
            .filter_map(Result::ok)
            .map(|entry| {
                let path = entry.path();
                if path.is_dir() {
                    count_dir(&path)
                } else {
                    1
                }
            })
            .sum()
    }
    count_dir(path)
}

#[allow(dead_code)]
fn first_json_item(value: &Value) -> &Value {
    value
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(value)
}

#[cfg(windows)]
fn add_llm_ready_args(
    command: &mut Command,
    corpus: &Path,
    packet: &Path,
    cache: &Path,
    model_id: &str,
    program: &Path,
    args: &[String],
) {
    let args_json = serde_json::to_string(args).expect("command args should serialize");
    command
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            "scripts/llm-ready.ps1",
            "-Luna",
        ])
        .arg(luna_bin())
        .args(["-Corpus"])
        .arg(corpus)
        .args(["-ModelId", model_id, "-ExtractorCommand"])
        .arg(program)
        .args(["-CommandArgsJson", &args_json, "-Cache"])
        .arg(cache)
        .args(["-OutDir"])
        .arg(packet)
        .args(["-TimeoutSecs", "10", "-AllowDirty"]);
}

#[test]
fn runtime_smoke_cli_writes_log_and_report() {
    let root = temp_root("smoke");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");
    let report = root.join("smoke-report.json");

    let mut command = Command::new(luna_bin());
    command
        .args(["runtime", "smoke", "--log"])
        .arg(&log)
        .args(["--reset", "--json", "--report"])
        .arg(&report);
    let stdout = assert_success(command);

    let stdout_json: Value = serde_json::from_str(&stdout).expect("smoke stdout should be JSON");
    assert_eq!(stdout_json["success"], Value::Bool(true));
    assert!(
        log.exists(),
        "expected runtime smoke log at {}",
        log.display()
    );
    assert!(
        report.exists(),
        "expected runtime smoke report at {}",
        report.display()
    );
    let report_json: Value =
        serde_json::from_str(&fs::read_to_string(&report).unwrap()).expect("report JSON");
    assert_eq!(report_json["success"], Value::Bool(true));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn bench_formation_command_backend_validates_and_reuses_cache() {
    let root = temp_root("formation_command_backend");
    fs::create_dir_all(&root).unwrap();
    let benchmarks = write_formation_benchmark_case(&root);
    let cache = root.join("cache");
    let out = root.join("formation-out");
    let call_log = root.join("command-calls.txt");
    let (program, args) = write_command_extractor_fixture(&root, "formation_valid", &call_log);

    let mut first = Command::new(luna_bin());
    add_formation_command_backend_args(
        &mut first,
        &benchmarks,
        &cache,
        &out,
        "local-formation-command@valid-v1",
        &program,
        &args,
    );
    let stdout = assert_success(first);
    assert!(
        stdout.contains("Formation eligible: 1"),
        "formation stdout:\n{stdout}"
    );
    assert_eq!(
        call_count(&call_log),
        1,
        "first run should call helper once"
    );
    assert!(
        cache_file_count(&cache) > 0,
        "valid command formation should populate cache"
    );
    let report_path = out.join("formation.json");
    assert!(
        report_path.exists(),
        "expected formation report at {}",
        report_path.display()
    );

    let mut second = Command::new(luna_bin());
    add_formation_command_backend_args(
        &mut second,
        &benchmarks,
        &cache,
        &out,
        "local-formation-command@valid-v1",
        &program,
        &args,
    );
    let stdout = assert_success(second);
    assert!(
        stdout.contains("Cache hit rate, second run | 100%"),
        "formation stdout:\n{stdout}"
    );
    assert_eq!(
        call_count(&call_log),
        1,
        "second CLI run with same schema/model/prompt/timestamp should hit cache"
    );

    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn llm_ready_packet_harness_records_pass_summary_and_hashes() {
    let root = temp_root("llm_ready_pass");
    let corpus = root.join("corpus");
    fs::create_dir_all(&corpus).unwrap();
    let scenario = write_command_extractor_scenario(&corpus);
    let packet = root.join("packet");
    let cache = root.join("cache");
    let call_log = root.join("command-calls.txt");
    let (program, args) = write_command_extractor_fixture(&root, "valid", &call_log);

    let mut command = Command::new("powershell");
    command.current_dir(workspace_root());
    add_llm_ready_args(
        &mut command,
        &scenario,
        &packet,
        &cache,
        "local-command-eval@valid-v1",
        &program,
        &args,
    );
    let stdout = assert_success(command);
    assert!(
        stdout.contains("LLM Ready packet:"),
        "script stdout:\n{stdout}"
    );

    let manifest_path = packet.join("manifest.json");
    assert!(
        manifest_path.exists(),
        "expected manifest at {}",
        manifest_path.display()
    );
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).expect("manifest JSON");
    assert_eq!(
        manifest["packet_kind"],
        Value::String("luna.llm_ready.command_extractor.v1".to_string())
    );
    assert_eq!(
        manifest["config"]["model_id"],
        Value::String("local-command-eval@valid-v1".to_string())
    );
    assert_eq!(
        manifest["config"]["harness_network_dependency"],
        Value::Bool(false)
    );
    assert_eq!(
        manifest["config"]["extractor_network_policy"],
        Value::String("caller_supplied_unverified".to_string())
    );
    assert_eq!(manifest["summary"]["total"], Value::from(1));
    assert_eq!(manifest["summary"]["passed"], Value::from(1));
    assert_eq!(manifest["summary"]["failed"], Value::from(0));
    assert_eq!(manifest["summary"]["success"], Value::Bool(true));
    assert!(
        manifest["cache"]["file_count"].as_u64().unwrap_or(0) > 0,
        "valid packet should record cache files"
    );
    let case = first_json_item(&manifest["cases"]);
    assert_eq!(case["passed"], Value::Bool(true));
    assert_eq!(
        case["hashes"]["stdout_sha256"]
            .as_str()
            .expect("stdout hash")
            .len(),
        64
    );
    assert_eq!(
        case["hashes"]["log_sha256"]
            .as_str()
            .expect("log hash")
            .len(),
        64
    );
    assert_eq!(
        first_json_item(&manifest["corpus"]["files"])["sha256"]
            .as_str()
            .expect("corpus hash")
            .len(),
        64
    );
    assert!(packet.join("commands.ps1").exists());
    assert!(packet.join("cache-files.json").exists());
    assert_eq!(call_count(&call_log), 1);

    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn llm_ready_packet_harness_records_failure_summary() {
    let root = temp_root("llm_ready_fail");
    let corpus = root.join("corpus");
    fs::create_dir_all(&corpus).unwrap();
    let scenario = write_command_extractor_scenario(&corpus);
    let packet = root.join("packet");
    let cache = root.join("cache");
    let call_log = root.join("command-calls.txt");
    let (program, args) = write_command_extractor_fixture(&root, "invalid_schema", &call_log);

    let mut command = Command::new("powershell");
    command.current_dir(workspace_root());
    add_llm_ready_args(
        &mut command,
        &scenario,
        &packet,
        &cache,
        "local-command-eval@schema-fail-v1",
        &program,
        &args,
    );
    let (_stdout, stderr) = assert_failure(command);
    assert!(
        stderr.contains("LLM Ready evaluation failed"),
        "script stderr:\n{stderr}"
    );

    let manifest_path = packet.join("manifest.json");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).expect("manifest JSON");
    assert_eq!(manifest["summary"]["total"], Value::from(1));
    assert_eq!(manifest["summary"]["passed"], Value::from(0));
    assert_eq!(manifest["summary"]["failed"], Value::from(1));
    assert_eq!(manifest["summary"]["success"], Value::Bool(false));
    assert_eq!(manifest["cache"]["file_count"], Value::from(0));
    let case = first_json_item(&manifest["cases"]);
    assert_eq!(case["passed"], Value::Bool(false));
    assert_ne!(case["exit_code"], Value::from(0));
    let case_stderr = case["outputs"]["stderr"]
        .as_str()
        .expect("case stderr path");
    let case_stderr = fs::read_to_string(case_stderr).expect("case stderr should exist");
    assert!(
        case_stderr.contains("extraction validation failed"),
        "case stderr:\n{case_stderr}"
    );
    assert_eq!(
        case["hashes"]["stderr_sha256"]
            .as_str()
            .expect("stderr hash")
            .len(),
        64
    );
    assert_eq!(call_count(&call_log), 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_scenario_command_extractor_validates_and_caches_without_network() {
    let root = temp_root("command_extractor_cache");
    fs::create_dir_all(&root).unwrap();
    let scenario = write_command_extractor_scenario(&root);
    let log = root.join("events.jsonl");
    let cache = root.join("cache");
    let call_log = root.join("command-calls.txt");
    let (program, args) = write_command_extractor_fixture(&root, "valid", &call_log);

    let mut first = Command::new(luna_bin());
    add_command_extractor_args(
        &mut first,
        &scenario,
        &log,
        &cache,
        "local-command-smoke@valid-v1",
        &program,
        &args,
    );
    let stdout = assert_success(first);
    assert!(stdout.contains("PASS:"), "scenario stdout:\n{stdout}");
    assert_eq!(call_count(&call_log), 1, "first pass should call helper");
    assert!(
        cache_file_count(&cache) > 0,
        "valid command extraction should populate cache"
    );

    let mut second = Command::new(luna_bin());
    add_command_extractor_args(
        &mut second,
        &scenario,
        &log,
        &cache,
        "local-command-smoke@valid-v1",
        &program,
        &args,
    );
    let stdout = assert_success(second);
    assert!(stdout.contains("PASS:"), "scenario stdout:\n{stdout}");
    assert_eq!(
        call_count(&call_log),
        1,
        "second pass with same schema/model/prompt/timestamp should hit cache"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_scenario_command_extractor_rejects_invalid_schema_without_caching() {
    let root = temp_root("command_extractor_schema");
    fs::create_dir_all(&root).unwrap();
    let scenario = write_command_extractor_scenario(&root);
    let log = root.join("events.jsonl");
    let cache = root.join("cache");
    let call_log = root.join("command-calls.txt");
    let (bad_program, bad_args) =
        write_command_extractor_fixture(&root, "invalid_schema", &call_log);

    let mut bad = Command::new(luna_bin());
    add_command_extractor_args(
        &mut bad,
        &scenario,
        &log,
        &cache,
        "local-command-smoke@schema-v1",
        &bad_program,
        &bad_args,
    );
    let (_stdout, stderr) = assert_failure(bad);
    assert!(
        stderr.contains("extraction validation failed"),
        "stderr should report schema validation failure:\n{stderr}"
    );
    assert_eq!(call_count(&call_log), 1);
    assert_eq!(
        cache_file_count(&cache),
        0,
        "invalid command output must not be cached"
    );

    let (good_program, good_args) = write_command_extractor_fixture(&root, "valid", &call_log);
    let mut good = Command::new(luna_bin());
    add_command_extractor_args(
        &mut good,
        &scenario,
        &log,
        &cache,
        "local-command-smoke@schema-v1",
        &good_program,
        &good_args,
    );
    let stdout = assert_success(good);
    assert!(stdout.contains("PASS:"), "scenario stdout:\n{stdout}");
    assert_eq!(
        call_count(&call_log),
        2,
        "successful retry should call helper because failed output was not cached"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_scenario_command_extractor_propagates_command_failures_without_caching() {
    let root = temp_root("command_extractor_failure");
    fs::create_dir_all(&root).unwrap();
    let scenario = write_command_extractor_scenario(&root);
    let log = root.join("events.jsonl");
    let cache = root.join("cache");
    let call_log = root.join("command-calls.txt");
    let (program, args) = write_command_extractor_fixture(&root, "fail", &call_log);

    let mut command = Command::new(luna_bin());
    add_command_extractor_args(
        &mut command,
        &scenario,
        &log,
        &cache,
        "local-command-smoke@failure-v1",
        &program,
        &args,
    );
    let (_stdout, stderr) = assert_failure(command);
    assert!(
        stderr.contains("CommandBackend") && stderr.contains("exited"),
        "stderr should include command failure boundary:\n{stderr}"
    );
    assert_eq!(call_count(&call_log), 1);
    assert_eq!(
        cache_file_count(&cache),
        0,
        "failed command invocation must not populate cache"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_scenario_cli_executes_registered_gate() {
    let root = temp_root("scenario");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("scenario.jsonl");
    let scenario = workspace_root()
        .join("scenarios")
        .join("runtime")
        .join("council5_runtime_topology_bridge.json");

    let mut command = Command::new(luna_bin());
    command
        .args(["runtime", "scenario"])
        .arg(&scenario)
        .args(["--log"])
        .arg(&log);
    let stdout = assert_success(command);

    assert!(stdout.contains("PASS:"), "scenario stdout:\n{stdout}");
    assert!(
        !log.exists(),
        "scenario log should be removed unless --keep-log is passed"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_turn_json_can_include_conversation_reply() {
    let root = temp_root("turn_include_reply");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    let mut command = Command::new(luna_bin());
    command
        .args([
            "runtime",
            "turn",
            "Morgan lives in Iowa.",
            "--format",
            "json",
            "--include-reply",
            "--log",
        ])
        .arg(&log);
    let stdout = assert_success(command);
    let payload: Value = serde_json::from_str(&stdout).expect("turn JSON should parse");

    assert!(
        payload["conversation_reply"]
            .as_str()
            .expect("conversation reply should be present")
            .contains("Morgan lives in Iowa"),
        "payload:\n{payload:#?}"
    );
    assert_eq!(
        payload["result"]["knowledge_delta"]["unconfirmed"][0]["value"],
        Value::from("Morgan lives in Iowa")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_trace_cli_reads_latest_and_named_turn_receipts() {
    let root = temp_root("trace");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    let mut first_turn = Command::new(luna_bin());
    first_turn
        .args([
            "runtime",
            "turn",
            "Morgan lives in Iowa.",
            "--format",
            "json",
            "--log",
        ])
        .arg(&log);
    let first_stdout = assert_success(first_turn);
    let first_payload: Value =
        serde_json::from_str(&first_stdout).expect("first turn JSON should parse");
    let first_turn_id = first_payload["turn_id"]
        .as_str()
        .expect("first turn id")
        .to_string();

    let mut second_turn = Command::new(luna_bin());
    second_turn
        .args([
            "runtime",
            "turn",
            "Morgan moved to Ohio.",
            "--format",
            "json",
            "--log",
        ])
        .arg(&log);
    let second_stdout = assert_success(second_turn);
    let second_payload: Value =
        serde_json::from_str(&second_stdout).expect("second turn JSON should parse");
    let second_turn_id = second_payload["turn_id"]
        .as_str()
        .expect("second turn id")
        .to_string();

    let mut latest = Command::new(luna_bin());
    latest
        .args(["runtime", "trace", "--latest", "--format", "json", "--log"])
        .arg(&log);
    let latest_stdout = assert_success(latest);
    let latest_json: Value = serde_json::from_str(&latest_stdout).expect("latest trace JSON");
    assert_eq!(
        latest_json["turn_id"],
        Value::String(second_turn_id.clone())
    );
    assert_eq!(
        latest_json["receipt_event_hash"]
            .as_str()
            .expect("receipt hash")
            .len(),
        64
    );
    assert!(latest_json["trace_steps"]
        .as_array()
        .expect("trace steps")
        .iter()
        .any(|step| step["name"] == Value::String("working_memory".to_string())));

    let mut named = Command::new(luna_bin());
    named
        .args([
            "runtime",
            "trace",
            "--turn",
            &first_turn_id,
            "--format",
            "json",
            "--log",
        ])
        .arg(&log);
    let named_stdout = assert_success(named);
    let named_json: Value = serde_json::from_str(&named_stdout).expect("named trace JSON");
    assert_eq!(named_json["turn_id"], Value::String(first_turn_id));
    assert_ne!(named_json["turn_id"], Value::String(second_turn_id));
    assert!(named_json["trace_steps"]
        .as_array()
        .expect("trace steps")
        .iter()
        .any(|step| step["name"] == Value::String("extract".to_string())));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_trace_cli_requires_exactly_one_selector() {
    let root = temp_root("trace_selector");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    let mut turn = Command::new(luna_bin());
    turn.args(["runtime", "turn", "Morgan lives in Iowa.", "--log"])
        .arg(&log);
    assert_success(turn);

    let mut missing_selector = Command::new(luna_bin());
    missing_selector
        .args(["runtime", "trace", "--log"])
        .arg(&log);
    let (_stdout, stderr) = assert_failure(missing_selector);
    assert!(stderr.contains("exactly one selector"), "stderr:\n{stderr}");

    let mut both_selectors = Command::new(luna_bin());
    both_selectors
        .args([
            "runtime",
            "trace",
            "--latest",
            "--turn",
            "00000000-0000-0000-0000-000000000000",
            "--log",
        ])
        .arg(&log);
    let (_stdout, stderr) = assert_failure(both_selectors);
    assert!(stderr.contains("exactly one selector"), "stderr:\n{stderr}");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_startup_check_accepts_clean_log_and_rejects_hash_tampering() {
    let root = temp_root("startup_check");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    let mut turn = Command::new(luna_bin());
    turn.args(["runtime", "turn", "Morgan lives in Iowa.", "--log"])
        .arg(&log);
    assert_success(turn);

    let mut clean = Command::new(luna_bin());
    clean
        .args(["runtime", "startup-check", "--format", "json", "--log"])
        .arg(&log);
    let stdout = assert_success(clean);
    let clean_json: Value = serde_json::from_str(&stdout).expect("startup JSON");
    assert_eq!(clean_json["status"], Value::String("clean".to_string()));

    let mut first_line: Value =
        serde_json::from_str(fs::read_to_string(&log).unwrap().lines().next().unwrap())
            .expect("first log line JSON");
    first_line["event_hash"] = Value::String("0".repeat(64));
    let mut lines = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    lines[0] = serde_json::to_string(&first_line).unwrap();
    fs::write(&log, format!("{}\n", lines.join("\n"))).unwrap();

    let mut tampered = Command::new(luna_bin());
    tampered
        .args(["runtime", "startup-check", "--log"])
        .arg(&log);
    let (_stdout, stderr) = assert_failure(tampered);
    assert!(
        stderr.contains("event hash mismatch") || stderr.contains("hash mismatch"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_inspect_filters_by_entity_and_lifecycle_status() {
    let root = temp_root("inspect_filters");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    for turn in [
        "Morgan lives in Iowa.",
        "Actually Morgan lives in Ohio now.",
        "I am a software developer.",
    ] {
        let mut command = Command::new(luna_bin());
        command
            .args(["runtime", "turn", turn])
            .args(["--log"])
            .arg(&log);
        assert_success(command);
    }

    let mut current = Command::new(luna_bin());
    current
        .args(["runtime", "inspect", "--current", "--entity", "Morgan"])
        .args(["--log"])
        .arg(&log);
    let stdout = assert_success(current);
    assert!(stdout.contains("Morgan lives in Ohio"), "stdout:\n{stdout}");
    assert!(
        !stdout.contains("Morgan lives in Iowa"),
        "current entity inspect should hide superseded claims:\n{stdout}"
    );
    assert!(
        !stdout.contains("software developer"),
        "entity inspect should hide unrelated claims:\n{stdout}"
    );

    let mut superseded = Command::new(luna_bin());
    superseded
        .args(["runtime", "inspect", "--superseded", "--entity", "Morgan"])
        .args(["--log"])
        .arg(&log);
    let stdout = assert_success(superseded);
    assert!(stdout.contains("Morgan lives in Iowa"), "stdout:\n{stdout}");
    assert!(
        !stdout.contains("Morgan lives in Ohio"),
        "superseded entity inspect should hide current claims:\n{stdout}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_inspect_targeted_reports_are_event_backed() {
    let root = temp_root("inspect_targeted");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    let mut seed = Command::new(luna_bin());
    seed.args([
        "runtime",
        "turn",
        "Morgan lives in Iowa.",
        "--format",
        "json",
        "--log",
    ])
    .arg(&log);
    let seed_stdout = assert_success(seed);
    let seed_json: Value = serde_json::from_str(&seed_stdout).expect("seed turn JSON");
    let seed_turn_id = seed_json["turn_id"].as_str().expect("seed turn id");

    let mut correction = Command::new(luna_bin());
    correction
        .args([
            "runtime",
            "turn",
            "Actually Morgan lives in Ohio now.",
            "--log",
        ])
        .arg(&log);
    assert_success(correction);

    let old_key = "person:location=Morgan_lives_in_Iowa";
    let mut claim = Command::new(luna_bin());
    claim
        .args([
            "runtime", "inspect", "--claim", old_key, "--format", "json", "--log",
        ])
        .arg(&log);
    let claim_stdout = assert_success(claim);
    let claim_json: Value = serde_json::from_str(&claim_stdout).expect("claim inspect JSON");
    assert_eq!(
        claim_json["selector"],
        Value::String(format!("claim:{old_key}"))
    );
    assert_eq!(
        claim_json["claims"][0]["lifecycle_status"],
        Value::String("superseded".to_string())
    );
    assert!(claim_json["events"]
        .as_array()
        .expect("claim events")
        .iter()
        .any(|event| event["payload_type"] == Value::String("assertion_corrected".to_string())));
    assert!(claim_json["events"]
        .as_array()
        .expect("claim events")
        .iter()
        .all(|event| event["event_hash"].as_str().unwrap_or_default().len() == 64));

    let mut turn = Command::new(luna_bin());
    turn.args([
        "runtime",
        "inspect",
        "--turn",
        seed_turn_id,
        "--format",
        "json",
        "--log",
    ])
    .arg(&log);
    let turn_stdout = assert_success(turn);
    let turn_json: Value = serde_json::from_str(&turn_stdout).expect("turn inspect JSON");
    assert_eq!(
        turn_json["selector"],
        Value::String(format!("turn:{seed_turn_id}"))
    );
    let events = turn_json["events"].as_array().expect("turn events");
    assert!(!events.is_empty());
    assert!(events
        .iter()
        .all(|event| event["turn_id"] == Value::String(seed_turn_id.to_string())));
    let first_event_id = events[0]["event_id"]
        .as_str()
        .expect("turn inspect event id")
        .to_string();

    let mut event = Command::new(luna_bin());
    event
        .args([
            "runtime",
            "inspect",
            "--event",
            &first_event_id,
            "--format",
            "json",
            "--log",
        ])
        .arg(&log);
    let event_stdout = assert_success(event);
    let event_json: Value = serde_json::from_str(&event_stdout).expect("event inspect JSON");
    assert_eq!(
        event_json["selector"],
        Value::String(format!("event:{first_event_id}"))
    );
    assert_eq!(event_json["events"].as_array().expect("events").len(), 1);

    let mut unknown_turn = Command::new(luna_bin());
    unknown_turn
        .args([
            "runtime",
            "inspect",
            "--turn",
            "00000000-0000-0000-0000-000000000000",
            "--log",
        ])
        .arg(&log);
    let (_stdout, stderr) = assert_failure(unknown_turn);
    assert!(
        stderr.contains("no events found for turn"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_inspect_missing_explains_why_not_remembered() {
    let root = temp_root("inspect_missing");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    for turn in [
        "Morgan lives in Iowa.",
        "Actually Morgan lives in Ohio now.",
        "I am a software developer.",
    ] {
        let mut command = Command::new(luna_bin());
        command
            .args(["runtime", "turn", turn])
            .args(["--log"])
            .arg(&log);
        assert_success(command);
    }

    // Test: superseded claim is found and explained
    let mut missing = Command::new(luna_bin());
    missing
        .args(["runtime", "inspect", "--missing", "Iowa"])
        .args(["--log"])
        .arg(&log);
    let stdout = assert_success(missing);
    assert!(
        stdout.contains("Why Not Remembered: \"Iowa\""),
        "stdout:\n{stdout}"
    );
    assert!(stdout.contains("Morgan lives in Iowa"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("superseded by a newer correction"),
        "stdout:\n{stdout}"
    );

    // Test: unknown term reports not found
    let mut unknown = Command::new(luna_bin());
    unknown
        .args(["runtime", "inspect", "--missing", "rocket_ship"])
        .args(["--log"])
        .arg(&log);
    let stdout = assert_success(unknown);
    assert!(stdout.contains("No claims found"), "stdout:\n{stdout}");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_why_not_reports_missing_superseded_and_unconfirmed_causes() {
    let root = temp_root("why_not");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    for turn in [
        "Morgan lives in Iowa.",
        "Actually Morgan lives in Ohio now.",
    ] {
        let mut command = Command::new(luna_bin());
        command
            .args(["runtime", "turn", turn])
            .args(["--log"])
            .arg(&log);
        assert_success(command);
    }

    let mut superseded = Command::new(luna_bin());
    superseded
        .args(["runtime", "why-not", "Iowa", "--format", "json", "--log"])
        .arg(&log);
    let stdout = assert_success(superseded);
    let report: Value = serde_json::from_str(&stdout).expect("why-not superseded JSON");
    assert_eq!(
        report["summary"],
        Value::String("memory_exists_but_is_not_answerable".to_string())
    );
    assert!(report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .any(|finding| finding["cause"] == Value::String("claim_superseded".to_string())));

    let unconfirmed_log = root.join("unconfirmed-events.jsonl");
    let mut seed_unconfirmed = Command::new(luna_bin());
    seed_unconfirmed
        .args(["runtime", "turn", "Morgan lives in Iowa.", "--log"])
        .arg(&unconfirmed_log);
    assert_success(seed_unconfirmed);

    let mut unconfirmed = Command::new(luna_bin());
    unconfirmed
        .args(["runtime", "why-not", "Iowa", "--format", "json", "--log"])
        .arg(&unconfirmed_log);
    let stdout = assert_success(unconfirmed);
    let report: Value = serde_json::from_str(&stdout).expect("why-not unconfirmed JSON");
    assert!(report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .any(|finding| finding["cause"]
            == Value::String("confidence_below_answer_threshold".to_string())));

    let mut missing = Command::new(luna_bin());
    missing
        .args([
            "runtime",
            "why-not",
            "rocket_ship",
            "--format",
            "json",
            "--log",
        ])
        .arg(&log);
    let stdout = assert_success(missing);
    let report: Value = serde_json::from_str(&stdout).expect("why-not missing JSON");
    assert_eq!(
        report["summary"],
        Value::String("no_matching_entity_or_claim".to_string())
    );
    assert!(report["findings"].as_array().expect("findings").is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_brief_is_event_derived_and_marks_corrections() {
    let root = temp_root("brief");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    for turn in [
        "Morgan lives in Iowa.",
        "Actually Morgan lives in Ohio now.",
        "Where does Morgan live?",
    ] {
        let mut command = Command::new(luna_bin());
        command
            .args(["runtime", "turn", turn])
            .args(["--log"])
            .arg(&log);
        assert_success(command);
    }

    let mut brief = Command::new(luna_bin());
    brief
        .args(["runtime", "brief", "--format", "json", "--log"])
        .arg(&log);
    let stdout = assert_success(brief);
    let report: Value = serde_json::from_str(&stdout).expect("brief JSON");
    assert_eq!(
        report["doctrine_gate_status"],
        Value::String("replay_clean".to_string())
    );
    assert!(
        report["recent_turns"]
            .as_array()
            .expect("recent turns")
            .len()
            >= 3
    );
    assert!(report["recent_turns"]
        .as_array()
        .expect("recent turns")
        .iter()
        .all(|turn| turn["event_hash"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64)));

    let corrections = report["recent_corrections"]
        .as_array()
        .expect("recent corrections");
    assert!(
        corrections.iter().any(|correction| {
            correction["old_value"]
                .as_str()
                .is_some_and(|value| value.contains("Iowa"))
                && correction["new_value"]
                    .as_str()
                    .is_some_and(|value| value.contains("Ohio"))
                && correction["event_hash"]
                    .as_str()
                    .is_some_and(|hash| hash.len() == 64)
        }),
        "report:\n{stdout}"
    );

    assert!(report["suppressed_claims"]
        .as_array()
        .expect("suppressed claims")
        .iter()
        .any(|claim| claim["value"]
            .as_str()
            .is_some_and(|value| value.contains("Iowa"))
            && claim["lifecycle_status"] == Value::String("superseded".to_string())));
    assert!(!report["current_high_confidence_claims"]
        .as_array()
        .expect("current high confidence claims")
        .iter()
        .any(|claim| claim["value"]
            .as_str()
            .is_some_and(|value| value.contains("Iowa"))));
    assert!(report["latest_working_memory_trace"].is_object());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_brief_shows_memory_repairs_after_failed_query() {
    let root = temp_root("brief_repairs");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    for turn in [
        "Who is Chris?",
        "Chris is my co-founder. Chris is married and lives in Iowa. Chris built Luna.",
    ] {
        let mut command = Command::new(luna_bin());
        command
            .args(["runtime", "turn", turn])
            .args(["--log"])
            .arg(&log);
        assert_success(command);
    }

    let mut brief = Command::new(luna_bin());
    brief
        .args(["runtime", "brief", "--format", "json", "--log"])
        .arg(&log);
    let stdout = assert_success(brief);
    let report: Value = serde_json::from_str(&stdout).expect("brief JSON");
    let repairs = report["recent_repairs"].as_array().expect("repairs");
    assert_eq!(repairs.len(), 1, "report:\n{stdout}");
    assert_eq!(
        repairs[0]["failed_query"],
        Value::String("Who is Chris?".to_string())
    );
    assert!(repairs[0]["repaired_claim_keys"]
        .as_array()
        .expect("repair claims")
        .iter()
        .any(|key| key == "person:role=Chris_is_my_co-founder"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_retention_exports_repair_ledger_jsonl() {
    let root = temp_root("retention");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");
    let retention_log = root.join("retention.jsonl");

    for turn in [
        "Who is Chris?",
        "Chris is my co-founder. Chris is married and lives in Iowa. Chris built Luna.",
    ] {
        let mut command = Command::new(luna_bin());
        command
            .args(["runtime", "turn", turn])
            .args(["--log"])
            .arg(&log);
        assert_success(command);
    }

    let mut retention = Command::new(luna_bin());
    retention
        .args(["runtime", "retention", "--format", "json", "--out"])
        .arg(&retention_log)
        .args(["--log"])
        .arg(&log);
    let stdout = assert_success(retention);
    let report: Value = serde_json::from_str(&stdout).expect("retention JSON");
    assert_eq!(
        report["retention_events"]
            .as_array()
            .expect("retention events")
            .len(),
        1
    );
    assert!(report["retention_events"][0]["future_recall_hint"]
        .as_str()
        .expect("future recall hint")
        .contains("boost these repaired claim keys"));
    let jsonl = fs::read_to_string(&retention_log).expect("retention JSONL");
    assert!(jsonl.contains("\"failed_query\":\"Who is Chris?\""));
    assert!(jsonl.contains("person:creation=Chris_built_Luna"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_import_onboarding_creates_replay_clean_memory_graph() {
    let root = temp_root("import_onboarding");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");
    let seed = root.join("seed.md");
    fs::write(
        &seed,
        r#"# Luna Onboarding Memory Seed

## User Identity

Name:
Joseph White.

Preferred name:
Joe.

## Core Projects

### Luna

What it is:
A local-first event-sourced memory runtime.

Why it matters:
It proves memory through replay and provenance.

### WriteMind

What it is:
A local-first desktop novel-writing app.
"#,
    )
    .unwrap();

    let mut import = Command::new(luna_bin());
    import
        .args(["runtime", "import-onboarding", "--format", "json"])
        .arg(&seed)
        .args(["--log"])
        .arg(&log);
    let stdout = assert_success(import);
    let report: Value = serde_json::from_str(&stdout).expect("import JSON");
    assert_eq!(
        report["replay_audit"]["status"],
        Value::String("clean".to_string())
    );
    assert!(
        report["imported_assertions"].as_u64().unwrap_or_default() >= 3,
        "report:\n{stdout}"
    );
    assert!(
        report["node_count"].as_u64().unwrap_or_default() >= 3,
        "report:\n{stdout}"
    );

    let mut status = Command::new(luna_bin());
    status
        .args(["runtime", "status", "--format", "json", "--log"])
        .arg(&log);
    let status_stdout = assert_success(status);
    let status_report: Value = serde_json::from_str(&status_stdout).expect("status JSON");
    assert_eq!(
        status_report["replay_audit"]["status"],
        Value::String("clean".to_string())
    );
    assert!(
        status_report["replay_audit"]["replayed_counts"]["memory_edges"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "status:\n{status_stdout}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_inspect_lattice_shows_sources() {
    let root = temp_root("inspect_lattice");
    fs::create_dir_all(&root).unwrap();
    let log = root.join("events.jsonl");

    let mut turn = Command::new(luna_bin());
    turn.args(["runtime", "turn", "I am a software developer."])
        .args(["--log"])
        .arg(&log);
    assert_success(turn);

    let mut inspect = Command::new(luna_bin());
    inspect
        .args(["runtime", "inspect", "--lattice"])
        .args(["--log"])
        .arg(&log);
    let stdout = assert_success(inspect);

    assert!(stdout.contains("Attention Lattice"), "stdout:\n{stdout}");
    assert!(stdout.contains("identity:"), "stdout:\n{stdout}");
    assert!(stdout.contains("identity:profession"), "stdout:\n{stdout}");

    let _ = fs::remove_dir_all(root);
}
