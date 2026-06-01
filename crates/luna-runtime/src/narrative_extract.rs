//! Narrative (third-person) extraction — structural story-prose heuristics.
//!
//! This is Luna-original code. It is NOT a port of AURA: AURA's disclosure
//! extractor is first-person only. This module captures third-person narrative
//! statements ("Jax is human", "Viserys is T'Sari's father", "Jax, also known
//! as Jackson Renn") and emits `manuscript` assertions so they flow through
//! Luna's existing character-entity derivation and entity-group recall.
//!
//! Coverage is deliberately high-precision over high-recall: leading proper-noun
//! subjects with a linking verb ("X is/are/was Y"), and aliases ("X also known
//! as Y" / "X is called Y"). Case is preserved so entity names survive
//! ("Jax", "T'Sari", "Crimson Fold"). Like the disclosure module, this is
//! composed into extraction only behind a runtime flag.

use luna_core::StructuredAssertion;

/// Extract third-person narrative assertions (manuscript domain) from prose.
pub fn narrative_assertions(input: &str) -> Vec<StructuredAssertion> {
    let mut out = Vec::new();
    for sentence in split_sentences(input) {
        let sentence = sentence.trim();
        if sentence.len() < 5 || is_question(sentence) {
            continue;
        }

        // Alias: "<Name> also known as <Alias>", "<Name> is called <Alias>".
        if let Some((name, alias)) = capture_alias(sentence) {
            push(&mut out, "character_alias", &format!("{name} is called {alias}"));
        }

        // Subject linking-verb fact: "<Proper Subject> is/are/was <predicate>".
        if let Some((subject, predicate)) = capture_subject_fact(sentence) {
            push(
                &mut out,
                "character_identity",
                &format!("{subject} is {predicate}"),
            );
        }
    }
    dedupe(out)
}

fn push(out: &mut Vec<StructuredAssertion>, kind: &str, value: &str) {
    if out.len() >= 32 || value.trim().is_empty() {
        return;
    }
    out.push(StructuredAssertion::inferred("manuscript", kind, value.trim()));
}

fn dedupe(items: Vec<StructuredAssertion>) -> Vec<StructuredAssertion> {
    let mut seen = std::collections::BTreeSet::new();
    items
        .into_iter()
        .filter(|a| seen.insert(format!("{}|{}|{}", a.domain, a.kind, a.value)))
        .collect()
}

