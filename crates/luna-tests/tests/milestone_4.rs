use jsonschema::JSONSchema;
use luna_ledger::{
    CompressionArtifact, EventPayload, EventSource, GenesisAttached, LedgerEvent, NodeCreated,
    NodeKind, RawEvent, RawEventDraft, TetherCreated, TetherKind, TopologyMutation,
};
use luna_cluster::{
    evolve_cluster, form_memory_cluster, issue_compression_receipt, validate_compression_receipt,
    validate_consolidation_event, validate_cluster_evolution_event, CompressionDecision,
    CompressionPolicy, CompressionReceipt, CompressionRequest, ConsolidationDecision,
    ConsolidationEvent, MetricEvidenceRef, ClusterEvolutionRequest, ClusterFormationPolicy,
    ClusterFormationRequest, SourceEventRef,
};
use luna_replay::{ReplayAuditStatus, ReplayAuditor, ReplayedTopology, TopologyReplay};
use serde_json::Value;

#[test]
fn test_consolidation_event_schema_declares_orb_formation_receipt() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../luna-memory/schemas/consolidation_event.schema.json"
    ))
    .unwrap();

    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        "luna.consolidation_event.v1"
    );
    assert_eq!(schema["properties"]["operation"]["const"], "cluster_formed");
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "source_node_ids"));
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "replay_trace_hash"));
}

#[test]
fn test_consolidation_event_schema_requires_auditable_rejection_fields() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../luna-memory/schemas/consolidation_event.schema.json"
    ))
    .unwrap();
    let all_of = schema["allOf"].as_array().unwrap();

    assert!(all_of.iter().any(|rule| {
        rule["if"]["properties"]["decision"]["const"] == "rejected"
            && rule["then"]["properties"]["rejection_reason"]["minLength"] == 1
    }));
    assert!(all_of.iter().any(|rule| {
        rule["if"]["properties"]["decision"]["const"] == "accepted"
            && rule["then"]["properties"]["source_node_ids"]["minItems"] == 2
    }));
}

#[test]
fn test_consolidation_event_schema_validates_receipt_examples() {
    let schema = consolidation_schema();
    let validator = JSONSchema::compile(&schema).unwrap();

    assert!(validator.is_valid(&accepted_receipt()));
    assert!(validator.is_valid(&rejected_receipt("below cohesion threshold")));

    let mut accepted_with_reason = accepted_receipt();
    accepted_with_reason["rejection_reason"] = Value::String("not allowed".to_string());
    assert!(!validator.is_valid(&accepted_with_reason));

    let mut rejected_without_reason = rejected_receipt("");
    rejected_without_reason["rejection_reason"] = Value::String(String::new());
    assert!(!validator.is_valid(&rejected_without_reason));

    let mut bad_hash = accepted_receipt();
    bad_hash["replay_trace_hash"] = Value::String("not-a-hex-hash".to_string());
    assert!(!validator.is_valid(&bad_hash));

    let mut missing_sources = accepted_receipt();
    missing_sources["source_node_ids"] = Value::Array(Vec::new());
    assert!(!validator.is_valid(&missing_sources));

    let mut one_node_accepted = accepted_receipt();
    one_node_accepted["source_node_ids"] = serde_json::json!(["node-1"]);
    assert!(!validator.is_valid(&one_node_accepted));

    let mut bad_score = accepted_receipt();
    bad_score["cohesion_score"] = serde_json::json!(1.5);
    assert!(!validator.is_valid(&bad_score));

    let mut blank_cohesion_rule = accepted_receipt();
    blank_cohesion_rule["cohesion_rule_id"] = Value::String(String::new());
    assert!(!validator.is_valid(&blank_cohesion_rule));

    let mut duplicate_refs = accepted_receipt();
    duplicate_refs["source_event_refs"] = serde_json::json!([
        {
            "event_id": "source-event-1",
            "event_hash": hex_hash()
        },
        {
            "event_id": "source-event-1",
            "event_hash": hex_hash()
        }
    ]);
    assert!(!validator.is_valid(&duplicate_refs));
}

#[test]
fn test_consolidation_event_schema_examples_deserialize_and_validate_in_rust() {
    let schema = consolidation_schema();
    let validator = JSONSchema::compile(&schema).unwrap();
    let examples = schema["examples"].as_array().unwrap();

    assert_eq!(examples.len(), 2);
    for example in examples {
        assert!(validator.is_valid(example));
        let event: ConsolidationEvent = serde_json::from_value(example.clone()).unwrap();
        validate_consolidation_event(&event).unwrap();
    }
}

