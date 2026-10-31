//! why-core: justification reachability engine
//!
//! A node is alive iff there exists a path to at least one root
//! whose attestation says alive. Unknown is first-class.

mod dominators;
mod parse;
mod reach;

pub use dominators::{dom_chain, dom_subtree, reverse_dominators, IDom};
pub use parse::{parse_attestation, parse_graph, ParseError};
pub use reach::Evaluation;

use std::collections::{HashMap, HashSet};

pub type NodeId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    Explicit,
    Inferred,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub child: NodeId,
    pub parent: NodeId,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    Alive,
    Dead,
}

#[derive(Debug, Clone)]
pub struct RootAttestation {
    pub id: NodeId,
    pub status: Liveness,
    pub since: Option<String>,
    pub by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Alive,
    Dead,
    Unknown,
    Absent,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Alive => write!(f, "alive"),
            Status::Dead => write!(f, "dead"),
            Status::Unknown => write!(f, "unknown"),
            Status::Absent => write!(f, "absent"),
        }
    }
}

#[derive(Debug, Default)]
pub struct Graph {
    /// child -> list of (parent, provenance)
    pub edges: HashMap<NodeId, Vec<(NodeId, Provenance)>>,
    /// all nodes we have ever seen
    pub nodes: HashSet<NodeId>,
}

impl Graph {
    pub fn add_edge(&mut self, child: NodeId, parent: NodeId, provenance: Provenance) {
        self.nodes.insert(child.clone());
        self.nodes.insert(parent.clone());
        self.edges
            .entry(child)
            .or_default()
            .push((parent, provenance));
    }
}

#[derive(Debug, Default)]
pub struct Attestation {
    pub roots: HashMap<NodeId, RootAttestation>,
}

impl Attestation {
    pub fn insert(&mut self, root: RootAttestation) {
        self.roots.insert(root.id.clone(), root);
    }
}
