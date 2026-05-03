//! [`LlmBackend`] trait and the test-only [`RecordingFakeBackend`].
//!
//! The trait is the seam Luna's extraction flow uses to call out to a
//! language model. PR 0.3 ships only the recording fake; the real
//! `CommandBackend` (shells out to a local `llama-cli`) lands in PR
//! 0.3b alongside its own determinism harness.
//!
//! ## Determinism contract
//!
//! [`LlmBackend::model_id`] is the **opaque key** that determines
//! cache equivalence. Two calls that share `model_id`, prompt, and
//! input MUST receive identical responses. If a backend changes its
//! decoding flags (temperature, top_p, seed, sampling strategy, CPU
//! vs GPU, quantization), the implementor MUST change `model_id`. The
//! cache key has no parallel `decoding_strategy_id` field; bake every
//! determinism-affecting parameter into `model_id`.
//!
//! Recommended `model_id` form:
//!
//! ```text
//! "<model-name>@<backend-mode>-<sampling>-<seed>-<version>"
//! e.g. "llama3-8b-q4@cpu-greedy-seed42-v1"
//! ```

use luna_core::{LunaError, Result};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// One LLM call's inputs. Carries the fully-rendered prompt the
/// extractor wants completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRequest {
    pub prompt: String,
}

pub trait LlmBackend {
    /// The opaque determinism key; see [module docs](self).
    fn model_id(&self) -> &str;

    /// Run the model on `request.prompt`. Must return identical bytes
    /// for identical inputs under the same `model_id`. Errors are
    /// propagated; the extractor never caches failed completions.
    fn complete(&self, request: &LlmRequest) -> Result<String>;
}

// Forwarding impl so the CLI can dispatch on a runtime-chosen backend
// (e.g. `--backend fixture|command`) by holding it as a trait object.
// `LunaExtractor<B, C>` is generic over a concrete backend; without
// this impl, `Box<dyn LlmBackend>` cannot be used as `B`.
impl LlmBackend for Box<dyn LlmBackend> {
    fn model_id(&self) -> &str {
        (**self).model_id()
    }
    fn complete(&self, request: &LlmRequest) -> Result<String> {
        (**self).complete(request)
    }
}

/// Shared mutable state behind the recording fake. Cloned handles see
/// the same registrations and the same call log.
#[derive(Debug, Default)]
struct RecordingFakeInner {
    /// Prescribed responses, matched by checking each registered
    /// `marker` against the request prompt as a substring. First match
    /// wins; insertion order is preserved by `Vec`.
    prescriptions: Vec<(String, String)>,
    calls: Vec<LlmRequest>,
}

/// Test-only backend that records every call and returns prescribed
/// responses based on substring matches against the prompt.
///
/// Cloning produces a new handle pointing at the same shared inner
/// state. Tests typically construct one fake, register expectations,
/// clone a handle into [`crate::LlmExtractor`], and inspect the
/// original handle's `call_count` and `last_request` after running.
#[derive(Debug, Clone)]
pub struct RecordingFakeBackend {
    model_id: String,
    inner: Arc<Mutex<RecordingFakeInner>>,
}

impl RecordingFakeBackend {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            inner: Arc::new(Mutex::new(RecordingFakeInner::default())),
        }
    }

    /// Register a prescribed response. The fake returns `response` for
    /// any subsequent `complete` whose request prompt contains
    /// `marker` as a substring. First registered match wins.
    pub fn expect(&self, marker: impl Into<String>, response: impl Into<String>) {
        self.inner
            .lock()
            .unwrap()
            .prescriptions
            .push((marker.into(), response.into()));
    }

    pub fn call_count(&self) -> usize {
        self.inner.lock().unwrap().calls.len()
    }

    pub fn last_request(&self) -> Option<LlmRequest> {
        self.inner.lock().unwrap().calls.last().cloned()
    }

    pub fn calls(&self) -> Vec<LlmRequest> {
        self.inner.lock().unwrap().calls.clone()
    }
}

impl LlmBackend for RecordingFakeBackend {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn complete(&self, request: &LlmRequest) -> Result<String> {
        let mut inner = self.inner.lock().unwrap();
        inner.calls.push(request.clone());
        for (marker, response) in &inner.prescriptions {
            if request.prompt.contains(marker) {
                return Ok(response.clone());
            }
        }
        Err(LunaError::new(format!(
            "RecordingFakeBackend: no prescription matches request (registered {} marker(s))",
            inner.prescriptions.len()
        )))
    }
}

