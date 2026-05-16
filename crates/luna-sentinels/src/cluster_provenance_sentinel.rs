use crate::{
    DefectClass, Sentinel, SentinelEvaluation, SentinelSchedule, TopologyView,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Sentinel that checks cluster provenance integrity:
/// - every node has a genesis certificate
/// - every tether has both endpoints present
/// - no orphan nodes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterProvenanceSentinel {
    pub stale_turn_threshold: u64,
    pub schedule: SentinelSchedule,
}

impl ClusterProvenanceSentinel {
    pub fn new(stale_turn_threshold: u64) -> Self {
        Self {
            stale_turn_threshold,
            schedule: SentinelSchedule::OnDemand,
        }
    }
}

impl Sentinel for ClusterProvenanceSentinel {
    fn name(&self) -> &'static str {
        "cluster_provenance"
    }

    fn defect_class(&self) -> DefectClass {
        DefectClass::ProvenanceIntegrity
    }

    fn schedule(&self) -> SentinelSchedule {
        self.schedule
    }

    fn evaluate(&self, view: &TopologyView) -> SentinelEvaluation {
        let nodes = view.nodes();
        let tethers = view.tethers();
        let genesis = view.genesis_certificates();

        let mut evidence: Vec<String> = Vec::new();

        // Check: every node has a genesis certificate
        let genesis_node_ids: BTreeSet<&str> =
            genesis.iter().map(|g| g.node_id.as_str()).collect();
        let all_node_ids: BTreeSet<&str> = nodes.iter().map(|n| n.node_id()).collect();

        for node_id in &all_node_ids {
            if !genesis_node_ids.contains(node_id) {
                evidence.push(format!(
                    "missing_genesis: node {} has no genesis certificate",
                    node_id
                ));
            }
        }

        // Check: every tether has both endpoints present
        for tether in tethers {
            if !all_node_ids.contains(tether.source_node_id.as_str()) {
                evidence.push(format!(
                    "broken_tether: source node {} missing for tether {}",
                    tether.source_node_id, tether.tether_id
                ));
            }
            if !all_node_ids.contains(tether.target_node_id.as_str()) {
                evidence.push(format!(
                    "broken_tether: target node {} missing for tether {}",
                    tether.target_node_id, tether.tether_id
                ));
            }
        }

        // Check: no orphan nodes (nodes with zero tethers)
        let tethered_nodes: BTreeSet<&str> = tethers
            .iter()
            .flat_map(|t| [t.source_node_id.as_str(), t.target_node_id.as_str()])
            .collect();

        for node_id in &all_node_ids {
            if !tethered_nodes.contains(node_id) {
                evidence.push(format!(
                    "orphan_node: node {} has no tethers connecting it to the graph",
                    node_id
                ));
            }
        }

        if evidence.is_empty() {
            SentinelEvaluation::Quiet
        } else {
            let count = evidence.len();
            SentinelEvaluation::Flag {
                score: count as f64 * 10.0,
                evidence,
                recommendation: format!(
                    "{} cluster provenance violation(s) found. Review genesis certificates, tether endpoints, and cluster connectivity.",
                    count
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ViewGenesisCertificate, ViewNode, ViewTether};

    fn make_view(
        nodes: Vec<ViewNode>,
        genesis: Vec<ViewGenesisCertificate>,
        tethers: Vec<ViewTether>,
    ) -> TopologyView {
        TopologyView::new(vec![], nodes, genesis, tethers, vec![])
    }

    fn node(id: &str) -> ViewNode {
        ViewNode::new(id, "ev1", "hash1")
    }

    fn genesis_cert(node_id: &str) -> ViewGenesisCertificate {
        ViewGenesisCertificate::new(
            format!("gen_{}", node_id),
            node_id,
            "ev1",
            "hash1",
        )
    }

    fn tether(id: &str, source: &str, target: &str) -> ViewTether {
        ViewTether::new(id, source, target, "ev1", "hash1", None)
    }

    #[test]
    fn clean_graph_produces_quiet() {
        let view = make_view(
            vec![node("A"), node("B")],
            vec![genesis_cert("A"), genesis_cert("B")],
            vec![tether("t1", "A", "B")],
        );
        let sentinel = ClusterProvenanceSentinel::new(50);
        assert_eq!(sentinel.evaluate(&view), SentinelEvaluation::Quiet);
    }

    #[test]
    fn missing_genesis_is_flagged() {
        let view = make_view(
            vec![node("A")],
            vec![], // no genesis cert
            vec![],
        );
        let sentinel = ClusterProvenanceSentinel::new(50);
        match sentinel.evaluate(&view) {
            SentinelEvaluation::Flag { evidence, .. } => {
                assert!(evidence.iter().any(|e| e.contains("missing_genesis")));
            }
            _ => panic!("Expected Flag"),
        }
    }

    #[test]
    fn broken_tether_is_flagged() {
        let view = make_view(
            vec![node("A")],
            vec![genesis_cert("A")],
            vec![tether("t1", "A", "B")], // B doesn't exist
        );
        let sentinel = ClusterProvenanceSentinel::new(50);
        match sentinel.evaluate(&view) {
            SentinelEvaluation::Flag { evidence, .. } => {
                assert!(evidence.iter().any(|e| e.contains("broken_tether")));
            }
            _ => panic!("Expected Flag"),
        }
    }

    #[test]
    fn orphan_node_is_flagged() {
        let view = make_view(
            vec![node("A"), node("B")],
            vec![genesis_cert("A"), genesis_cert("B")],
            vec![], // no tethers
        );
        let sentinel = ClusterProvenanceSentinel::new(50);
        match sentinel.evaluate(&view) {
            SentinelEvaluation::Flag { evidence, .. } => {
                assert!(evidence.iter().any(|e| e.contains("orphan_node")));
            }
            _ => panic!("Expected Flag"),
        }
    }

    #[test]
    fn deterministic_evaluation() {
        let view = make_view(
            vec![node("A"), node("B")],
            vec![genesis_cert("A"), genesis_cert("B")],
            vec![tether("t1", "A", "B")],
        );
        let sentinel = ClusterProvenanceSentinel::new(50);
        let a = sentinel.evaluate(&view);
        let b = sentinel.evaluate(&view);
        assert_eq!(a, b);
    }
}
