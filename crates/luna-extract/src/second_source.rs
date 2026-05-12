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
        Self {
            cues: TEMPORAL_CUES,
        }
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
    ("freaking out", 0.95),
    ("burned out", 0.85),
    ("hated", 0.85),
    ("hate", 0.8),
    ("anxious", 0.8),
    ("angry", 0.8),
    ("urgent", 0.85),
    ("stressed", 0.8),
    ("stressful", 0.75),
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
        Self {
            words: AFFECT_WORDS,
        }
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
    // Declarative identity (PR 0.4 baseline).
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
    // Aspirational identity — covers career_goal cases that describe
    // who the speaker is becoming rather than who they are. Phrase-
    // shaped, not bare. Bare "i want to be" would fire on "I want to
    // be heard" / "I want to be done"; the "a/an" gating preserves
    // the identity-claim intent without those false positives.
    ("i want to be a ", 0.8),
    ("i want to be an ", 0.8),
    ("i want to become ", 0.8),
    ("i'm trying to become ", 0.8),
    ("i am trying to become ", 0.8),
    ("i'm trying to move into ", 0.75),
    ("i am trying to move into ", 0.75),
    // Bare "becoming" is too broad ("the leaves are becoming yellow"),
    // but "about becoming X" requires the verb-noun pair and rarely
    // fires outside a first-person aspirational claim like "I care
    // about becoming a better mentor" or "I think about becoming a
    // teacher." The qualifier-tolerant form catches cases like "I
    // care a lot about becoming X" where the literal
    // "care about becoming" substring is broken by an intervening
    // adverb. Two-source rule means the LLM must agree before this
    // lifts a contour dimension; third-person prose like "stories
    // about becoming heroes" never gets identity_relevance from the
    // LLM and so this detector firing alone has no effect.
    ("about becoming ", 0.7),
    ("i'm shifting from ", 0.75),
    ("i am shifting from ", 0.75),
    // Paraphrastic identity — covers profession_paraphrase cases that
    // avoid the canonical "I am a/an X" phrasing on purpose.
    ("i make a living as ", 0.85),
    ("professionally, i ", 0.8),
    ("what i do professionally", 0.85),
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
        let normalized = format!(
            " {} ",
            turn.content.to_ascii_lowercase().replace("i’m", "i'm")
        );
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
    // Explicit deadline / urgency phrasings (PR 0.4 baseline).
    ("by tomorrow", 0.9),
    ("by friday", 0.85),
    ("by monday", 0.85),
    ("deadline", 0.9),
    ("urgent", 0.9),
    ("asap", 0.9),
    ("immediately", 0.85),
    ("hurry", 0.85),
    ("rush", 0.85),
    // First-person obligation / intention.
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
    // Stress-as-pressure (PR 0.7). Cross-listed with the affect
    // lexicon for phrases that signal both an emotional state and an
    // active burden / pressure / proving-yourself dynamic. NOT every
    // negative emotion — only language that implies burden, deadline,
    // pressure, or proving.
    ("stressing me", 0.7),
    ("stressful thing", 0.7),
    ("wearing me down", 0.7),
    ("struggling with", 0.7),
    ("had me tense", 0.7),
    ("current pressure", 0.85),
    ("need to prove", 0.9),
    ("trying to prove", 0.85),
    ("prove myself", 0.85),
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
        turn.timestamp = Some(chrono::Utc::now());
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
        let turn =
            user("Yesterday I was terrified, I am a mechanical engineer, deadline tomorrow.");
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

    // PR 0.7 vocabulary expansion — tests for each failing-case phrase
    // surfaced by the first real Stage 0 formation run. Each
    // assertion is a regression guard: if a future detector edit
    // narrows the lexicon, these break.

    #[test]
    fn identity_fires_on_im_trying_to_move_into() {
        let signals =
            FirstPersonIdentityDetector::new().detect(&user("I'm trying to move into management."));
        assert!(signals.get("identity_relevance").is_some());
    }

    #[test]
    fn identity_fires_on_i_want_to_become() {
        let signals = FirstPersonIdentityDetector::new()
            .detect(&user("I want to become a team lead this year."));
        assert!(signals.get("identity_relevance").is_some());
    }

    #[test]
    fn identity_fires_on_i_care_about_becoming() {
        let signals = FirstPersonIdentityDetector::new()
            .detect(&user("I care a lot about becoming a better mentor."));
        assert!(signals.get("identity_relevance").is_some());
    }

    #[test]
    fn identity_fires_on_im_shifting_from() {
        let signals = FirstPersonIdentityDetector::new().detect(&user(
            "I'm shifting from hands-on engineering toward strategy work.",
        ));
        assert!(signals.get("identity_relevance").is_some());
    }

    #[test]
    fn identity_fires_on_i_make_a_living_as() {
        let signals = FirstPersonIdentityDetector::new()
            .detect(&user("I make a living as a mechanical engineer."));
        assert!(signals.get("identity_relevance").is_some());
    }

    #[test]
    fn identity_fires_on_professionally_i() {
        let signals = FirstPersonIdentityDetector::new()
            .detect(&user("Professionally, I do mechanical engineering."));
        assert!(signals.get("identity_relevance").is_some());
    }

    #[test]
    fn identity_fires_on_what_i_do_professionally() {
        let signals = FirstPersonIdentityDetector::new().detect(&user(
            "Engineering is what I do professionally, specifically mechanical engineering.",
        ));
        assert!(signals.get("identity_relevance").is_some());
    }

    #[test]
    fn identity_fires_on_i_want_to_be_a_or_an_role() {
        let detector = FirstPersonIdentityDetector::new();
        assert!(detector
            .detect(&user("I want to be a doctor someday."))
            .get("identity_relevance")
            .is_some());
        assert!(detector
            .detect(&user("I want to be an architect eventually."))
            .get("identity_relevance")
            .is_some());
    }

    #[test]
    fn identity_silent_on_bare_i_want_to_be() {
        // Locks the tightening: bare "i want to be" is too broad; we
        // only fire when followed by "a/an + role". Otherwise "I want
        // to be heard" / "I want to be done" would falsely fire
        // identity_relevance.
        let detector = FirstPersonIdentityDetector::new();
        assert!(detector
            .detect(&user("I want to be heard at the meeting."))
            .get("identity_relevance")
            .is_none());
        assert!(detector
            .detect(&user("I want to be done with this project."))
            .get("identity_relevance")
            .is_none());
    }

    #[test]
    fn goal_fires_on_struggling_with() {
        let signals =
            GoalPhraseLexicon::new().detect(&user("Last week I was struggling with my job."));
        assert!(signals.get("goal_pressure").is_some());
    }

    #[test]
    fn goal_fires_on_wearing_me_down() {
        let signals = GoalPhraseLexicon::new()
            .detect(&user("Today the budget review is what's wearing me down."));
        assert!(signals.get("goal_pressure").is_some());
    }

    #[test]
    fn goal_fires_on_stressful_thing() {
        let signals = GoalPhraseLexicon::new().detect(&user(
            "This week the product launch is the stressful thing.",
        ));
        assert!(signals.get("goal_pressure").is_some());
    }

    #[test]
    fn goal_fires_on_stressing_me() {
        let signals = GoalPhraseLexicon::new().detect(&user("Recently, what was stressing me?"));
        assert!(signals.get("goal_pressure").is_some());
    }

    #[test]
    fn goal_fires_on_had_me_tense() {
        let signals = GoalPhraseLexicon::new()
            .detect(&user("This morning the client deadline had me tense."));
        assert!(signals.get("goal_pressure").is_some());
    }

    #[test]
    fn goal_fires_on_prove_myself() {
        let signals =
            GoalPhraseLexicon::new().detect(&user("I need to prove myself this quarter."));
        assert!(signals.get("goal_pressure").is_some());
    }

    #[test]
    fn goal_fires_on_need_to_prove_and_trying_to_prove() {
        let detector = GoalPhraseLexicon::new();
        assert!(detector
            .detect(&user("I need to prove that the architecture works."))
            .get("goal_pressure")
            .is_some());
        assert!(detector
            .detect(&user("I'm trying to prove the hypothesis."))
            .get("goal_pressure")
            .is_some());
    }

    #[test]
    fn affect_fires_on_stressful() {
        let signals = AffectLexicon::new().detect(&user(
            "This week the product launch is the stressful thing.",
        ));
        assert!(signals.get("emotional_arousal").is_some());
    }

    #[test]
    fn affect_fires_on_freaking_out() {
        let signals = AffectLexicon::new().detect(&user("I was freaking out before the demo."));
        assert!(signals.get("emotional_arousal").is_some());
    }

    #[test]
    fn affect_and_goal_both_fire_on_wearing_me_down_when_cross_listed() {
        // Documents the deliberate cross-listing of "wearing me down"
        // in PR 0.7. Both detectors fire because the phrase carries
        // both affect (emotional state) and burden (pressure).
        // Downstream fusion gates each dimension on its own
        // two-source rule, so this doesn't bypass anything.
        let turn = user("The budget review is wearing me down today.");
        assert!(AffectLexicon::new()
            .detect(&turn)
            .get("emotional_arousal")
            .is_some());
        assert!(GoalPhraseLexicon::new()
            .detect(&turn)
            .get("goal_pressure")
            .is_some());
    }
}
