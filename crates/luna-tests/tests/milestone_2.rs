use chrono::Utc;
use luna_gauges::{
    calibrate_thresholds, AverageTetherFanOutGauge, BaselineStats, DriftConfig, DriftDirection,
    DriftStatus, EventsPerSecondGauge, Gauge, GaugeReading, GaugeReadingLog, GaugeRuntime,
    GaugeSnapshot, InspectorRejectionRateGauge, MutationEventsPerSecondGauge, ReplayDurationGauge,
    RollingBaseline,
};
use std::sync::{Arc, Mutex};

#[test]
fn test_gauge_trait_produces_stable_readings_on_stable_input() {
    let snapshot = GaugeSnapshot {
        elapsed_seconds: 2.0,
        raw_event_count: 10,
        mutation_event_count: 6,
        node_count: 3,
        tether_count: 6,
        replay_duration_ms: 40.0,
        inspector_rejection_count: 2,
        inspector_check_count: 10,
    };
    let gauges: Vec<Box<dyn Gauge>> = vec![
        Box::new(EventsPerSecondGauge::new(snapshot.clone())),
        Box::new(MutationEventsPerSecondGauge::new(snapshot.clone())),
        Box::new(AverageTetherFanOutGauge::new(snapshot.clone())),
        Box::new(ReplayDurationGauge::new(snapshot.clone())),
        Box::new(InspectorRejectionRateGauge::new(snapshot)),
    ];

    let first: Vec<(&str, &str, f64)> = gauges
        .iter()
        .map(|gauge| (gauge.name(), gauge.unit(), gauge.read()))
        .collect();
    let second: Vec<f64> = gauges.iter().map(|gauge| gauge.read()).collect();

    assert_eq!(
        first,
        vec![
            ("events_per_second", "events/second", 8.0),
            ("mutation_events_per_second", "mutations/second", 3.0),
            ("average_tether_fan_out", "tethers/node", 2.0),
            (
                "replay_duration_per_thousand_events",
                "milliseconds/1000_events",
                2500.0,
            ),
            ("inspector_rejection_rate", "rejections/check", 0.2),
        ]
    );
    assert_eq!(
        second,
        first.iter().map(|(_, _, value)| *value).collect::<Vec<_>>()
    );
}

#[test]
fn test_dynamic_gauge_reads_fresh_snapshot_each_tick() {
    let snapshot = Arc::new(Mutex::new(GaugeSnapshot {
        elapsed_seconds: 1.0,
        raw_event_count: 1,
        mutation_event_count: 0,
        node_count: 0,
        tether_count: 0,
        replay_duration_ms: 0.0,
        inspector_rejection_count: 0,
        inspector_check_count: 0,
    }));
    let source = {
        let snapshot = Arc::clone(&snapshot);
        Arc::new(move || snapshot.lock().unwrap().clone())
    };
    let gauge = EventsPerSecondGauge::from_source(source);

    assert_eq!(gauge.read(), 1.0);
    snapshot.lock().unwrap().mutation_event_count = 4;
    assert_eq!(gauge.read(), 5.0);
}

#[test]
fn test_rolling_baseline_updates_across_window() {
    let mut baseline = RollingBaseline::new(3).unwrap();
    baseline.push(1.0);
    baseline.push(2.0);
    baseline.push(3.0);
    baseline.push(4.0);

    let stats = baseline.stats().unwrap();

    assert_eq!(baseline.readings(), &[2.0, 3.0, 4.0]);
    assert_eq!(stats.mean, 3.0);
    assert!((stats.std_dev - 0.816_496_58).abs() < 0.000_001);
}

#[test]
fn test_drift_detector_fires_beyond_threshold() {
    let baseline = BaselineStats {
        count: 4,
        mean: 10.0,
        std_dev: 2.0,
    };
    let config = DriftConfig::new(2.0);

    let stable = luna_gauges::detect_drift(13.0, Some(baseline), config);
    let drift = luna_gauges::detect_drift(15.0, Some(baseline), config);

    assert_eq!(stable, DriftStatus::Stable);
    assert_eq!(
        drift,
        DriftStatus::Drift {
            magnitude: 2.5,
            direction: DriftDirection::Up
        }
    );
}

#[test]
fn test_drift_detector_edge_cases_are_finite() {
    let config = DriftConfig::new(2.0);

    assert_eq!(
        luna_gauges::detect_drift(10.0, None, config),
        DriftStatus::Stable
    );
    assert_eq!(
        luna_gauges::detect_drift(
            10.0,
            Some(BaselineStats {
                count: 0,
                mean: 0.0,
                std_dev: 1.0,
            }),
            config,
        ),
        DriftStatus::Stable
    );
    assert_eq!(
        luna_gauges::detect_drift(
            10.0,
            Some(BaselineStats {
                count: 3,
                mean: 10.0,
                std_dev: 0.0,
            }),
            config,
        ),
        DriftStatus::Stable
    );
    let changed = luna_gauges::detect_drift(
        11.0,
        Some(BaselineStats {
            count: 3,
            mean: 10.0,
            std_dev: 0.0,
        }),
        config,
    );

    match changed {
        DriftStatus::Drift {
            magnitude,
            direction,
        } => {
            assert_eq!(direction, DriftDirection::Up);
            assert!(magnitude.is_finite());
        }
        DriftStatus::Stable => panic!("expected drift for zero-variance change"),
    }
    assert_eq!(
        luna_gauges::detect_drift(
            6.0,
            Some(BaselineStats {
                count: 4,
                mean: 10.0,
                std_dev: 1.0,
            }),
            config,
        ),
        DriftStatus::Drift {
            magnitude: 4.0,
            direction: DriftDirection::Down,
        }
    );
    assert_eq!(
        luna_gauges::detect_drift(
            12.0,
            Some(BaselineStats {
                count: 4,
                mean: 10.0,
                std_dev: 1.0,
            }),
            config,
        ),
        DriftStatus::Stable
    );
}

