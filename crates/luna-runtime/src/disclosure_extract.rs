//! Disclosure extraction — structural first-person fact heuristics.
//!
//! PROVENANCE: ported and rewritten from AURA
//! (`AuraCoreTCFGenesis/ANEWAuraAttemptTCF/crates/aura-input/src/assertions.rs`,
//! the `structured_assertions_from_input` / `universal_disclosure_extract`
//! pipeline). Re-implemented in Luna conventions: emits typed
//! [`StructuredAssertion`]s (not AURA's flat `domain:sub=value` strings), uses
//! Luna `domain`/`kind`/`value` naming, and carries no `aura-*` crate
//! dependency.
//!
//! Scope of this import: the pure **structural** extractor only. AURA's learned
//! term-domain table and shape-store routing tiers are intentionally NOT ported
//! here — only Tier 1 (syntactic structure) is included, which is deterministic
//! and dependency-free. Coverage is **first-person disclosure** ("I am X", "I
//! live in Y", "my name is Z"); third-person narrative generalisation is a
//! separate, later extension.
//!
//! This module is self-contained and is not yet wired into the default
//! extraction path; it is imported, tested, and ready to be composed into
//! `entity_sieve_assertions` deliberately.

use luna_core::StructuredAssertion;

/// 7D field axes (AURA TCF order): attention, meaning, goal, trust, skill,
/// context, identity. Used only to route a sentence's structural geometry to a
/// domain via cosine similarity to canonical domain vectors.
const CANONICALS: &[(&str, &str, [f32; 7])] = &[
    ("activity", "practice", [0.80, 0.38, 0.48, 0.42, 0.45, 0.48, 0.42]),
    ("constraint", "requirement", [0.80, 0.40, 0.35, 0.35, 0.35, 0.40, 0.35]),
    ("preference", "taste", [0.42, 0.80, 0.42, 0.48, 0.42, 0.42, 0.48]),
    ("goal", "project", [0.48, 0.45, 0.80, 0.42, 0.48, 0.45, 0.48]),
    ("date", "start", [0.38, 0.42, 0.48, 0.42, 0.45, 0.55, 0.42]),
    ("social_bond", "companion", [0.42, 0.40, 0.42, 0.80, 0.42, 0.45, 0.50]),
    ("emotional_state", "fear", [0.42, 0.55, 0.42, 0.80, 0.42, 0.42, 0.42]),
    ("skill", "profession", [0.42, 0.45, 0.48, 0.42, 0.80, 0.42, 0.48]),
    ("location", "residence", [0.45, 0.42, 0.42, 0.42, 0.42, 0.80, 0.45]),
    ("identity", "name", [0.35, 0.35, 0.38, 0.38, 0.38, 0.35, 0.80]),
    ("possession", "object", [0.42, 0.45, 0.42, 0.42, 0.42, 0.45, 0.75]),
];

/// Extract first-person disclosure assertions from free text.
///
/// Questions are ignored (Luna's recall path owns query handling). Each
/// supported disclosure sentence is routed to a domain by structural geometry
/// and yields one typed assertion per extracted value, plus third-person
/// numeric/technical facts.
pub fn disclosure_assertions(input: &str) -> Vec<StructuredAssertion> {
    let lower = expand_contractions(&bounded(&input.to_ascii_lowercase(), 1_000));
    let mut out = Vec::new();

    if is_question(&lower) {
        return out;
    }

    universal_disclosure_extract(&lower, &mut out);
    push_technical_fact_assertions(&lower, &mut out);

    dedupe(out)
}

fn dedupe(items: Vec<StructuredAssertion>) -> Vec<StructuredAssertion> {
    let mut seen = std::collections::BTreeSet::new();
    items
        .into_iter()
        .filter(|a| seen.insert(format!("{}|{}|{}", a.domain, a.kind, a.value)))
        .collect()
}

