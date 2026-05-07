use chrono::Duration;
use luna_genesis::{GenesisCertificate, GenesisRegistry};
use luna_ledger::{EventPayload, EventSource, InMemoryLedger, RawEvent, RawEventDraft};
use luna_node::{MemoryNode, NodeKind};
use luna_replay::{ReplayEvent, ReplayLedger, ReplayedTopology, TopologyReplay};
use luna_tether::{Tether, TetherKind};

fn sample_event() -> RawEvent {
    RawEvent::from_draft(RawEventDraft::new(
        "event-1",
        EventSource::User,
        EventPayload::Text("Chris started a new project journal.".to_string()),
    ))
}

fn second_event() -> RawEvent {
    RawEvent::from_draft(RawEventDraft::new(
        "event-1",
        EventSource::System,
        EventPayload::Text("The node is supported by its raw event.".to_string()),
    ))
}

fn live_milestone_topology() -> (ReplayedTopology, ReplayLedger) {
    let event = sample_event();
    let node = MemoryNode::from_event("node-1", NodeKind::Event, "project journal", &event);
    let evidence_node = MemoryNode::from_event("node-2", NodeKind::Evidence, "raw event", &event);
    let certificate = GenesisCertificate::for_node("genesis-1", &node, &event).unwrap();
    let evidence_certificate =
        GenesisCertificate::for_node("genesis-2", &evidence_node, &event).unwrap();
    let tether = Tether::new(
        "tether-1",
        &node,
        &evidence_node,
        Some(TetherKind::SupportedBy),
        TetherKind::EvidenceFor,
        &event,
    )
    .unwrap();
    let mut live = ReplayedTopology::default();
    let mut replay_ledger = ReplayLedger::default();

    live.ledger.append(event.clone()).unwrap();
    replay_ledger.append(ReplayEvent::RawEventRecorded(event));
    live.nodes.insert(node.clone()).unwrap();
    replay_ledger.append(ReplayEvent::NodeCreated(node));
    live.nodes.insert(evidence_node.clone()).unwrap();
    replay_ledger.append(ReplayEvent::NodeCreated(evidence_node));
    live.genesis_certificates
        .insert(certificate.clone())
        .unwrap();
    replay_ledger.append(ReplayEvent::GenesisCertificateCreated(certificate));
    live.genesis_certificates
        .insert(evidence_certificate.clone())
        .unwrap();
    replay_ledger.append(ReplayEvent::GenesisCertificateCreated(evidence_certificate));
    live.tethers.insert(tether.clone()).unwrap();
    replay_ledger.append(ReplayEvent::TetherCreated(tether));

    (live, replay_ledger)
}

#[test]
fn test_event_is_immutable() {
    let mut ledger = InMemoryLedger::default();
    let event = sample_event();
    let changed = second_event();

    ledger.append(event).unwrap();

    let error = ledger.append(changed).unwrap_err();

    assert!(error.to_string().contains("already exists"));
}

#[test]
fn test_event_hash_is_stable() {
    let first = sample_event();
    let second = sample_event();

    assert_eq!(first.hash, second.hash);
}

#[test]
fn test_event_hash_excludes_recorded_at() {
    let first = sample_event();
    let mut second = first.clone();
    second.recorded_at = first.recorded_at + Duration::seconds(1);

    assert_eq!(first.hash, second.hash);
    second.verify_hash().unwrap();
}

#[test]
fn test_event_hash_changes_when_content_changes() {
    let first = sample_event();
    let second = second_event();

    assert_ne!(first.hash, second.hash);
}

#[test]
fn test_node_requires_source_event() {
    let node = MemoryNode::new(
        "node-1",
        NodeKind::Event,
        "project journal",
        None,
        Some("hash"),
    );

    assert!(node.is_err());
}

#[test]
fn test_genesis_certificate_is_created_once() {
    let event = sample_event();
    let node = MemoryNode::from_event("node-1", NodeKind::Event, "project journal", &event);
    let certificate = GenesisCertificate::for_node("genesis-1", &node, &event).unwrap();
    let duplicate = GenesisCertificate::for_node("genesis-2", &node, &event).unwrap();
    let mut registry = GenesisRegistry::default();

    registry.insert(certificate).unwrap();

    assert!(registry.insert(duplicate).is_err());
}