#[test]
fn test_compression_receipt_schema_declares_receipted_operation() {
    let schema = compression_schema();

    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        "luna.compression_receipt.v1"
    );
    assert_eq!(
        schema["properties"]["operation"]["const"],
        "compression_receipted"
    );
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "input_set_hash"));
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "proof_hash"));
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "output_artifact_ref"));
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "output_byte_len"));
}

#[test]
fn test_compression_receipt_schema_requires_auditable_decision_fields() {
    let schema = compression_schema();
    let all_of = schema["allOf"].as_array().unwrap();

    assert!(all_of.iter().any(|rule| {
        rule["if"]["properties"]["decision"]["const"] == "accepted"
            && rule["then"]["properties"]["rejection_reason"]["type"] == "null"
            && rule["then"]["properties"]["input_event_refs"]["minItems"] == 2
    }));
    assert!(all_of.iter().any(|rule| {
        rule["if"]["properties"]["decision"]["const"] == "rejected"
            && rule["then"]["properties"]["rejection_reason"]["minLength"] == 1
    }));
    assert_eq!(
        schema["properties"]["input_event_refs"]["uniqueItems"],
        true
    );
    assert_eq!(
        schema["properties"]["retained_event_refs"]["uniqueItems"],
        true
    );
    assert_eq!(schema["properties"]["output_artifact_ref"]["minLength"], 1);
    assert_eq!(schema["properties"]["output_byte_len"]["minimum"], 0);
}

#[test]
fn test_compression_receipt_schema_examples_deserialize_and_validate_in_rust() {
    let schema = compression_schema();
    let validator = JSONSchema::compile(&schema).unwrap();
    let examples = schema["examples"].as_array().unwrap();

    assert_eq!(examples.len(), 2);
    for example in examples {
        assert!(validator.is_valid(example));
        let receipt: CompressionReceipt = serde_json::from_value(example.clone()).unwrap();
        validate_compression_receipt(&receipt).unwrap();
    }
}

#[test]
fn test_compression_receipt_schema_rejects_invalid_examples() {
    let schema = compression_schema();
    let validator = JSONSchema::compile(&schema).unwrap();

    for invalid_example in schema["x-invalid-examples"].as_array().unwrap() {
        assert!(!validator.is_valid(invalid_example));
    }

    let mut accepted_with_reason = compression_accepted_receipt();
    accepted_with_reason["rejection_reason"] = Value::String("not allowed".to_string());
    assert!(!validator.is_valid(&accepted_with_reason));

    let mut accepted_under_proven = compression_accepted_receipt();
    accepted_under_proven["input_event_refs"] = serde_json::json!([
        {
            "event_id": "compression-event-1",
            "event_hash": hex_hash()
        }
    ]);
    accepted_under_proven["retained_event_refs"] =
        accepted_under_proven["input_event_refs"].clone();
    assert!(!validator.is_valid(&accepted_under_proven));

    let mut rejected_without_reason = compression_rejected_receipt();
    rejected_without_reason["rejection_reason"] = Value::Null;
    assert!(!validator.is_valid(&rejected_without_reason));

    let mut duplicate_refs = compression_accepted_receipt();
    duplicate_refs["input_event_refs"] = serde_json::json!([
        {
            "event_id": "compression-event-1",
            "event_hash": hex_hash()
        },
        {
            "event_id": "compression-event-1",
            "event_hash": hex_hash()
        }
    ]);
    assert!(!validator.is_valid(&duplicate_refs));

    let mut bad_hash = compression_accepted_receipt();
    bad_hash["proof_hash"] = Value::String("not-a-hex-hash".to_string());
    assert!(!validator.is_valid(&bad_hash));

    let mut empty_accepted_output = compression_accepted_receipt();
    empty_accepted_output["output_byte_len"] = serde_json::json!(0);
    assert!(!validator.is_valid(&empty_accepted_output));
}

#[test]
fn test_accepted_consolidation_event_forms_replayable_orb() {
    let mut topology = m1_live_topology();
    let event = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());

    topology.record_consolidation_event(event.clone()).unwrap();
    let replayed = TopologyReplay::replay_ledger(topology.ledger()).unwrap();

    assert_eq!(replayed, topology);
    assert_eq!(
        replayed.clusters().clusters()["orb-1"].source_node_ids,
        vec!["node-1", "node-2"]
    );
    assert!(matches!(
        topology.ledger().events().last(),
        Some(LedgerEvent::ConsolidationEvent(_))
    ));
}