fn universal_disclosure_extract(input: &str, out: &mut Vec<StructuredAssertion>) {
    let lower = input.to_lowercase();
    for raw_sentence in lower.split(['.', '!', '?', ';']) {
        let mut sentence = raw_sentence.trim();
        if sentence.len() < 3 {
            continue;
        }

        // Strip leading discourse markers ("actually,", "well,", "hey luna,").
        let filler_prefixes = [
            "hey luna,", "hey luna ", "hi luna,", "hi luna ", "hello luna,", "hello luna ",
            "actually,", "actually ", "well,", "well ", "so,", "so ", "anyway,", "anyway ",
            "look,", "look ", "basically,", "basically ", "honestly,", "honestly ",
            "frankly,", "frankly ", "hi,", "hi ", "hey,", "hey ", "hello,", "hello ",
        ];
        loop {
            let mut stripped = false;
            for prefix in &filler_prefixes {
                if let Some(rest) = sentence.strip_prefix(prefix) {
                    sentence = rest.trim();
                    stripped = true;
                    break;
                }
            }
            if !stripped {
                break;
            }
        }
        let content_chars = sentence.chars().filter(|c| c.is_alphanumeric()).count();
        if content_chars < 3 || !is_supported_disclosure_sentence(sentence) {
            continue;
        }

        // "I have a dog named Reef" -> social_bond companion.
        if let Some((entity, value)) = extract_after_named_companion(sentence) {
            push_domain_value(out, "social_bond", "companion", &format!("{entity} {value}"));
            continue;
        }
        if is_pure_transition_sentence(sentence) {
            continue; // change event, value is in the following sentence
        }
        if push_structural_update_assertion(sentence, out) {
            continue;
        }

        let value_sentence = strip_transition_prefix(sentence);
        let values = extract_universal_values_from_sentence(value_sentence);
        if values.is_empty() {
            continue;
        }
        let (domain, sub) = determine_domain_for_sentence(sentence);
        for value in &values {
            let v_clean: String = value
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == ' ')
                .collect();
            if v_clean.trim().len() < 3 && !v_clean.chars().any(|c| c.is_uppercase()) {
                continue;
            }
            push_domain_value(out, &domain, &sub, value);
        }
    }
}

/// Tier 1 structural routing: recognise sentence shape, route to a domain.
fn determine_domain_for_sentence(sentence: &str) -> (String, String) {
    if let Some(geo) = sentence_structural_geometry(sentence) {
        let (d, s) = closest_domain(&geo);
        let s = sentence_sub_override(sentence, d, s);
        return (d.to_string(), s.to_string());
    }
    ("identity".to_string(), "name".to_string())
}

/// Cosine similarity of a sentence geometry to each canonical domain vector.
fn closest_domain(geo: &[f32; 7]) -> (&'static str, &'static str) {
    let mut best_sim = -2.0f32;
    let mut best = ("identity", "name");
    for (domain, sub, canonical) in CANONICALS {
        let mut dot = 0.0;
        let mut mag_a = 0.0;
        let mut mag_b = 0.0;
        for i in 0..7 {
            dot += geo[i] * canonical[i];
            mag_a += geo[i] * geo[i];
            mag_b += canonical[i] * canonical[i];
        }
        let denom = (mag_a * mag_b).sqrt();
        let sim = if denom < 1e-9 { 0.0 } else { dot / denom };
        if sim > best_sim {
            best_sim = sim;
            best = (domain, sub);
        }
    }
    best
}

