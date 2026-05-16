use crate::SourceEventRef;
use chrono::{DateTime, SecondsFormat, Utc};
use luna_core::{LunaError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const COMPRESSION_RECEIPT_SCHEMA_VERSION: &str = "luna.compression_receipt.v1";
pub const COMPRESSION_RECEIPT_OPERATION: &str = "compression_receipted";
pub const DEFAULT_MIN_COMPRESSION_INPUT_EVENTS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionDecision {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompressionPolicy {
    pub min_input_events: usize,
}

impl Default for CompressionPolicy {
    fn default() -> Self {
        Self {
            min_input_events: DEFAULT_MIN_COMPRESSION_INPUT_EVENTS,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompressionRequest {
    pub compression_id: String,
    pub input_event_refs: Vec<SourceEventRef>,
    pub retained_event_refs: Vec<SourceEventRef>,
    pub algorithm_id: String,
    pub output_hash: String,
    pub output_artifact_ref: String,
    pub output_byte_len: usize,
    output_bytes: Option<Vec<u8>>,
}

impl CompressionRequest {
    pub fn new(
        compression_id: impl Into<String>,
        input_event_refs: Vec<SourceEventRef>,
        retained_event_refs: Vec<SourceEventRef>,
        algorithm_id: impl Into<String>,
        output_hash: impl Into<String>,
    ) -> Self {
        Self {
            compression_id: compression_id.into(),
            input_event_refs,
            retained_event_refs,
            algorithm_id: algorithm_id.into(),
            output_hash: output_hash.into(),
            output_artifact_ref: String::new(),
            output_byte_len: 0,
            output_bytes: None,
        }
    }

    pub fn with_output_artifact(
        mut self,
        output_artifact_ref: impl Into<String>,
        output_bytes: &[u8],
    ) -> Self {
        self.output_bytes = Some(output_bytes.to_vec());
        self.output_artifact_ref = output_artifact_ref.into();
        self.output_byte_len = output_bytes.len();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressionReceipt {
    pub schema_version: String,
    pub event_id: String,
    pub operation: String,
    pub compression_id: String,
    pub input_event_refs: Vec<SourceEventRef>,
    pub retained_event_refs: Vec<SourceEventRef>,
    pub algorithm_id: String,
    pub input_set_hash: String,
    pub output_hash: String,
    pub output_artifact_ref: String,
    pub output_byte_len: usize,
    pub decision: CompressionDecision,
    pub rejection_reason: Option<String>,
    pub proof_hash: String,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CompressionReceiptRegistry {
    accepted: BTreeMap<String, CompressionReceipt>,
    rejected_event_ids: Vec<String>,
}

impl CompressionReceiptRegistry {
    pub fn apply_receipt(&mut self, receipt: &CompressionReceipt) -> Result<()> {
        validate_compression_receipt(receipt)?;
        match receipt.decision {
            CompressionDecision::Accepted => {
                if self.accepted.contains_key(&receipt.compression_id) {
                    return Err(LunaError::new(format!(
                        "compression {} already accepted during replay",
                        receipt.compression_id
                    )));
                }
                self.accepted
                    .insert(receipt.compression_id.clone(), receipt.clone());
            }
            CompressionDecision::Rejected => {
                self.rejected_event_ids.push(receipt.event_id.clone());
            }
        }
        Ok(())
    }

    pub fn accepted(&self) -> &BTreeMap<String, CompressionReceipt> {
        &self.accepted
    }

    pub fn rejected_event_ids(&self) -> &[String] {
        &self.rejected_event_ids
    }
}

pub fn issue_compression_receipt(
    request: CompressionRequest,
    policy: &CompressionPolicy,
) -> CompressionReceipt {
    let mut request = canonical_request(request);
    if let Some(output_bytes) = request.output_bytes.as_deref() {
        request.output_hash = compression_output_hash(output_bytes);
        request.output_byte_len = output_bytes.len();
    }
    let rejection_reason = compression_rejection_reason(&request, policy);
    let decision = if rejection_reason.is_some() {
        CompressionDecision::Rejected
    } else {
        CompressionDecision::Accepted
    };
    let input_set_hash = input_set_hash(&request.input_event_refs);
    let recorded_at = Utc::now();
    let proof_hash = compression_proof_hash(
        &request,
        &input_set_hash,
        decision,
        rejection_reason.as_deref(),
        &recorded_at,
    );
    let event_id = compression_event_id(&request.compression_id, &proof_hash, decision);

    CompressionReceipt {
        schema_version: COMPRESSION_RECEIPT_SCHEMA_VERSION.to_string(),
        event_id,
        operation: COMPRESSION_RECEIPT_OPERATION.to_string(),
        compression_id: request.compression_id,
        input_event_refs: request.input_event_refs,
        retained_event_refs: request.retained_event_refs,
        algorithm_id: request.algorithm_id,
        input_set_hash,
        output_hash: request.output_hash,
        output_artifact_ref: request.output_artifact_ref,
        output_byte_len: request.output_byte_len,
        decision,
        rejection_reason,
        proof_hash,
        recorded_at,
    }
}

pub fn validate_compression_receipt(receipt: &CompressionReceipt) -> Result<()> {
    if receipt.schema_version != COMPRESSION_RECEIPT_SCHEMA_VERSION {
        return Err(LunaError::new(
            "unexpected compression receipt schema version",
        ));
    }
    if receipt.operation != COMPRESSION_RECEIPT_OPERATION {
        return Err(LunaError::new("unexpected compression receipt operation"));
    }
    if receipt.event_id.trim().is_empty() || receipt.compression_id.trim().is_empty() {
        return Err(LunaError::new(
            "compression receipt event and compression ids are required",
        ));
    }
    if receipt.algorithm_id.trim().is_empty() {
        return Err(LunaError::new("compression algorithm id is required"));
    }
    if receipt.input_event_refs.is_empty() || receipt.retained_event_refs.is_empty() {
        return Err(LunaError::new(
            "compression receipt must preserve raw event ancestry",
        ));
    }
    if !receipt.input_event_refs.iter().all(valid_source_event_ref)
        || !receipt
            .retained_event_refs
            .iter()
            .all(valid_source_event_ref)
    {
        return Err(LunaError::new("invalid compression source event ref"));
    }
    if receipt.input_event_refs != sorted_unique_event_refs(receipt.input_event_refs.clone())
        || receipt.retained_event_refs
            != sorted_unique_event_refs(receipt.retained_event_refs.clone())
    {
        return Err(LunaError::new(
            "compression receipt ancestry refs must be sorted and unique",
        ));
    }
    if receipt.input_set_hash != input_set_hash(&receipt.input_event_refs) {
        return Err(LunaError::new("compression input set hash mismatch"));
    }
    if !is_hex_hash(&receipt.output_hash) {
        return Err(LunaError::new("invalid compression output hash"));
    }
    if receipt.output_artifact_ref.trim().is_empty() {
        return Err(LunaError::new(
            "compression output artifact ref is required",
        ));
    }
    if !is_hex_hash(&receipt.proof_hash) {
        return Err(LunaError::new("invalid compression proof hash"));
    }
    let expected_request = CompressionRequest::new(
        receipt.compression_id.clone(),
        receipt.input_event_refs.clone(),
        receipt.retained_event_refs.clone(),
        receipt.algorithm_id.clone(),
        receipt.output_hash.clone(),
    )
    .with_output_metadata(receipt.output_artifact_ref.clone(), receipt.output_byte_len);
    let expected_proof_hash = compression_proof_hash(
        &expected_request,
        &receipt.input_set_hash,
        receipt.decision,
        receipt.rejection_reason.as_deref(),
        &receipt.recorded_at,
    );
    if receipt.proof_hash != expected_proof_hash {
        return Err(LunaError::new("compression proof hash mismatch"));
    }
    let expected_event_id = compression_event_id(
        &receipt.compression_id,
        &receipt.proof_hash,
        receipt.decision,
    );
    if receipt.event_id != expected_event_id {
        return Err(LunaError::new("compression event id mismatch"));
    }
    match receipt.decision {
        CompressionDecision::Accepted if receipt.rejection_reason.is_some() => Err(LunaError::new(
            "accepted compression cannot carry a rejection reason",
        )),
        CompressionDecision::Accepted
            if receipt.input_event_refs.len() < DEFAULT_MIN_COMPRESSION_INPUT_EVENTS =>
        {
            Err(LunaError::new("accepted compression is under-proven"))
        }
        CompressionDecision::Accepted
            if receipt.input_event_refs != receipt.retained_event_refs =>
        {
            Err(LunaError::new(
                "accepted compression loses raw event ancestry",
            ))
        }
        CompressionDecision::Accepted if receipt.output_byte_len == 0 => Err(LunaError::new(
            "accepted compression must bind non-empty output bytes",
        )),
        CompressionDecision::Rejected
            if receipt
                .rejection_reason
                .as_deref()
                .is_none_or(|reason| reason.trim().is_empty()) =>
        {
            Err(LunaError::new(
                "rejected compression requires a rejection reason",
            ))
        }
        _ => Ok(()),
    }
}

fn compression_rejection_reason(
    request: &CompressionRequest,
    policy: &CompressionPolicy,
) -> Option<String> {
    if request.algorithm_id.trim().is_empty() {
        return Some("compression algorithm id is required".to_string());
    }
    if request.input_event_refs.len() < policy.min_input_events {
        return Some("not enough raw event ancestry for compression".to_string());
    }
    if !request.input_event_refs.iter().all(valid_source_event_ref)
        || !request
            .retained_event_refs
            .iter()
            .all(valid_source_event_ref)
    {
        return Some("compression refs must include ids and 64-char lowercase hashes".to_string());
    }
    if request.input_event_refs.len()
        != sorted_unique_event_refs(request.input_event_refs.clone()).len()
        || request.retained_event_refs.len()
            != sorted_unique_event_refs(request.retained_event_refs.clone()).len()
    {
        return Some("compression ancestry refs must be unique and lossless".to_string());
    }
    if !is_hex_hash(&request.output_hash) {
        return Some("compression output hash must be a 64-char lowercase hash".to_string());
    }
    if request.output_artifact_ref.trim().is_empty() {
        return Some("compression output artifact ref is required".to_string());
    }
    if request.output_bytes.is_none() || request.output_byte_len == 0 {
        return Some("compression output bytes are required".to_string());
    }
    if request.input_event_refs != request.retained_event_refs {
        return Some("compression would lose raw event ancestry".to_string());
    }
    None
}

fn canonical_request(request: CompressionRequest) -> CompressionRequest {
    CompressionRequest {
        compression_id: request.compression_id,
        input_event_refs: sorted_unique_event_refs(request.input_event_refs),
        retained_event_refs: sorted_unique_event_refs(request.retained_event_refs),
        algorithm_id: request.algorithm_id,
        output_hash: request.output_hash,
        output_artifact_ref: request.output_artifact_ref,
        output_byte_len: request.output_byte_len,
        output_bytes: request.output_bytes,
    }
}

impl CompressionRequest {
    fn with_output_metadata(
        mut self,
        output_artifact_ref: impl Into<String>,
        output_byte_len: usize,
    ) -> Self {
        self.output_artifact_ref = output_artifact_ref.into();
        self.output_byte_len = output_byte_len;
        self.output_bytes = None;
        self
    }
}

pub fn compression_output_hash(output_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hash_field(
        &mut hasher,
        "schema_version",
        COMPRESSION_RECEIPT_SCHEMA_VERSION,
    );
    hasher.update(b"output_bytes");
    hasher.update([0]);
    hasher.update(output_bytes.len().to_string().as_bytes());
    hasher.update([0]);
    hasher.update(output_bytes);
    format!("{:x}", hasher.finalize())
}

fn input_set_hash(refs: &[SourceEventRef]) -> String {
    let mut hasher = Sha256::new();
    hash_field(
        &mut hasher,
        "schema_version",
        COMPRESSION_RECEIPT_SCHEMA_VERSION,
    );
    for reference in refs {
        hash_field(&mut hasher, "input_event_id", &reference.event_id);
        hash_field(&mut hasher, "input_event_hash", &reference.event_hash);
    }
    format!("{:x}", hasher.finalize())
}

fn compression_proof_hash(
    request: &CompressionRequest,
    input_set_hash: &str,
    decision: CompressionDecision,
    rejection_reason: Option<&str>,
    recorded_at: &DateTime<Utc>,
) -> String {
    let mut hasher = Sha256::new();
    hash_field(
        &mut hasher,
        "schema_version",
        COMPRESSION_RECEIPT_SCHEMA_VERSION,
    );
    hash_field(&mut hasher, "compression_id", &request.compression_id);
    hash_field(&mut hasher, "input_set_hash", input_set_hash);
    for reference in &request.input_event_refs {
        hash_field(&mut hasher, "input_event_id", &reference.event_id);
        hash_field(&mut hasher, "input_event_hash", &reference.event_hash);
    }
    for reference in &request.retained_event_refs {
        hash_field(&mut hasher, "retained_event_id", &reference.event_id);
        hash_field(&mut hasher, "retained_event_hash", &reference.event_hash);
    }
    hash_field(&mut hasher, "algorithm_id", &request.algorithm_id);
    hash_field(&mut hasher, "output_hash", &request.output_hash);
    hash_field(
        &mut hasher,
        "output_artifact_ref",
        &request.output_artifact_ref,
    );
    hash_field(
        &mut hasher,
        "output_byte_len",
        &request.output_byte_len.to_string(),
    );
    hash_field(&mut hasher, "decision", &format!("{decision:?}"));
    if let Some(reason) = rejection_reason {
        hash_field(&mut hasher, "rejection_reason", reason);
    }
    hash_field(
        &mut hasher,
        "recorded_at",
        &recorded_at.to_rfc3339_opts(SecondsFormat::Nanos, true),
    );
    format!("{:x}", hasher.finalize())
}

fn compression_event_id(
    compression_id: &str,
    proof_hash: &str,
    decision: CompressionDecision,
) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "compression_id", compression_id);
    hash_field(&mut hasher, "proof_hash", proof_hash);
    hash_field(&mut hasher, "decision", &format!("{decision:?}"));
    format!("compression-{:x}", hasher.finalize())
}

fn valid_source_event_ref(reference: &SourceEventRef) -> bool {
    !reference.event_id.trim().is_empty() && is_hex_hash(&reference.event_hash)
}

fn is_hex_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
}

fn sorted_unique_event_refs(values: Vec<SourceEventRef>) -> Vec<SourceEventRef> {
    let mut refs = BTreeMap::<String, SourceEventRef>::new();
    for value in values {
        refs.entry(value.event_id.clone()).or_insert(value);
    }
    refs.into_values().collect()
}

fn hash_field(hasher: &mut Sha256, label: &str, value: &str) {
    hasher.update(label.as_bytes());
    hasher.update([0]);
    hasher.update(value.len().to_string().as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hasher.update([0xff]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ref_a() -> SourceEventRef {
        SourceEventRef::new("event-a", "a".repeat(64))
    }

    fn ref_b() -> SourceEventRef {
        SourceEventRef::new("event-b", "b".repeat(64))
    }

    fn request() -> CompressionRequest {
        let refs = vec![ref_b(), ref_a()];
        CompressionRequest::new(
            "compression-1",
            refs.clone(),
            refs,
            "lossless-v1",
            "c".repeat(64),
        )
        .with_output_artifact("artifact://compression-1", b"lossless compressed bytes")
    }

    #[test]
    fn accepted_compression_receipt_preserves_raw_event_ancestry() {
        let receipt = issue_compression_receipt(request(), &CompressionPolicy::default());

        assert_eq!(receipt.decision, CompressionDecision::Accepted);
        assert_eq!(receipt.input_event_refs, vec![ref_a(), ref_b()]);
        assert_eq!(receipt.retained_event_refs, receipt.input_event_refs);
        assert_eq!(
            receipt.output_hash,
            compression_output_hash(b"lossless compressed bytes")
        );
        assert_ne!(receipt.output_hash, "c".repeat(64));
        validate_compression_receipt(&receipt).unwrap();
    }

    #[test]
    fn lossy_compression_is_a_rejected_receipt() {
        let receipt = issue_compression_receipt(
            CompressionRequest::new(
                "compression-lossy",
                vec![ref_a(), ref_b()],
                vec![ref_a()],
                "summary-v1",
                "c".repeat(64),
            )
            .with_output_artifact("artifact://compression-lossy", b"lossy bytes"),
            &CompressionPolicy::default(),
        );

        assert_eq!(receipt.decision, CompressionDecision::Rejected);
        assert!(receipt
            .rejection_reason
            .as_deref()
            .unwrap()
            .contains("lose raw event ancestry"));
        validate_compression_receipt(&receipt).unwrap();
    }

    #[test]
    fn forged_compression_receipt_rejects_validation() {
        let mut receipt = issue_compression_receipt(request(), &CompressionPolicy::default());
        receipt.algorithm_id = "forged-algorithm".to_string();

        let error = validate_compression_receipt(&receipt).unwrap_err();

        assert!(error.to_string().contains("proof hash mismatch"));
    }

    #[test]
    fn under_proven_accepted_compression_rejects_validation() {
        let permissive_policy = CompressionPolicy {
            min_input_events: 1,
        };
        let receipt = issue_compression_receipt(
            CompressionRequest::new(
                "compression-under-proven",
                vec![ref_a()],
                vec![ref_a()],
                "lossless-v1",
                "c".repeat(64),
            )
            .with_output_artifact("artifact://compression-under-proven", b"single input bytes"),
            &permissive_policy,
        );

        let error = validate_compression_receipt(&receipt).unwrap_err();

        assert!(error.to_string().contains("under-proven"));
    }

    #[test]
    fn invalid_ancestry_is_not_silently_dropped_into_acceptance() {
        let invalid = SourceEventRef::new("event-bad", "not-a-hash");
        let receipt = issue_compression_receipt(
            CompressionRequest::new(
                "compression-invalid",
                vec![ref_a(), ref_b(), invalid.clone()],
                vec![ref_a(), ref_b(), invalid],
                "lossless-v1",
                "c".repeat(64),
            )
            .with_output_artifact("artifact://compression-invalid", b"invalid ref bytes"),
            &CompressionPolicy::default(),
        );

        assert_eq!(receipt.decision, CompressionDecision::Rejected);
        assert!(receipt
            .rejection_reason
            .as_deref()
            .unwrap()
            .contains("64-char lowercase hashes"));
    }

    #[test]
    fn accepted_compression_requires_bound_output_bytes() {
        let receipt = issue_compression_receipt(
            CompressionRequest::new(
                "compression-unbound",
                vec![ref_a(), ref_b()],
                vec![ref_a(), ref_b()],
                "lossless-v1",
                "c".repeat(64),
            ),
            &CompressionPolicy::default(),
        );

        assert_eq!(receipt.decision, CompressionDecision::Rejected);
        assert!(receipt
            .rejection_reason
            .as_deref()
            .unwrap()
            .contains("output artifact ref"));
    }

    #[test]
    fn caller_supplied_output_hash_is_replaced_by_artifact_bytes() {
        let receipt = issue_compression_receipt(
            CompressionRequest::new(
                "compression-bound",
                vec![ref_a(), ref_b()],
                vec![ref_a(), ref_b()],
                "lossless-v1",
                "f".repeat(64),
            )
            .with_output_artifact("artifact://compression-bound", b"verified artifact bytes"),
            &CompressionPolicy::default(),
        );

        assert_eq!(receipt.decision, CompressionDecision::Accepted);
        assert_eq!(
            receipt.output_hash,
            compression_output_hash(b"verified artifact bytes")
        );
        assert_ne!(receipt.output_hash, "f".repeat(64));
        validate_compression_receipt(&receipt).unwrap();
    }
}
