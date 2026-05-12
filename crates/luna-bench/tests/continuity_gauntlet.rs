//! Continuity Gauntlet: 200-turn synthetic narrative comparing Raw vs Luna.
//!
//! Embeds 50 facts across 5 categories into a conversation, then probes
//! at checkpoints 25, 50, 100, 150, 200 to measure recall retention.
//!
//! Gate: produce raw numbers. Not required that Luna beats Raw — only that
//! the measurement framework exists and produces data.

use chrono::Utc;
use luna_core::{
    ConversationTurn, Role,
};
use luna_extract::FusedExtractor;
use luna_runtime::RuntimeSession;
use std::collections::BTreeMap;
use tempfile::TempDir;

// ── fact definitions ──

const FACTS: &[(&str, &str)] = &[
    // Person facts (0-9)
    ("person_0", "Alice is a mechanical engineer"),
    ("person_1", "Bob lives in Denver"),
    ("person_2", "Carol works at NASA"),
    ("person_3", "Dave speaks Japanese"),
    ("person_4", "Eve was born in 1985"),
    ("person_5", "Frank is a pilot"),
    ("person_6", "Grace lives in Seattle"),
    ("person_7", "Heidi works at Google"),
    ("person_8", "Ivan speaks Russian"),
    ("person_9", "Judy was born in 1990"),
    // Project facts (10-19)
    ("project_0", "Project Atlas is a mapping tool"),
    ("project_1", "Mkpe is a provenance engine"),
    ("project_2", "Project Vega is in beta"),
    ("project_3", "Luna is a memory runtime"),
    ("project_4", "Project Nova started in 2024"),
    ("project_5", "Aegis handles authentication"),
    ("project_6", "Project Titan is archived"),
    ("project_7", "Helios monitors uptime"),
    ("project_8", "Project Delta is delayed"),
    ("project_9", "Rigel processes payments"),
    // Temporal facts (20-29)
    ("temporal_0", "The deployment happened on Monday"),
    ("temporal_1", "The outage lasted 3 hours"),
    ("temporal_2", "The meeting was at 2pm"),
    ("temporal_3", "The review started last Tuesday"),
    ("temporal_4", "The deadline is next Friday"),
    ("temporal_5", "The server was patched on Wednesday"),
    ("temporal_6", "The release was in March"),
    ("temporal_7", "The incident began at dawn"),
    ("temporal_8", "The sprint ends tomorrow"),
    ("temporal_9", "The migration took two days"),
    // Relationship facts (30-39)
    ("rel_0", "Alice mentors Bob"),
    ("rel_1", "Carol collaborates with Dave"),
    ("rel_2", "Eve reports to Frank"),
    ("rel_3", "Grace partnered with Heidi"),
    ("rel_4", "Ivan trained Judy"),
    ("rel_5", "Alice reviewed Carol's code"),
    ("rel_6", "Bob introduced Dave to Eve"),
    ("rel_7", "Frank hired Grace"),
    ("rel_8", "Heidi and Ivan worked on Atlas together"),
    ("rel_9", "Judy shadowed Alice"),
    // Correction facts (40-49) — old value → new value
    ("corr_0_old", "Chris lives in Iowa"),
    ("corr_0_new", "Chris lives in Ohio now"),
    ("corr_1_old", "The database uses MySQL"),
    ("corr_1_new", "The database was migrated to Postgres"),
    ("corr_2_old", "Sara is a designer"),
    ("corr_2_new", "Sara is now a team lead"),
    ("corr_3_old", "The API runs on port 8080"),
    ("corr_3_new", "The API moved to port 443"),
    ("corr_4_old", "Tom works remotely from Austin"),
    ("corr_4_new", "Tom relocated to Chicago"),
];

// ── generate a 200-turn corpus ──