fn sentence_sub_override<'a>(sentence: &str, domain: &str, default_sub: &'a str) -> &'a str {
    let lower = sentence.trim();
    match (domain, default_sub) {
        ("skill", _)
            if lower.starts_with("i use ")
                || lower.starts_with("i am writing ")
                || lower.starts_with("i am coding ") =>
        {
            "tool"
        }
        ("preference", _) if lower.starts_with("i eat ") || lower.starts_with("my favorite food ") => "food",
        ("preference", _) if lower.starts_with("i drink ") || lower.starts_with("my favorite drink ") => "drink",
        ("goal", _) if lower.starts_with("my goal is ") || lower.starts_with("my current goal is ") => "thesis",
        ("skill", _) if starts_with_profession_identity(lower) => "profession",
        ("skill", _)
            if lower.starts_with("i build ") || lower.starts_with("i design ") || lower.starts_with("i fabricate ") =>
        {
            "creative"
        }
        ("activity", _) if lower.contains("weekend") || lower.contains("saturday") || lower.contains("sunday") => "weekend",
        ("constraint", _) if lower.starts_with("i am allergic ") => "allergy",
        _ => default_sub,
    }
}

/// Universal English syntactic patterns -> 7D geometry. Structural, not
/// semantic: the patterns work for any content word.
fn sentence_structural_geometry(sentence: &str) -> Option<[f32; 7]> {
    let lower = sentence.trim();
    if lower.starts_with("my name is ") || lower.starts_with("my name's ") {
        return Some([0.35, 0.35, 0.38, 0.38, 0.38, 0.35, 0.80]);
    }
    if lower.contains(" work as ") || lower.contains(" works as ") {
        return Some([0.42, 0.45, 0.48, 0.42, 0.80, 0.42, 0.48]);
    }
    if starts_with_profession_identity(lower) {
        return Some([0.42, 0.45, 0.48, 0.42, 0.80, 0.42, 0.48]);
    }
    if lower.starts_with("i am a ") || lower.starts_with("i am an ") {
        return Some([0.42, 0.45, 0.48, 0.42, 0.80, 0.42, 0.48]);
    }
    if lower.starts_with("i build ") || lower.starts_with("i design ") || lower.starts_with("i fabricate ") {
        return Some([0.42, 0.45, 0.48, 0.42, 0.80, 0.42, 0.48]);
    }
    if lower.starts_with("i code ") || lower.starts_with("i develop ") {
        return Some([0.42, 0.45, 0.48, 0.42, 0.80, 0.42, 0.48]);
    }
    if lower.contains(" live in ")
        || lower.contains(" live at ")
        || lower.contains(" lives in ")
        || lower.contains(" lives at ")
        || lower.starts_with("i am from ")
        || lower.contains(" moved to ")
    {
        return Some([0.45, 0.42, 0.42, 0.42, 0.42, 0.80, 0.45]);
    }
    if ((lower.contains(" have a ") || lower.contains(" have an ")) && lower.contains(" named "))
        || lower.starts_with("my partner ")
        || lower.starts_with("my wife ")
        || lower.starts_with("my husband ")
        || lower.starts_with("my friend ")
        || lower.starts_with("my dog ")
        || lower.starts_with("my cat ")
    {
        return Some([0.42, 0.40, 0.42, 0.80, 0.42, 0.45, 0.50]);
    }
    if lower.starts_with("my goal is ") || lower.starts_with("my current goal is ") {
        return Some([0.48, 0.45, 0.80, 0.42, 0.48, 0.45, 0.48]);
    }
    if lower.starts_with("i like ")
        || lower.starts_with("i love ")
        || lower.starts_with("i enjoy ")
        || lower.starts_with("i prefer ")
        || lower.starts_with("my favorite ")
        || lower.starts_with("i eat ")
        || lower.starts_with("i drink ")
    {
        return Some([0.42, 0.80, 0.42, 0.48, 0.42, 0.42, 0.48]);
    }
    if (lower.starts_with("i am learning ")) && (lower.contains(" to ") || lower.contains(" how ")) {
        return Some([0.80, 0.38, 0.48, 0.42, 0.45, 0.48, 0.42]);
    }
    if lower.starts_with("i am working on ")
        || lower.starts_with("i am building ")
        || lower.starts_with("i am stuck on ")
    {
        return Some([0.48, 0.45, 0.80, 0.42, 0.48, 0.45, 0.48]);
    }
    if lower.starts_with("i am terrified ") || lower.starts_with("i am scared ") || lower.starts_with("i am afraid ") {
        return Some([0.42, 0.55, 0.42, 0.80, 0.42, 0.42, 0.42]);
    }
    if lower.starts_with("i need ") || lower.starts_with("i require ") || lower.starts_with("i am allergic ") {
        return Some([0.80, 0.42, 0.35, 0.35, 0.35, 0.40, 0.35]);
    }
    if lower.starts_with("i drive ") || lower.starts_with("i own ") {
        return Some([0.42, 0.45, 0.42, 0.42, 0.42, 0.45, 0.75]);
    }
    if lower.starts_with("i go ")
        || lower.starts_with("i spend ")
        || lower.contains(" on weekends")
        || lower.contains(" on saturdays")
    {
        return Some([0.80, 0.42, 0.48, 0.42, 0.45, 0.48, 0.42]);
    }
    // Identity catch-all: "i am X" without an article.
    if let Some(after) = lower.strip_prefix("i am ") {
        if !after.starts_with("a ") && !after.starts_with("an ") {
            return Some([0.35, 0.35, 0.38, 0.38, 0.38, 0.35, 0.80]);
        }
    }
    None
}