#[test]
fn test_replay_auditor_accepts_valid_orb_backed_replay() {
    let mut topology = m1_live_topology();
    let event = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());

    topology.record_consolidation_event(event.clone()).unwrap();
    let report = ReplayAuditor::audit_ledger(&topology).unwrap();

    assert_eq!(report.status, ReplayAuditStatus::Clean);
    assert!(report.is_clean());
    assert!(!report.quarantine_required);
    assert_eq!(report.live_snapshot_hash, report.replayed_snapshot_hash);
    assert!(report.count_diffs.is_empty());
    assert_eq!(report.live_orb_lineage, report.replayed_orb_lineage);
    assert_eq!(report.live_orb_lineage.len(), 1);
    assert_eq!(report.live_orb_lineage[0].orb_id, "orb-1");
    assert_eq!(report.live_orb_lineage[0].accepted_event_id, event.event_id);
    assert_eq!(
        report.live_orb_lineage[0].source_tether_ids,
        vec!["tether-1"]
    );
    assert_eq!(
        report.live_orb_lineage[0].source_event_refs,
        orb_request(&topology, 0.91).source_event_refs
    );
}

#[test]
fn test_replay_auditor_quarantines_forced_divergence_without_repair() {
    let mut topology = m1_live_topology();
    let replay_ledger_before_orb = topology.ledger().clone();
    let event = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());

    topology.record_consolidation_event(event).unwrap();
    let before_audit = topology.clone();
    let report = ReplayAuditor::audit_against_ledger(&topology, &replay_ledger_before_orb).unwrap();

    assert_eq!(report.status, ReplayAuditStatus::Quarantined);
    assert!(report.is_quarantined());
    assert!(report.quarantine_required);
    assert_ne!(report.live_snapshot_hash, report.replayed_snapshot_hash);
    assert_eq!(report.live_counts.accepted_orbs, 1);
    assert_eq!(report.replayed_counts.accepted_orbs, 0);
    assert!(report
        .count_diffs
        .iter()
        .any(|diff| diff.field == "accepted_orbs" && diff.live == 1 && diff.replayed == 0));
    assert_eq!(report.live_orb_lineage.len(), 1);
    assert!(report.replayed_orb_lineage.is_empty());
    assert_eq!(topology, before_audit);
}

#[test]
fn test_rejected_consolidation_event_logs_without_forming_orb() {
    let mut topology = m1_live_topology();
    let event = form_memory_cluster(orb_request(&topology, 0.2), &ClusterFormationPolicy::default());

    topology.record_consolidation_event(event.clone()).unwrap();
    let replayed = TopologyReplay::replay_ledger(topology.ledger()).unwrap();

    assert_eq!(event.decision, ConsolidationDecision::Rejected);
    assert!(replayed.clusters().clusters().is_empty());
    assert_eq!(replayed.clusters().rejected_event_ids(), &[event.event_id]);
}
#[test]
fn test_low_cohesion_accepted_receipt_is_rejected_before_append() {
    let mut topology = m1_live_topology();
    let permissive_policy = ClusterFormationPolicy {
        min_cohesion_score: 0.1,
        ..ClusterFormationPolicy::default()
    };
    let event = form_memory_cluster(orb_request(&topology, 0.2), &permissive_policy);
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error.to_string().contains("below threshold"));
    assert_eq!(topology, before);
}

#[test]
fn test_forged_replay_trace_rejects_before_append() {
    let mut topology = m1_live_topology();
    let mut event = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    event.source_node_ids = vec!["node-1".to_string(), "node-2".to_string()];
    event.cohesion_rule_id = "forged-rule".to_string();
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error.to_string().contains("replay trace hash mismatch"));
    assert_eq!(topology, before);
}

#[test]
fn test_low_order_score_mutation_rejects_before_append() {
    let mut topology = m1_live_topology();
    let mut event = form_memory_cluster(
        orb_request(&topology, 0.91234561),
        &ClusterFormationPolicy::default(),
    );
    event.cohesion_score = 0.91234562;
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error.to_string().contains("replay trace hash mismatch"));
    assert_eq!(topology, before);
}

#[test]
fn test_recorded_at_mutation_rejects_before_append() {
    let mut topology = m1_live_topology();
    let mut event = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    event.recorded_at += chrono::TimeDelta::seconds(1);
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error.to_string().contains("replay trace hash mismatch"));
    assert_eq!(topology, before);
}

#[test]
fn test_blank_cohesion_rule_id_rejects_before_append() {
    let mut topology = m1_live_topology();
    let mut event = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    event.cohesion_rule_id = " ".to_string();
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error.to_string().contains("cohesion rule id"));
    assert_eq!(topology, before);
}

#[test]
fn test_duplicate_sources_canonicalize_before_policy() {
    let topology = m1_live_topology();
    let event = form_memory_cluster(
        ClusterFormationRequest::new(
            "orb-duplicate-source",
            vec!["node-1".to_string(), "node-1".to_string()],
            Vec::new(),
            orb_request(&topology, 0.91).source_event_refs,
            "shared-manuscript-evidence",
            0.91,
        ),
        &ClusterFormationPolicy::default(),
    );

    assert_eq!(event.source_node_ids, vec!["node-1"]);
    assert_eq!(event.decision, ConsolidationDecision::Rejected);
    assert!(event
        .rejection_reason
        .as_deref()
        .unwrap()
        .contains("not enough source nodes"));
}

