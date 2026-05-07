use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    num::NonZeroUsize,
    path::Path,
    sync::Arc,
    time::Duration,
};

pub trait Gauge {
    fn read(&self) -> f64;
    fn name(&self) -> &'static str;
    fn unit(&self) -> &'static str;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaugeSnapshot {
    pub elapsed_seconds: f64,
    pub raw_event_count: usize,
    pub mutation_event_count: usize,
    pub node_count: usize,
    pub tether_count: usize,
    pub replay_duration_ms: f64,
    pub inspector_rejection_count: usize,
    pub inspector_check_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BaselineStats {
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollingBaseline {
    window_size: NonZeroUsize,
    readings: Vec<f64>,
}

impl RollingBaseline {
    pub fn new(window_size: usize) -> Result<Self, GaugeError> {
        let window_size =
            NonZeroUsize::new(window_size).ok_or(GaugeError::InvalidWindowSize { window_size })?;
        Ok(Self {
            window_size,
            readings: Vec::new(),
        })
    }

    pub fn push(&mut self, reading: f64) {
        self.readings.push(reading);
        let max = self.window_size.get();
        if self.readings.len() > max {
            let overflow = self.readings.len() - max;
            self.readings.drain(0..overflow);
        }
    }

    pub fn stats(&self) -> Option<BaselineStats> {
        let count = self.readings.len();
        if count == 0 {
            return None;
        }
        let mean = self.readings.iter().sum::<f64>() / count as f64;
        let variance = self
            .readings
            .iter()
            .map(|reading| (reading - mean).powi(2))
            .sum::<f64>()
            / count as f64;
        Some(BaselineStats {
            count,
            mean,
            std_dev: variance.sqrt(),
        })
    }

    pub fn readings(&self) -> &[f64] {
        &self.readings
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DriftConfig {
    pub threshold_std_devs: f64,
}

impl DriftConfig {
    pub fn new(threshold_std_devs: f64) -> Self {
        Self {
            threshold_std_devs: threshold_std_devs.max(0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriftDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DriftStatus {
    Stable,
    Drift {
        magnitude: f64,
        direction: DriftDirection,
    },
}

pub fn detect_drift(
    current: f64,
    baseline: Option<BaselineStats>,
    config: DriftConfig,
) -> DriftStatus {
    let Some(baseline) = baseline else {
        return DriftStatus::Stable;
    };
    if baseline.count == 0 {
        return DriftStatus::Stable;
    }

    let delta = current - baseline.mean;
    let direction = if delta >= 0.0 {
        DriftDirection::Up
    } else {
        DriftDirection::Down
    };
    let magnitude = if baseline.std_dev == 0.0 {
        if delta == 0.0 {
            0.0
        } else {
            config.threshold_std_devs + 1.0
        }
    } else {
        (delta / baseline.std_dev).abs()
    };

    if magnitude > config.threshold_std_devs {
        DriftStatus::Drift {
            magnitude,
            direction,
        }
    } else {
        DriftStatus::Stable
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaugeReading {
    pub gauge_name: String,
    pub value: f64,
    pub timestamp: DateTime<Utc>,
    pub baseline_at_read: Option<BaselineStats>,
    pub drift_status: DriftStatus,
}

impl GaugeReading {
    pub fn new(
        gauge_name: impl Into<String>,
        value: f64,
        timestamp: DateTime<Utc>,
        baseline_at_read: Option<BaselineStats>,
        drift_status: DriftStatus,
    ) -> Self {
        Self {
            gauge_name: gauge_name.into(),
            value,
            timestamp,
            baseline_at_read,
            drift_status,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaugeReadingLog {
    readings: Vec<GaugeReading>,
}

impl GaugeReadingLog {
    pub fn append(&mut self, reading: GaugeReading) {
        self.readings.push(reading);
    }

    pub fn readings(&self) -> &[GaugeReading] {
        &self.readings
    }

    pub fn readings_mut_for_tests(&mut self) -> Option<&mut Vec<GaugeReading>> {
        None
    }

    pub fn append_jsonl(path: &Path, reading: &GaugeReading) -> Result<(), GaugeError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| GaugeError::Io {
                message: err.to_string(),
            })?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| GaugeError::Io {
                message: err.to_string(),
            })?;
        let line = serde_json::to_string(reading).map_err(|err| GaugeError::Io {
            message: err.to_string(),
        })?;
        writeln!(file, "{line}").map_err(|err| GaugeError::Io {
            message: err.to_string(),
        })?;
        Ok(())
    }

    pub fn load_jsonl(path: &Path) -> Result<Vec<GaugeReading>, GaugeError> {
        let file = std::fs::File::open(path).map_err(|err| GaugeError::Io {
            message: err.to_string(),
        })?;
        let reader = BufReader::new(file);
        let mut readings = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|err| GaugeError::Io {
                message: err.to_string(),
            })?;
            if line.trim().is_empty() {
                continue;
            }
            readings.push(serde_json::from_str(&line).map_err(|err| GaugeError::Io {
                message: err.to_string(),
            })?);
        }
        Ok(readings)
    }
}

pub struct GaugeRuntime {
    gauges: Vec<Box<dyn Gauge>>,
    baselines: BTreeMap<String, RollingBaseline>,
    log: GaugeReadingLog,
    config: DriftConfig,
    window_size: usize,
    tick_interval: Duration,
}

impl GaugeRuntime {
    pub fn new(config: DriftConfig, window_size: usize) -> Result<Self, GaugeError> {
        RollingBaseline::new(window_size)?;
        Ok(Self {
            gauges: Vec::new(),
            baselines: BTreeMap::new(),
            log: GaugeReadingLog::default(),
            config,
            window_size,
            tick_interval: Duration::from_secs(1),
        })
    }

    pub fn register(&mut self, gauge: Box<dyn Gauge>) {
        if self.baselines.contains_key(gauge.name()) {
            return;
        }
        self.baselines
            .entry(gauge.name().to_string())
            .or_insert_with(|| {
                RollingBaseline::new(self.window_size)
                    .expect("GaugeRuntime validates non-zero window size at construction")
            });
        self.gauges.push(gauge);
    }

    pub fn new_with_interval(
        config: DriftConfig,
        window_size: usize,
        tick_interval: Duration,
    ) -> Result<Self, GaugeError> {
        let mut runtime = Self::new(config, window_size)?;
        runtime.tick_interval = tick_interval;
        Ok(runtime)
    }

    pub fn tick_at(&mut self, timestamp: DateTime<Utc>) -> Result<Vec<GaugeReading>, GaugeError> {
        let mut readings = Vec::new();
        for gauge in &self.gauges {
            let value = gauge.read();
            let baseline = self
                .baselines
                .get(gauge.name())
                .and_then(RollingBaseline::stats);
            let drift_status = detect_drift(value, baseline, self.config);
            let reading = GaugeReading::new(gauge.name(), value, timestamp, baseline, drift_status);
            self.baselines
                .get_mut(gauge.name())
                .expect("registered gauge has a baseline")
                .push(value);
            self.log.append(reading.clone());
            readings.push(reading);
        }
        Ok(readings)
    }

    pub fn log(&self) -> &GaugeReadingLog {
        &self.log
    }

    pub fn baseline(&self, gauge_name: &str) -> Option<&RollingBaseline> {
        self.baselines.get(gauge_name)
    }

    pub fn tick_interval(&self) -> Duration {
        self.tick_interval
    }

    pub fn run_ticks(
        &mut self,
        start: DateTime<Utc>,
        count: usize,
    ) -> Result<Vec<GaugeReading>, GaugeError> {
        let mut readings = Vec::new();
        for index in 0..count {
            let timestamp = start
                + chrono::Duration::from_std(self.tick_interval * index as u32).map_err(|err| {
                    GaugeError::Io {
                        message: err.to_string(),
                    }
                })?;
            readings.extend(self.tick_at(timestamp)?);
        }
        Ok(readings)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThresholdSuggestion {
    pub gauge_name: String,
    pub sample_count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub suggested_threshold_std_devs: f64,
}

pub fn calibrate_thresholds(
    readings: &[GaugeReading],
    multiplier: f64,
) -> Vec<ThresholdSuggestion> {
    let mut by_gauge: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for reading in readings {
        by_gauge
            .entry(reading.gauge_name.clone())
            .or_default()
            .push(reading.value);
    }

    by_gauge
        .into_iter()
        .filter_map(|(gauge_name, values)| {
            let mut baseline = RollingBaseline::new(values.len()).ok()?;
            for value in values {
                baseline.push(value);
            }
            let stats = baseline.stats()?;
            Some(ThresholdSuggestion {
                gauge_name,
                sample_count: stats.count,
                mean: stats.mean,
                std_dev: stats.std_dev,
                suggested_threshold_std_devs: multiplier.max(0.0),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GaugeError {
    InvalidWindowSize { window_size: usize },
    Io { message: String },
}

impl std::fmt::Display for GaugeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWindowSize { window_size } => {
                write!(
                    f,
                    "gauge baseline window size must be non-zero, got {window_size}"
                )
            }
            Self::Io { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for GaugeError {}

macro_rules! snapshot_gauge {
    ($name:ident, $gauge_name:literal, $unit:literal, $body:expr) => {
        #[derive(Clone)]
        pub struct $name {
            source: Arc<dyn Fn() -> GaugeSnapshot + Send + Sync>,
        }

        impl $name {
            pub fn new(snapshot: GaugeSnapshot) -> Self {
                Self {
                    source: Arc::new(move || snapshot.clone()),
                }
            }

            pub fn from_source(source: Arc<dyn Fn() -> GaugeSnapshot + Send + Sync>) -> Self {
                Self { source }
            }
        }

        impl Gauge for $name {
            fn read(&self) -> f64 {
                let snapshot = &(self.source)();
                $body(snapshot)
            }

            fn name(&self) -> &'static str {
                $gauge_name
            }

            fn unit(&self) -> &'static str {
                $unit
            }
        }
    };
}

snapshot_gauge!(
    EventsPerSecondGauge,
    "events_per_second",
    "events/second",
    |snapshot: &GaugeSnapshot| {
        rate(
            snapshot.raw_event_count + snapshot.mutation_event_count,
            snapshot.elapsed_seconds,
        )
    }
);

snapshot_gauge!(
    MutationEventsPerSecondGauge,
    "mutation_events_per_second",
    "mutations/second",
    |snapshot: &GaugeSnapshot| rate(snapshot.mutation_event_count, snapshot.elapsed_seconds)
);

snapshot_gauge!(
    AverageTetherFanOutGauge,
    "average_tether_fan_out",
    "tethers/node",
    |snapshot: &GaugeSnapshot| {
        if snapshot.node_count == 0 {
            0.0
        } else {
            snapshot.tether_count as f64 / snapshot.node_count as f64
        }
    }
);

snapshot_gauge!(
    ReplayDurationGauge,
    "replay_duration_per_thousand_events",
    "milliseconds/1000_events",
    |snapshot: &GaugeSnapshot| {
        let event_count = snapshot.raw_event_count + snapshot.mutation_event_count;
        if event_count == 0 {
            0.0
        } else {
            snapshot.replay_duration_ms / event_count as f64 * 1000.0
        }
    }
);

snapshot_gauge!(
    InspectorRejectionRateGauge,
    "inspector_rejection_rate",
    "rejections/check",
    |snapshot: &GaugeSnapshot| {
        if snapshot.inspector_check_count == 0 {
            0.0
        } else {
            snapshot.inspector_rejection_count as f64 / snapshot.inspector_check_count as f64
        }
    }
);

fn rate(count: usize, elapsed_seconds: f64) -> f64 {
    if elapsed_seconds <= 0.0 {
        0.0
    } else {
        count as f64 / elapsed_seconds
    }
}