fn starts_with_profession_identity(lower: &str) -> bool {
    let value = lower
        .strip_prefix("i am a ")
        .or_else(|| lower.strip_prefix("i am an "));
    let Some(value) = value else { return false };
    contains_any(
        value,
        &[
            "artist", "chef", "technician", "engineer", "designer", "developer", "doctor",
            "nurse", "teacher", "scientist", "researcher", "architect", "manager", "writer",
            "lawyer", "analyst",
        ],
    )
}

fn extract_universal_values_from_sentence(sentence: &str) -> Vec<String> {
    let clause = sentence.trim();
    if clause.is_empty() {
        return vec![];
    }
    let clause = strip_disclosure_prefix(clause);
    clause
        .split(',')
        .flat_map(|s| s.split(" and "))
        .flat_map(|s| s.split(" or "))
        .map(|s| {
            let mut v = normalize_extracted_value(s.trim());
            let filler_strips: &[&str] = &[
                "lots of ", "a lot of ", "mainly ", "especially ", "mostly ", "some ",
                "various ", "different ", "all kinds of ", "i am now ", "now i am ",
            ];
            for prefix in filler_strips {
                if let Some(rest) = v.strip_prefix(prefix) {
                    v = rest.to_string();
                    break;
                }
            }
            if let Some(rest) = v.strip_suffix(" now") {
                v = rest.to_string();
            }
            v = extract_named_value(&v);
            v = extract_favorite_value(&v);
            v = extract_is_value(&v);
            v
        })
        .filter(|v| v.len() >= 2)
        .collect()
}

fn is_supported_disclosure_sentence(sentence: &str) -> bool {
    let s = sentence.trim();
    s.starts_with("i ")
        || s.starts_with("i am ")
        || s.starts_with("i have ")
        || s.starts_with("my ")
        || s.starts_with("we ")
        || s.starts_with("our ")
        || s.contains(" named ")
}

fn push_structural_update_assertion(sentence: &str, out: &mut Vec<StructuredAssertion>) -> bool {
    let s = sentence.trim();
    if let Some(rest) = s.strip_prefix("i am learning to play ") {
        push_domain_value(out, "activity", "practice", rest);
        if let Some(instrument) = rest.split_whitespace().last() {
            if is_instrument(instrument) {
                push_domain_value(out, "skill", "instrument", instrument);
            }
        }
        return true;
    }
    if let Some(value) = s
        .strip_prefix("i switched to a ")
        .or_else(|| s.strip_prefix("i switched to an "))
    {
        push_domain_value(out, "possession", "object", value);
        return true;
    }
    if let Some(value) = s.strip_prefix("i eat ") {
        push_domain_value(out, "preference", "food", value);
        return true;
    }
    if let Some(value) = s.strip_prefix("i use ") {
        push_domain_value(out, "skill", "tool", value);
        return true;
    }
    false
}

