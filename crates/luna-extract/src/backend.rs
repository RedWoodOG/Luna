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
}
