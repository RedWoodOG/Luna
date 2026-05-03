//! Deterministic second sources for the four contour signal
//! dimensions used by the v0.1 proof program.
//!
//! Each detector reads a [`ConversationTurn`] and returns zero or more
//! signals tagged by dimension name. The signals are independent of
//! the LLM's [`crate::LlmObservation`] — they are the structural
//! second source that, together with the LLM's claim, satisfies
//! [`luna_core::Signal::can_influence_recall`]'s `source_count >= 2`
//! invariant.
//!
//! Detectors emit only signals; they never produce assertions.
//! Assertion construction happens exclusively from validated
//! [`crate::LlmObservation`] data in [`crate::fusion`]. Detectors
//! also never produce signals for dimensions outside the
//! four-axis allowlist.
//!
//! ## Temporal-detector scope (PR 0.4)
//!
//! The [`TemporalDetector`] is **lexical only** in PR 0.4. It scans
//! turn content for temporal cue phrases and emits a signal if any
//! match. It does not read [`ConversationTurn::timestamp`] for signal
//! generation; timestamps stay metadata at extraction time and become
//! signal at recall time. Timestamp-driven temporal arithmetic
//! (relational gaps between disclosure and probe) belongs to
//! PR 0.5/formation or Stage 3 dynamics.

use luna_core::{ConversationTurn, Signal, SignalReliability};
use std::collections::HashMap;

pub trait SecondSource: Send + Sync {
    fn detect(&self, turn: &ConversationTurn) -> HashMap<String, Signal>;
}

/// Lexical temporal cue detector. Emits `temporal_relevance` when any
/// of a frozen list of phrases appears in the turn content.
pub struct TemporalDetector {
    cues: &'static [(&'static str, f32)],
}

const TEMPORAL_CUES: &[(&str, f32)] = &[
    ("yesterday", 0.85),
    ("today", 0.75),
    ("this morning", 0.85),
    ("tonight", 0.75),
    ("this week", 0.7),
    ("last week", 0.85),
    ("last month", 0.65),
    ("last year", 0.55),
    ("recently", 0.7),
    ("a while back", 0.55),
    ("earlier this year", 0.6),
    ("earlier this", 0.55),
    ("months ago", 0.6),
    ("a few days ago", 0.75),
    ("a few weeks ago", 0.65),
    ("right now", 0.85),
    ("currently", 0.7),
    ("ago", 0.5),
    ("week of", 0.65),
    ("during", 0.5),
    ("just now", 0.85),
    ("lately", 0.7),
    ("the other day", 0.7),
];

impl TemporalDetector {
    pub fn new() -> Self {
        Self { cues: TEMPORAL_CUES }
    }
}

impl Default for TemporalDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SecondSource for TemporalDetector {
    fn detect(&self, turn: &ConversationTurn) -> HashMap<String, Signal> {
        let normalized = turn.content.to_ascii_lowercase();
        let mut max_intensity = 0.0_f32;
        for (phrase, intensity) in self.cues {
            if normalized.contains(phrase) {
                max_intensity = max_intensity.max(*intensity);
            }
        }
        let mut out = HashMap::new();
        if max_intensity > 0.0 {
            out.insert(
                "temporal_relevance".to_string(),
                Signal::new(max_intensity, 0.7, SignalReliability::Heuristic),
            );
        }
        out
    }
}

/// Frozen affect lexicon. Emits `emotional_arousal` when any matching
/// word or short phrase appears in the turn content.
pub struct AffectLexicon {
    words: &'static [(&'static str, f32)],
}

const AFFECT_WORDS: &[(&str, f32)] = &[
    ("terrified", 0.95),
    ("panicked", 0.95),
    ("furious", 0.9),
    ("devastated", 0.9),
    ("burned out", 0.85),
    ("hated", 0.85),
    ("hate", 0.8),
    ("anxious", 0.8),
    ("angry", 0.8),
    ("urgent", 0.85),
    ("stressed", 0.8),
    ("worried", 0.75),
    ("frustrated", 0.75),
    ("nervous", 0.75),
    ("tense", 0.7),
    ("upset", 0.7),
    ("draining", 0.7),
    ("wearing me down", 0.8),
    ("struggling", 0.75),
    ("excited", 0.8),
    ("thrilled", 0.85),
    ("happy", 0.55),
    ("sad", 0.7),
    ("heavy", 0.6),
    ("bothered", 0.65),
    ("rough", 0.6),
    ("overwhelmed", 0.85),
];