fn is_instrument(word: &str) -> bool {
    matches!(
        word,
        "piano" | "guitar" | "violin" | "cello" | "drums" | "trumpet" | "saxophone" | "flute"
            | "clarinet" | "bass" | "ukulele" | "banjo" | "viola" | "harp" | "trombone" | "oboe"
            | "harmonica" | "mandolin" | "keyboard" | "organ" | "fiddle"
    )
}

fn strip_disclosure_prefix(clause: &str) -> String {
    let lower = clause.to_lowercase();
    let prefixes: &[(&str, &str)] = &[
        ("i have a ", ""), ("i have an ", ""), ("my name is ", ""), ("my name's ", ""),
        ("i work as a ", ""), ("i work as an ", ""), ("i work in ", ""), ("i work at ", ""),
        ("i live in ", ""), ("i live at ", ""), ("i prefer ", ""), ("i like ", ""),
        ("i love ", ""), ("i enjoy ", ""), ("i hate ", ""), ("i drink ", ""), ("i eat ", ""),
        ("i drive a ", ""), ("i drive an ", ""), ("i own a ", ""), ("i own an ", ""),
        ("i need ", ""), ("i build ", ""), ("i go ", ""), ("i spend ", ""), ("i use ", ""),
        ("i moved to ", ""), ("i am learning ", "learning "), ("i am a ", ""), ("i am an ", ""),
    ];
    for (prefix, replacement) in prefixes {
        if lower.starts_with(prefix) {
            let rest = &clause[prefix.len()..];
            return if replacement.is_empty() {
                rest.trim().to_string()
            } else {
                format!("{replacement}{rest}").trim().to_string()
            };
        }
    }
    clause.to_string()
}

fn extract_named_value(value: &str) -> String {
    let lower = value.to_lowercase();
    if let Some(pos) = lower.rfind(" named ") {
        let after = value[pos + 7..].trim();
        let name: String = after
            .split([',', '.', ';', '!'])
            .next()
            .unwrap_or(after)
            .trim()
            .to_string();
        let parts: Vec<&str> = name.split_whitespace().collect();
        if parts.len() <= 3 && name.len() >= 2 {
            return name;
        }
    }
    value.to_string()
}

fn extract_favorite_value(value: &str) -> String {
    let lower = value.to_lowercase();
    if lower.starts_with("my favorite ") {
        if let Some(is_pos) = lower.find(" is ") {
            let after = value[is_pos + 4..].trim();
            return after
                .split([',', '.', ';'])
                .next()
                .unwrap_or(after)
                .trim()
                .to_string();
        }
    }
    value.to_string()
}

fn extract_is_value(value: &str) -> String {
    let lower = value.to_lowercase();
    for verb in &[" is ", " are ", " was ", " were ", " am "] {
        if let Some(pos) = lower.find(verb) {
            if lower[..pos].contains(" if ") {
                continue;
            }
            let after = value[pos + verb.len()..].trim();
            let after = after.strip_prefix("a ").unwrap_or(after);
            let after = after.strip_prefix("an ").unwrap_or(after);
            let after = after.strip_prefix("the ").unwrap_or(after);
            let clean: String = after
                .split([',', '.', ';', '!'])
                .next()
                .unwrap_or(after)
                .trim()
                .to_string();
            if clean.len() >= 2 && clean.split_whitespace().count() <= 6 {
                return clean;
            }
        }
    }
    value.to_string()
}

fn is_pure_transition_sentence(sentence: &str) -> bool {
    let s = sentence.trim();
    if s.contains(" got ") && s.split_whitespace().count() <= 4 {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() >= 3 && parts[1] == "got" {
            return true;
        }
    }
    if (s.starts_with("i gave up ") || s.starts_with("i quit ") || s.starts_with("i left "))
        && !s.contains(" now ")
        && !s.contains(" instead")
    {
        return true;
    }
    if s.starts_with("i had to give up ") || s.starts_with("i had to quit ") {
        return true;
    }
    if s.starts_with("i am not ") && s.contains(" anymore") {
        return true;
    }
    if s.starts_with("i finished ") || s.starts_with("i completed ") {
        return true;
    }
    false
}

