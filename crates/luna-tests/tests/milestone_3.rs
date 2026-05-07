use chrono::{Duration, Utc};
use luna_sentinels::{
    ContradictionSentinel, DefectClass, ProvenanceIntegritySentinel, Sentinel, SentinelEvaluation,
    SentinelReportLog, SentinelRuntime, SentinelSchedule, SplinterPressureSentinel, TopologyView,
    ViewAssertion, ViewGenesisCertificate, ViewNode, ViewOrb, ViewRawEvent, ViewTether,
};

fn good_view() -> TopologyView {
    TopologyView::new(
        vec![ViewRawEvent::new("event-1", "hash-1")],
        vec![ViewNode::new("node-1", "event-1", "hash-1")],
        vec![ViewGenesisCertificate::new(
            "genesis-1",
            "node-1",
            "event-1",
            "hash-1",
        )],
        vec![ViewTether::new(
            "tether-1",
            "node-1",
            "node-1",
            "event-1",
            "hash-1",
            Some(ViewAssertion::new("node-1", "status", "active")),
        )],
        vec![ViewOrb::new("orb-1", vec![0.2, 0.3], vec![0.9, 0.91])],
    )
}

#[test]
fn test_contradiction_sentinel_fires_on_known_bad_input() {
    let view = TopologyView::new(
        vec![ViewRawEvent::new("event-1", "hash-1")],
        vec![ViewNode::new("node-1", "event-1", "hash-1")],
        vec![ViewGenesisCertificate::new(
            "genesis-1",
            "node-1",
            "event-1",
            "hash-1",
        )],
        vec![
            ViewTether::new(
                "tether-1",
                "node-1",
                "node-1",
                "event-1",
                "hash-1",
                Some(ViewAssertion::new("node-1", "location", "Iowa")),
            ),
            ViewTether::new(
                "tether-2",
                "node-1",
                "node-1",
                "event-1",
                "hash-1",
                Some(ViewAssertion::new("node-1", "location", "Ohio")),
            ),
        ],
        vec![],
    );
    let sentinel = ContradictionSentinel::new(1.0);

    let evaluation = sentinel.evaluate(&view);

    match evaluation {
        SentinelEvaluation::Flag {
            score,
            evidence,
            recommendation,
        } => {
            assert_eq!(score, 2.0);
            assert_eq!(
                evidence,
                vec!["tether-1".to_string(), "tether-2".to_string()]
            );
            assert!(recommendation.contains("contradiction"));
        }
        SentinelEvaluation::Quiet => panic!("expected contradiction flag"),
    }
}

#[test]
fn test_sentinels_stay_quiet_on_known_good_input() {
    let view = good_view();
    let sentinels: Vec<Box<dyn Sentinel>> = vec![
        Box::new(ContradictionSentinel::new(1.0)),
        Box::new(ProvenanceIntegritySentinel::new()),
        Box::new(SplinterPressureSentinel::new(0.2)),
    ];

    for sentinel in sentinels {
        assert_eq!(sentinel.evaluate(&view), SentinelEvaluation::Quiet);
    }
}

#[test]
fn test_multi_valued_predicates_are_not_contradictions() {
    let view = TopologyView::new(
        vec![ViewRawEvent::new("event-1", "hash-1")],
        vec![ViewNode::new("node-1", "event-1", "hash-1")],
        vec![ViewGenesisCertificate::new(
            "genesis-1",
            "node-1",
            "event-1",
            "hash-1",
        )],
        vec![
            ViewTether::new(
                "tether-1",
                "node-1",
                "node-1",
                "event-1",
                "hash-1",
                Some(ViewAssertion::multi_valued("node-1", "interest", "music")),
            ),
            ViewTether::new(
                "tether-2",
                "node-1",
                "node-1",
                "event-1",
                "hash-1",
                Some(ViewAssertion::multi_valued(
                    "node-1",
                    "interest",
                    "basketball",
                )),
            ),
        ],
        vec![],
    );

    assert_eq!(
        ContradictionSentinel::new(1.0).evaluate(&view),
        SentinelEvaluation::Quiet
    );
}

#[test]
fn test_provenance_integrity_sentinel_fires_on_hash_mismatch() {
    let view = TopologyView::new(
        vec![ViewRawEvent::new("event-1", "hash-1")],
        vec![ViewNode::new("node-1", "event-1", "wrong-hash")],
        vec![ViewGenesisCertificate::new(
            "genesis-1",
            "node-1",
            "event-1",
            "hash-1",
        )],
        vec![],
        vec![],
    );
    let sentinel = ProvenanceIntegritySentinel::new();

    let evaluation = sentinel.evaluate(&view);

    match evaluation {
        SentinelEvaluation::Flag {
            score,
            evidence,
            recommendation,
        } => {
            assert_eq!(score, 2.0);
            assert_eq!(
                evidence,
                vec![
                    "node:node-1->event:event-1".to_string(),
                    "certificate:genesis-1->node:node-1".to_string()
                ]
            );
            assert!(recommendation.contains("provenance"));
        }
        SentinelEvaluation::Quiet => panic!("expected provenance flag"),
    }
}