impl AffectLexicon {
    pub fn new() -> Self {
        Self { words: AFFECT_WORDS }
    }
}

impl Default for AffectLexicon {
    fn default() -> Self {
        Self::new()
    }
}

impl SecondSource for AffectLexicon {
    fn detect(&self, turn: &ConversationTurn) -> HashMap<String, Signal> {
        let normalized = turn.content.to_ascii_lowercase();
        let mut max_intensity = 0.0_f32;
        for (word, intensity) in self.words {
            if normalized.contains(word) {
                max_intensity = max_intensity.max(*intensity);
            }
        }
        let mut out = HashMap::new();
        if max_intensity > 0.0 {
            out.insert(
                "emotional_arousal".to_string(),
                Signal::new(max_intensity, 0.7, SignalReliability::Heuristic),
            );
        }
        out
    }
}

/// Strict first-person identity-claim detector. Emits
/// `identity_relevance` only when a deliberate identity phrasing
/// matches — "I am a X", "my profession", "I work as", etc. — to keep
/// false positives off the contour.
pub struct FirstPersonIdentityDetector {
    patterns: &'static [(&'static str, f32)],
}

const IDENTITY_PATTERNS: &[(&str, f32)] = &[
    ("my name is ", 0.95),
    ("i work as ", 0.9),
    ("i am a ", 0.85),
    ("i am an ", 0.85),
    ("i'm a ", 0.85),
    ("i'm an ", 0.85),
    ("i was a ", 0.7),
    ("i was an ", 0.7),
    ("i used to be ", 0.65),
    ("i work at ", 0.8),
    ("i work for ", 0.85),
    ("i work in ", 0.75),
    ("i specialize in ", 0.8),
    ("my profession ", 0.9),
    ("my career ", 0.85),
    ("my role ", 0.8),
    ("my job ", 0.8),
    ("my work ", 0.7),
    ("my mission ", 0.85),
    ("my background ", 0.75),
    ("i'm an only child", 0.95),
    ("i am an only child", 0.95),
];

impl FirstPersonIdentityDetector {
    pub fn new() -> Self {
        Self {
            patterns: IDENTITY_PATTERNS,
        }
    }
}

impl Default for FirstPersonIdentityDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SecondSource for FirstPersonIdentityDetector {
    fn detect(&self, turn: &ConversationTurn) -> HashMap<String, Signal> {
        let normalized = format!(" {} ", turn.content.to_ascii_lowercase().replace("i'm", "i'm"));
        let mut max_intensity = 0.0_f32;
        for (pattern, intensity) in self.patterns {
            if normalized.contains(pattern) {
                max_intensity = max_intensity.max(*intensity);
            }
        }
        let mut out = HashMap::new();
        if max_intensity > 0.0 {
            out.insert(
                "identity_relevance".to_string(),
                Signal::new(max_intensity, 0.7, SignalReliability::Heuristic),
            );
        }
        out
    }
}

/// Frozen goal/pressure phrase lexicon. Emits `goal_pressure` when any
/// goal-state phrasing appears in the turn content.
pub struct GoalPhraseLexicon {
    phrases: &'static [(&'static str, f32)],
}

const GOAL_PHRASES: &[(&str, f32)] = &[
    ("by tomorrow", 0.9),
    ("by friday", 0.85),
    ("by monday", 0.85),
    ("deadline", 0.9),
    ("urgent", 0.9),
    ("asap", 0.9),
    ("immediately", 0.85),
    ("hurry", 0.85),
    ("rush", 0.85),
    ("i need to ", 0.85),
    ("i have to ", 0.8),
    ("i must ", 0.85),
    ("i'm trying to ", 0.8),
    ("trying to ", 0.7),
    ("right now", 0.8),
    ("goal is ", 0.85),
    ("aim is ", 0.8),
    ("plan to ", 0.7),
    ("supposed to ", 0.7),
    ("required ", 0.75),
    ("requires ", 0.7),
    ("pressure ", 0.7),
    ("under pressure", 0.85),
    ("running out of time", 0.9),
];

impl GoalPhraseLexicon {
    pub fn new() -> Self {
        Self {
            phrases: GOAL_PHRASES,
        }
    }
}

impl Default for GoalPhraseLexicon {
    fn default() -> Self {
        Self::new()
    }
}