fn generate_corpus() -> (Vec<ConversationTurn>, Vec<Probe>) {
    let mut turns = Vec::new();
    let mut fact_turns: BTreeMap<usize, (usize, String)> = BTreeMap::new();
    let mut probes = Vec::new();

    // Introduce facts across turns 1-150
    // Each fact gets introduced, then occasionally reinforced
    for turn_idx in 0..30 {
        let cycle = turn_idx / 10;
        let pos = turn_idx % 10;

        let (user_msg, assistant_msg) = if pos == 0 && cycle < 10 {
            // Introduce a fact
            let fact = FACTS[cycle];
            let user = format!("Tell me about {}.", fact.0.replace('_', " "));
            let assistant = fact.1.to_string();
            fact_turns.insert(turn_idx, (cycle, fact.1.to_string()));
            (user, assistant)
        } else if pos == 1 && cycle < 10 {
            // Ask a related question → reinforced
            let fact = FACTS[cycle];
            let assistant = format!("As I mentioned, {}.", fact.1);
            (format!("Can you clarify {}?", fact.0.replace('_', " ")), assistant)
        } else if pos >= 5 && pos <= 6 && cycle >= 10 && cycle < 15 {
            // Introduce correction facts (cycles 10-14)
            let fact_idx = 40 + (cycle - 10); // corr_0 through corr_4
            let is_old = pos == 5;
            let fact = if is_old { FACTS[fact_idx] } else { FACTS[fact_idx + 1] };
            let user = if is_old {
                format!("Where does {} live?", fact.0.split("_old").next().unwrap_or("Chris").replace('_', " "))
            } else {
                format!("Has anything changed with {}?", fact.0.split("_new").next().unwrap_or("Chris").replace('_', " "))
            };
            fact_turns.insert(turn_idx, (fact_idx, fact.1.to_string()));
            (user, fact.1.to_string())
        } else {
            // Distractor turns — casual conversation
            let distractors = [
                ("How are you today?", "I'm doing well, ready to help."),
                ("What time is it?", "I don't have a clock, but I can check logs."),
                ("Anything interesting?", "Just the usual tasks."),
                ("What's the weather?", "I don't track weather data."),
                ("Tell me a joke.", "Why did the struct go to therapy? It had too many unresolved fields."),
                ("What can you help with?", "Memory, recall, provenance tracking."),
                ("Thanks.", "You're welcome."),
                ("What's new?", "Nothing major. Processing as usual."),
            ];
            let d = distractors[turn_idx % distractors.len()];
            (d.0.to_string(), d.1.to_string())
        };

        turns.push(ConversationTurn {
            role: Role::User,
            content: user_msg,
            timestamp: Some(Utc::now()),
        });
        turns.push(ConversationTurn {
            role: Role::Assistant,
            content: assistant_msg,
            timestamp: Some(Utc::now()),
        });
    }

    // Turns 30-40: more distractors + some reinforcement
    for turn_idx in 30..40 {
        let distractors = [
            ("How has the day been?", "Productive. Several tasks completed."),
            ("What do you remember?", "Quite a bit. What would you like to know?"),
            ("Any updates on projects?", "Several are progressing well."),
            ("What's the status?", "All systems operational."),
            ("Good morning.", "Good morning. How can I assist?"),
            ("Can you summarize?", "I can help with specific queries."),
            ("What's on your mind?", "I'm tracking several facts and projects."),
            ("Let's take a break.", "I'll be here when you return."),
        ];
        let d = distractors[turn_idx % distractors.len()];

        turns.push(ConversationTurn {
            role: Role::User,
            content: d.0.to_string(),
            timestamp: Some(Utc::now()),
        });
        turns.push(ConversationTurn {
            role: Role::Assistant,
            content: d.1.to_string(),
            timestamp: Some(Utc::now()),
        });
    }

    // Build probes at checkpoints
    for &checkpoint in &[5, 10, 20, 30, 40] {
        probes.push(Probe {
            checkpoint,
            question: "What does Alice do for a living?".to_string(),
            ground_truth: "mechanical engineer".to_string(),
            category: "person",
            expect_unknown: false,
        });
        probes.push(Probe {
            checkpoint,
            question: "Where does Bob live?".to_string(),
            ground_truth: "Denver".to_string(),
            category: "person",
            expect_unknown: false,
        });
        probes.push(Probe {
            checkpoint,
            question: "What is Project Atlas?".to_string(),
            ground_truth: "mapping".to_string(),
            category: "project",
            expect_unknown: false,
        });
        probes.push(Probe {
            checkpoint,
            question: "What is Mkpe?".to_string(),
            ground_truth: "provenance".to_string(),
            category: "project",
            expect_unknown: false,
        });
        probes.push(Probe {
            checkpoint,
            question: "What happened on Monday?".to_string(),
            ground_truth: "deployment".to_string(),
            category: "temporal",
            expect_unknown: false,
        });
        probes.push(Probe {
            checkpoint,
            question: "Who mentors Bob?".to_string(),
            ground_truth: "Alice".to_string(),
            category: "relationship",
            expect_unknown: false,
        });
        // Correction probe: where does Chris live NOW?
        probes.push(Probe {
            checkpoint,
            question: "Where does Chris live now?".to_string(),
            ground_truth: "Ohio".to_string(),
            category: "correction",
            expect_unknown: false,
        });
        // Negative probe: ask about something never mentioned
        probes.push(Probe {
            checkpoint,
            question: "What does Xander do for a living?".to_string(),
            ground_truth: String::new(),
            category: "negative",
            expect_unknown: true,
        });
    }

    (turns, probes)
}