#[test]
fn test_out_of_range_cohesion_rejected_by_rust_validation() {
    let mut topology = m1_live_topology();
    let event = form_memory_cluster(orb_request(&topology, 1.5), &ClusterFormationPolicy::default());
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error.to_string().contains("between 0 and 1"));
    assert_eq!(topology, before);
}

#[test]
fn test_orb_requires_existing_source_nodes() {
    let mut topology = m1_live_topology();
    let mut request = orb_request(&topology, 0.91);
    request.source_node_ids.push("missing-node".to_string());
    let event = form_memory_cluster(request, &ClusterFormationPolicy::default());
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error.to_string().contains("missing source node"));
    assert_eq!(topology, before);
}

#[test]
fn test_orb_requires_existing_source_tethers_when_listed() {
    let mut topology = m1_live_topology();
    let mut request = orb_request(&topology, 0.91);
    request.source_tether_ids.push("missing-tether".to_string());
    let event = form_memory_cluster(request, &ClusterFormationPolicy::default());
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error.to_string().contains("missing source tether"));
    assert_eq!(topology, before);
}

#[test]
fn test_orb_source_event_hash_mismatch_rejects() {
    let mut topology = m1_live_topology();
    let mut request = orb_request(&topology, 0.91);
    request.source_event_refs[0].event_hash = "b".repeat(64);
    let event = form_memory_cluster(request, &ClusterFormationPolicy::default());
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error
        .to_string()
        .contains("not backed by listed source events"));
    assert_eq!(topology, before);
}

#[test]
fn test_orb_nodes_must_be_backed_by_listed_source_events() {
    let other_event = RawEvent::from_draft(RawEventDraft::new(
        "event-2",
        EventSource::User,
        EventPayload::Text("unrelated provenance".to_string()),
    ));
    let mut topology = m1_live_topology();
    topology.record_raw_event(other_event.clone()).unwrap();
    let mut request = orb_request(&topology, 0.91);
    request.source_event_refs = vec![SourceEventRef::new(&other_event.id, &other_event.hash)];
    let event = form_memory_cluster(request, &ClusterFormationPolicy::default());
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error
        .to_string()
        .contains("not backed by listed source events"));
    assert_eq!(topology, before);
}

#[test]
fn test_extra_source_event_ref_must_back_a_listed_source() {
    let other_event = RawEvent::from_draft(RawEventDraft::new(
        "event-2",
        EventSource::User,
        EventPayload::Text("unrelated provenance".to_string()),
    ));
    let mut topology = m1_live_topology();
    topology.record_raw_event(other_event.clone()).unwrap();
    let mut request = orb_request(&topology, 0.91);
    request
        .source_event_refs
        .push(SourceEventRef::new(&other_event.id, &other_event.hash));
    let event = form_memory_cluster(request, &ClusterFormationPolicy::default());
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error
        .to_string()
        .contains("does not back any listed source node or tether"));
    assert_eq!(topology, before);
}

#[test]
fn test_orb_tethers_must_connect_listed_source_nodes() {
    let event = sample_event();
    let mut topology = ReplayedTopology::default();
    topology.record_raw_event(event.clone()).unwrap();
    for node_id in ["node-1", "node-2", "node-3"] {
        topology
            .commit(TopologyMutation::NodeCreated(NodeCreated::new(
                node_id,
                NodeKind::Event,
                node_id,
                &event.id,
                &event.hash,
            )))
            .unwrap();
    }
    topology
        .commit(TopologyMutation::TetherCreated(TetherCreated::new(
            "tether-1",
            "node-1",
            "node-3",
            Some(TetherKind::SupportedBy),
            TetherKind::EvidenceFor,
            &event.id,
            &event.hash,
        )))
        .unwrap();
    let request = ClusterFormationRequest::new(
        "orb-bad-tether",
        vec!["node-1".to_string(), "node-2".to_string()],
        vec!["tether-1".to_string()],
        vec![SourceEventRef::new(&event.id, &event.hash)],
        "shared-manuscript-evidence",
        0.91,
    );
    let formed = form_memory_cluster(request, &ClusterFormationPolicy::default());
    let before = topology.clone();

    let error = topology.record_consolidation_event(formed).unwrap_err();

    assert!(error
        .to_string()
        .contains("does not connect listed source nodes"));
    assert_eq!(topology, before);
}

