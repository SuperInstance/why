//! Status evaluation over the justification graph.
//!
//! v0.2 semantics, in precedence order:
//!
//! 1. `alive` — a path of EXPLICIT edges to a live root exists, or a
//!    conjunctive group whose members are ALL alive.
//! 2. `unknown` — the evidence is incomplete: an unattested or stale root is
//!    reachable, or the only path to a root runs through an INFERRED edge
//!    (candidate justifications never become verdicts), or a conjunctive
//!    group has an unknown member.
//! 3. `dead` — certainty that no live reason exists: an explicit path to a
//!    dead (or `until`-expired) root, or every conjunctive group contains a
//!    dead member.
//!
//! Inferred edges can therefore never produce a `dead` finding either: the
//! tool refuses to recommend removal on a guessed link.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::dominators::{reverse_dominators, IDom};
use crate::{Attestation, Graph, Liveness, NodeId, Provenance, RootStatus, Status};

/// Wired/discovered coverage. `discovered` comes from the graph's
/// `# discovered: N` header (written by `why extract`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coverage {
    pub wired: usize,
    pub discovered: usize,
}

impl Coverage {
    /// Percentage of discovered artifacts that carry a justification edge.
    pub fn pct(&self) -> f64 {
        if self.discovered == 0 {
            return 0.0;
        }
        (self.wired as f64 / self.discovered as f64).min(1.0) * 100.0
    }
}

#[derive(Debug)]
pub struct Evaluation {
    pub status: HashMap<NodeId, Status>,
    /// predecessor used to reach this node (for derivation chains)
    pub pred: HashMap<NodeId, NodeId>,
    /// reverse dominator tree on the Alive subgraph (node → idom)
    pub idom: IDom,
    /// coverage, if the graph declared a `# discovered: N` header
    pub coverage: Option<Coverage>,
    /// alive-attested roots that went stale (review-by + grace passed)
    pub stale_roots: Vec<NodeId>,
    /// alive-attested roots that hard-expired via `until`
    pub expired_roots: Vec<NodeId>,
}

