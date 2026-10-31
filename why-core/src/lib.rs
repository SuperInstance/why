//! why-core: justification reachability engine
//!
//! A node is alive iff there exists a path of EXPLICIT edges to at least one
//! root whose attestation is currently alive (and not expired or stale).
//!
//! v0.2 semantics:
//! - Inferred edges never confer status. A node whose only path to any root
//!   runs through an inferred edge is `unknown` — a candidate justification
//!   awaiting human confirmation, never a verdict.
//! - Roots are mortal: `until` is a hard expiry (dead after the date), and
//!   `review-by` is a soft expiry (unknown after the date + 30d grace).
//! - `all-of:` groups are conjunctive: alive only when every member is alive.

mod dates;
mod dominators;
mod parse;
mod reach;

pub use dates::{add_days, is_valid_date, today};
pub use dominators::{dom_chain, dom_subtree, reverse_dominators, IDom};
pub use parse::{parse_attestation, parse_graph, ParseError};
pub use reach::{Coverage, Evaluation};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

/// Effective root status at the evaluation date.
///
/// `Stale` means: attested alive, but the `review-by` soft expiry (plus the
/// 30-day grace period) has passed. A stale attestation is never silently
/// alive — it degrades the root (and everything that depends on it) to
/// `unknown` until re-attested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootStatus {
    Alive,
    Dead,
    Stale,
}

#[derive(Debug, Clone)]
pub struct RootAttestation {
    pub id: NodeId,
    pub status: Liveness,
    pub since: Option<String>,
    pub by: Option<String>,
    /// Hard expiry (YYYY-MM-DD). After this date the root is dead. No grace.
    pub until: Option<String>,
    /// Soft expiry (YYYY-MM-DD). After this date + 30 days of grace the root
    /// degrades to unknown until re-attested.
    pub review_by: Option<String>,
}

impl RootAttestation {
    /// Effective liveness at `as_of` (YYYY-MM-DD). `None` means no clock:
    /// expiry fields are ignored entirely (useful for tests and offline diffs).
    pub fn effective(&self, as_of: Option<&str>) -> RootStatus {
        if let (Some(until), Some(now)) = (self.until.as_deref(), as_of) {
            if until < now {
                return RootStatus::Dead;
            }
        }
        match self.status {
            Liveness::Dead => RootStatus::Dead,
            Liveness::Alive => {
                if let (Some(rb), Some(now)) = (self.review_by.as_deref(), as_of) {
                    if let Some(expiry) = add_days(rb, 30) {
                        if expiry.as_str() < now {
                            return RootStatus::Stale;
                        }
                    }
                }
                RootStatus::Alive
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct Graph {
    /// child -> list of (parent, provenance)
    pub edges: HashMap<NodeId, Vec<(NodeId, Provenance)>>,
    /// all nodes we have ever seen
    pub nodes: HashSet<NodeId>,
    /// Conjunctive groups (v0.2): child -> list of groups. A group confers
    /// liveness only when ALL of its members are alive. Written as
    /// `child -> all-of: p1 p2 p3`.
    pub conjunctive: HashMap<NodeId, Vec<Vec<NodeId>>>,
    /// Discovered-artifact count declared by a `# discovered: N` header
    /// (written by `why extract`). Coverage = wired / discovered.
    pub discovered: Option<usize>,
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

    pub fn add_group(&mut self, child: NodeId, parents: Vec<NodeId>) {
        self.nodes.insert(child.clone());
        for p in &parents {
            self.nodes.insert(p.clone());
        }
        self.conjunctive.entry(child).or_default().push(parents);
    }

    /// Nodes that carry at least one justification (children of edges or
    /// members of a conjunctive group's child side).
    pub fn wired(&self) -> usize {
        let mut wired: HashSet<&NodeId> = self.edges.keys().collect();
        wired.extend(self.conjunctive.keys());
        wired.len()
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
