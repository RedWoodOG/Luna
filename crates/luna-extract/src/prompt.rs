//! Frozen extraction prompt embedded at compile time and the SHA-256
//! digest of its bytes.
//!
//! The prompt template is `include_str!`-ed so the binary always runs
//! against the bytes that were hashed at build time. A prompt edit
//! triggers a recompile, recomputes [`prompt_v2_hash`], invalidates
//! every cache entry that referenced the old hash, and forces re-
//! extraction. There is no runtime read of the on-disk prompt file.
//!
//! The hash is materialized once into a `OnceLock<String>` so callers
//! can take it by `&str` without re-hashing per call.
//!
//! ## Versioning
//!
//! The active prompt is whichever one this module imports via
//! `include_str!`. PR 0.9 promoted `extract_v2.md` (with the
//! assertion-value rule that fixed the v1 "category-label
//! substitution" failure mode) to active; v1 is preserved in git
//! history. Bumping to v3 means: write `extract_v3.md`, switch the
//! `include_str!` target, rename functions, accept the cache
//! invalidation that follows from the prompt-hash change.

use chrono::SecondsFormat;
use luna_core::{ConversationTurn, Role};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::sync::OnceLock;

const PROMPT_V2_BYTES: &str = include_str!("../prompts/extract_v2.md");

/// SHA-256 hex of the embedded prompt template. Used in
/// [`crate::CacheKey`] derivation so a prompt change invalidates
/// previously-cached extractions.
pub fn prompt_v2_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        let mut hasher = Sha256::new();
        hasher.update(PROMPT_V2_BYTES.as_bytes());
        let bytes = hasher.finalize();
        let mut out = String::with_capacity(64);
        for byte in bytes {
            write!(&mut out, "{:02x}", byte).expect("write to String");
        }
        out
    })
}

/// Returns the embedded prompt with the turn's `{{ROLE}}`,
/// `{{TIMESTAMP}}`, and `{{CONTENT}}` placeholders substituted in.
///
/// The template hash does NOT depend on the substituted values — it
/// only depends on the embedded bytes. The cache key already covers
/// turn content and timestamp separately.
pub fn build_prompt_v2(turn: &ConversationTurn) -> String {
    let role = match turn.role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
    };
    let timestamp = match turn.timestamp {
        Some(ts) => ts.to_rfc3339_opts(SecondsFormat::Secs, true),
        None => "unknown".to_string(),
    };
    PROMPT_V2_BYTES
        .replace("{{ROLE}}", role)
        .replace("{{TIMESTAMP}}", &timestamp)
        .replace("{{CONTENT}}", &turn.content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn prompt_v2_hash_is_64_lowercase_hex_chars() {
        let hash = prompt_v2_hash();
        assert_eq!(hash.len(), 64);
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn prompt_v2_hash_is_stable_across_calls() {
        assert_eq!(prompt_v2_hash(), prompt_v2_hash());
    }

    #[test]
    fn build_prompt_substitutes_role_timestamp_content() {
        let turn = ConversationTurn {
            role: Role::User,
            content: "I work as a mechanical engineer.".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2026, 5, 3, 10, 0, 0).unwrap()),
        };
        let rendered = build_prompt_v2(&turn);
        assert!(rendered.contains("Role: user"));
        assert!(rendered.contains("2026-05-03T10:00:00Z"));
        assert!(rendered.contains("I work as a mechanical engineer."));
        assert!(!rendered.contains("{{ROLE}}"));
        assert!(!rendered.contains("{{TIMESTAMP}}"));
        assert!(!rendered.contains("{{CONTENT}}"));
    }

    #[test]
    fn build_prompt_renders_unknown_for_absent_timestamp() {
        let turn = ConversationTurn::user("hello");
        let rendered = build_prompt_v2(&turn);
        assert!(rendered.contains("Timestamp: unknown"));
    }

    #[test]
    fn prompt_v2_contains_assertion_value_rule() {
        // Lock that v2's distinguishing instruction is present in the
        // embedded bytes. If a future edit accidentally drops the
        // rule, this catches it.
        assert!(PROMPT_V2_BYTES.contains("Assertion value rule"));
        assert!(PROMPT_V2_BYTES.contains("preserve that phrase"));
    }
}
