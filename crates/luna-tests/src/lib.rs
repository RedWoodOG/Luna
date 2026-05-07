//! Shared milestone test crate.
//!
//! The behavior contracts live in integration tests so they exercise the public
//! APIs of the topology crates.

//! Direct registry mutation is not a public M1 write path.
//!
//! ```compile_fail
//! use luna_ledger::NodeKind;
//! use luna_node::{MemoryNode, NodeRegistry};
//!
//! let mut registry = NodeRegistry::default();
//! let node = MemoryNode::new(
//!     "node-1",
//!     NodeKind::Event,
//!     "project journal",
//!     Some("event-1"),
//!     Some("hash"),
//! ).unwrap();
//! registry.insert(node).unwrap();
//! ```
//!
//! Direct tether construction and registry mutation are not public M1 write paths.
//!
//! ```compile_fail
//! use luna_ledger::{NodeKind, TetherKind};
//! use luna_node::MemoryNode;
//! use luna_tether::{Tether, TetherRegistry};
//!
//! let source = MemoryNode::new(
//!     "node-1",
//!     NodeKind::Event,
//!     "source",
//!     Some("event-1"),
//!     Some("hash"),
//! ).unwrap();
//! let target = MemoryNode::new(
//!     "node-2",
//!     NodeKind::Evidence,
//!     "target",
//!     Some("event-1"),
//!     Some("hash"),
//! ).unwrap();
//! let tether = Tether::new(
//!     "tether-1",
//!     &source,
//!     &target,
//!     Some(TetherKind::SupportedBy),
//!     TetherKind::EvidenceFor,
//!     "event-1",
//!     "hash",
//! ).unwrap();
//! let mut registry = TetherRegistry::default();
//! registry.insert(tether).unwrap();
//! ```
