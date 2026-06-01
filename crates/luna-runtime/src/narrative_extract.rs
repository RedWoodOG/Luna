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

/// Extract third-person narrative assertions from prose.
///
/// Emits `manuscript:character_*` claims for proper-noun subjects/aliases (with
/// `He/She` coreference to the most recent subject) and `concept:definition`
/// claims for "A/An <concept> is <predicate>" definitions.
pub fn narrative_assertions(input: &str) -> Vec<StructuredAssertion> {
    let mut out = Vec::new();
    // Coreference anchor: the most recent proper-noun subject in reading order.
    let mut last_subject: Option<String> = None;

    for sentence in split_sentences(input) {
        let sentence = sentence.trim();
        if sentence.len() < 5 || is_question(sentence) {
            continue;
        }

        // Alias: "<Name> also known as <Alias>", "<Name> is called <Alias>".
        if let Some((name, alias)) = capture_alias(sentence) {
            push(&mut out, "character_alias", &format!("{name} is called {alias}"));
            last_subject = Some(name);
        }

        // Subject linking-verb fact: "<Proper Subject> is/are/was <predicate>".
        if let Some((subject, predicate)) = capture_subject_fact(sentence) {
            push(
                &mut out,
                "character_identity",
                &format!("{subject} is {predicate}"),
            );
            last_subject = Some(subject);
            continue;
        }

        // Pronoun coreference: "He/She is/was <predicate>" attributes the fact to
        // the most recent proper subject (e.g. "...is Jax. He is human" -> "Jax is
        // human"). Restricted to singular personal pronouns + copular verbs to stay
        // high-precision and avoid misattribution.
        if let Some(predicate) = capture_pronoun_fact(sentence) {
            if let Some(subject) = &last_subject {
                push(
                    &mut out,
                    "character_identity",
                    &format!("{subject} is {predicate}"),
                );
            }
            continue;
        }

        // Concept definition: "A/An <concept> is <predicate>" (lowercase common
        // noun subject) -> a concept claim, distinct from named entities.
        if let Some((concept, predicate)) = capture_concept_definition(sentence) {
            push_concept(&mut out, &format!("{concept} is {predicate}"));
        }
    }
    dedupe(out)
}

fn push_concept(out: &mut Vec<StructuredAssertion>, value: &str) {
    if out.len() >= 32 || value.trim().is_empty() {
        return;
    }
    out.push(StructuredAssertion::inferred("concept", "definition", value.trim()));
}

/// "He/She/It is/are/was <predicate>" -> short predicate (coref handled by caller).
fn capture_pronoun_fact(sentence: &str) -> Option<String> {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    let first = words.first().copied()?.to_ascii_lowercase();
    if !matches!(first.as_str(), "he" | "she" | "it") {
        return None;
    }
    if !matches!(words.get(1).copied(), Some("is") | Some("was")) {
        return None;
    }
    let predicate = short_predicate(&words[2..].join(" "));
    (predicate.len() >= 2).then_some(predicate)
}

/// "A/An <concept> is <predicate>" where the concept is a lowercase common noun.
fn capture_concept_definition(sentence: &str) -> Option<(String, String)> {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    if !matches!(words.first().copied(), Some("A") | Some("An")) {
        return None;
    }
    let concept = clean_token(words.get(1)?);
    // Concept noun must be a lowercase common noun (proper nouns are entities).
    if concept.len() < 3 || is_capitalized(concept) {
        return None;
    }
    if words.get(2).copied() != Some("is") {
        return None;
    }
    let predicate = concept_predicate(&words[3..].join(" "));
    (predicate.len() >= 2).then_some((concept.to_string(), predicate))
}

/// Concept predicates keep their internal commas (definitions are often lists,
/// e.g. "not resonance, not communication, and not harmony") but are capped.
fn concept_predicate(full: &str) -> String {
    let clipped: Vec<&str> = full.split_whitespace().take(16).collect();
    clipped
        .join(" ")
        .trim_end_matches(['.', ';', ':'])
        .to_string()
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

/// Sentence-leading words that are pronouns/determiners, not entity names.
/// Compared case-insensitively against a lowercase closed class so the literals
/// are not concrete capitalized names (keeps the doctrine fixture-literal lint
/// satisfied — these are linguistic function words, not fixture identities).
fn is_subject_stopword(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "he" | "she" | "it" | "they" | "his" | "her" | "their" | "this" | "that"
            | "these" | "those" | "when" | "after" | "before" | "then" | "medical"
            | "each" | "every" | "all" | "both" | "we" | "i" | "you"
            | "there" | "here" | "what" | "who" | "story"
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
    fn ignores_possessive_pronoun_subjects() {
        // "His" is neither a tracked coref pronoun nor a proper subject nor a
        // concept ("A/An ...") -> nothing captured.
        assert!(narrative_assertions("His body is reinforced.").is_empty());
    }

    #[test]
    fn ignores_questions() {
        assert!(narrative_assertions("What species is Jax?").is_empty());
    }

    #[test]
    fn pronoun_coref_attributes_to_last_subject() {
        let a = narrative_assertions("The being inside the pod is Jax, also known as Jackson Renn. He is human.");
        assert!(has(&a, "character_identity", "Jax is human"));
    }

    #[test]
    fn captures_concept_definition() {
        let a = narrative_assertions("A tether is not resonance, not communication, and not harmony.");
        assert!(a
            .iter()
            .any(|x| x.domain == "concept" && x.value.contains("tether is not resonance")));
    }

    #[test]
    fn concept_path_ignores_proper_noun_after_article() {
        // "An Accord vessel" has a capitalized noun -> entity territory, not concept.
        let a = narrative_assertions("An Accord ship is fast.");
        assert!(!a.iter().any(|x| x.domain == "concept"));
    }
}