#[test]
fn test_tether_requires_direction() {
    let event = sample_event();
    let source = MemoryNode::from_event("node-1", NodeKind::Event, "project journal", &event);
    let target = MemoryNode::from_event("node-2", NodeKind::Evidence, "raw event", &event);

    let tether = Tether::new(
        "tether-1",
        &source,
        &target,
        None,
        TetherKind::EvidenceFor,
        &event,
    );

    assert!(tether.is_err());
}

#[test]
fn test_reverse_tether_has_distinct_meaning() {
    let event = sample_event();
    let source = MemoryNode::from_event("node-1", NodeKind::Event, "project journal", &event);
    let target = MemoryNode::from_event("node-2", NodeKind::Evidence, "raw event", &event);
    let forward = Tether::new(
        "tether-1",
        &source,
        &target,
        Some(TetherKind::SupportedBy),
        TetherKind::EvidenceFor,
        &event,
    )
    .unwrap();
    let reverse = forward.reverse("tether-2").unwrap();

    assert_eq!(forward.kind, TetherKind::SupportedBy);
    assert_eq!(reverse.kind, TetherKind::EvidenceFor);
    assert_ne!(forward.kind, reverse.kind);
}

#[test]
fn test_tether_rejects_same_reverse_meaning() {
    let event = sample_event();
    let source = MemoryNode::from_event("node-1", NodeKind::Event, "project journal", &event);
    let target = MemoryNode::from_event("node-2", NodeKind::Evidence, "raw event", &event);

    let tether = Tether::new(
        "tether-1",
        &source,
        &target,
        Some(TetherKind::SupportedBy),
        TetherKind::SupportedBy,
        &event,
    );

    assert!(tether.is_err());
}

#[test]
fn test_replay_reconstructs_identical_state() {
    let (live, replay_ledger) = live_milestone_topology();

    let replayed = TopologyReplay::replay_ledger(&replay_ledger).unwrap();

    assert_eq!(replayed, live);
    assert_eq!(replayed.ledger.events().len(), 1);
    assert_eq!(replayed.nodes.nodes().len(), 2);
    assert_eq!(replayed.genesis_certificates.certificates().len(), 2);
    assert_eq!(replayed.tethers.tethers().len(), 1);
}

#[test]
fn test_replay_is_deterministic_for_same_ledger() {
    let (_, replay_ledger) = live_milestone_topology();

    let first = TopologyReplay::replay_ledger(&replay_ledger).unwrap();
    let second = TopologyReplay::replay_ledger(&replay_ledger).unwrap();

    assert_eq!(first, second);
}

#[test]
fn test_missing_provenance_fails() {
    let event = sample_event();
    let node = MemoryNode::from_event("node-1", NodeKind::Event, "project journal", &event);
    let events = vec![ReplayEvent::NodeCreated(node)];

    let error = TopologyReplay::replay(&events).unwrap_err();

    assert!(error.to_string().contains("missing source event"));
}

#[test]
fn test_replay_rejects_forged_tether_with_undefined_reverse_meaning() {
    let event = sample_event();
    let node = MemoryNode::from_event("node-1", NodeKind::Event, "project journal", &event);
    let evidence_node = MemoryNode::from_event("node-2", NodeKind::Evidence, "raw event", &event);
    let certificate = GenesisCertificate::for_node("genesis-1", &node, &event).unwrap();
    let evidence_certificate =
        GenesisCertificate::for_node("genesis-2", &evidence_node, &event).unwrap();
    let mut forged = Tether::new(
        "tether-1",
        &node,
        &evidence_node,
        Some(TetherKind::SupportedBy),
        TetherKind::EvidenceFor,
        &event,
    )
    .unwrap();
    forged.reverse_kind = TetherKind::SupportedBy;
    let events = vec![
        ReplayEvent::RawEventRecorded(event),
        ReplayEvent::NodeCreated(node),
        ReplayEvent::NodeCreated(evidence_node),
        ReplayEvent::GenesisCertificateCreated(certificate),
        ReplayEvent::GenesisCertificateCreated(evidence_certificate),
        ReplayEvent::TetherCreated(forged),
    ];

    let error = TopologyReplay::replay(&events).unwrap_err();

    assert!(error.to_string().contains("reverse tether meaning"));
}