impl Graph {
    /// Evaluate the status of every node given the attestations and the
    /// evaluation date. `as_of` is None for "no clock" (expiry fields
    /// ignored); the CLI defaults it to today.
    pub fn evaluate(&self, att: &Attestation, as_of: Option<&str>) -> Evaluation {
        // ---- classify roots ----
        let mut live_roots: Vec<NodeId> = Vec::new();
        let mut dead_roots: HashSet<NodeId> = HashSet::new();
        let mut stale_roots: Vec<NodeId> = Vec::new();
        let mut expired_roots: Vec<NodeId> = Vec::new();
        for (id, r) in &att.roots {
            match r.effective(as_of) {
                RootStatus::Alive => live_roots.push(id.clone()),
                RootStatus::Dead => {
                    if matches!(r.status, Liveness::Alive) && r.until.is_some() {
                        expired_roots.push(id.clone());
                    }
                    dead_roots.insert(id.clone());
                }
                RootStatus::Stale => stale_roots.push(id.clone()),
            }
        }
        live_roots.sort();
        stale_roots.sort();
        expired_roots.sort();

        let mut status: HashMap<NodeId, Status> = HashMap::new();
        for r in &dead_roots {
            status.insert(r.clone(), Status::Dead);
        }
        for r in &stale_roots {
            status.insert(r.clone(), Status::Unknown);
        }

        // reverse adjacency: parent -> [(child, provenance)]
        let mut reverse: HashMap<NodeId, Vec<(NodeId, Provenance)>> = HashMap::new();
        for (child, parents) in &self.edges {
            for (parent, prov) in parents {
                reverse
                    .entry(parent.clone())
                    .or_default()
                    .push((child.clone(), prov.clone()));
            }
        }

        // ---- Pass 1: explicit-alive fixpoint (edges + conjunctive groups) ----
        let mut alive_set: HashSet<NodeId> = live_roots.iter().cloned().collect();
        let mut pred: HashMap<NodeId, NodeId> = HashMap::new();
        loop {
            let before = alive_set.len();
            let mut queue: VecDeque<NodeId> = live_roots.clone().into();
            let mut visited: HashSet<NodeId> = live_roots.iter().cloned().collect();
            while let Some(node) = queue.pop_front() {
                if let Some(children) = reverse.get(&node) {
                    for (child, prov) in children {
                        if matches!(prov, Provenance::Explicit)
                            && visited.insert(child.clone())
                        {
                            alive_set.insert(child.clone());
                            pred.entry(child.clone()).or_insert_with(|| node.clone());
                            queue.push_back(child.clone());
                        }
                    }
                }
            }
            for (child, groups) in &self.conjunctive {
                if alive_set.contains(child) {
                    continue;
                }
                for group in groups {
                    if group.iter().all(|m| alive_set.contains(m)) {
                        alive_set.insert(child.clone());
                        pred.entry(child.clone()).or_insert_with(|| group[0].clone());
                        break;
                    }
                }
            }
            if alive_set.len() == before {
                break;
            }
        }
        for n in &alive_set {
            status.insert(n.clone(), Status::Alive);
        }

        // ---- Pass 2: candidate justification via INFERRED edges -> unknown ----
        // Anything reachable from a live root through any edges but NOT
        // alive through explicit ones is a candidate: unknown, never alive.
        let mut seen_any: HashSet<NodeId> = live_roots.iter().cloned().collect();
        let mut cqueue: VecDeque<NodeId> = live_roots.clone().into();
        while let Some(node) = cqueue.pop_front() {
            if let Some(children) = reverse.get(&node) {
                for (child, _) in children {
                    if seen_any.insert(child.clone()) {
                        if !alive_set.contains(child) {
                            status.insert(child.clone(), Status::Unknown);
                        }
                        cqueue.push_back(child.clone());
                    }
                }
            }
        }

        // ---- Pass 3: dangling roots (unattested, no chain of their own) or
        // stale roots -> unknown, propagated ----
        // A parent that carries its own justification chain is NOT a source:
        // its status derives from that chain, so deadness can propagate
        // through it at any depth (external audit 2026-10-30, Finding 2).
        let mut unknown_sources: HashSet<NodeId> = HashSet::new();
        for parents in self.edges.values() {
            for (parent, _) in parents {
                if !att.roots.contains_key(parent)
                    && !self.edges.contains_key(parent)
                    && !self.conjunctive.contains_key(parent)
                {
                    unknown_sources.insert(parent.clone());
                }
            }
        }
        for groups in self.conjunctive.values() {
            for group in groups {
                for m in group {
                    let known_root = att.roots.contains_key(m);
                    let has_own_path =
                        self.edges.contains_key(m) || self.conjunctive.contains_key(m);
                    if !known_root && !has_own_path {
                        unknown_sources.insert(m.clone());
                    }
                }
            }
        }
        for r in &stale_roots {
            unknown_sources.insert(r.clone());
        }
        for u in &unknown_sources {
            if !alive_set.contains(u) {
                status.insert(u.clone(), Status::Unknown);
            }
        }
        let mut uqueue: VecDeque<NodeId> = unknown_sources.iter().cloned().collect();
        while let Some(node) = uqueue.pop_front() {
            if let Some(children) = reverse.get(&node) {
                for (child, _) in children {
                    if !alive_set.contains(child) && !status.contains_key(child) {
                        status.insert(child.clone(), Status::Unknown);
                        uqueue.push_back(child.clone());
                    }
                }
            }
        }

        // ---- Pass 4: dead through EXPLICIT paths to dead roots ----
        let mut dqueue: VecDeque<NodeId> = dead_roots.iter().cloned().collect();
        while let Some(node) = dqueue.pop_front() {
            if let Some(children) = reverse.get(&node) {
                for (child, prov) in children {
                    if matches!(prov, Provenance::Explicit)
                        && !alive_set.contains(child)
                        && !status.contains_key(child)
                    {
                        status.insert(child.clone(), Status::Dead);
                        dqueue.push_back(child.clone());
                    }
                }
            }
        }

        // ---- Pass 5: conjunctive resolution (dead/unknown groups) ----
        // Groups are disjunctive between themselves: any fully-alive group
        // confers alive (handled in pass 1). Otherwise a group with an
        // unknown member makes the child unknown; if every group contains a
        // dead member and none has an unknown one, the child is dead.
        for _ in 0..self.conjunctive.len() + 2 {
            let mut changed = false;
            for (child, groups) in &self.conjunctive {
                if status.contains_key(child) || groups.is_empty() {
                    continue;
                }
                // Only decide once every member of every group has a status.
                if !groups.iter().flatten().all(|m| status.contains_key(m)) {
                    continue;
                }
                let mut any_alive_group = false;
                let mut any_unknown_group = false;
                let mut all_groups_killed = true;
                for group in groups {
                    let mut has_dead = false;
                    let mut has_unknown = false;
                    for m in group {
                        match status.get(m) {
                            Some(Status::Alive) => {}
                            Some(Status::Dead) => has_dead = true,
                            _ => has_unknown = true,
                        }
                    }
                    if !has_dead && !has_unknown {
                        any_alive_group = true;
                    }
                    if has_unknown {
                        any_unknown_group = true;
                    }
                    if !has_dead {
                        all_groups_killed = false;
                    }
                }
                let verdict = if any_alive_group {
                    Status::Alive
                } else if any_unknown_group {
                    Status::Unknown
                } else if all_groups_killed {
                    Status::Dead
                } else {
                    continue;
                };
                status.insert(child.clone(), verdict);
                changed = true;
            }
            if !changed {
                break;
            }
        }

        // ---- Final sweep: everything else connected is unknown ----
        // (Covers: inferred-only paths to dead roots, mixed groups, and any
        // node whose evidence is simply incomplete.)
        for node in &self.nodes {
            if !status.contains_key(node) {
                status.insert(node.clone(), Status::Unknown);
            }
        }

        // ---- Reverse dominator tree over the alive subgraph ----
        let live_root_set: Vec<NodeId> = live_roots
            .iter()
            .filter(|r| alive_set.contains(*r))
            .cloned()
            .collect();
        let mut reverse_plain: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (parent, children) in &reverse {
            reverse_plain
                .entry(parent.clone())
                .or_default()
                .extend(children.iter().map(|(c, _)| c.clone()));
        }
        let idom = reverse_dominators(&alive_set, &live_root_set, &reverse_plain);

        let coverage = self
            .discovered
            .map(|d| Coverage {
                wired: self.wired(),
                discovered: d,
            });

        Evaluation {
            status,
            pred,
            idom,
            coverage,
            stale_roots,
            expired_roots,
        }
    }

