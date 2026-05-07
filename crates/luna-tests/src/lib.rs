//! Shared milestone test crate.
//!
//! The behavior contracts live in integration tests so they exercise the public
//! APIs of the topology crates.

//! Direct registry mutation is not a public M1 write path.
//!
//! ```compile_fail
//! use luna_ledger::{NodeCreated, NodeKind};
//! use luna_node::NodeRegistry;
//!
//! let mut registry = NodeRegistry::default();
//! let event = NodeCreated::new(
//!     "node-1",
//!     NodeKind::Event,
//!     "project journal",
//!     "event-1",
//!     "hash",
//! );
//! registry.apply_created(&event).unwrap();
//! ```
//!
//! Direct tether construction and registry mutation are not public M1 write paths.
//!
//! ```compile_fail
//! use luna_ledger::{TetherCreated, TetherKind};
//! use luna_tether::TetherRegistry;
//!
//! let event = TetherCreated::new(
//!     "tether-1",
//!     "node-1",
//!     "node-2",
//!     Some(TetherKind::SupportedBy),
//!     TetherKind::EvidenceFor,
//!     "event-1",
//!     "hash",
//! );
//! let mut registry = TetherRegistry::default();
//! registry.apply_created(&event).unwrap();
//! ```
//!
//! Direct genesis registry mutation is not a public M1 write path.
//!
//! ```compile_fail
//! use luna_genesis::GenesisRegistry;
//! use luna_ledger::GenesisAttached;
//!
//! let event = GenesisAttached::new("genesis-1", "node-1", "event-1", "hash");
//! let mut registry = GenesisRegistry::default();
//! registry.apply_attached(&event).unwrap();
//! ```
//!
//! Safe callers cannot append topology mutations directly to the ledger.
//!
//! ```compile_fail
//! use luna_ledger::{
//!     InMemoryLedger, NodeCreated, NodeKind, TopologyMutation,
//! };
//!
//! let mut ledger = InMemoryLedger::default();
//! let mutation = TopologyMutation::NodeCreated(NodeCreated::new(
//!     "node-1",
//!     NodeKind::Event,
//!     "project journal",
//!     "event-1",
//!     "hash",
//! ));
//! ledger.append_mutation_unchecked(mutation).unwrap();
//! ```
