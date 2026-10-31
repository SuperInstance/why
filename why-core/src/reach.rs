use std::collections::{HashMap, HashSet, VecDeque};

use crate::dominators::{reverse_dominators, IDom};
use crate::{Attestation, Graph, Liveness, NodeId, Status};

#[derive(Debug)]
pub struct Evaluation {
    pub status: HashMap<NodeId, Status>,
    /// predecessor used to reach this node (for derivation chains)
    pub pred: HashMap<NodeId, NodeId>,
    /// reverse dominator tree on the Alive subgraph (node → idom)
    pub idom: IDom,
}

impl Graph {
    /// Evaluate status of every node given the current attestations.
    pub fn evaluate(&self, att: &Attestation) -> Evaluation {
        // reverse adjacency: parent -> [children]
        let mut reverse: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (child, parents) in &self.edges {
            for (parent, _) in parents {
                reverse.entry(parent.clone()).or_default().push(child.clone());
            }
        }

        let mut status: HashMap<NodeId, Status> = HashMap::new();
        let mut pred: HashMap<NodeId, NodeId> = HashMap::new();
        let mut visited = HashSet::new();

        // Pass 1: propagate Alive from live roots (walk reverse edges)
        let mut queue = VecDeque::new();
        for (id, root) in &att.roots {
            if root.status == Liveness::Alive {
                status.insert(id.clone(), Status::Alive);
                queue.push_back(id.clone());
                visited.insert(id.clone());
            }
        }

        while let Some(node) = queue.pop_front() {
            if let Some(children) = reverse.get(&node) {
                for child in children {
                    if visited.insert(child.clone()) {
                        status.insert(child.clone(), Status::Alive);
                        pred.insert(child.clone(), node.clone());
                        queue.push_back(child.clone());
                    }
                }
            }
        }

        // Pass 2: mark nodes that can reach an *unattested parent/root* as Unknown.
        // An unattested root is any node that appears as a parent in an edge
        // but has no entry in the attestation map.
        let mut unattested_roots = HashSet::new();
        for parents in self.edges.values() {
            for (parent, _) in parents {
                if !att.roots.contains_key(parent) {
                    unattested_roots.insert(parent.clone());
                }
            }
        }

        let mut uqueue = VecDeque::new();
        for u in &unattested_roots {
            if !status.contains_key(u) {
                status.insert(u.clone(), Status::Unknown);
                uqueue.push_back(u.clone());
            }
        }

        let mut ureached = HashSet::new();
        while let Some(node) = uqueue.pop_front() {
            if ureached.insert(node.clone()) {
                if let Some(children) = reverse.get(&node) {
                    for child in children {
                        if !status.contains_key(child) {
                            status.insert(child.clone(), Status::Unknown);
                            uqueue.push_back(child.clone());
                        }
                    }
                }
            }
        }

        // Pass 3: remaining nodes with any connectivity are Dead; isolated are Absent
        for node in &self.nodes {
            if status.contains_key(node) {
                continue;
            }
            let has_edges = self.edges.contains_key(node) || reverse.contains_key(node);
            if has_edges {
                status.insert(node.clone(), Status::Dead);
            } else {
                status.insert(node.clone(), Status::Absent);
            }
        }

        // Explicitly attested Dead roots are Dead if not already Alive
        for (id, root) in &att.roots {
            if root.status == Liveness::Dead {
                status.entry(id.clone()).or_insert(Status::Dead);
            }
        }

        // Reverse dominator tree on Alive nodes only.
        let mut alive_set = HashSet::new();
        let mut live_roots = Vec::new();
        for (id, st) in &status {
            if *st == Status::Alive {
                alive_set.insert(id.clone());
            }
        }
        for (id, root) in &att.roots {
            if root.status == Liveness::Alive && alive_set.contains(id) {
                live_roots.push(id.clone());
            }
        }
        live_roots.sort();
        let idom = reverse_dominators(&alive_set, &live_roots, &reverse);

        Evaluation { status, pred, idom }
    }

    /// Reconstruct a justification chain from a node toward roots.
    /// Prefers the pred map (from alive propagation). Falls back to
    /// walking the first outgoing edge so dead/unknown nodes still
    /// show their structural "because".
    pub fn chain(&self, eval: &Evaluation, id: &str) -> Vec<NodeId> {
        let mut chain = vec![id.to_string()];
        let mut current = id.to_string();
        let mut guard = 0;
        loop {
            if let Some(prev) = eval.pred.get(&current) {
                chain.push(prev.clone());
                current = prev.clone();
            } else if let Some(parents) = self.edges.get(&current) {
                if let Some((parent, _)) = parents.first() {
                    chain.push(parent.clone());
                    current = parent.clone();
                } else {
                    break;
                }
            } else {
                break;
            }
            guard += 1;
            if guard > 10_000 {
                break; // cycle protection
            }
        }
        chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_attestation, parse_graph};

    #[test]
    fn alive_propagation() {
        let g = parse_graph(
            r#"
role:analyst -> project:active (explicit)
workflow:deploy -> project:active (explicit)
"#,
        )
        .unwrap();
        let a = parse_attestation("project:active alive 2024-01-01 casey").unwrap();
        let eval = g.evaluate(&a);
        assert_eq!(eval.status["role:analyst"], Status::Alive);
        assert_eq!(eval.status["workflow:deploy"], Status::Alive);
        assert_eq!(eval.status["project:active"], Status::Alive);
    }

    #[test]
    fn dead_when_root_dead() {
        let g = parse_graph(
            r#"
role:legacy-export -> project:old-migration (explicit)
workflow:legacy-step -> project:old-migration (inferred)
"#,
        )
        .unwrap();
        let a = parse_attestation("project:old-migration dead 2023-04-12 ci").unwrap();
        let eval = g.evaluate(&a);
        assert_eq!(eval.status["role:legacy-export"], Status::Dead);
        assert_eq!(eval.status["workflow:legacy-step"], Status::Dead);
        assert_eq!(eval.status["project:old-migration"], Status::Dead);
    }

    #[test]
    fn unknown_when_unattested() {
        let g = parse_graph("service:billing -> ticket:BILL-99 (inferred)").unwrap();
        let a = parse_attestation("").unwrap(); // empty
        let eval = g.evaluate(&a);
        assert_eq!(eval.status["service:billing"], Status::Unknown);
        assert_eq!(eval.status["ticket:BILL-99"], Status::Unknown);
    }
}
