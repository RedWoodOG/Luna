use chrono::Duration;
use luna_inspector::InspectorRejectReason;
use luna_ledger::{EventPayload, EventSource, InMemoryLedger, RawEvent, RawEventDraft};
use luna_ledger::{
    GenesisAttached, LedgerEvent, NodeCreated, NodeKind, TetherCreated, TetherKind,
    TopologyMutation,
};
use luna_replay::{ReplayLedger, ReplayedTopology, TopologyReplay};

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
    let node = TopologyMutation::NodeCreated(NodeCreated::new(
        "node-1",
        NodeKind::Event,
        "project journal",
        &event.id,
        &event.hash,
    ));
    let evidence_node = TopologyMutation::NodeCreated(NodeCreated::new(
        "node-2",
        NodeKind::Evidence,
        "raw event",
        &event.id,
        &event.hash,
    ));
    let certificate = TopologyMutation::GenesisAttached(GenesisAttached::new(
        "genesis-1",
        "node-1",
        &event.id,
        &event.hash,
    ));
    let evidence_certificate = TopologyMutation::GenesisAttached(GenesisAttached::new(
        "genesis-2",
        "node-2",
        &event.id,
        &event.hash,
    ));
    let tether = TopologyMutation::TetherCreated(TetherCreated::new(
        "tether-1",
        "node-1",
        "node-2",
        Some(TetherKind::SupportedBy),
        TetherKind::EvidenceFor,
        &event.id,
        &event.hash,
    ));
    let mut live = ReplayedTopology::default();

    live.record_raw_event(event).unwrap();
    live.commit(node).unwrap();
    live.commit(evidence_node).unwrap();
    live.commit(certificate).unwrap();
    live.commit(evidence_certificate).unwrap();
    live.commit(tether).unwrap();

    let replay_ledger = live.ledger.clone();
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
    let mut topology = ReplayedTopology::default();
    let node = TopologyMutation::NodeCreated(NodeCreated::new(
        "node-1",
        NodeKind::Event,
        "project journal",
        "missing-event",
        "hash",
    ));

    let error = topology.commit(node).unwrap_err();

    assert_eq!(
        error.reason(),
        &InspectorRejectReason::SourceEventMissing {
            event_id: "missing-event".to_string()
        }
    );
}

#[test]
fn test_genesis_certificate_is_created_once() {
    let event = sample_event();
    let mut topology = ReplayedTopology::default();
    topology.record_raw_event(event.clone()).unwrap();
    topology
        .commit(TopologyMutation::NodeCreated(NodeCreated::new(
            "node-1",
            NodeKind::Event,
            "project journal",
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

    let duplicate = topology
        .commit(TopologyMutation::GenesisAttached(GenesisAttached::new(
            "genesis-2",
            "node-1",
            &event.id,
            &event.hash,
        )))
        .unwrap_err();

    assert_eq!(
        duplicate.reason(),
        &InspectorRejectReason::DuplicateGenesis {
            node_id: "node-1".to_string()
        }
    );
}

#[test]
fn test_tether_requires_direction() {
    let event = sample_event();
    let mut topology = ReplayedTopology::default();
    topology.record_raw_event(event.clone()).unwrap();
    topology
        .commit(TopologyMutation::NodeCreated(NodeCreated::new(
            "node-1",
            NodeKind::Event,
            "project journal",
            &event.id,
            &event.hash,
        )))
        .unwrap();
    topology
        .commit(TopologyMutation::NodeCreated(NodeCreated::new(
            "node-2",
            NodeKind::Evidence,
            "raw event",
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

    let error = topology
        .commit(TopologyMutation::TetherCreated(TetherCreated::new(
            "tether-1",
            "node-1",
            "node-2",
            None,
            TetherKind::EvidenceFor,
            &event.id,
            &event.hash,
        )))
        .unwrap_err();

    assert_eq!(
        error.reason(),
        &InspectorRejectReason::DirectionMissing {
            tether_id: "tether-1".to_string()
        }
    );
}

#[test]
fn test_reverse_tether_has_distinct_meaning() {
    let event = sample_event();
    let forward = TetherCreated::new(
        "tether-1",
        "node-1",
        "node-2",
        Some(TetherKind::SupportedBy),
        TetherKind::EvidenceFor,
        &event.id,
        &event.hash,
    );
    let reverse = TetherCreated::new(
        "tether-2",
        "node-2",
        "node-1",
        Some(forward.reverse_kind),
        forward.kind.unwrap(),
        &event.id,
        &event.hash,
    );

    assert_eq!(forward.kind, Some(TetherKind::SupportedBy));
    assert_eq!(reverse.kind, Some(TetherKind::EvidenceFor));
    assert_ne!(forward.kind, reverse.kind);
}

#[test]
fn test_tether_rejects_same_reverse_meaning() {
    let event = sample_event();
    let mut topology = live_milestone_topology().0;

    let error = topology
        .commit(TopologyMutation::TetherCreated(TetherCreated::new(
            "tether-2",
            "node-1",
            "node-2",
            Some(TetherKind::SupportedBy),
            TetherKind::SupportedBy,
            &event.id,
            &event.hash,
        )))
        .unwrap_err();

    assert_eq!(
        error.reason(),
        &InspectorRejectReason::ReverseMeaningNotDistinct {
            tether_id: "tether-2".to_string()
        }
    );
}

#[test]
fn test_replay_reconstructs_identical_state() {
    let (live, replay_ledger) = live_milestone_topology();

    let replayed = TopologyReplay::replay_ledger(&replay_ledger).unwrap();

    assert_eq!(replayed, live);
    assert_eq!(replayed.ledger.raw_events().len(), 1);
    assert_eq!(replayed.ledger.events().len(), 6);
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
    let events = vec![LedgerEvent::TopologyMutation(
        TopologyMutation::NodeCreated(NodeCreated::new(
            "node-1",
            NodeKind::Event,
            "project journal",
            "event-1",
            "hash",
        )),
    )];

    let error = TopologyReplay::replay(&events).unwrap_err();

    assert!(error.to_string().contains("SourceEventMissing"));
}

#[test]
fn test_replay_rejects_forged_tether_with_undefined_reverse_meaning() {
    let event = sample_event();
    let forged = TetherCreated::new(
        "tether-1",
        "node-1",
        "node-2",
        Some(TetherKind::SupportedBy),
        TetherKind::SupportedBy,
        &event.id,
        &event.hash,
    );
    let events = vec![
        LedgerEvent::RawEventRecorded(event.clone()),
        LedgerEvent::TopologyMutation(TopologyMutation::NodeCreated(NodeCreated::new(
            "node-1",
            NodeKind::Event,
            "project journal",
            &event.id,
            &event.hash,
        ))),
        LedgerEvent::TopologyMutation(TopologyMutation::NodeCreated(NodeCreated::new(
            "node-2",
            NodeKind::Evidence,
            "raw event",
            &event.id,
            &event.hash,
        ))),
        LedgerEvent::TopologyMutation(TopologyMutation::GenesisAttached(GenesisAttached::new(
            "genesis-1",
            "node-1",
            &event.id,
            &event.hash,
        ))),
        LedgerEvent::TopologyMutation(TopologyMutation::GenesisAttached(GenesisAttached::new(
            "genesis-2",
            "node-2",
            &event.id,
            &event.hash,
        ))),
        LedgerEvent::TopologyMutation(TopologyMutation::TetherCreated(forged)),
    ];

    let error = TopologyReplay::replay(&events).unwrap_err();

    assert!(error.to_string().contains("ReverseMeaningNotDistinct"));
}
