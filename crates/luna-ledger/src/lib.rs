use chrono::{DateTime, Utc};
use luna_core::{LunaError, Result};

use luna_cluster::{
    compression_output_hash, ClusterEvolutionEvent, CompressionReceipt, ConsolidationEvent,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const RAW_EVENT_HASH_VERSION: &str = "luna.raw_event.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum EventPayload {
    Text(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Event,
    Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TetherKind {
    SupportedBy,
    EvidenceFor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEventDraft {
    pub id: String,
    pub source: EventSource,
    pub payload: EventPayload,
}

impl RawEventDraft {
    pub fn new(id: impl Into<String>, source: EventSource, payload: EventPayload) -> Self {
        Self {
            id: id.into(),
            source,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEvent {
    pub id: String,
    pub recorded_at: DateTime<Utc>,
    pub source: EventSource,
    pub payload: EventPayload,
    pub hash: String,
}

impl RawEvent {
    pub fn from_draft(draft: RawEventDraft) -> Self {
        let hash = stable_event_hash(&draft.id, &draft.source, &draft.payload)
            .expect("serializing supported raw event payload should not fail");
        Self {
            id: draft.id,
            recorded_at: Utc::now(),
            source: draft.source,
            payload: draft.payload,
            hash,
        }
    }

    pub fn verify_hash(&self) -> Result<()> {
        let recomputed = stable_event_hash(&self.id, &self.source, &self.payload)?;
        if recomputed == self.hash {
            Ok(())
        } else {
            Err(LunaError::new(format!(
                "raw event {} hash mismatch: stored {}, recomputed {}",
                self.id, self.hash, recomputed
            )))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressionArtifact {
    artifact_ref: String,
    output_hash: String,
    byte_len: usize,
}

impl CompressionArtifact {
    pub fn from_bytes(artifact_ref: impl Into<String>, output_bytes: &[u8]) -> Self {
        Self {
            artifact_ref: artifact_ref.into(),
            output_hash: compression_output_hash(output_bytes),
            byte_len: output_bytes.len(),
        }
    }

    pub fn artifact_ref(&self) -> &str {
        &self.artifact_ref
    }

    pub fn output_hash(&self) -> &str {
        &self.output_hash
    }

    pub fn byte_len(&self) -> usize {
        self.byte_len
    }

    pub fn verify(&self) -> Result<()> {
        if self.artifact_ref.trim().is_empty() {
            return Err(LunaError::new("compression artifact ref is required"));
        }
        if self.byte_len == 0 {
            return Err(LunaError::new(
                "compression artifact must bind non-empty bytes",
            ));
        }
        if !is_hex_hash(&self.output_hash) {
            return Err(LunaError::new("invalid compression artifact output hash"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeCreated {
    pub node_id: String,
    pub kind: NodeKind,
    pub label: String,
    pub source_event_id: String,
    pub source_event_hash: String,
}

impl NodeCreated {
    pub fn new(
        node_id: impl Into<String>,
        kind: NodeKind,
        label: impl Into<String>,
        source_event_id: impl Into<String>,
        source_event_hash: impl Into<String>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            kind,
            label: label.into(),
            source_event_id: source_event_id.into(),
            source_event_hash: source_event_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenesisAttached {
    pub certificate_id: String,
    pub node_id: String,
    pub source_event_id: String,
    pub source_event_hash: String,
    pub created_at: DateTime<Utc>,
}

impl GenesisAttached {
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
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TetherCreated {
    pub tether_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: Option<TetherKind>,
    pub reverse_kind: TetherKind,
    pub source_event_id: String,
    pub source_event_hash: String,
}

impl TetherCreated {
    pub fn new(
        tether_id: impl Into<String>,
        source_node_id: impl Into<String>,
        target_node_id: impl Into<String>,
        kind: Option<TetherKind>,
        reverse_kind: TetherKind,
        source_event_id: impl Into<String>,
        source_event_hash: impl Into<String>,
    ) -> Self {
        Self {
            tether_id: tether_id.into(),
            source_node_id: source_node_id.into(),
            target_node_id: target_node_id.into(),
            kind,
            reverse_kind,
            source_event_id: source_event_id.into(),
            source_event_hash: source_event_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TopologyMutation {
    NodeCreated(NodeCreated),
    GenesisAttached(GenesisAttached),
    TetherCreated(TetherCreated),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum LedgerEvent {
    RawEventRecorded(RawEvent),
    TopologyMutation(TopologyMutation),
    CompressionArtifactRecorded(CompressionArtifact),
    ConsolidationEvent(ConsolidationEvent),
    CompressionReceipt(CompressionReceipt),

    ClusterEvolutionEvent(ClusterEvolutionEvent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InMemoryLedger {
    events: Vec<LedgerEvent>,
    raw_events: BTreeMap<String, RawEvent>,
    compression_artifacts: BTreeMap<String, CompressionArtifact>,
}

impl InMemoryLedger {
    pub fn append(&mut self, event: RawEvent) -> Result<()> {
        event.verify_hash()?;
        if self.raw_events.contains_key(&event.id) {
            return Err(LunaError::new(format!(
                "raw event {} already exists in append-only ledger",
                event.id
            )));
        }
        self.raw_events.insert(event.id.clone(), event.clone());
        self.events.push(LedgerEvent::RawEventRecorded(event));
        Ok(())
    }

    /// Append a topology mutation after the caller has run the inspector chain.
    ///
    /// # Safety
    ///
    /// This bypasses inspection and registry application. Callers must prove the
    /// mutation already passed inspectors and will be applied to the live
    /// topology by the same commit operation.
    pub unsafe fn append_mutation_unchecked(&mut self, mutation: TopologyMutation) -> Result<()> {
        self.events.push(LedgerEvent::TopologyMutation(mutation));
        Ok(())
    }

    pub fn append_consolidation_event(&mut self, event: ConsolidationEvent) -> Result<()> {
        luna_cluster::validate_consolidation_event(&event)?;
        if self
            .events
            .iter()
            .any(|ledger_event| matches!(ledger_event, LedgerEvent::ConsolidationEvent(existing) if existing.event_id == event.event_id))
        {
            return Err(LunaError::new(format!(
                "consolidation event {} already exists in append-only ledger",
                event.event_id
            )));
        }
        self.events.push(LedgerEvent::ConsolidationEvent(event));
        Ok(())
    }

    pub fn append_compression_receipt(&mut self, receipt: CompressionReceipt) -> Result<()> {
        luna_cluster::validate_compression_receipt(&receipt)?;
        self.verify_compression_artifact(&receipt)?;
        self.verify_compression_source_events(&receipt)?;
        if self
            .events
            .iter()
            .any(|ledger_event| matches!(ledger_event, LedgerEvent::CompressionReceipt(existing) if existing.event_id == receipt.event_id))
        {
            return Err(LunaError::new(format!(
                "compression receipt {} already exists in append-only ledger",
                receipt.event_id
            )));
        }

        if matches!(
            receipt.decision,
            luna_cluster::CompressionDecision::Accepted
        ) && self.events.iter().any(|ledger_event| {
            matches!(
                ledger_event,
                LedgerEvent::CompressionReceipt(existing)
                    if matches!(existing.decision, luna_cluster::CompressionDecision::Accepted)
                        && existing.compression_id == receipt.compression_id
            )
        }) {
            return Err(LunaError::new(format!(
                "compression {} already accepted in append-only ledger",
                receipt.compression_id
            )));
        }
        self.events.push(LedgerEvent::CompressionReceipt(receipt));
        Ok(())
    }

    fn verify_compression_source_events(&self, receipt: &CompressionReceipt) -> Result<()> {
        for reference in receipt
            .input_event_refs
            .iter()
            .chain(receipt.retained_event_refs.iter())
        {
            let raw_event = self.raw_events.get(&reference.event_id).ok_or_else(|| {
                LunaError::new(format!(
                    "compression {} references missing raw event {}",
                    receipt.compression_id, reference.event_id
                ))
            })?;
            if raw_event.hash != reference.event_hash {
                return Err(LunaError::new(format!(
                    "compression {} raw event {} hash mismatch",
                    receipt.compression_id, reference.event_id
                )));
            }
        }
        Ok(())
    }

    pub fn append_compression_artifact(&mut self, artifact: CompressionArtifact) -> Result<()> {
        artifact.verify()?;
        if self
            .compression_artifacts
            .contains_key(&artifact.artifact_ref)
        {
            return Err(LunaError::new(format!(
                "compression artifact {} already exists in append-only ledger",
                artifact.artifact_ref
            )));
        }
        self.compression_artifacts
            .insert(artifact.artifact_ref.clone(), artifact.clone());
        self.events
            .push(LedgerEvent::CompressionArtifactRecorded(artifact));
        Ok(())
    }

    fn verify_compression_artifact(&self, receipt: &CompressionReceipt) -> Result<()> {
        let artifact = self
            .compression_artifacts
            .get(&receipt.output_artifact_ref)
            .ok_or_else(|| {
                LunaError::new(format!(
                    "compression {} references missing output artifact {}",
                    receipt.compression_id, receipt.output_artifact_ref
                ))
            })?;
        if artifact.output_hash != receipt.output_hash {
            return Err(LunaError::new(format!(
                "compression {} output artifact hash mismatch",
                receipt.compression_id
            )));
        }
        if artifact.byte_len != receipt.output_byte_len {
            return Err(LunaError::new(format!(
                "compression {} output artifact byte length mismatch",
                receipt.compression_id
            )));
        }
        Ok(())
    }

    pub fn append_cluster_evolution_event(&mut self, event: ClusterEvolutionEvent) -> Result<()> {
        luna_cluster::validate_cluster_evolution_event(&event)?;
        self.verify_cluster_evolution_metric_provenance(&event)?;
        if self
            .events
            .iter()
            .any(|ledger_event| matches!(ledger_event, LedgerEvent::ClusterEvolutionEvent(existing) if existing.event_id == event.event_id))
        {
            return Err(LunaError::new(format!(
                "cluster evolution event {} already exists in append-only ledger",
                event.event_id
            )));
        }
        self.events.push(LedgerEvent::ClusterEvolutionEvent(event));
        Ok(())
    }

    fn verify_cluster_evolution_metric_provenance(
        &self,
        event: &ClusterEvolutionEvent,
    ) -> Result<()> {
        for reference in &event.metric_evidence_refs {
            let raw_event = self.raw_events.get(&reference.event_id).ok_or_else(|| {
                LunaError::new(format!(
                    "cluster evolution metric {} references missing evidence event {}",
                    reference.metric_id, reference.event_id
                ))
            })?;
            if raw_event.hash != reference.event_hash {
                return Err(LunaError::new(format!(
                    "cluster evolution metric {} evidence event {} hash mismatch",
                    reference.metric_id, reference.event_id
                )));
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&RawEvent> {
        self.raw_events.get(id)
    }

    pub fn events(&self) -> &[LedgerEvent] {
        &self.events
    }

    pub fn raw_events(&self) -> &BTreeMap<String, RawEvent> {
        &self.raw_events
    }

    pub fn compression_artifacts(&self) -> &BTreeMap<String, CompressionArtifact> {
        &self.compression_artifacts
    }
}

fn is_hex_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
}

fn stable_event_hash(id: &str, source: &EventSource, payload: &EventPayload) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(canonical_raw_event_bytes(id, source, payload));
    let digest = hasher.finalize();
    Ok(format!("{digest:x}"))
}

fn canonical_raw_event_bytes(id: &str, source: &EventSource, payload: &EventPayload) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_canonical_field(&mut bytes, RAW_EVENT_HASH_VERSION.as_bytes());
    push_canonical_field(&mut bytes, id.as_bytes());
    push_canonical_field(&mut bytes, event_source_tag(source).as_bytes());
    match payload {
        EventPayload::Text(text) => {
            push_canonical_field(&mut bytes, b"text");
            push_canonical_field(&mut bytes, text.as_bytes());
        }
    }
    bytes
}

fn event_source_tag(source: &EventSource) -> &'static str {
    match source {
        EventSource::User => "user",
        EventSource::Assistant => "assistant",
        EventSource::System => "system",
    }
}

fn push_canonical_field(bytes: &mut Vec<u8>, field: &[u8]) {
    bytes.extend_from_slice(field.len().to_string().as_bytes());
    bytes.push(b':');
    bytes.extend_from_slice(field);
    bytes.push(b'\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    use luna_cluster::{
        issue_compression_receipt, CompressionPolicy, CompressionRequest, SourceEventRef,
    };

    fn raw_event(id: &str, text: &str) -> RawEvent {
        RawEvent::from_draft(RawEventDraft::new(
            id,
            EventSource::User,
            EventPayload::Text(text.to_string()),
        ))
    }

    fn compression_receipt(refs: Vec<SourceEventRef>) -> CompressionReceipt {
        issue_compression_receipt(
            CompressionRequest::new(
                "compression-ledger",
                refs.clone(),
                refs,
                "lossless-v1",
                "0".repeat(64),
            )
            .with_output_artifact("artifact://compression-ledger", b"ledger compression bytes"),
            &CompressionPolicy::default(),
        )
    }

    #[test]
    fn append_compression_receipt_rejects_missing_raw_event_refs() {
        let mut ledger = InMemoryLedger::default();
        ledger
            .append_compression_artifact(CompressionArtifact::from_bytes(
                "artifact://compression-ledger",
                b"ledger compression bytes",
            ))
            .unwrap();
        let refs = vec![
            SourceEventRef::new("missing-a", "a".repeat(64)),
            SourceEventRef::new("missing-b", "b".repeat(64)),
        ];
        let receipt = compression_receipt(refs);

        let error = ledger.append_compression_receipt(receipt).unwrap_err();

        assert!(error
            .to_string()
            .contains("references missing raw event missing-a"));
        assert!(ledger
            .events()
            .iter()
            .all(|event| { !matches!(event, LedgerEvent::CompressionReceipt(_)) }));
    }

    #[test]
    fn append_compression_receipt_rejects_raw_event_hash_mismatch() {
        let mut ledger = InMemoryLedger::default();
        let event_a = raw_event("event-a", "Chris lives in Iowa.");
        let event_b = raw_event("event-b", "Chris works on Luna.");
        ledger.append(event_a.clone()).unwrap();
        ledger.append(event_b.clone()).unwrap();
        ledger
            .append_compression_artifact(CompressionArtifact::from_bytes(
                "artifact://compression-ledger",
                b"ledger compression bytes",
            ))
            .unwrap();
        let refs = vec![
            SourceEventRef::new(&event_a.id, "a".repeat(64)),
            SourceEventRef::new(&event_b.id, event_b.hash.clone()),
        ];
        let receipt = compression_receipt(refs);

        let error = ledger.append_compression_receipt(receipt).unwrap_err();

        assert!(error
            .to_string()
            .contains("raw event event-a hash mismatch"));
        assert!(ledger
            .events()
            .iter()
            .all(|event| { !matches!(event, LedgerEvent::CompressionReceipt(_)) }));
    }

    #[test]
    fn append_compression_receipt_accepts_verified_raw_event_refs() {
        let mut ledger = InMemoryLedger::default();
        let event_a = raw_event("event-a", "Chris lives in Iowa.");
        let event_b = raw_event("event-b", "Chris works on Luna.");
        ledger.append(event_a.clone()).unwrap();
        ledger.append(event_b.clone()).unwrap();
        ledger
            .append_compression_artifact(CompressionArtifact::from_bytes(
                "artifact://compression-ledger",
                b"ledger compression bytes",
            ))
            .unwrap();
        let refs = vec![
            SourceEventRef::new(&event_a.id, event_a.hash.clone()),
            SourceEventRef::new(&event_b.id, event_b.hash.clone()),
        ];
        let receipt = compression_receipt(refs);

        ledger.append_compression_receipt(receipt).unwrap();

        assert!(ledger
            .events()
            .iter()
            .any(|event| matches!(event, LedgerEvent::CompressionReceipt(_))));
    }
}