#[test]
fn test_rejected_receipt_requires_valid_source_event_hash() {
    let mut topology = m1_live_topology();
    let mut request = orb_request(&topology, 0.2);
    request.source_event_refs[0].event_hash = "b".repeat(64);
    let event = form_memory_cluster(request, &ClusterFormationPolicy::default());
    let before = topology.clone();

    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error
        .to_string()
        .contains("not backed by listed source events"));
    assert_eq!(topology, before);
}

#[test]
fn test_duplicate_orb_id_rejects() {
    let mut topology = m1_live_topology();
    let first = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    let second = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());

    topology.record_consolidation_event(first).unwrap();
    let before = topology.clone();
    let error = topology.record_consolidation_event(second).unwrap_err();

    assert!(error.to_string().contains("already exists"));
    assert_eq!(topology, before);
}

#[test]
fn test_duplicate_rejected_consolidation_event_id_rejects() {
    let mut topology = m1_live_topology();
    let event = form_memory_cluster(orb_request(&topology, 0.2), &ClusterFormationPolicy::default());

    topology.record_consolidation_event(event.clone()).unwrap();
    let before = topology.clone();
    let error = topology.record_consolidation_event(event).unwrap_err();

    assert!(error.to_string().contains("already exists"));
    assert_eq!(topology, before);
}

#[test]
fn test_accepted_compression_receipt_replays_with_raw_event_ancestry() {
    let mut topology = compression_topology();
    let receipt = issue_compression_receipt(
        compression_request(&topology, "compression-1"),
        &CompressionPolicy::default(),
    );

    record_compression_artifact(&mut topology, &receipt);
    topology
        .record_compression_receipt(receipt.clone())
        .unwrap();
    let replayed = TopologyReplay::replay_ledger(topology.ledger()).unwrap();

    assert_eq!(replayed, topology);
    assert_eq!(
        replayed.compression_receipts().accepted()["compression-1"].input_event_refs,
        receipt.input_event_refs
    );
    assert!(matches!(
        topology.ledger().events().last(),
        Some(LedgerEvent::CompressionReceipt(_))
    ));
}

#[test]
fn test_lossy_compression_receipt_is_rejected_without_accepted_record() {
    let mut topology = compression_topology();
    let mut request = compression_request(&topology, "compression-lossy");
    request.retained_event_refs.pop();
    let receipt = issue_compression_receipt(request, &CompressionPolicy::default());

    record_compression_artifact(&mut topology, &receipt);
    topology
        .record_compression_receipt(receipt.clone())
        .unwrap();
    let replayed = TopologyReplay::replay_ledger(topology.ledger()).unwrap();

    assert_eq!(receipt.decision, CompressionDecision::Rejected);
    assert!(replayed.compression_receipts().accepted().is_empty());
    assert_eq!(
        replayed.compression_receipts().rejected_event_ids(),
        &[receipt.event_id]
    );
}

#[test]
fn test_forged_compression_receipt_rejects_before_append() {
    let mut topology = compression_topology();
    let mut receipt = issue_compression_receipt(
        compression_request(&topology, "compression-forged"),
        &CompressionPolicy::default(),
    );
    record_compression_artifact(&mut topology, &receipt);
    receipt.algorithm_id = "forged-algorithm".to_string();
    let before = topology.clone();

    let error = topology.record_compression_receipt(receipt).unwrap_err();

    assert!(error.to_string().contains("proof hash mismatch"));
    assert_eq!(topology, before);
}

#[test]
fn test_compression_receipt_requires_existing_raw_event_ancestry() {
    let mut topology = compression_topology();
    let mut request = compression_request(&topology, "compression-missing-raw");
    request
        .input_event_refs
        .push(SourceEventRef::new("event-missing", "d".repeat(64)));
    request
        .retained_event_refs
        .push(SourceEventRef::new("event-missing", "d".repeat(64)));
    let receipt = issue_compression_receipt(request, &CompressionPolicy::default());
    record_compression_artifact(&mut topology, &receipt);
    let before = topology.clone();

    let error = topology.record_compression_receipt(receipt).unwrap_err();

    assert!(error.to_string().contains("references missing raw event"));
    assert_eq!(topology, before);
}

#[test]
fn test_compression_receipt_requires_recorded_output_artifact() {
    let mut topology = compression_topology();
    let receipt = issue_compression_receipt(
        compression_request(&topology, "compression-missing-artifact"),
        &CompressionPolicy::default(),
    );
    let before = topology.clone();

    let error = topology.record_compression_receipt(receipt).unwrap_err();

    assert!(error
        .to_string()
        .contains("references missing output artifact"));
    assert_eq!(topology, before);
}

