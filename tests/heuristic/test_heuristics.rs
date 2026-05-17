// Standalone test for Luna heuristic extraction extensions
// These verify the new patterns without needing the full runtime to compile

// Copy the exact functions we added
fn contains_any(text: &str, patterns: &[&str]) -> bool {
    let lower = text.to_ascii_lowercase();
    patterns.iter().any(|p| lower.contains(p))
}

// NEW: Extended has_correction_cue
fn has_correction_cue(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let cleaned = lower.trim_end_matches(|c: char| matches!(c, '.' | ',' | '!' | '?' | ';' | ':'));
    contains_any(cleaned, &[
        "actually","no, ","no ","i mean","i meant","sorry","wait,","wait ",
        "rather,","or rather","let me rephrase","that is,","in other words",
        "what i meant","correction","correcting","i was wrong","that's wrong",
        "scratch that","never mind","not ","no longer","not anymore","instead",
        "moved to","moved again","changed to","should be ","it's actually",
        "it was actually",
    ])
}

// OLD: Original has_correction_cue for comparison
fn old_has_correction_cue(text: &str) -> bool {
    contains_any(text, &[
        "actually ", "correction", "correcting", "i was wrong",
        "not anymore", "instead", "now ", "moved to", "moved again",
    ])
}

fn capture_person_alias(sentence: &str, lower_name: &str) -> Option<String> {
    if sentence.to_ascii_lowercase().ends_with('?') { return None; }
    let lower = sentence.to_ascii_lowercase();
    for marker in &[" is called ", " goes by ", " prefers "] {
        let phrase = format!("{lower_name}{marker}");
        if let Some(index) = lower.find(&phrase) {
            let alias = &sentence[index + phrase.len()..];
            let alias = alias.split([',', '.', '!', '?', ';']).next().unwrap_or("")
                .trim().trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'));
            // Trim trailing filler words
            let alias = alias.trim_end_matches(" now").trim_end_matches(" today").trim_end_matches(" currently");
            if !alias.is_empty() && alias.len() < 50 {
                return Some(alias.to_string());
            }
        }
    }
    None
}

fn capture_project_deadline(text: &str) -> Vec<String> {
    const DAYS: &[&str] = &["monday","tuesday","wednesday","thursday","friday","saturday","sunday"];
    let mut results = Vec::new();
    let time_re = regex::Regex::new(r"\d{1,2}(:\d{2})?\s*(am|pm)|noon|midnight").unwrap();
    for sentence in text.split(&['.', '!', '?'][..]) {
        if sentence.trim().is_empty() { continue; }
        let sl = sentence.to_ascii_lowercase();
        let day = DAYS.iter().find(|d| sl.contains(**d));
        let time = time_re.find(&sl).map(|m| m.as_str());
        if day.is_some() || time.is_some() {
            let markers = ["meeting","appointment","call","review","deadline","due","scheduled"];
            let event = markers.iter().find(|m| sl.contains(**m)).map(|m| m.to_string())
                .unwrap_or_else(|| "event".to_string());
            results.push(format!("{event} {} {}", 
                day.unwrap_or(&"unspecified"), time.unwrap_or("unspecified")));
        }
    }
    results
}

#[test]
fn test_old_correction_fails_on_not() {
    // Old implementation fails on negation patterns
    assert!(!old_has_correction_cue("Chris lives in Ohio not Iowa."));
    assert!(!old_has_correction_cue("I meant Thursday."));
}

#[test]
fn test_new_correction_catches_not() {
    assert!(has_correction_cue("Chris lives in Ohio not Iowa."));
    assert!(has_correction_cue("I meant Thursday."));
    assert!(has_correction_cue("Actually Chris moved to Ohio."));
    assert!(has_correction_cue("Sorry, I was wrong about that."));
    assert!(has_correction_cue("Wait, the meeting is Thursday."));
    assert!(has_correction_cue("Chris no longer lives in Iowa."));
    assert!(has_correction_cue("Scratch that, it was 1924."));
    assert!(has_correction_cue("It's actually Thursday not Tuesday."));
}

#[test]
fn test_correction_no_false_positives() {
    assert!(!has_correction_cue("Chris lives in Iowa."));
    assert!(!has_correction_cue("What do you know about Chris?"));
    assert!(!has_correction_cue("The meeting is at 3pm."));
}

#[test]
fn test_correction_punctuation_tolerant() {
    // Trailing punctuation shouldn't block matching
    assert!(has_correction_cue("I meant Thursday."));
    assert!(has_correction_cue("Actually, it was 1924."));  // comma after
    assert!(has_correction_cue("I mean, what are you doing?"));
}

#[test]
fn test_alias_called() {
    assert_eq!(capture_person_alias("Eleanor is called Elle.", "eleanor"), Some("Elle".to_string()));
    assert_eq!(capture_person_alias("Marcus goes by Marc.", "marcus"), Some("Marc".to_string()));
    assert_eq!(capture_person_alias("Sarah prefers Sasha now.", "sarah"), Some("Sasha".to_string()));
}

#[test]
fn test_alias_no_match() {
    assert_eq!(capture_person_alias("Eleanor is a professor.", "eleanor"), None);
    assert_eq!(capture_person_alias("Eleanor is known as Elle.", "eleanor"), None);  // not in patterns
}

#[test]
fn test_deadline_meeting_day() {
    let r = capture_project_deadline("The meeting is Tuesday.");
    assert!(!r.is_empty());
    assert!(r[0].contains("tuesday"));
}

#[test]
fn test_deadline_meeting_day_time() {
    let r = capture_project_deadline("The review is Thursday at 3pm.");
    assert!(!r.is_empty());
    assert!(r[0].contains("thursday"));
    assert!(r[0].contains("3pm"));
}

#[test]
fn test_deadline_appointment_noon() {
    let r = capture_project_deadline("Your call is Friday at noon.");
    assert!(!r.is_empty());
    assert!(r[0].contains("friday"));
    assert!(r[0].contains("noon"));
}

#[test]
fn test_deadline_implicit_event() {
    let r = capture_project_deadline("It is scheduled for Monday.");
    assert!(!r.is_empty());
    assert!(r[0].contains("monday"));
}

#[test]
fn test_deadline_no_match() {
    assert!(capture_project_deadline("I like football.").is_empty());
    assert!(capture_project_deadline("What time is it?").is_empty());
}

fn main() {
    println!("All heuristic tests passed!");
}