    /// Reconstruct a justification chain from a node toward roots.
    /// Prefers the pred map (from alive propagation). Falls back to the
    /// first outgoing edge, then to conjunctive group members, so dead and
    /// unknown nodes still show their structural "because".
    pub fn chain(&self, eval: &Evaluation, id: &str) -> Vec<NodeId> {
        let mut chain = vec![id.to_string()];
        let mut current = id.to_string();
        let mut guard = 0;
        loop {
            if let Some(prev) = eval.pred.get(&current) {
                chain.push(prev.clone());
                current = prev.clone();
            } else if let Some(parents) = self.edges.get(&current) {
                // Prefer an explicit parent so the printed "because" matches
                // the edge that can actually determine status (audit note).
                let chosen = parents
                    .iter()
                    .find(|(_, p)| matches!(p, Provenance::Explicit))
                    .or_else(|| parents.first());
                if let Some((parent, _)) = chosen {
                    chain.push(parent.clone());
                    current = parent.clone();
                } else {
                    break;
                }
            } else if let Some(groups) = self.conjunctive.get(&current) {
                if let Some(first_group) = groups.first() {
                    for m in first_group {
                        chain.push(m.clone());
                    }
                }
                break;
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
    fn alive_propagation_explicit() {
        let g = parse_graph(
            r#"
role:analyst -> project:active (explicit)
workflow:deploy -> project:active (explicit)
"#,
        )
        .unwrap();
        let a = parse_attestation("project:active alive 2024-01-01 casey").unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["role:analyst"], Status::Alive);
        assert_eq!(eval.status["workflow:deploy"], Status::Alive);
        assert_eq!(eval.status["project:active"], Status::Alive);
    }

    #[test]
    fn inferred_edge_never_confers_liveness() {
        let g = parse_graph("app:legacy -> project:active (inferred)").unwrap();
        let a = parse_attestation("project:active alive 2024-01-01 casey").unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["app:legacy"], Status::Unknown);
    }

    #[test]
    fn inferred_edge_never_confers_dead() {
        let g = parse_graph(
            r#"
app:report        -> project:old (explicit)
app:legacy-export -> project:old (inferred)
"#,
        )
        .unwrap();
        let a = parse_attestation("project:old dead 2023-04-12 ci").unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["app:report"], Status::Dead);
        assert_eq!(eval.status["app:legacy-export"], Status::Unknown);
        assert_eq!(eval.status["project:old"], Status::Dead);
    }

