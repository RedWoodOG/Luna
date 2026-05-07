use luna_inspector::{InspectorRejectReason, MutationRejected};
use luna_ledger::{
    EventPayload, EventSource, GenesisAttached, LedgerEvent, NodeCreated, NodeKind, RawEvent,
    RawEventDraft, TetherCreated, TetherKind, TopologyMutation,
};
use luna_replay::{ReplayedTopology, TopologyReplay};

fn sample_event() -> RawEvent {
    RawEvent::from_draft(RawEventDraft::new(
        "event-1",
        EventSource::User,
        EventPayload::Text("Chris started a new project journal.".to_string()),
    ))
}

fn node_created(node_id: &str, event: &RawEvent) -> TopologyMutation {
    TopologyMutation::NodeCreated(NodeCreated::new(
        node_id,
        NodeKind::Event,
        "project journal",
        &event.id,
        &event.hash,
    ))
}

fn evidence_created(node_id: &str, event: &RawEvent) -> TopologyMutation {
    TopologyMutation::NodeCreated(NodeCreated::new(
        node_id,
        NodeKind::Evidence,
        "raw event evidence",
        &event.id,
        &event.hash,
    ))
}

fn genesis_attached(certificate_id: &str, node_id: &str, event: &RawEvent) -> TopologyMutation {
    TopologyMutation::GenesisAttached(GenesisAttached::new(
        certificate_id,
        node_id,
        &event.id,
        &event.hash,
    ))
}

fn tether_created(
    tether_id: &str,
    source_node_id: &str,
    target_node_id: &str,
    event: &RawEvent,
) -> TopologyMutation {
    TopologyMutation::TetherCreated(TetherCreated::new(
        tether_id,
        source_node_id,
        target_node_id,
        Some(TetherKind::SupportedBy),
        TetherKind::EvidenceFor,
        &event.id,
        &event.hash,
    ))
}

fn m1_live_topology() -> ReplayedTopology {
    let event = sample_event();
    let mut topology = ReplayedTopology::default();

    topology.record_raw_event(event.clone()).unwrap();
    topology.commit(node_created("node-1", &event)).unwrap();
    topology.commit(evidence_created("node-2", &event)).unwrap();
    topology
        .commit(genesis_attached("genesis-1", "node-1", &event))
        .unwrap();
    topology
        .commit(genesis_attached("genesis-2", "node-2", &event))
        .unwrap();
    topology
        .commit(tether_created("tether-1", "node-1", "node-2", &event))
        .unwrap();

    topology
}

#[test]
fn test_mutations_flow_through_append_only_ledger() {
    let topology = m1_live_topology();
    let events = topology.ledger.events();

    assert_eq!(events.len(), 6);
    assert!(matches!(events[0], LedgerEvent::RawEventRecorded(_)));
    assert!(matches!(
        events[1],
        LedgerEvent::TopologyMutation(TopologyMutation::NodeCreated(_))
    ));
    assert!(matches!(
        events[3],
        LedgerEvent::TopologyMutation(TopologyMutation::GenesisAttached(_))
    ));
    assert!(matches!(
        events[5],
        LedgerEvent::TopologyMutation(TopologyMutation::TetherCreated(_))
    ));
}

#[test]
fn test_commit_pipeline_preserves_live_replay_equality() {
    let live = m1_live_topology();

    let replayed = TopologyReplay::replay_ledger(&live.ledger).unwrap();

    assert_eq!(replayed, live);
}

#[test]
fn test_missing_source_event_rejects_with_specific_error() {
    let event = sample_event();
    let mut topology = ReplayedTopology::default();

    let error = topology
        .commit(TopologyMutation::NodeCreated(NodeCreated::new(
            "node-1",
            NodeKind::Event,
            "project journal",
            "missing-event",
            &event.hash,
        )))
        .unwrap_err();

    assert_reject_reason(
        error,
        InspectorRejectReason::SourceEventMissing {
            event_id: "missing-event".to_string(),
        },
    );
}

#[test]
fn test_tether_missing_direction_rejects_with_specific_error() {
    let event = sample_event();
    let mut topology = ReplayedTopology::default();
    topology.record_raw_event(event.clone()).unwrap();
    topology.commit(node_created("node-1", &event)).unwrap();
    topology.commit(evidence_created("node-2", &event)).unwrap();
    topology
        .commit(genesis_attached("genesis-1", "node-1", &event))
        .unwrap();
    topology
        .commit(genesis_attached("genesis-2", "node-2", &event))
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

    assert_reject_reason(
        error,
        InspectorRejectReason::DirectionMissing {
            tether_id: "tether-1".to_string(),
        },
    );
}

#[test]
fn test_duplicate_genesis_rejects_with_specific_error() {
    let event = sample_event();
    let mut topology = ReplayedTopology::default();
    topology.record_raw_event(event.clone()).unwrap();
    topology.commit(node_created("node-1", &event)).unwrap();
    topology
        .commit(genesis_attached("genesis-1", "node-1", &event))
        .unwrap();

    let error = topology
        .commit(genesis_attached("genesis-2", "node-1", &event))
        .unwrap_err();

    assert_reject_reason(
        error,
        InspectorRejectReason::DuplicateGenesis {
            node_id: "node-1".to_string(),
        },
    );
}

#[test]
fn test_unresolved_tether_endpoint_rejects_with_specific_error() {
    let event = sample_event();
    let mut topology = ReplayedTopology::default();
    topology.record_raw_event(event.clone()).unwrap();
    topology.commit(node_created("node-1", &event)).unwrap();
    topology
        .commit(genesis_attached("genesis-1", "node-1", &event))
        .unwrap();

    let error = topology
        .commit(tether_created("tether-1", "node-1", "missing-node", &event))
        .unwrap_err();

    assert_reject_reason(
        error,
        InspectorRejectReason::EndpointMissing {
            tether_id: "tether-1".to_string(),
            node_id: "missing-node".to_string(),
        },
    );
}

fn assert_reject_reason(error: MutationRejected, expected: InspectorRejectReason) {
    assert_eq!(error.reason(), &expected);
}