fn strip_transition_prefix(sentence: &str) -> &str {
    let s = sentence.trim();
    find_after_transition_to(s).unwrap_or(s)
}

fn find_after_transition_to(s: &str) -> Option<&str> {
    let markers = ["moved to ", "switched to ", "shifted to ", "changed to "];
    for marker in &markers {
        if let Some(pos) = s.find(marker) {
            let after = &s[pos + marker.len()..];
            let trimmed = strip_temporal_suffix(after);
            if trimmed.len() >= 2 {
                return Some(trimmed);
            }
        }
    }
    None
}

fn strip_temporal_suffix(value: &str) -> &str {
    let temporal_suffixes = [
        " last month", " last week", " last year", " recently", " this month", " this week",
        " this year", " yesterday", " today", " now", " just now",
    ];
    for suffix in &temporal_suffixes {
        if let Some(pos) = value.rfind(suffix) {
            return value[..pos].trim();
        }
    }
    value.trim()
}

fn trim_temporal_context(value: &str) -> String {
    let lower = value.to_lowercase();
    let suffixes: &[&str] = &[
        "in the mornings", "in the morning", "in the afternoons", "in the afternoon",
        "in the evenings", "in the evening", "at night", "on weekends", "on weekdays",
        "during the day", "after work", "before bed", "these days", "nowadays", "lately",
        "recently", "most days", "every day",
    ];
    for suffix in suffixes {
        if lower.ends_with(suffix) && lower.len() > suffix.len() + 2 {
            let clean = lower[..lower.len() - suffix.len()].trim().to_string();
            if clean.len() >= 3 {
                return clean;
            }
        }
    }
    value.to_string()
}

fn normalize_extracted_value(raw: &str) -> String {
    let stripped = strip_leading_article(raw);
    trim_temporal_context(&stripped)
}

fn strip_leading_article(value: &str) -> String {
    let trimmed = value.trim();
    for article in ["a ", "an ", "the ", "my "] {
        if let Some(rest) = trimmed.strip_prefix(article) {
            return rest.trim().to_string();
        }
    }
    trimmed.to_string()
}

fn extract_after_named_companion(input: &str) -> Option<(String, String)> {
    let start = input
        .find("i have ")
        .or_else(|| input.find("i had "))
        .or_else(|| input.find("i got "))?;
    let tail = &input[start..];
    let marker = if tail.contains(" named ") {
        " named "
    } else if tail.contains(" called ") {
        " called "
    } else {
        return None;
    };
    let before = tail
        .split(marker)
        .next()
        .unwrap_or("")
        .replace("i have ", "")
        .replace("i had ", "")
        .replace("i got ", "");
    let after = tail.split(marker).nth(1).map(take_clause).unwrap_or_default();
    let entity = strip_leading_article(&before);
    if entity.is_empty() || after.is_empty() {
        None
    } else {
        Some((entity, after))
    }
}

fn take_clause(value: &str) -> String {
    bounded(
        value
            .split(['.', '!', '?', ';', ','])
            .next()
            .unwrap_or("")
            .trim(),
        80,
    )
}