    #[test]
    fn explicit_and_inferred_mixed_prefers_unknown() {
        // Explicit path to a dead root + inferred path to a live root:
        // the candidate live reason blocks a dead verdict.
        let g = parse_graph(
            r#"
app:report -> project:old (explicit)
app:report -> project:active (inferred)
"#,
        )
        .unwrap();
        let a = parse_attestation(
            "project:old dead 2023-04-12 ci\nproject:active alive 2024-01-01 casey",
        )
        .unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["app:report"], Status::Unknown);
    }

    #[test]
    fn deadness_propagates_full_depth() {
        // External audit 2026-10-30, Finding 2: explicit chains under a dead
        // root are dead at ANY depth, not just one hop.
        let g = parse_graph(
            "leaf:x -> mid:y (explicit)\nmid:y -> root:dead (explicit)",
        )
        .unwrap();
        let a = parse_attestation("root:dead dead 2023-04-12 probe").unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["root:dead"], Status::Dead);
        assert_eq!(eval.status["mid:y"], Status::Dead);
        assert_eq!(eval.status["leaf:x"], Status::Dead);
    }

    #[test]
    fn dangling_branch_beats_dead_branch() {
        // A node with one path to a dead root and one path to a dangling
        // (unattested, no chain) parent is unknown: the unresolved branch
        // could still be alive.
        let g = parse_graph(
            "leaf:a -> mid:b (explicit)\nmid:b -> root:dead (explicit)\nmid:b -> root:dangling (explicit)",
        )
        .unwrap();
        let a = parse_attestation("root:dead dead 2023-01-01 probe").unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["root:dead"], Status::Dead);
        assert_eq!(eval.status["root:dangling"], Status::Unknown);
        assert_eq!(eval.status["mid:b"], Status::Unknown);
        assert_eq!(eval.status["leaf:a"], Status::Unknown);
    }

    #[test]
    fn unknown_when_unattested() {
        let g = parse_graph("service:billing -> ticket:BILL-99 (inferred)").unwrap();
        let a = parse_attestation("").unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["service:billing"], Status::Unknown);
        assert_eq!(eval.status["ticket:BILL-99"], Status::Unknown);
    }

    #[test]
    fn until_hard_expiry() {
        let g = parse_graph("app:tmp-role -> project:burst (explicit)").unwrap();
        let a =
            parse_attestation("project:burst alive 2024-01-01 ops until=2026-03-01").unwrap();
        let before = g.evaluate(&a, Some("2026-02-01"));
        assert_eq!(before.status["project:burst"], Status::Alive);
        assert_eq!(before.status["app:tmp-role"], Status::Alive);
        let after = g.evaluate(&a, Some("2026-04-01"));
        assert_eq!(after.status["project:burst"], Status::Dead);
        assert_eq!(after.status["app:tmp-role"], Status::Dead);
        assert_eq!(after.expired_roots, vec!["project:burst".to_string()]);
    }

    #[test]
    fn review_by_soft_expiry() {
        let g = parse_graph("app:audit-role -> reg:SOC2 (explicit)").unwrap();
        let a =
            parse_attestation("reg:SOC2 alive 2024-01-01 security review-by=2026-01-01")
                .unwrap();
        // Within the 30-day grace period (grace ends 2026-01-31): still alive.
        let within = g.evaluate(&a, Some("2026-01-15"));
        assert_eq!(within.status["reg:SOC2"], Status::Alive);
        // Past grace: stale -> unknown, propagated.
        let past = g.evaluate(&a, Some("2026-06-01"));
        assert_eq!(past.status["reg:SOC2"], Status::Unknown);
        assert_eq!(past.status["app:audit-role"], Status::Unknown);
        assert_eq!(past.stale_roots, vec!["reg:SOC2".to_string()]);
    }

    #[test]
    fn no_clock_ignores_expiry() {
        let g = parse_graph("app:audit-role -> reg:SOC2 (explicit)").unwrap();
        let a =
            parse_attestation("reg:SOC2 alive 2024-01-01 security review-by=2020-01-01")
                .unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["reg:SOC2"], Status::Alive);
        assert_eq!(eval.status["app:audit-role"], Status::Alive);
    }

    #[test]
    fn conjunctive_all_of_alive() {
        let g = parse_graph(
            "orderset:icu-sepsis-v3 -> all-of: protocol:sepsis-v3 policy:condition-active",
        )
        .unwrap();
        let a = parse_attestation(
            "protocol:sepsis-v3 alive 2024-01-01 board\npolicy:condition-active alive 2026-10-01 physician",
        )
        .unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["orderset:icu-sepsis-v3"], Status::Alive);
    }

    #[test]
    fn conjunctive_all_of_dead_member() {
        let g = parse_graph(
            "orderset:old-orderset -> all-of: protocol:sepsis-v3 project:cancelled",
        )
        .unwrap();
        let a = parse_attestation(
            "protocol:sepsis-v3 alive 2024-01-01 board\nproject:cancelled dead 2025-01-01 admin",
        )
        .unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["orderset:old-orderset"], Status::Dead);
    }

    #[test]
    fn conjunctive_unknown_member() {
        let g = parse_graph(
            "orderset:maybe -> all-of: protocol:sepsis-v3 policy:unchecked",
        )
        .unwrap();
        let a = parse_attestation("protocol:sepsis-v3 alive 2024-01-01 board").unwrap();
        let eval = g.evaluate(&a, None);
        assert_eq!(eval.status["policy:unchecked"], Status::Unknown);
        assert_eq!(eval.status["orderset:maybe"], Status::Unknown);
    }

    #[test]
    fn conjunctive_group_disjunctive_across_groups() {
        let g = parse_graph(
            r#"
orderset:dual -> all-of: protocol:a project:dead-one
orderset:dual -> all-of: protocol:b policy:live-two
"#,
        )
        .unwrap();
        let a = parse_attestation(
            "protocol:a alive 2024-01-01 b\nproject:dead-one dead 2024-01-01 b\nprotocol:b alive 2024-01-01 b\npolicy:live-two alive 2024-01-01 b",
        )
        .unwrap();
        let eval = g.evaluate(&a, None);
        // Second group fully alive -> the node is alive despite the dead group.
        assert_eq!(eval.status["orderset:dual"], Status::Alive);
    }

    #[test]
    fn coverage_reported_when_declared() {
        let g = parse_graph("# discovered: 4\nworkflow:ci -> project:active (explicit)\n")
            .unwrap();
        let a = parse_attestation("project:active alive 2024-01-01 casey").unwrap();
        let eval = g.evaluate(&a, None);
        let cov = eval.coverage.unwrap();
        assert_eq!(cov.wired, 1);
        assert_eq!(cov.discovered, 4);
        assert!((cov.pct() - 25.0).abs() < 1e-9);
    }

    #[test]
    fn coverage_absent_without_header() {
        let g = parse_graph("a:b -> c:d (explicit)\n").unwrap();
        let eval = g.evaluate(&Attestation::default(), None);
        assert!(eval.coverage.is_none());
    }
}