/// Production-style fake: reads the LLM response from a directory of
/// pre-authored JSON files, keyed by SHA-256 of the rendered prompt.
///
/// Used by `luna bench formation` (PR 0.5b) when no real LLM is wired
/// (PR 0.6 / 0.3b). A fixture covers exactly one (prompt template,
/// turn content, turn timestamp, role) tuple — change any of those
/// and the fixture file is stale, by design. The
/// [`crate::ExtractionCache`] still sits above this backend, so a
/// second formation pass returns from cache without re-reading the
/// fixture file.
///
/// Layout: `<root>/<sha256_of_rendered_prompt_hex>.json`. File
/// content is the LLM's would-be response (a JSON
/// [`crate::LlmObservation`]), exactly as if a real backend had
/// produced it.
///
/// `model_id` is `"fixture@v1"`. Bump if the fixture format ever
/// changes.
#[derive(Debug, Clone)]
pub struct FixtureBackend {
    root: PathBuf,
}

impl FixtureBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path the fixture for a given prompt hash should live at.
    /// Helpful for fixture-authoring tools (and tests).
    pub fn path_for_prompt(&self, prompt: &str) -> PathBuf {
        self.root.join(format!("{}.json", prompt_hash_hex(prompt)))
    }
}

impl LlmBackend for FixtureBackend {
    fn model_id(&self) -> &str {
        "fixture@v1"
    }

    fn complete(&self, request: &LlmRequest) -> Result<String> {
        let path = self.path_for_prompt(&request.prompt);
        if !path.exists() {
            return Err(LunaError::new(format!(
                "FixtureBackend: missing fixture {} (rendered-prompt hash {}, prompt starts: '{}...')",
                path.display(),
                prompt_hash_hex(&request.prompt),
                request.prompt.chars().take(60).collect::<String>(),
            )));
        }
        std::fs::read_to_string(&path).map_err(|err| {
            LunaError::new(format!("FixtureBackend: read {}: {err}", path.display()))
        })
    }
}

fn prompt_hash_hex(prompt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let bytes = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in bytes {
        write!(&mut out, "{:02x}", byte).expect("write to String");
    }
    out
}

/// Spawns an external program for each `complete` call, writes the
/// prompt to its stdin, and reads the response from stdout. Errors
/// propagate; nothing is cached on a non-zero exit, on a timeout, or
/// on a stderr-only failure.
///
/// ## Contract
///
/// The configured command MUST:
///
/// 1. Read prompt bytes from stdin until EOF.
/// 2. Emit the response (a JSON [`crate::LlmObservation`]) on stdout.
/// 3. Exit with code 0 on success.
/// 4. Be deterministic for identical input under the same `model_id`.
///    Determinism (CPU mode, temperature 0, fixed seed, no streaming
///    artifacts) is the user's responsibility — encode every flag
///    that affects output into `model_id` so the cache invalidates
///    correctly when the user changes them. CommandBackend itself
///    does NOT enforce determinism.
///
/// If the user's preferred LLM CLI does not read stdin, they wrap it
/// in a small shell script that reads stdin into a temp file and
/// passes the file path to the CLI.
///
/// ## Example invocation
///
/// ```ignore
/// // Wraps llama-cli in a script that takes prompt on stdin and
/// // writes a JSON response to stdout. The script's exact form is
/// // user-supplied.
/// let backend = CommandBackend::new(
///     "/usr/local/bin/llama-extract.sh",
///     vec!["--model".to_string(), "models/llama3-8b-q4.gguf".to_string()],
///     "llama3-8b-q4@cpu-greedy-seed42-v1",
/// )
/// .with_timeout(std::time::Duration::from_secs(60));
/// ```
pub struct CommandBackend {
    program: PathBuf,
    args: Vec<String>,
    model_id: String,
    timeout: std::time::Duration,
}

impl CommandBackend {
    pub fn new(
        program: impl Into<PathBuf>,
        args: Vec<String>,
        model_id: impl Into<String>,
    ) -> Self {
        Self {
            program: program.into(),
            args,
            model_id: model_id.into(),
            timeout: std::time::Duration::from_secs(120),
        }
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn timeout(&self) -> std::time::Duration {
        self.timeout
    }
}

impl LlmBackend for CommandBackend {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn complete(&self, request: &LlmRequest) -> Result<String> {
        use std::io::{Read, Write};
        use std::process::{Command, Stdio};
        use std::thread;
        use std::time::{Duration, Instant};

        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                LunaError::new(format!(
                    "CommandBackend: failed to spawn '{}': {err}",
                    self.program.display(),
                ))
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            LunaError::new("CommandBackend: child stdin handle unexpectedly missing")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            LunaError::new("CommandBackend: child stdout handle unexpectedly missing")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            LunaError::new("CommandBackend: child stderr handle unexpectedly missing")
        })?;

        // Three concurrent helper threads avoid the classic deadlock
        // where the child blocks writing to a full stdout pipe while
        // we're blocked reading stderr (or vice versa).
        let prompt = request.prompt.clone();
        let writer = thread::spawn(move || -> std::io::Result<()> {
            let mut stdin = stdin;
            stdin.write_all(prompt.as_bytes())?;
            stdin.flush()?;
            drop(stdin);
            Ok(())
        });