fn split_sentences(text: &str) -> Vec<&str> {
    text.split(['.', '!', '?', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_question(sentence: &str) -> bool {
    let lower = sentence.trim().to_ascii_lowercase();
    lower.starts_with("what ")
        || lower.starts_with("who ")
        || lower.starts_with("where ")
        || lower.starts_with("when ")
        || lower.starts_with("why ")
        || lower.starts_with("how ")
        || lower.starts_with("does ")
        || lower.starts_with("which ")
}

/// True when a word begins with an uppercase letter (proper-noun candidate).
/// Tolerates internal apostrophes/accents ("T'Sari", "Enû").
fn is_capitalized(word: &str) -> bool {
    word.chars()
        .next()
        .is_some_and(|c| c.is_uppercase())
}

/// Sentence-leading capitalized words that are not entity names.
fn is_subject_stopword(word: &str) -> bool {
    matches!(
        word,
        "He" | "She" | "It" | "They" | "His" | "Her" | "Their" | "This" | "That"
            | "These" | "Those" | "When" | "After" | "Before" | "Then" | "Medical"
            | "Story" | "Each" | "Every" | "All" | "Both" | "We" | "I" | "You"
            | "There" | "Here" | "What" | "Who"
    )
}

/// Consume a leading proper-noun subject: an optional "The/A/An" article followed
/// by one or more capitalized tokens. Returns (subject, index of first token
/// after the subject) where the token list is `words`.
fn leading_subject(words: &[&str]) -> Option<(String, usize)> {
    if words.is_empty() {
        return None;
    }
    let mut start = 0;
    let mut article = "";
    if matches!(words[0], "The" | "A" | "An") {
        // Article only counts if the next token is a capitalized entity word.
        if words.len() < 2 || !is_capitalized(words[1]) {
            return None;
        }
        article = words[0];
        start = 1;
    }
    if !is_capitalized(words[start]) || is_subject_stopword(words[start]) {
        return None;
    }
    let mut end = start;
    while end < words.len() && is_capitalized(words[end]) && words[end] != "is" {
        end += 1;
        if end - start >= 4 {
            break; // cap subject length
        }
    }
    let name = words[start..end].join(" ");
    let subject = if article.is_empty() {
        name
    } else {
        format!("{article} {name}")
    };
    Some((subject, end))
}

/// "<Proper Subject> is/are/was <predicate>" -> (subject, short predicate).
fn capture_subject_fact(sentence: &str) -> Option<(String, String)> {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    let (subject, idx) = leading_subject(&words)?;
    let verb = words.get(idx)?;
    if !matches!(*verb, "is" | "are" | "was") {
        return None;
    }
    let predicate_words = &words[idx + 1..];
    if predicate_words.is_empty() {
        return None;
    }
    let predicate_full = predicate_words.join(" ");
    // Keep the first clause (up to a comma) and cap to ~12 words for a clean,
    // recallable fact.
    let predicate = short_predicate(&predicate_full);
    if predicate.len() < 2 {
        return None;
    }
    Some((subject, predicate))
}

fn short_predicate(full: &str) -> String {
    let first_clause = full.split(',').next().unwrap_or(full).trim();
    let clipped: Vec<&str> = first_clause.split_whitespace().take(12).collect();
    clipped
        .join(" ")
        .trim_end_matches(['.', ',', ';', ':'])
        .to_string()
}

/// "<Name> [is] [also] known as <Alias>" or "<Name> is called <Alias>".
fn capture_alias(sentence: &str) -> Option<(String, String)> {
    let markers = [
        " also known as ",
        " is also known as ",
        " known as ",
        " is called ",
        " also called ",
    ];
    let lower = sentence.to_ascii_lowercase();
    let (marker_pos, marker_len) = markers
        .iter()
        .filter_map(|m| lower.find(m).map(|p| (p, m.len())))
        .min_by_key(|(p, _)| *p)?;

    let before = &sentence[..marker_pos];
    let after = &sentence[marker_pos + marker_len..];

    let name = trailing_proper_phrase(before)?;
    let alias = leading_proper_phrase(after)?;
    if name.eq_ignore_ascii_case(&alias) {
        return None;
    }
    Some((name, alias))
}

/// Trailing run of capitalized words at the end of `text` ("...the pod is Jax," -> "Jax").
/// Punctuation on each token is trimmed before the capitalization check.
fn clean_token(w: &str) -> &str {
    w.trim_matches(|c: char| !c.is_alphanumeric())
}

fn trailing_proper_phrase(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let end = words.len();
    let mut start = end;
    while start > 0 {
        let w = clean_token(words[start - 1]);
        if w.is_empty() || !is_capitalized(w) || is_subject_stopword(w) {
            break;
        }
        start -= 1;
        if end - start >= 3 {
            break;
        }
    }
    if start == end {
        return None;
    }
    let phrase = words[start..end]
        .iter()
        .map(|w| clean_token(w))
        .collect::<Vec<_>>()
        .join(" ");
    (phrase.len() >= 2).then_some(phrase)
}

/// Leading run of capitalized words at the start of `text` ("Jackson Renn." -> "Jackson Renn").
fn leading_proper_phrase(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut end = 0;
    while end < words.len()
        && is_capitalized(words[end].trim_end_matches(|c: char| !c.is_alphanumeric()))
    {
        end += 1;
        if end >= 3 {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let phrase = words[..end]
        .join(" ")
        .trim_end_matches(|c: char| !c.is_alphanumeric())
        .to_string();
    (phrase.len() >= 2).then_some(phrase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(items: &[StructuredAssertion], kind: &str, contains: &str) -> bool {
        items
            .iter()
            .any(|a| a.domain == "manuscript" && a.kind == kind && a.value.contains(contains))
    }

    #[test]
    fn captures_simple_subject_fact() {
        let a = narrative_assertions("Jax is human.");
        assert!(has(&a, "character_identity", "Jax is human"));
    }

    #[test]
    fn captures_the_entity_fact() {
        let a = narrative_assertions("The Crimson Fold is forbidden space, not merely restricted space.");
        assert!(has(&a, "character_identity", "The Crimson Fold is forbidden space"));
    }

    #[test]
    fn captures_possessive_relation_as_fact() {
        let a = narrative_assertions("Primarch Viserys is T'Sari's father, but she addresses him by title.");
        assert!(has(&a, "character_identity", "T'Sari's father"));
    }

    #[test]
    fn captures_alias() {
        let a = narrative_assertions("The being inside the pod is Jax, also known as Jackson Renn.");
        assert!(has(&a, "character_alias", "Jax is called Jackson Renn"));
    }

    #[test]
    fn ignores_pronoun_and_lowercase_subjects() {
        assert!(narrative_assertions("His body is reinforced.").is_empty());
        assert!(narrative_assertions("A tether is load-sharing.").is_empty());
    }

    #[test]
    fn ignores_questions() {
        assert!(narrative_assertions("What species is Jax?").is_empty());
    }
}