impl SecondSource for GoalPhraseLexicon {
    fn detect(&self, turn: &ConversationTurn) -> HashMap<String, Signal> {
        let normalized = turn.content.to_ascii_lowercase();
        let mut max_intensity = 0.0_f32;
        for (phrase, intensity) in self.phrases {
            if normalized.contains(phrase) {
                max_intensity = max_intensity.max(*intensity);
            }
        }
        let mut out = HashMap::new();
        if max_intensity > 0.0 {
            out.insert(
                "goal_pressure".to_string(),
                Signal::new(max_intensity, 0.7, SignalReliability::Heuristic),
            );
        }
        out
    }
}

/// The four canonical detectors plumbed for the v0.1 proof program.
/// Pair with [`crate::LunaExtractor::with_default_v1_sources`] to get
/// a fully wired extractor that satisfies the two-source rule for
/// every contour dimension that has plumbing.
pub fn default_v1_sources() -> Vec<Box<dyn SecondSource>> {
    vec![
        Box::new(TemporalDetector::new()),
        Box::new(AffectLexicon::new()),
        Box::new(FirstPersonIdentityDetector::new()),
        Box::new(GoalPhraseLexicon::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_observation::ALLOWED_DIMENSIONS;
    use luna_core::ConversationTurn;

    fn user(content: &str) -> ConversationTurn {
        ConversationTurn::user(content)
    }

    #[test]
    fn temporal_detector_fires_on_yesterday() {
        let signals = TemporalDetector::new().detect(&user("Yesterday I went to the office."));
        let signal = signals.get("temporal_relevance").unwrap();
        assert!(signal.value() >= 0.8);
        assert_eq!(signal.reliability(), SignalReliability::Heuristic);
    }

    #[test]
    fn temporal_detector_silent_without_cue() {
        let signals = TemporalDetector::new().detect(&user("The sky is blue."));
        assert!(signals.is_empty());
    }

    #[test]
    fn temporal_detector_does_not_read_timestamp() {
        let mut turn = user("The sky is blue.");
        turn.timestamp =
            Some(chrono::Utc::now());
        let signals = TemporalDetector::new().detect(&turn);
        assert!(
            signals.is_empty(),
            "temporal detector must not produce signal from timestamp alone"
        );
    }

    #[test]
    fn affect_lexicon_fires_on_terrified() {
        let signals = AffectLexicon::new().detect(&user("I was terrified."));
        let signal = signals.get("emotional_arousal").unwrap();
        assert!(signal.value() >= 0.9);
    }

    #[test]
    fn affect_lexicon_silent_on_neutral_content() {
        let signals = AffectLexicon::new().detect(&user("The meeting is at 3pm."));
        assert!(signals.is_empty());
    }

    #[test]
    fn first_person_identity_fires_on_i_am_a() {
        let signals =
            FirstPersonIdentityDetector::new().detect(&user("I am a mechanical engineer."));
        let signal = signals.get("identity_relevance").unwrap();
        assert!(signal.value() >= 0.8);
    }

    #[test]
    fn first_person_identity_silent_on_bare_i_am() {
        // "I am tired" should NOT fire the strict identity detector;
        // tiredness is affect, not identity. Pattern requires "I am a/an X".
        let signals = FirstPersonIdentityDetector::new().detect(&user("I am tired."));
        assert!(
            signals.is_empty(),
            "strict identity detector must not match 'I am tired'"
        );
    }

    #[test]
    fn goal_phrase_lexicon_fires_on_deadline() {
        let signals = GoalPhraseLexicon::new().detect(&user("There's a deadline tomorrow."));
        let signal = signals.get("goal_pressure").unwrap();
        assert!(signal.value() >= 0.8);
    }

    #[test]
    fn goal_phrase_lexicon_fires_on_i_need_to() {
        let signals = GoalPhraseLexicon::new().detect(&user("I need to finish the report."));
        assert!(signals.get("goal_pressure").is_some());
    }

    #[test]
    fn detectors_only_emit_listed_dimensions() {
        let detectors = default_v1_sources();
        let turn = user("Yesterday I was terrified, I am a mechanical engineer, deadline tomorrow.");
        for detector in &detectors {
            for (dim, _) in detector.detect(&turn) {
                assert!(
                    ALLOWED_DIMENSIONS.contains(&dim.as_str()),
                    "detector emitted dimension '{dim}' not in allowlist"
                );
            }
        }
    }

    #[test]
    fn default_v1_sources_returns_four_detectors() {
        assert_eq!(default_v1_sources().len(), 4);
    }
}