#[test]
fn test_compression_receipt_rejects_output_artifact_hash_mismatch() {
    let mut topology = compression_topology();
    let receipt = issue_compression_receipt(
        compression_request(&topology, "compression-bad-artifact"),
        &CompressionPolicy::default(),
    );
    topology
        .record_compression_artifact(CompressionArtifact::from_bytes(
            receipt.output_artifact_ref.clone(),
            b"different compressed output bytes",
        ))
        .unwrap();
    let before = topology.clone();

    let error = topology.record_compression_receipt(receipt).unwrap_err();

    assert!(error.to_string().contains("output artifact hash mismatch"));
    assert_eq!(topology, before);
}


#[test]
fn test_split_receipt_replays_with_reversible_parent_lineage() {
    let mut registry = replayed_cluster_registry();
    let formation_event_id = registry.clusters()["orb-1"].accepted_event_id.clone();
    let split = evolve_cluster(
        ClusterEvolutionRequest::split(
            "orb-1",
            vec!["orb-1-character".to_string(), "orb-1-evidence".to_string()],
            "splinter pressure from density rising while precision falls",
            vec![registry_metric_ref("sentinel:splinter_pressure:run-1")],
        ),
        &registry,
    );

    validate_cluster_evolution_event(&split).unwrap();
    registry.apply_evolution_event(&split).unwrap();

    assert!(!registry.clusters().contains_key("orb-1"));
    assert!(registry.retired_clusters().contains_key("orb-1"));
    let child = &registry.clusters()["orb-1-character"];
    assert!(child.lineage_event_ids.contains(&formation_event_id));
    assert!(child.lineage_event_ids.contains(&split.event_id));
    assert_eq!(child.source_event_refs.len(), 1);
    assert_eq!(registry.evolution_event_ids(), &[split.event_id]);
}


#[test]
fn test_split_receipt_appends_to_topology_ledger_and_replays() {
    let mut topology = m1_live_topology();
    let formation = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    topology
        .record_consolidation_event(formation.clone())
        .unwrap();
    let split = evolve_cluster(
        ClusterEvolutionRequest::split(
            "orb-1",
            vec!["orb-1-character".to_string(), "orb-1-evidence".to_string()],
            "splinter pressure from density rising while precision falls",
            vec![metric_ref(
                &topology,
                "sentinel:splinter_pressure:run-ledger",
            )],
        ),
        topology.clusters(),
    );

    topology.record_cluster_evolution_event(split.clone()).unwrap();
    let replayed = TopologyReplay::replay_ledger(topology.ledger()).unwrap();

    assert_eq!(replayed, topology);
    assert!(matches!(
        topology.ledger().events().last(),
        Some(LedgerEvent::ClusterEvolutionEvent(_))
    ));
    assert!(replayed.clusters().retired_clusters().contains_key("orb-1"));
    assert!(replayed.clusters().clusters()["orb-1-character"]
        .lineage_event_ids
        .contains(&formation.event_id));
    assert!(replayed.clusters().clusters()["orb-1-character"]
        .lineage_event_ids
        .contains(&split.event_id));
}

#[test]

fn test_split_receipt_rejects_missing_metric_evidence_event() {
    let mut topology = m1_live_topology();
    let formation = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    topology.record_consolidation_event(formation).unwrap();
    let split = evolve_cluster(
        ClusterEvolutionRequest::split(
            "orb-1",
            vec!["orb-1-character".to_string(), "orb-1-evidence".to_string()],
            "splinter pressure from unbacked metric evidence",
            vec![MetricEvidenceRef::new(
                "sentinel:splinter_pressure:missing",
                "missing-metric-event",
                "d".repeat(64),
            )],
        ),
        topology.clusters(),
    );
    let before = topology.clone();

    let error = topology.record_cluster_evolution_event(split).unwrap_err();

    assert!(error
        .to_string()
        .contains("references missing evidence event"));
    assert_eq!(topology, before);
}

#[test]
fn test_split_receipt_rejects_metric_evidence_hash_mismatch() {
    let mut topology = m1_live_topology();
    let formation = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    topology.record_consolidation_event(formation).unwrap();
    let mut evidence = metric_ref(&topology, "sentinel:splinter_pressure:forged");
    evidence.event_hash = "e".repeat(64);
    let split = evolve_cluster(
        ClusterEvolutionRequest::split(
            "orb-1",
            vec!["orb-1-character".to_string(), "orb-1-evidence".to_string()],
            "splinter pressure from forged metric evidence",
            vec![evidence],
        ),
        topology.clusters(),
    );
    let before = topology.clone();

    let error = topology.record_cluster_evolution_event(split).unwrap_err();


    assert!(error.to_string().contains("evidence event"));
    assert!(error.to_string().contains("hash mismatch"));
    assert_eq!(topology, before);
}

