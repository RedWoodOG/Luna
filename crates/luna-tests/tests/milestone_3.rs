use chrono::{Duration, Utc};
use luna_sentinels::{
    ContradictionSentinel, DefectClass, ProvenanceIntegritySentinel, Sentinel,
    SentinelEvaluation, SentinelReportLog, SentinelRuntime, SentinelSchedule,
    SplinterPressureSentinel, TopologyView, ViewAssertion, ViewGenesisCertificate, ViewNode,
    ViewOrb, ViewRawEvent, ViewTether,
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
            assert_eq!(evidence, vec!["tether-1".to_string(), "tether-2".to_string()]);
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
            assert_eq!(score, 1.0);
            assert_eq!(evidence, vec!["node:node-1->event:event-1".to_string()]);
            assert!(recommendation.contains("provenance"));
        }
        SentinelEvaluation::Quiet => panic!("expected provenance flag"),
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
            assert_eq!(evidence, vec!["orb:orb-1 density_delta=0.400 precision_delta=-0.400"]);
            assert!(recommendation.contains("splinter"));
        }
        SentinelEvaluation::Quiet => panic!("expected splinter pressure flag"),
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
fn test_runtime_functions_with_all_sentinels_disabled() {
    let view = good_view();
    let mut runtime = SentinelRuntime::disabled();

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

    assert!(runtime.run_due(&view, 1, start).is_empty());
    assert_eq!(runtime.run_due(&view, 2, start).len(), 1);
    assert!(runtime.run_due(&view, 3, start + Duration::seconds(3)).is_empty());
    assert_eq!(runtime.run_due(&view, 3, start + Duration::seconds(6)).len(), 1);
}

#[test]
fn test_sentinel_contract_metadata_is_declared() {
    let sentinel = ContradictionSentinel::new(1.0);

    assert_eq!(sentinel.name(), "contradiction");
    assert_eq!(sentinel.defect_class(), DefectClass::Contradiction);
    assert_eq!(sentinel.schedule(), SentinelSchedule::OnDemand);
}
