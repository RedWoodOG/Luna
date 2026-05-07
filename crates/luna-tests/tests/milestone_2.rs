use chrono::Utc;
use luna_gauges::{
    calibrate_thresholds, AverageTetherFanOutGauge, BaselineStats, DriftConfig, DriftDirection,
    DriftStatus, EventsPerSecondGauge, Gauge, GaugeReading, GaugeReadingLog, GaugeRuntime,
    GaugeSnapshot, InspectorRejectionRateGauge, MutationEventsPerSecondGauge, ReplayDurationGauge,
    RollingBaseline,
};

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

    let first: Vec<f64> = gauges.iter().map(|gauge| gauge.read()).collect();
    let second: Vec<f64> = gauges.iter().map(|gauge| gauge.read()).collect();

    assert_eq!(first, second);
    assert_eq!(gauges[0].name(), "events_per_second");
    assert_eq!(gauges[0].unit(), "events/second");
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
fn test_gauge_reading_log_is_append_only() {
    let mut log = GaugeReadingLog::default();
    let reading = GaugeReading {
        gauge_name: "events_per_second".to_string(),
        value: 1.0,
        timestamp: Utc::now(),
        baseline_at_read: None,
        drift_status: DriftStatus::Stable,
    };

    log.append(reading.clone());
    log.append(reading);

    assert_eq!(log.readings().len(), 2);
    assert!(log.readings_mut_for_tests().is_none());
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
    assert_eq!(
        runtime.baseline("events_per_second").unwrap().readings(),
        &[2.0, 2.0]
    );
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
    assert!(suggestions[0].suggested_threshold > 0.0);
    serde_json::to_string_pretty(&suggestions).unwrap();
}