/// Third-person numeric/technical facts: "The runtime has 34 Rust crates."
fn push_technical_fact_assertions(input: &str, out: &mut Vec<StructuredAssertion>) {
    const SKIP_NOUNS: &[&str] = &[
        "times", "turns", "ways", "things", "items", "words", "years", "months", "days",
        "hours", "minutes", "seconds", "percent", "people", "users",
    ];
    for sentence in input.split(['.', '!', '?', ';']) {
        let s = sentence.trim();
        if s.len() < 5 {
            continue;
        }
        if !s.starts_with("the ") && !s.starts_with("every ") && !s.starts_with("each ") && !s.starts_with("all ") {
            continue;
        }
        if !s.bytes().any(|b| b.is_ascii_digit()) {
            continue;
        }
        let words: Vec<&str> = s.split_whitespace().collect();
        let mut i = 0;
        while i < words.len() {
            let raw = words[i].trim_matches(|c: char| matches!(c, ',' | ';' | ':'));
            let n: u32 = match raw.parse::<u32>() {
                Ok(n) => n,
                Err(_) => {
                    i += 1;
                    continue;
                }
            };
            if !(2..=9_999).contains(&n) {
                i += 1;
                continue;
            }
            let mut noun: Vec<&str> = Vec::new();
            let mut j = i + 1;
            while j < words.len() && noun.len() < 4 {
                let wc = words[j].trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
                if wc.is_empty() || matches!(wc, "and" | "or" | "a" | "an" | "the" | "of" | "to" | "for") {
                    break;
                }
                noun.push(wc);
                if words[j].ends_with(',') {
                    break;
                }
                j += 1;
            }
            if let Some(first) = noun.first() {
                let head = first.trim_matches(|c: char| !c.is_alphabetic());
                if !SKIP_NOUNS.contains(&head) {
                    let value = format!("{} {}", n, noun.join(" "));
                    if value.len() <= 60 {
                        push_assertion(out, "skill", "technical", &value);
                    }
                }
            }
            i += 1;
        }
    }
}

fn push_domain_value(out: &mut Vec<StructuredAssertion>, domain: &str, sub: &str, value: &str) {
    if domain == "unknown" {
        return;
    }
    for segment in value.split(',') {
        let mut v = normalize_extracted_value(segment.trim());
        for prefix in &["lots of ", "a lot of ", "mainly ", "especially ", "mostly "] {
            if let Some(rest) = v.strip_prefix(prefix) {
                v = rest.to_string();
                break;
            }
        }
        if !is_plausible_disclosure_value(domain, sub, &v) {
            continue;
        }
        let v = v.trim();
        if v.is_empty() {
            continue;
        }
        push_assertion(out, domain, sub, v);
    }
}

/// Reject sentence fragments / function-word starts before they become memory.
fn is_plausible_disclosure_value(domain: &str, sub: &str, value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() || v.len() > 60 {
        return false;
    }
    let words: Vec<&str> = v.split_whitespace().collect();
    let word_count = words.len();
    let first = words.first().copied().unwrap_or("");
    if first == "i" && word_count > 1 {
        return false;
    }
    if word_count > 3
        && matches!(
            first,
            "we" | "they" | "it" | "this" | "that" | "so" | "but" | "and" | "then" | "my" | "he"
                | "she" | "you" | "the" | "a" | "to" | "of" | "in" | "at"
        )
    {
        return false;
    }
    if word_count > 5 {
        return false;
    }
    match (domain, sub) {
        ("identity", "name") => {
            const NON_NAMES: &[&str] = &[
                "me", "mostly", "it", "this", "that", "here", "there", "now", "then", "yes", "no",
                "just", "not", "more", "most", "very", "well", "good", "bad", "learning",
                "building", "working", "doing", "going", "running", "training", "trying",
                "starting", "thinking", "feeling", "getting", "making", "being", "having",
                "things", "stuff", "something", "everything", "nothing",
            ];
            !NON_NAMES.contains(&first) && word_count <= 3
        }
        ("skill", "profession") => {
            if word_count > 3 {
                return false;
            }
            const NON_PROFESSION_STARTS: &[&str] = &[
                "then", "been", "just", "now", "still", "already", "also", "not", "never",
                "always", "sometimes", "often", "startup", "company", "team", "group",
            ];
            !NON_PROFESSION_STARTS.contains(&first) && !is_stop_word(first)
        }
        ("location", _) => word_count <= 4,
        _ => true,
    }
}

