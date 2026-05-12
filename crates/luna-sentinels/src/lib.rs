use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    path::Path,
};

pub trait Sentinel: Send + Sync {
    fn name(&self) -> &'static str;
    fn defect_class(&self) -> DefectClass;
    fn evaluate(&self, topology_view: &TopologyView) -> SentinelEvaluation;
    fn schedule(&self) -> SentinelSchedule;

    fn report(&self, topology_view: &TopologyView, timestamp: DateTime<Utc>) -> SentinelReport {
        match self.evaluate(topology_view) {
            SentinelEvaluation::Quiet => {
                SentinelReport::quiet(self.name(), self.defect_class(), self.schedule(), timestamp)
            }
            SentinelEvaluation::Flag {
                score,
                evidence,
                recommendation,
            } => SentinelReport {
                sentinel_name: self.name().to_string(),
                defect_class: self.defect_class(),
                score,
                evidence,
                recommendation,
                timestamp,
                schedule: self.schedule(),
                topology_event_count: None,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DefectClass {
    Contradiction,
    ProvenanceIntegrity,
    SplinterPressure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SentinelSchedule {
    OnDemand,
    EveryEvents(u64),
    EverySeconds(u64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SentinelEvaluation {
    Quiet,
    Flag {
        score: f64,
        evidence: Vec<String>,
        recommendation: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SentinelReport {
    pub sentinel_name: String,
    pub defect_class: DefectClass,
    pub score: f64,
    pub evidence: Vec<String>,
    pub recommendation: String,
    pub timestamp: DateTime<Utc>,
    pub schedule: SentinelSchedule,
    pub topology_event_count: Option<u64>,
}

impl SentinelReport {
    fn quiet(
        sentinel_name: &str,
        defect_class: DefectClass,
        schedule: SentinelSchedule,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            sentinel_name: sentinel_name.to_string(),
            defect_class,
            score: 0.0,
            evidence: Vec::new(),
            recommendation: "no defect observed".to_string(),
            timestamp,
            schedule,
            topology_event_count: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SentinelReportLog {
    reports: Vec<SentinelReport>,
}

impl SentinelReportLog {
    pub fn append(&mut self, report: SentinelReport) {
        self.reports.push(report);
    }

    pub fn reports(&self) -> &[SentinelReport] {
        &self.reports
    }

    pub fn reports_mut_for_tests(&mut self) -> Option<&mut Vec<SentinelReport>> {
        None
    }

    pub fn append_jsonl(path: &Path, report: &SentinelReport) -> Result<(), SentinelError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| SentinelError::Io {
                message: err.to_string(),
            })?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| SentinelError::Io {
                message: err.to_string(),
            })?;
        let line = serde_json::to_string(report).map_err(|err| SentinelError::Io {
            message: err.to_string(),
        })?;
        writeln!(file, "{line}").map_err(|err| SentinelError::Io {
            message: err.to_string(),
        })?;
        Ok(())
    }

    pub fn load_jsonl(path: &Path) -> Result<Vec<SentinelReport>, SentinelError> {
        let file = std::fs::File::open(path).map_err(|err| SentinelError::Io {
            message: err.to_string(),
        })?;
        let reader = BufReader::new(file);
        let mut reports = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|err| SentinelError::Io {
                message: err.to_string(),
            })?;
            if line.trim().is_empty() {
                continue;
            }
            reports.push(
                serde_json::from_str(&line).map_err(|err| SentinelError::Io {
                    message: err.to_string(),
                })?,
            );
        }
        Ok(reports)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SentinelError {
    Io { message: String },
}

impl std::fmt::Display for SentinelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SentinelError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewRawEvent {
    event_id: String,
    hash: String,
}

impl ViewRawEvent {
    pub fn new(event_id: impl Into<String>, hash: impl Into<String>) -> Self {
        Self {
            event_id: event_id.into(),
            hash: hash.into(),
        }
    }

    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewNode {
    node_id: String,
    source_event_id: String,
    source_event_hash: String,
}

impl ViewNode {
    pub fn new(
        node_id: impl Into<String>,
        source_event_id: impl Into<String>,
        source_event_hash: impl Into<String>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            source_event_id: source_event_id.into(),
            source_event_hash: source_event_hash.into(),
        }
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewGenesisCertificate {
    certificate_id: String,
    node_id: String,
    source_event_id: String,
    source_event_hash: String,
}

impl ViewGenesisCertificate {
    pub fn new(
        certificate_id: impl Into<String>,
        node_id: impl Into<String>,
        source_event_id: impl Into<String>,
        source_event_hash: impl Into<String>,
    ) -> Self {
        Self {
            certificate_id: certificate_id.into(),
            node_id: node_id.into(),
            source_event_id: source_event_id.into(),
            source_event_hash: source_event_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewAssertion {
    subject_node_id: String,
    predicate: String,
    value: String,
    single_valued: bool,
}

impl ViewAssertion {
    pub fn new(
        subject_node_id: impl Into<String>,
        predicate: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            subject_node_id: subject_node_id.into(),
            predicate: predicate.into(),
            value: value.into(),
            single_valued: true,
        }
    }

    pub fn multi_valued(
        subject_node_id: impl Into<String>,
        predicate: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            subject_node_id: subject_node_id.into(),
            predicate: predicate.into(),
            value: value.into(),
            single_valued: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewTether {
    tether_id: String,
    source_node_id: String,
    target_node_id: String,
    source_event_id: String,
    source_event_hash: String,
    assertion: Option<ViewAssertion>,
}

impl ViewTether {
    pub fn new(
        tether_id: impl Into<String>,
        source_node_id: impl Into<String>,
        target_node_id: impl Into<String>,
        source_event_id: impl Into<String>,
        source_event_hash: impl Into<String>,
        assertion: Option<ViewAssertion>,
    ) -> Self {
        Self {
            tether_id: tether_id.into(),
            source_node_id: source_node_id.into(),
            target_node_id: target_node_id.into(),
            source_event_id: source_event_id.into(),
            source_event_hash: source_event_hash.into(),
            assertion,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewOrb {
    orb_id: String,
    density_history: Vec<f64>,
    precision_history: Vec<f64>,
}

impl ViewOrb {
    pub fn new(
        orb_id: impl Into<String>,
        density_history: Vec<f64>,
        precision_history: Vec<f64>,
    ) -> Self {
        Self {
            orb_id: orb_id.into(),
            density_history,
            precision_history,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyView {
    raw_events: Vec<ViewRawEvent>,
    nodes: Vec<ViewNode>,
    genesis_certificates: Vec<ViewGenesisCertificate>,
    tethers: Vec<ViewTether>,
    orbs: Vec<ViewOrb>,
}

impl TopologyView {
    pub fn new(
        raw_events: Vec<ViewRawEvent>,
        nodes: Vec<ViewNode>,
        genesis_certificates: Vec<ViewGenesisCertificate>,
        tethers: Vec<ViewTether>,
        orbs: Vec<ViewOrb>,
    ) -> Self {
        Self {
            raw_events,
            nodes,
            genesis_certificates,
            tethers,
            orbs,
        }
    }

    pub fn raw_events(&self) -> &[ViewRawEvent] {
        &self.raw_events
    }

    pub fn nodes(&self) -> &[ViewNode] {
        &self.nodes
    }

    pub fn genesis_certificates(&self) -> &[ViewGenesisCertificate] {
        &self.genesis_certificates
    }

    pub fn tethers(&self) -> &[ViewTether] {
        &self.tethers
    }

    pub fn orbs(&self) -> &[ViewOrb] {
        &self.orbs
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContradictionSentinel {
    threshold: f64,
    schedule: SentinelSchedule,
}

impl ContradictionSentinel {
    pub fn new(threshold: f64) -> Self {
        Self::with_schedule(threshold, SentinelSchedule::OnDemand)
    }

    pub fn with_schedule(threshold: f64, schedule: SentinelSchedule) -> Self {
        Self {
            threshold,
            schedule,
        }
    }
}

impl Sentinel for ContradictionSentinel {
    fn name(&self) -> &'static str {
        "contradiction"
    }

    fn defect_class(&self) -> DefectClass {
        DefectClass::Contradiction
    }

    fn evaluate(&self, topology_view: &TopologyView) -> SentinelEvaluation {
        let mut by_key: BTreeMap<(String, String), BTreeMap<String, Vec<String>>> = BTreeMap::new();
        for tether in topology_view.tethers() {
            if let Some(assertion) = &tether.assertion {
                if !assertion.single_valued {
                    continue;
                }
                by_key
                    .entry((
                        assertion.subject_node_id.clone(),
                        assertion.predicate.clone(),
                    ))
                    .or_default()
                    .entry(assertion.value.clone())
                    .or_default()
                    .push(tether.tether_id.clone());
            }
        }

        let mut evidence = BTreeSet::new();
        for values in by_key.values() {
            if values.len() > 1 {
                for tether_ids in values.values() {
                    evidence.extend(tether_ids.iter().cloned());
                }
            }
        }

        let score = evidence.len() as f64;
        if score >= self.threshold && score > 0.0 {
            SentinelEvaluation::Flag {
                score,
                evidence: evidence.into_iter().collect(),
                recommendation: "review contradiction evidence before topology compression"
                    .to_string(),
            }
        } else {
            SentinelEvaluation::Quiet
        }
    }

    fn schedule(&self) -> SentinelSchedule {
        self.schedule
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceIntegritySentinel {
    schedule: SentinelSchedule,
}

impl ProvenanceIntegritySentinel {
    pub fn new() -> Self {
        Self::with_schedule(SentinelSchedule::OnDemand)
    }

    pub fn with_schedule(schedule: SentinelSchedule) -> Self {
        Self { schedule }
    }
}

impl Default for ProvenanceIntegritySentinel {
    fn default() -> Self {
        Self::new()
    }
}

impl Sentinel for ProvenanceIntegritySentinel {
    fn name(&self) -> &'static str {
        "provenance_integrity"
    }

    fn defect_class(&self) -> DefectClass {
        DefectClass::ProvenanceIntegrity
    }

    fn evaluate(&self, topology_view: &TopologyView) -> SentinelEvaluation {
        let events = topology_view
            .raw_events()
            .iter()
            .map(|event| (event.event_id.as_str(), event.hash.as_str()))
            .collect::<BTreeMap<_, _>>();
        let nodes = topology_view
            .nodes()
            .iter()
            .map(|node| (node.node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let mut evidence = Vec::new();

        for node in topology_view.nodes() {
            if events.get(node.source_event_id.as_str()).copied()
                != Some(node.source_event_hash.as_str())
            {
                evidence.push(format!(
                    "node:{}->event:{}",
                    node.node_id, node.source_event_id
                ));
            }
        }

        for certificate in topology_view.genesis_certificates() {
            match nodes.get(certificate.node_id.as_str()) {
                Some(node)
                    if node.source_event_id == certificate.source_event_id
                        && node.source_event_hash == certificate.source_event_hash => {}
                _ => evidence.push(format!(
                    "certificate:{}->node:{}",
                    certificate.certificate_id, certificate.node_id
                )),
            }
        }

        for tether in topology_view.tethers() {
            if !nodes.contains_key(tether.source_node_id.as_str()) {
                evidence.push(format!(
                    "tether:{}->source_node:{}",
                    tether.tether_id, tether.source_node_id
                ));
            }
            if !nodes.contains_key(tether.target_node_id.as_str()) {
                evidence.push(format!(
                    "tether:{}->target_node:{}",
                    tether.tether_id, tether.target_node_id
                ));
            }
            if events.get(tether.source_event_id.as_str()).copied()
                != Some(tether.source_event_hash.as_str())
            {
                evidence.push(format!(
                    "tether:{}->event:{}",
                    tether.tether_id, tether.source_event_id
                ));
            }
        }

        if evidence.is_empty() {
            SentinelEvaluation::Quiet
        } else {
            SentinelEvaluation::Flag {
                score: evidence.len() as f64,
                evidence,
                recommendation: "quarantine derived topology and inspect provenance chain"
                    .to_string(),
            }
        }
    }

    fn schedule(&self) -> SentinelSchedule {
        self.schedule
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplinterPressureSentinel {
    threshold: f64,
    schedule: SentinelSchedule,
}

impl SplinterPressureSentinel {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            schedule: SentinelSchedule::OnDemand,
        }
    }
}

impl Sentinel for SplinterPressureSentinel {
    fn name(&self) -> &'static str {
        "splinter_pressure"
    }

    fn defect_class(&self) -> DefectClass {
        DefectClass::SplinterPressure
    }

    fn evaluate(&self, topology_view: &TopologyView) -> SentinelEvaluation {
        let mut evidence = Vec::new();
        let mut total_score = 0.0;
        for orb in topology_view.orbs() {
            if orb.density_history.len() < 2 || orb.precision_history.len() < 2 {
                continue;
            }
            let density_delta = orb.density_history[orb.density_history.len() - 1]
                - orb.density_history[orb.density_history.len() - 2];
            let precision_delta = orb.precision_history[orb.precision_history.len() - 1]
                - orb.precision_history[orb.precision_history.len() - 2];
            if density_delta > 0.0 && precision_delta < 0.0 {
                let score = density_delta + precision_delta.abs();
                total_score += score;
                evidence.push(format!(
                    "orb:{} density_delta={:.3} precision_delta={:.3}",
                    orb.orb_id, density_delta, precision_delta
                ));
            }
        }

        if evidence.is_empty() || total_score < self.threshold {
            SentinelEvaluation::Quiet
        } else {
            SentinelEvaluation::Flag {
                score: total_score,
                evidence,
                recommendation: "increase splinter pressure and inspect orb membership".to_string(),
            }
        }
    }

    fn schedule(&self) -> SentinelSchedule {
        self.schedule
    }
}

#[derive(Default)]
pub struct SentinelRuntime {
    enabled: bool,
    sentinels: Vec<Box<dyn Sentinel>>,
    log: SentinelReportLog,
    last_event_run: BTreeMap<String, u64>,
    last_time_run: BTreeMap<String, DateTime<Utc>>,
}

impl SentinelRuntime {
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Self::default()
        }
    }

    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn register(&mut self, sentinel: Box<dyn Sentinel>) {
        if self
            .sentinels
            .iter()
            .any(|existing| existing.name() == sentinel.name())
        {
            return;
        }
        self.sentinels.push(sentinel);
    }

    pub fn run_on_demand(
        &mut self,
        topology_view: &TopologyView,
        event_count: u64,
        now: DateTime<Utc>,
    ) -> Vec<SentinelReport> {
        if !self.enabled {
            return Vec::new();
        }

        let mut reports = Vec::new();
        for sentinel in &self.sentinels {
            if !matches!(sentinel.schedule(), SentinelSchedule::OnDemand) {
                continue;
            }
            let mut report = sentinel.report(topology_view, now);
            report.topology_event_count = Some(event_count);
            self.log.append(report.clone());
            reports.push(report);
        }
        reports
    }

    pub fn run_due(
        &mut self,
        topology_view: &TopologyView,
        event_count: u64,
        now: DateTime<Utc>,
    ) -> Vec<SentinelReport> {
        if !self.enabled {
            return Vec::new();
        }

        let mut reports = Vec::new();
        for index in 0..self.sentinels.len() {
            let sentinel = self.sentinels[index].as_ref();
            if matches!(sentinel.schedule(), SentinelSchedule::OnDemand) {
                continue;
            }
            if matches!(sentinel.schedule(), SentinelSchedule::EverySeconds(_))
                && !self.last_time_run.contains_key(sentinel.name())
            {
                self.last_time_run.insert(sentinel.name().to_string(), now);
                continue;
            }
            if !self.is_due(sentinel, event_count, now) {
                continue;
            }
            let mut report = sentinel.report(topology_view, now);
            report.topology_event_count = Some(event_count);
            self.mark_run(sentinel.name(), sentinel.schedule(), event_count, now);
            self.log.append(report.clone());
            reports.push(report);
        }
        reports
    }

    pub fn log(&self) -> &SentinelReportLog {
        &self.log
    }

    fn is_due(&self, sentinel: &dyn Sentinel, event_count: u64, now: DateTime<Utc>) -> bool {
        match sentinel.schedule() {
            SentinelSchedule::OnDemand => true,
            SentinelSchedule::EveryEvents(interval) => {
                interval > 0
                    && event_count > 0
                    && event_count.saturating_sub(
                        self.last_event_run
                            .get(sentinel.name())
                            .copied()
                            .unwrap_or_default(),
                    ) >= interval
            }
            SentinelSchedule::EverySeconds(interval) => self
                .last_time_run
                .get(sentinel.name())
                .map(|last| now.signed_duration_since(*last).num_seconds() >= interval as i64)
                .unwrap_or(false),
        }
    }

    fn mark_run(
        &mut self,
        sentinel_name: &str,
        schedule: SentinelSchedule,
        event_count: u64,
        now: DateTime<Utc>,
    ) {
        match schedule {
            SentinelSchedule::OnDemand => {}
            SentinelSchedule::EveryEvents(_) => {
                self.last_event_run
                    .insert(sentinel_name.to_string(), event_count);
            }
            SentinelSchedule::EverySeconds(_) => {
                self.last_time_run.insert(sentinel_name.to_string(), now);
            }
        }
    }
}
