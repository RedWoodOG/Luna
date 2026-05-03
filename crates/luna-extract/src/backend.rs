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
}