#[test]
fn test_replay_rejects_mismatched_metric_evidence_event() {
    let mut topology = m1_live_topology();
    let formation = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    topology
        .record_consolidation_event(formation.clone())
        .unwrap();
    let mut evidence = metric_ref(&topology, "sentinel:splinter_pressure:replay-forged");
    evidence.event_hash = "e".repeat(64);
    let forged_split = evolve_cluster(
        ClusterEvolutionRequest::split(
            "orb-1",
            vec!["orb-1-character".to_string(), "orb-1-evidence".to_string()],
            "splinter pressure from forged replay metric evidence",
            vec![evidence],
        ),
        topology.clusters(),
    );
    let mut events = topology.ledger().events().to_vec();
    events.push(LedgerEvent::ClusterEvolutionEvent(forged_split));

    let error = TopologyReplay::replay(&events).unwrap_err();

    assert!(error.to_string().contains("evidence event"));
    assert!(error.to_string().contains("hash mismatch"));
}

#[test]

fn test_merge_receipt_replays_without_losing_source_event_ancestry() {
    let mut registry = replayed_two_cluster_registry();
    let orb_1_event_id = registry.clusters()["orb-1"].accepted_event_id.clone();
    let orb_2_event_id = registry.clusters()["orb-2"].accepted_event_id.clone();
    let merge = evolve_cluster(
        ClusterEvolutionRequest::merge(
            vec!["orb-1".to_string(), "orb-2".to_string()],
            "orb-merged",
            "compatible manuscript evidence clusters",
            vec![registry_metric_ref("metric:compatibility:0.93")],
        ),
        &registry,
    );

    registry.apply_evolution_event(&merge).unwrap();

    assert!(registry.retired_clusters().contains_key("orb-1"));
    assert!(registry.retired_clusters().contains_key("orb-2"));
    let merged = &registry.clusters()["orb-merged"];
    assert!(merged.lineage_event_ids.contains(&orb_1_event_id));
    assert!(merged.lineage_event_ids.contains(&orb_2_event_id));
    assert!(merged.lineage_event_ids.contains(&merge.event_id));
    assert_eq!(merged.source_node_ids, vec!["node-1", "node-2"]);
    assert_eq!(merged.source_event_refs.len(), 1);
}

#[test]
fn test_forged_split_receipt_is_gated_before_claiming_mechanics() {
    let mut registry = replayed_cluster_registry();
    let mut split = evolve_cluster(
        ClusterEvolutionRequest::split(
            "orb-1",
            vec!["orb-1-a".to_string(), "orb-1-b".to_string()],
            "splinter pressure",
            vec![registry_metric_ref("sentinel:splinter_pressure:run-forged")],
        ),
        &registry,
    );
    split.child_orb_ids.push("orb-1-c".to_string());
    let before = registry.clone();

    let error = registry.apply_evolution_event(&split).unwrap_err();

    assert!(error
        .to_string()
        .contains("cluster evolution replay trace hash mismatch"));
    assert_eq!(registry, before);
}

fn consolidation_schema() -> Value {
    serde_json::from_str(include_str!(
        "../../../luna-memory/schemas/consolidation_event.schema.json"
    ))
    .unwrap()
}

fn compression_schema() -> Value {
    serde_json::from_str(include_str!(
        "../../../luna-memory/schemas/compression_receipt.schema.json"
    ))
    .unwrap()
}

fn accepted_receipt() -> Value {
    consolidation_schema()["examples"][0].clone()
}

fn rejected_receipt(reason: &str) -> Value {
    let mut receipt = accepted_receipt();
    receipt["decision"] = Value::String("rejected".to_string());
    receipt["rejection_reason"] = Value::String(reason.to_string());
    receipt
}

fn compression_accepted_receipt() -> Value {
    compression_schema()["examples"][0].clone()
}

fn compression_rejected_receipt() -> Value {
    compression_schema()["examples"][1].clone()
}

fn hex_hash() -> String {
    "a".repeat(64)
}

fn sample_event() -> RawEvent {
    RawEvent::from_draft(RawEventDraft::new(
        "event-1",
        EventSource::User,
        EventPayload::Text("Mara Vey carries the Tidefall chart.".to_string()),
    ))
}

fn metric_event() -> RawEvent {
    RawEvent::from_draft(RawEventDraft::new(
        "metric-event-1",
        EventSource::System,
        EventPayload::Text("Sentinel metric: splinter pressure rose above threshold.".to_string()),
    ))
}

fn metric_ref(topology: &ReplayedTopology, metric_id: &str) -> MetricEvidenceRef {
    let event = topology.ledger().get("metric-event-1").unwrap();
    MetricEvidenceRef::new(metric_id, &event.id, &event.hash)
}