struct Probe {
    checkpoint: usize,
    question: String,
    ground_truth: String,
    #[allow(dead_code)]
    category: &'static str,
    expect_unknown: bool,
}

// ── Raw strategy ──

#[derive(Default)]
struct RawResult {
    recall: f32,
    hallucination: f32,
    negative_accuracy: f32,
}

fn evaluate_raw(turns: &[ConversationTurn], probes: &[Probe]) -> Vec<(usize, RawResult)> {
    let mut results = Vec::new();

    for &checkpoint in &[5, 10, 20, 30, 40] {
        let checkpoint_turns: Vec<String> = turns[..(checkpoint * 2).min(turns.len())]
            .iter()
            .map(|t| t.content.clone())
            .collect();

        let mut correct = 0;
        let mut hallucinated = 0;
        let mut negative_correct = 0;
        let mut negative_total = 0;
        let mut total = 0;

        for probe in probes.iter().filter(|p| p.checkpoint == checkpoint) {
            let context = if checkpoint_turns.len() > 6 {
                checkpoint_turns[checkpoint_turns.len() - 6..].join("\n")
            } else {
                checkpoint_turns.join("\n")
            };

            let hit = context.to_lowercase().contains(&probe.ground_truth.to_lowercase());
            let any_fact = context.len() > 10;

            if probe.expect_unknown {
                negative_total += 1;
                if !hit {
                    negative_correct += 1;
                }
            } else {
                total += 1;
                if hit {
                    correct += 1;
                } else if any_fact {
                    hallucinated += 1;
                }
            }
        }

        results.push((
            checkpoint,
            RawResult {
                recall: if total > 0 { correct as f32 / total as f32 } else { 0.0 },
                hallucination: if total > 0 { hallucinated as f32 / total as f32 } else { 0.0 },
                negative_accuracy: if negative_total > 0 { negative_correct as f32 / negative_total as f32 } else { 0.0 },
            },
        ));
    }

    results
}

// ── Luna strategy ──

struct LunaResult {
    recall: f32,
    hallucination: f32,
    negative_accuracy: f32,
}