fn expand_contractions(input: &str) -> String {
    let mut s = input.to_string();
    let expansions: &[(&str, &str)] = &[
        ("i'm", "i am"), ("i've", "i have"), ("i'll", "i will"), ("i'd", "i would"),
        ("can't", "cannot"), ("won't", "will not"), ("don't", "do not"), ("doesn't", "does not"),
        ("didn't", "did not"), ("isn't", "is not"), ("aren't", "are not"), ("wasn't", "was not"),
        ("weren't", "were not"), ("haven't", "have not"), ("hasn't", "has not"),
        ("there's", "there is"), ("that's", "that is"), ("it's", "it is"),
        ("they're", "they are"), ("we're", "we are"), ("you're", "you are"),
    ];
    for (from, to) in expansions {
        s = s.replace(from, to);
    }
    s
}

fn is_question(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.ends_with('?') {
        return true;
    }
    let lower = trimmed.to_lowercase();
    let question_prefixes = [
        "what ", "where ", "when ", "who ", "how ", "which ", "why ", "do you ", "tell me ",
        "give me ", "describe ", "explain ", "list ", "show me ", "find ", "search ",
        "can you ", "could you ", "would you ", "will you ", "did you ", "does ", "is my ",
        "are my ", "was my ", "were my ", "am i ",
    ];
    question_prefixes.iter().any(|p| lower.starts_with(p))
}

fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "i" | "me" | "my" | "a" | "an" | "the" | "to" | "of" | "in" | "on" | "at" | "for"
            | "with" | "it" | "is" | "was" | "were" | "be" | "been" | "have" | "has" | "had"
            | "do" | "does" | "did" | "not" | "no" | "so" | "then" | "than" | "just" | "now"
            | "also" | "very" | "only" | "even" | "still" | "already" | "really"
    )
}

fn contains_any(input: &str, needles: &[&str]) -> bool {
    let lower = input.to_lowercase();
    needles.iter().any(|needle| lower.contains(needle))
}

fn push_assertion(out: &mut Vec<StructuredAssertion>, domain: &str, kind: &str, value: &str) {
    if out.len() >= 24 || value.trim().is_empty() {
        return;
    }
    out.push(StructuredAssertion::inferred(domain, kind, value.trim()));
}

fn bounded(value: &str, max_len: usize) -> String {
    value.chars().take(max_len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(items: &'a [StructuredAssertion], domain: &str, kind: &str) -> Option<&'a StructuredAssertion> {
        items.iter().find(|a| a.domain == domain && a.kind == kind)
    }

    #[test]
    fn extracts_name_from_first_person_disclosure() {
        let a = disclosure_assertions("My name is Casey Quinn.");
        let name = find(&a, "identity", "name").expect("name assertion");
        assert!(name.value.contains("casey"));
    }

    #[test]
    fn extracts_profession_as_skill() {
        let a = disclosure_assertions("I work as a marine biologist.");
        let prof = find(&a, "skill", "profession").expect("profession assertion");
        assert!(prof.value.contains("marine biologist"));
    }

    #[test]
    fn extracts_location_from_live_in() {
        let a = disclosure_assertions("I live in Santa Cruz.");
        let loc = find(&a, "location", "residence").expect("location assertion");
        assert!(loc.value.contains("santa cruz"));
    }

    #[test]
    fn questions_produce_no_disclosure_assertions() {
        assert!(disclosure_assertions("Where do I live?").is_empty());
    }

    #[test]
    fn rejects_fragment_values_via_plausibility_guard() {
        // "i got promoted" is a pure transition: no new state value, no assertion.
        assert!(disclosure_assertions("I got promoted.").is_empty());
    }

    #[test]
    fn third_person_numeric_fact_is_captured() {
        let a = disclosure_assertions("The runtime has 34 Rust crates.");
        assert!(find(&a, "skill", "technical").is_some());
    }
}