fn registry_metric_ref(metric_id: &str) -> MetricEvidenceRef {
    MetricEvidenceRef::new(metric_id, "metric-event-1", "b".repeat(64))
}

fn compression_topology() -> ReplayedTopology {
    let mut topology = ReplayedTopology::default();
    for event in compression_events() {
        topology.record_raw_event(event).unwrap();
    }
    topology
}

fn compression_events() -> Vec<RawEvent> {
    vec![
        RawEvent::from_draft(RawEventDraft::new(
            "compression-event-1",
            EventSource::User,
            EventPayload::Text(
                "Raw scene evidence: Tidefall chart is in Mara's satchel.".to_string(),
            ),
        )),
        RawEvent::from_draft(RawEventDraft::new(
            "compression-event-2",
            EventSource::Assistant,
            EventPayload::Text("Raw scene evidence: the satchel is sealed at dusk.".to_string()),
        )),
    ]
}

fn compression_request(topology: &ReplayedTopology, compression_id: &str) -> CompressionRequest {
    let refs: Vec<SourceEventRef> = ["compression-event-1", "compression-event-2"]
        .into_iter()
        .map(|id| {
            let event = topology.ledger().get(id).unwrap();
            SourceEventRef::new(&event.id, &event.hash)
        })
        .collect();
    CompressionRequest::new(
        compression_id,
        refs.clone(),
        refs,
        "lossless-raw-ancestry-v1",
        "c".repeat(64),
    )
    .with_output_artifact(
        format!("artifact://{compression_id}"),
        compression_output_bytes(compression_id).as_bytes(),
    )
}

fn record_compression_artifact(topology: &mut ReplayedTopology, receipt: &CompressionReceipt) {
    topology
        .record_compression_artifact(CompressionArtifact::from_bytes(
            receipt.output_artifact_ref.clone(),
            compression_output_bytes(&receipt.compression_id).as_bytes(),
        ))
        .unwrap();
}

fn compression_output_bytes(compression_id: &str) -> String {
    format!("lossless compressed output for {compression_id}")
}

fn m1_live_topology() -> ReplayedTopology {
    let event = sample_event();
    let metric = metric_event();
    let mut topology = ReplayedTopology::default();
    topology.record_raw_event(event.clone()).unwrap();
    topology.record_raw_event(metric).unwrap();
    topology
        .commit(TopologyMutation::NodeCreated(NodeCreated::new(
            "node-1",
            NodeKind::Event,
            "Mara Vey carries the Tidefall chart",
            &event.id,
            &event.hash,
        )))
        .unwrap();
    topology
        .commit(TopologyMutation::NodeCreated(NodeCreated::new(
            "node-2",
            NodeKind::Evidence,
            "raw manuscript evidence",
            &event.id,
            &event.hash,
        )))
        .unwrap();
    topology
        .commit(TopologyMutation::GenesisAttached(GenesisAttached::new(
            "genesis-1",
            "node-1",
            &event.id,
            &event.hash,
        )))
        .unwrap();
    topology
        .commit(TopologyMutation::GenesisAttached(GenesisAttached::new(
            "genesis-2",
            "node-2",
            &event.id,
            &event.hash,
        )))
        .unwrap();
    topology
        .commit(TopologyMutation::TetherCreated(TetherCreated::new(
            "tether-1",
            "node-1",
            "node-2",
            Some(TetherKind::SupportedBy),
            TetherKind::EvidenceFor,
            &event.id,
            &event.hash,
        )))
        .unwrap();
    topology
}

fn orb_request(topology: &ReplayedTopology, cohesion_score: f64) -> ClusterFormationRequest {
    let event = topology.ledger().get("event-1").unwrap();
    ClusterFormationRequest::new(
        "orb-1",
        vec!["node-1".to_string(), "node-2".to_string()],
        vec!["tether-1".to_string()],
        vec![SourceEventRef::new(&event.id, &event.hash)],
        "shared-manuscript-evidence",
        cohesion_score,
    )
}

fn replayed_cluster_registry() -> luna_cluster::ClusterRegistry {
    let topology = m1_live_topology();
    let formation = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    luna_cluster::replay_consolidation_events(&[formation]).unwrap()
}

fn replayed_two_cluster_registry() -> luna_cluster::ClusterRegistry {
    let topology = m1_live_topology();
    let first = form_memory_cluster(orb_request(&topology, 0.91), &ClusterFormationPolicy::default());
    let mut second_request = orb_request(&topology, 0.9);
    second_request.orb_id = "orb-2".to_string();
    let second = form_memory_cluster(second_request, &ClusterFormationPolicy::default());
    luna_cluster::replay_consolidation_events(&[first, second]).unwrap()
}