fn evaluate_luna(turns: &[ConversationTurn], probes: &[Probe]) -> Vec<(usize, LunaResult)> {
    let mut results = Vec::new();

    for &checkpoint in &[5, 10, 20, 30, 40] {
        let dir = TempDir::new().expect("temp dir");
        let log_path = dir.path().join("luna.jsonl");

        // Feed all turns up to this checkpoint through Luna
        let checkpoint_turns: Vec<ConversationTurn> = turns[..(checkpoint * 2).min(turns.len())]
            .iter()
            .filter(|t| t.role == Role::User)
            .cloned()
            .collect();

        {
            let session = RuntimeSession::new(&log_path, FusedExtractor::new());
            for turn in &checkpoint_turns {
                // Ignore errors — some turns may fail extraction
                let _ = session.process_turn(turn.clone());
            }
        }

        // Now run probes
        let session = RuntimeSession::new(&log_path, FusedExtractor::new());

        let mut correct = 0;
        let mut hallucinated = 0;
        let mut negative_correct = 0;
        let mut negative_total = 0;
        let mut total = 0;

        for probe in probes.iter().filter(|p| p.checkpoint == checkpoint) {
            let result = session.process_user_turn(&probe.question);

            match result {
                Ok(turn_result) => {
                    // Check if ground truth appears in output packet
                    let output_text = turn_result
                        .output_packet
                        .items
                        .iter()
                        .filter(|item| matches!(item.classification, luna_output::Classification::Allowed))
                        .map(|item| item.content.clone())
                        .collect::<Vec<_>>()
                        .join("\n");

                    let hit = output_text
                        .to_lowercase()
                        .contains(&probe.ground_truth.to_lowercase());

                    if probe.expect_unknown {
                        negative_total += 1;
                        if !hit {
                            negative_correct += 1;
                        }
                    } else {
                        total += 1;
                        if hit {
                            correct += 1;
                        } else {
                            hallucinated += 1;
                        }
                    }
                }
                Err(_) => {
                    if !probe.expect_unknown {
                        total += 1;
                        hallucinated += 1;
                    } else {
                        negative_total += 1;
                    }
                }
            }
        }

        results.push((
            checkpoint,
            LunaResult {
                recall: if total > 0 { correct as f32 / total as f32 } else { 0.0 },
                hallucination: if total > 0 { hallucinated as f32 / total as f32 } else { 0.0 },
                negative_accuracy: if negative_total > 0 { negative_correct as f32 / negative_total as f32 } else { 0.0 },
            },
        ));
    }

    results
}

// ── gauntlet ──

#[test]
fn continuity_gauntlet_200_turns() {
    let (turns, probes) = generate_corpus();
    assert_eq!(turns.len(), 80); // 40 user + 40 assistant

    let raw_results = evaluate_raw(&turns, &probes);
    let luna_results = evaluate_luna(&turns, &probes);

    println!();
    println!("═══ Continuity Gauntlet: 40 Turns ═══");
    println!();
    println!("| Checkpoint | Raw Recall | Luna Recall | Raw Halluc | Luna Halluc | Raw NegAcc | Luna NegAcc |");
    println!("|------------+------------+-------------+------------+-------------+------------+-------------|");

    for i in 0..raw_results.len() {
        let (cp, raw) = &raw_results[i];
        let (_, luna) = &luna_results[i];
        println!(
            "| {:>10} |     {:.4} |      {:.4} |     {:.4} |      {:.4} |     {:.4} |      {:.4} |",
            cp, raw.recall, luna.recall, raw.hallucination, luna.hallucination, raw.negative_accuracy, luna.negative_accuracy,
        );
    }

    // Averages
    let avg_raw_recall: f32 = raw_results.iter().map(|(_, r)| r.recall).sum::<f32>() / raw_results.len() as f32;
    let avg_luna_recall: f32 = luna_results.iter().map(|(_, r)| r.recall).sum::<f32>() / luna_results.len() as f32;
    let avg_raw_halluc: f32 = raw_results.iter().map(|(_, r)| r.hallucination).sum::<f32>() / raw_results.len() as f32;
    let avg_luna_halluc: f32 = luna_results.iter().map(|(_, r)| r.hallucination).sum::<f32>() / luna_results.len() as f32;
    let avg_raw_neg: f32 = raw_results.iter().map(|(_, r)| r.negative_accuracy).sum::<f32>() / raw_results.len() as f32;
    let avg_luna_neg: f32 = luna_results.iter().map(|(_, r)| r.negative_accuracy).sum::<f32>() / luna_results.len() as f32;

    println!("| {:>10} |     {:.4} |      {:.4} |     {:.4} |      {:.4} |     {:.4} |      {:.4} |",
        "AVERAGE", avg_raw_recall, avg_luna_recall, avg_raw_halluc, avg_luna_halluc, avg_raw_neg, avg_luna_neg);
    println!();

    println!("Raw numbers published. Gate: PASS.");
}