        let stdout_reader = thread::spawn(move || -> std::io::Result<String> {
            let mut buf = String::new();
            let mut stdout = stdout;
            stdout.read_to_string(&mut buf)?;
            Ok(buf)
        });

        let stderr_reader = thread::spawn(move || -> std::io::Result<String> {
            let mut buf = String::new();
            let mut stderr = stderr;
            stderr.read_to_string(&mut buf)?;
            Ok(buf)
        });

        // Polled wait. Sleep granularity is 50ms; it bounds the latency
        // we add to a fast successful call but keeps the timeout path
        // responsive enough.
        let start = Instant::now();
        let exit_status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if start.elapsed() > self.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(LunaError::new(format!(
                            "CommandBackend: timeout after {}s waiting for '{}'",
                            self.timeout.as_secs(),
                            self.program.display(),
                        )));
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(err) => {
                    return Err(LunaError::new(format!(
                        "CommandBackend: try_wait error: {err}",
                    )));
                }
            }
        };

        let _ = writer.join();
        let stdout_text = stdout_reader
            .join()
            .map_err(|_| LunaError::new("CommandBackend: stdout reader thread panicked"))?
            .map_err(|err| LunaError::new(format!("CommandBackend: stdout read: {err}")))?;
        let stderr_text = stderr_reader
            .join()
            .map_err(|_| LunaError::new("CommandBackend: stderr reader thread panicked"))?
            .unwrap_or_default();

        if !exit_status.success() {
            return Err(LunaError::new(format!(
                "CommandBackend: '{}' exited with status {} (stderr: {})",
                self.program.display(),
                exit_status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "?".to_string()),
                stderr_text.trim(),
            )));
        }

        Ok(stdout_text)
    }
}

/// Decorator that counts every `complete` call passing through. Used
/// by formation runs (PR 0.5) to compute second-pass cache hit rate
/// without modifying the underlying backend's interface. Wraps any
/// `LlmBackend` impl.
pub struct CountingBackend<B: LlmBackend> {
    inner: B,
    count: std::sync::atomic::AtomicUsize,
}

