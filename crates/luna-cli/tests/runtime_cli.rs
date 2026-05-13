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
    assert!(
        stdout.contains("Morgan lives in Iowa"),
        "stdout:\n{stdout}"
    );
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
    assert!(
        stdout.contains("No claims found"),
        "stdout:\n{stdout}"
    );

    let _ = fs::remove_dir_all(root);
}