#[test]
fn test_provenance_integrity_sentinel_fires_on_bad_tether_chain() {
    let view = TopologyView::new(
        vec![ViewRawEvent::new("event-1", "hash-1")],
        vec![ViewNode::new("node-1", "event-1", "hash-1")],
        vec![ViewGenesisCertificate::new(
            "genesis-1",
            "node-1",
            "event-1",
            "hash-1",
        )],
        vec![ViewTether::new(
            "tether-1",
            "missing-source",
            "missing-target",
            "event-1",
            "wrong-hash",
            None,
        )],
        vec![],
    );

    let evaluation = ProvenanceIntegritySentinel::new().evaluate(&view);

    match evaluation {
        SentinelEvaluation::Flag {
            score, evidence, ..
        } => {
            assert_eq!(score, 3.0);
            assert_eq!(
                evidence,
                vec![
                    "tether:tether-1->source_node:missing-source".to_string(),
                    "tether:tether-1->target_node:missing-target".to_string(),
                    "tether:tether-1->event:event-1".to_string()
                ]
            );
        }
        SentinelEvaluation::Quiet => panic!("expected tether provenance flag"),
    }
}

#[test]
fn test_splinter_pressure_sentinel_fires_on_density_precision_divergence() {
    let view = TopologyView::new(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![ViewOrb::new("orb-1", vec![0.4, 0.8], vec![0.9, 0.5])],
    );
    let sentinel = SplinterPressureSentinel::new(0.2);

    let evaluation = sentinel.evaluate(&view);

    match evaluation {
        SentinelEvaluation::Flag {
            score,
            evidence,
            recommendation,
        } => {
            assert!((score - 0.8).abs() < 0.000_001);
            assert_eq!(
                evidence,
                vec!["orb:orb-1 density_delta=0.400 precision_delta=-0.400"]
            );
            assert!(recommendation.contains("splinter"));
        }
        SentinelEvaluation::Quiet => panic!("expected splinter pressure flag"),
    }
}

#[test]
fn test_splinter_pressure_aggregates_across_orbs() {
    let view = TopologyView::new(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![
            ViewOrb::new("orb-1", vec![0.1, 0.2], vec![0.9, 0.8]),
            ViewOrb::new("orb-2", vec![0.1, 0.2], vec![0.9, 0.8]),
        ],
    );

    let evaluation = SplinterPressureSentinel::new(0.3).evaluate(&view);

    match evaluation {
        SentinelEvaluation::Flag {
            score, evidence, ..
        } => {
            assert!((score - 0.4).abs() < 0.000_001);
            assert_eq!(evidence.len(), 2);
        }
        SentinelEvaluation::Quiet => panic!("expected aggregate splinter pressure flag"),
    }
}

#[test]
fn test_sentinel_reports_are_reproducible_from_same_view() {
    let view = good_view();
    let sentinel = ProvenanceIntegritySentinel::new();
    let timestamp = Utc::now();

    let first = sentinel.report(&view, timestamp);
    let second = sentinel.report(&view, timestamp);

    assert_eq!(first, second);
}

#[test]
fn test_flagged_sentinel_reports_are_reproducible_from_same_view() {
    let view = TopologyView::new(
        vec![ViewRawEvent::new("event-1", "hash-1")],
        vec![ViewNode::new("node-1", "event-1", "hash-1")],
        vec![ViewGenesisCertificate::new(
            "genesis-1",
            "node-1",
            "event-1",
            "hash-1",
        )],
        vec![
            ViewTether::new(
                "tether-1",
                "node-1",
                "node-1",
                "event-1",
                "hash-1",
                Some(ViewAssertion::new("node-1", "location", "Iowa")),
            ),
            ViewTether::new(
                "tether-2",
                "node-1",
                "node-1",
                "event-1",
                "hash-1",
                Some(ViewAssertion::new("node-1", "location", "Ohio")),
            ),
        ],
        vec![],
    );
    let sentinel = ContradictionSentinel::new(1.0);
    let timestamp = Utc::now();

    assert_eq!(
        sentinel.report(&view, timestamp),
        sentinel.report(&view, timestamp)
    );
}