#[test]
fn test_gauge_reading_log_is_append_only() {
    let mut log = GaugeReadingLog::default();
    let reading = GaugeReading {
        gauge_name: "events_per_second".to_string(),
        value: 1.0,
        timestamp: Utc::now(),
        baseline_at_read: None,
        drift_status: DriftStatus::Stable,
    };
    let second = GaugeReading {
        gauge_name: "mutation_events_per_second".to_string(),
        value: 2.0,
        timestamp: Utc::now(),
        baseline_at_read: Some(BaselineStats {
            count: 1,
            mean: 1.0,
            std_dev: 0.0,
        }),
        drift_status: DriftStatus::Stable,
    };

    log.append(reading.clone());
    log.append(second.clone());

    assert_eq!(log.readings().len(), 2);
    assert_eq!(log.readings(), &[reading, second]);
    assert!(log.readings_mut_for_tests().is_none());
}

#[test]
fn test_gauge_reading_jsonl_round_trips() {
    let path = std::env::temp_dir().join(format!(
        "luna-gauge-readings-{}.jsonl",
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    let reading = GaugeReading::new(
        "events_per_second",
        1.0,
        Utc::now(),
        None,
        DriftStatus::Stable,
    );

    GaugeReadingLog::append_jsonl(&path, &reading).unwrap();
    GaugeReadingLog::append_jsonl(&path, &reading).unwrap();
    let loaded = GaugeReadingLog::load_jsonl(&path).unwrap();

    assert_eq!(loaded, vec![reading.clone(), reading]);
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_runtime_functions_with_all_gauges_disabled() {
    let mut runtime = GaugeRuntime::new(DriftConfig::new(2.0), 5).unwrap();
    let tick = runtime.tick_at(Utc::now()).unwrap();

    assert!(tick.is_empty());
    assert!(runtime.log().readings().is_empty());
}

#[test]
fn test_runtime_ticks_update_baselines_and_append_readings() {
    let snapshot = GaugeSnapshot {
        elapsed_seconds: 1.0,
        raw_event_count: 2,
        mutation_event_count: 1,
        node_count: 2,
        tether_count: 1,
        replay_duration_ms: 4.0,
        inspector_rejection_count: 0,
        inspector_check_count: 1,
    };
    let mut runtime = GaugeRuntime::new(DriftConfig::new(2.0), 3).unwrap();
    runtime.register(Box::new(EventsPerSecondGauge::new(snapshot)));

    let first = runtime.tick_at(Utc::now()).unwrap();
    let second = runtime.tick_at(Utc::now()).unwrap();

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(runtime.log().readings().len(), 2);
    assert_eq!(second[0].baseline_at_read.unwrap().mean, 3.0);
    assert_eq!(second[0].drift_status, DriftStatus::Stable);
    assert_eq!(
        runtime.baseline("events_per_second").unwrap().readings(),
        &[3.0, 3.0]
    );
}

#[test]
fn test_runtime_rejects_duplicate_gauge_names() {
    let snapshot = GaugeSnapshot {
        elapsed_seconds: 1.0,
        raw_event_count: 1,
        mutation_event_count: 0,
        node_count: 0,
        tether_count: 0,
        replay_duration_ms: 0.0,
        inspector_rejection_count: 0,
        inspector_check_count: 0,
    };
    let mut runtime = GaugeRuntime::new(DriftConfig::new(2.0), 3).unwrap();
    runtime.register(Box::new(EventsPerSecondGauge::new(snapshot.clone())));
    runtime.register(Box::new(EventsPerSecondGauge::new(snapshot)));

    let readings = runtime.tick_at(Utc::now()).unwrap();

    assert_eq!(readings.len(), 1);
}

#[test]
fn test_calibration_suggests_threshold_from_historical_variance() {
    let now = Utc::now();
    let readings = vec![
        GaugeReading::new("events_per_second", 10.0, now, None, DriftStatus::Stable),
        GaugeReading::new("events_per_second", 12.0, now, None, DriftStatus::Stable),
        GaugeReading::new("events_per_second", 14.0, now, None, DriftStatus::Stable),
    ];

    let suggestions = calibrate_thresholds(&readings, 3.0);

    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].gauge_name, "events_per_second");
    assert_eq!(suggestions[0].sample_count, 3);
    assert_eq!(suggestions[0].mean, 12.0);
    assert!((suggestions[0].std_dev - 1.632_993_16).abs() < 0.000_001);
    assert_eq!(suggestions[0].suggested_threshold_std_devs, 3.0);
    serde_json::to_string_pretty(&suggestions).unwrap();
}