impl<B: LlmBackend> CountingBackend<B> {
    pub fn new(inner: B) -> Self {
        Self {
            inner,
            count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Number of completions that have flowed through this decorator
    /// since construction. Cache hits at the [`crate::ExtractionCache`]
    /// layer never reach the backend, so this counts only true LLM
    /// invocations.
    pub fn count(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn inner(&self) -> &B {
        &self.inner
    }
}

impl<B: LlmBackend> LlmBackend for CountingBackend<B> {
    fn model_id(&self) -> &str {
        self.inner.model_id()
    }

    fn complete(&self, request: &LlmRequest) -> Result<String> {
        self.count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.complete(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_returns_prescribed_response_for_matching_marker() {
        let fake = RecordingFakeBackend::new("test-model");
        fake.expect("HELLO", r#"{"a": 1}"#);
        let response = fake
            .complete(&LlmRequest {
                prompt: "this prompt contains HELLO somewhere".to_string(),
            })
            .unwrap();
        assert_eq!(response, r#"{"a": 1}"#);
    }

    #[test]
    fn fake_errors_when_no_marker_matches() {
        let fake = RecordingFakeBackend::new("test-model");
        fake.expect("HELLO", "ok");
        let result = fake.complete(&LlmRequest {
            prompt: "no greeting here".to_string(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn fake_records_all_calls_across_clones() {
        let fake = RecordingFakeBackend::new("test-model");
        fake.expect("X", "ok");
        let handle = fake.clone();
        let _ = handle.complete(&LlmRequest {
            prompt: "X-1".to_string(),
        });
        let _ = handle.complete(&LlmRequest {
            prompt: "X-2".to_string(),
        });
        assert_eq!(fake.call_count(), 2);
        assert_eq!(handle.call_count(), 2);
        assert_eq!(fake.last_request().unwrap().prompt, "X-2");
    }

    #[test]
    fn fake_first_registered_match_wins() {
        let fake = RecordingFakeBackend::new("test-model");
        fake.expect("X", "first");
        fake.expect("X", "second");
        let response = fake
            .complete(&LlmRequest {
                prompt: "X".to_string(),
            })
            .unwrap();
        assert_eq!(response, "first");
    }

    #[test]
    fn model_id_is_returned_verbatim() {
        let fake = RecordingFakeBackend::new("llama3-8b-q4@cpu-greedy-seed42-v1");
        assert_eq!(fake.model_id(), "llama3-8b-q4@cpu-greedy-seed42-v1");
    }

    #[test]
    fn counting_backend_increments_on_each_complete() {
        let fake = RecordingFakeBackend::new("test-model");
        fake.expect("X", "ok");
        let counted = CountingBackend::new(fake);
        assert_eq!(counted.count(), 0);
        let _ = counted.complete(&LlmRequest {
            prompt: "X-1".to_string(),
        });
        assert_eq!(counted.count(), 1);
        let _ = counted.complete(&LlmRequest {
            prompt: "X-2".to_string(),
        });
        assert_eq!(counted.count(), 2);
    }

    #[test]
    fn counting_backend_passes_through_model_id() {
        let counted = CountingBackend::new(RecordingFakeBackend::new("inner-model"));
        assert_eq!(counted.model_id(), "inner-model");
    }

    #[test]
    fn fixture_backend_returns_file_contents_for_matching_prompt() {
        let dir = std::env::temp_dir().join(format!("luna_fixture_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = FixtureBackend::new(&dir);
        let prompt = "rendered prompt with content X";
        let response = r#"{"hello": "world"}"#;
        let path = fixture.path_for_prompt(prompt);
        std::fs::write(&path, response).unwrap();

        let got = fixture
            .complete(&LlmRequest {
                prompt: prompt.to_string(),
            })
            .unwrap();
        assert_eq!(got, response);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fixture_backend_errors_on_missing_fixture_with_helpful_path() {
        let dir = std::env::temp_dir().join(format!("luna_fixture_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = FixtureBackend::new(&dir);
        let result = fixture.complete(&LlmRequest {
            prompt: "no fixture for this".to_string(),
        });
        let err = result.unwrap_err().to_string();
        assert!(err.contains("FixtureBackend"));
        assert!(err.contains("missing"));
        // The error should include the expected on-disk path so the
        // user knows where to write the fixture.
        assert!(err.contains(".json"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fixture_backend_path_changes_with_prompt() {
        let fixture = FixtureBackend::new("/tmp/luna_fixtures");
        let a = fixture.path_for_prompt("prompt A");
        let b = fixture.path_for_prompt("prompt B");
        assert_ne!(a, b);
    }

    #[test]
    fn command_backend_returns_command_stdout() {
        // `cargo --version` is universally available in any luna dev
        // environment and produces deterministic output across platforms.
        // Verifies spawn, wait, and stdout collection.
        let backend = CommandBackend::new(
            "cargo",
            vec!["--version".to_string()],
            "cargo-version-test",
        );
        let response = backend
            .complete(&LlmRequest {
                prompt: "ignored by cargo".to_string(),
            })
            .expect("cargo --version should succeed");
        assert!(
            response.contains("cargo"),
            "expected cargo version banner, got: {response:?}"
        );
    }

    #[test]
    fn command_backend_errors_on_nonexistent_program() {
        let backend = CommandBackend::new(
            "this-binary-does-not-exist-and-should-fail-to-spawn-12345",
            vec![],
            "nonexistent-test",
        );
        let result = backend.complete(&LlmRequest {
            prompt: "any".to_string(),
        });
        let err = result.unwrap_err().to_string();
        assert!(err.contains("CommandBackend"));
        assert!(err.contains("failed to spawn"));
    }

    #[test]
    fn command_backend_errors_on_nonzero_exit_with_stderr_capture() {
        // `cargo run-a-subcommand-that-does-not-exist` exits non-zero
        // and prints a clear error to stderr. The backend must capture
        // stderr in its error message.
        let backend = CommandBackend::new(
            "cargo",
            vec!["run-a-subcommand-that-does-not-exist".to_string()],
            "cargo-bad-subcommand",
        );
        let result = backend.complete(&LlmRequest {
            prompt: "ignored".to_string(),
        });
        let err = result.unwrap_err().to_string();
        assert!(err.contains("CommandBackend"));
        assert!(err.contains("exited"));
    }

    #[test]
    fn command_backend_model_id_is_the_user_supplied_string() {
        let backend = CommandBackend::new(
            "noop",
            vec![],
            "llama3-8b-q4@cpu-greedy-seed42-v1",
        );
        assert_eq!(backend.model_id(), "llama3-8b-q4@cpu-greedy-seed42-v1");
    }

    #[test]
    fn box_dyn_llm_backend_forwards_to_inner() {
        // Locks the impl needed for runtime CLI dispatch on
        // --backend fixture|command.
        let fake = RecordingFakeBackend::new("inner-model");
        fake.expect("X", "ok");
        let boxed: Box<dyn LlmBackend> = Box::new(fake);
        assert_eq!(boxed.model_id(), "inner-model");
        let response = boxed
            .complete(&LlmRequest {
                prompt: "X".to_string(),
            })
            .unwrap();
        assert_eq!(response, "ok");
    }
}