#[test]
fn test_sentinel_report_log_is_append_only() {
    let view = good_view();
    let sentinel = ContradictionSentinel::new(1.0);
    let mut log = SentinelReportLog::default();
    let first = sentinel.report(&view, Utc::now());
    let second = sentinel.report(&view, Utc::now() + Duration::seconds(1));

    log.append(first.clone());
    log.append(second.clone());

    assert_eq!(log.reports(), &[first, second]);
    assert!(log.reports_mut_for_tests().is_none());
}

#[test]
fn test_sentinel_report_jsonl_round_trips() {
    let path = std::env::temp_dir().join(format!(
        "luna-sentinel-reports-{}.jsonl",
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    let report = ContradictionSentinel::new(1.0).report(&good_view(), Utc::now());

    SentinelReportLog::append_jsonl(&path, &report).unwrap();
    SentinelReportLog::append_jsonl(&path, &report).unwrap();
    let loaded = SentinelReportLog::load_jsonl(&path).unwrap();

    assert_eq!(loaded, vec![report.clone(), report]);
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_runtime_functions_with_all_sentinels_disabled() {
    let view = good_view();
    let mut runtime = SentinelRuntime::disabled();
    runtime.register(Box::new(ContradictionSentinel::with_schedule(
        1.0,
        SentinelSchedule::EveryEvents(1),
    )));

    let reports = runtime.run_due(&view, 10, Utc::now());

    assert!(reports.is_empty());
    assert!(runtime.log().reports().is_empty());
}

#[test]
fn test_runtime_respects_event_and_time_schedules() {
    let view = good_view();
    let start = Utc::now();
    let mut runtime = SentinelRuntime::enabled();
    runtime.register(Box::new(ContradictionSentinel::with_schedule(
        1.0,
        SentinelSchedule::EveryEvents(2),
    )));
    runtime.register(Box::new(ProvenanceIntegritySentinel::with_schedule(
        SentinelSchedule::EverySeconds(5),
    )));
    runtime.register(Box::new(SplinterPressureSentinel::new(0.2)));

    assert!(runtime.run_due(&view, 1, start).is_empty());
    let event_report = runtime.run_due(&view, 2, start);
    assert_eq!(event_report.len(), 1);
    assert_eq!(event_report[0].sentinel_name, "contradiction");
    assert_eq!(event_report[0].schedule, SentinelSchedule::EveryEvents(2));
    assert!(runtime.run_due(&view, 2, start).is_empty());
    assert!(runtime
        .run_due(&view, 3, start + Duration::seconds(3))
        .is_empty());
    let time_report = runtime.run_due(&view, 3, start + Duration::seconds(6));
    assert_eq!(time_report.len(), 1);
    assert_eq!(time_report[0].sentinel_name, "provenance_integrity");
    assert_eq!(time_report[0].schedule, SentinelSchedule::EverySeconds(5));
    assert!(runtime
        .run_due(&view, 3, start + Duration::seconds(6))
        .is_empty());
    let skipped = runtime.run_due(&view, 6, start + Duration::seconds(6));
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].sentinel_name, "contradiction");
    assert_eq!(runtime.log().reports().len(), 3);
}

#[test]
fn test_runtime_event_schedule_handles_batched_event_counts() {
    let view = good_view();
    let mut runtime = SentinelRuntime::enabled();
    runtime.register(Box::new(ContradictionSentinel::with_schedule(
        1.0,
        SentinelSchedule::EveryEvents(5),
    )));

    let reports = runtime.run_due(&view, 6, Utc::now());

    assert_eq!(reports.len(), 1);
}

#[test]
fn test_runtime_rejects_duplicate_sentinel_names() {
    let view = good_view();
    let mut runtime = SentinelRuntime::enabled();
    runtime.register(Box::new(ContradictionSentinel::with_schedule(
        1.0,
        SentinelSchedule::EveryEvents(1),
    )));
    runtime.register(Box::new(ContradictionSentinel::with_schedule(
        1.0,
        SentinelSchedule::EveryEvents(1),
    )));

    assert_eq!(runtime.run_due(&view, 1, Utc::now()).len(), 1);
}

#[test]
fn test_runtime_on_demand_is_separate_from_scheduled_polling() {
    let view = good_view();
    let mut runtime = SentinelRuntime::enabled();
    runtime.register(Box::new(ContradictionSentinel::new(1.0)));

    assert!(runtime.run_due(&view, 100, Utc::now()).is_empty());
    assert_eq!(runtime.run_on_demand(&view, 100, Utc::now()).len(), 1);
}

#[test]
fn test_sentinel_contract_metadata_is_declared() {
    let sentinel = ContradictionSentinel::new(1.0);

    assert_eq!(sentinel.name(), "contradiction");
    assert_eq!(sentinel.defect_class(), DefectClass::Contradiction);
    assert_eq!(sentinel.schedule(), SentinelSchedule::OnDemand);
}
