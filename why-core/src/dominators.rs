//! Reverse dominator tree on the Alive subgraph.
//!
//! Liveness flows on the reverse justification graph (root → artifacts).
//! Dominators are computed on that graph so that idom(n) is the closest
//! unavoidable gate on every live path that keeps n alive.
//!
//! Simple iterative algorithm (Cooper–Harvey–Kennedy style).
//! Readable over asymptotic cleverness.

use std::collections::{HashMap, HashSet};

use crate::NodeId;

/// Immediate dominator map: node → idom(node).
/// Roots map to themselves. Only Alive nodes appear.
pub type IDom = HashMap<NodeId, NodeId>;

/// Compute the reverse dominator tree.
///
/// * `alive` – nodes with Status::Alive
/// * `live_roots` – the attested-alive roots (subset of alive)
/// * `reverse` – parent → children (liveness-flow edges)
///
/// Returns idom map. Live roots (or the synthetic source's children)
/// are their own idoms.
pub fn reverse_dominators(
    alive: &HashSet<NodeId>,
    live_roots: &[NodeId],
    reverse: &HashMap<NodeId, Vec<NodeId>>,
) -> IDom {
    if alive.is_empty() || live_roots.is_empty() {
        return IDom::new();
    }

    // Restrict reverse edges to alive nodes only.
    let mut preds: HashMap<NodeId, Vec<NodeId>> = HashMap::new(); // child → parents (in flow graph)
    let mut succ: HashMap<NodeId, Vec<NodeId>> = HashMap::new(); // parent → children

    for (parent, children) in reverse {
        if !alive.contains(parent) {
            continue;
        }
        for child in children {
            if !alive.contains(child) {
                continue;
            }
            succ.entry(parent.clone()).or_default().push(child.clone());
            preds.entry(child.clone()).or_default().push(parent.clone());
        }
    }

    // Ensure every alive node has an entry.
    for n in alive {
        preds.entry(n.clone()).or_default();
        succ.entry(n.clone()).or_default();
    }

    // Single source: if multiple roots, introduce synthetic source.
    let synthetic = "__why_synthetic_source__".to_string();
    let use_synthetic = live_roots.len() > 1 || !alive.contains(&live_roots[0]);
    let source = if use_synthetic {
        for r in live_roots {
            if alive.contains(r) {
                succ.entry(synthetic.clone()).or_default().push(r.clone());
                preds.entry(r.clone()).or_default().push(synthetic.clone());
            }
        }
        synthetic.clone()
    } else {
        live_roots[0].clone()
    };

    // Nodes participating in the flow graph.
    let mut nodes: Vec<NodeId> = alive.iter().cloned().collect();
    if use_synthetic {
        nodes.push(synthetic.clone());
    }
    nodes.sort(); // stable

    // Intersection-based iterative idom.
    // idom[n] = n means "undefined" during iteration except for source.
    let mut idom: HashMap<NodeId, NodeId> = HashMap::new();
    idom.insert(source.clone(), source.clone());

    // RPO order helps convergence; simple approximation: roots first, then rest.
    let mut order: Vec<NodeId> = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![source.clone()];
    while let Some(n) = stack.pop() {
        if !seen.insert(n.clone()) {
            continue;
        }
        order.push(n.clone());
        if let Some(children) = succ.get(&n) {
            for c in children {
                stack.push(c.clone());
            }
        }
    }
    // Append any alive nodes not reached (should not happen for truly alive nodes)
    for n in &nodes {
        if !seen.contains(n) {
            order.push(n.clone());
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for n in &order {
            if n == &source {
                continue;
            }
            let parents = match preds.get(n) {
                Some(p) if !p.is_empty() => p,
                _ => continue,
            };

            // Start with first parent that already has an idom.
            let mut new_idom: Option<NodeId> = None;
            for p in parents {
                if idom.contains_key(p) {
                    new_idom = Some(p.clone());
                    break;
                }
            }
            let Some(mut acc) = new_idom else {
                continue;
            };

            for p in parents {
                if p == &acc || !idom.contains_key(p) {
                    continue;
                }
                acc = intersect(&acc, p, &idom, &source);
            }

            if idom.get(n) != Some(&acc) {
                idom.insert(n.clone(), acc);
                changed = true;
            }
        }
    }

    // Strip synthetic source: children of synthetic become self-idoms (roots).
    if use_synthetic {
        idom.remove(&synthetic);
        for r in live_roots {
            if alive.contains(r) {
                idom.insert(r.clone(), r.clone());
            }
        }
        // Fix any node whose idom was synthetic → should be the live root that actually dominates.
        // After removing synthetic, re-point: if idom was synthetic, set to self if root else leave
        // (iterative pass should have set real idoms; only roots pointed at synthetic).
        let mut fixups = Vec::new();
        for (n, d) in &idom {
            if d == &synthetic {
                fixups.push(n.clone());
            }
        }
        for n in fixups {
            idom.insert(n.clone(), n);
        }
    }

    // Only return entries for alive nodes.
    idom.retain(|k, _| alive.contains(k));
    idom
}

/// Finger-search style intersect on the current idom tree.
fn intersect(a: &NodeId, b: &NodeId, idom: &IDom, source: &NodeId) -> NodeId {
    let mut a = a.clone();
    let mut b = b.clone();

    // Climb until equal. Depth is not explicit; use a visited set to avoid cycles
    // during the incomplete phase of iteration.
    let mut guard = 0;
    while a != b {
        // Prefer climbing the one that is "deeper" — without depth, climb both
        // toward source using a simple strategy: move a if we can, else b.
        // Cooper-Harvey-Kennedy uses reverse postorder numbers; we approximate
        // by walking both until they meet.
        let a_next = idom.get(&a).cloned().unwrap_or_else(|| a.clone());
        let b_next = idom.get(&b).cloned().unwrap_or_else(|| b.clone());

        if a_next == a && b_next == b {
            // Both stuck; should not happen once source is set.
            break;
        }
        // Move the one that is not source-ward preferentially.
        if a != *source && a_next != a {
            a = a_next;
        } else if b != *source && b_next != b {
            b = b_next;
        } else if a != *source {
            a = a_next;
        } else {
            b = b_next;
        }

        guard += 1;
        if guard > 100_000 {
            break;
        }
    }
    a
}

/// Nodes in the dominator subtree of `gate` (including gate itself).
pub fn dom_subtree(idom: &IDom, gate: &str) -> HashSet<NodeId> {
    // Build children map from idom.
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for (n, d) in idom {
        if n != d {
            children.entry(d.clone()).or_default().push(n.clone());
        }
    }
    let mut out = HashSet::new();
    let mut stack = vec![gate.to_string()];
    while let Some(n) = stack.pop() {
        if out.insert(n.clone()) {
            if let Some(cs) = children.get(&n) {
                for c in cs {
                    stack.push(c.clone());
                }
            }
        }
    }
    out
}

/// Dominator chain from node up to a root (node, idom(node), ..., root).
pub fn dom_chain(idom: &IDom, id: &str) -> Vec<NodeId> {
    let mut chain = vec![id.to_string()];
    let mut current = id.to_string();
    let mut guard = 0;
    while let Some(d) = idom.get(&current) {
        if d == &current {
            break; // root
        }
        chain.push(d.clone());
        current = d.clone();
        guard += 1;
        if guard > 10_000 {
            break;
        }
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn single_path_idom() {
        // r → a → b
        let mut reverse = HashMap::new();
        reverse.insert("r".into(), vec!["a".into()]);
        reverse.insert("a".into(), vec!["b".into()]);

        let mut alive = HashSet::new();
        alive.insert("r".into());
        alive.insert("a".into());
        alive.insert("b".into());

        let idom = reverse_dominators(&alive, &["r".into()], &reverse);
        assert_eq!(idom.get("r"), Some(&"r".to_string()));
        assert_eq!(idom.get("a"), Some(&"r".to_string()));
        assert_eq!(idom.get("b"), Some(&"a".to_string()));
    }

    #[test]
    fn diamond_idom() {
        //     r
        //    / \
        //   a   b
        //    \ /
        //     c
        let mut reverse = HashMap::new();
        reverse.insert("r".into(), vec!["a".into(), "b".into()]);
        reverse.insert("a".into(), vec!["c".into()]);
        reverse.insert("b".into(), vec!["c".into()]);

        let mut alive = HashSet::new();
        for n in ["r", "a", "b", "c"] {
            alive.insert(n.into());
        }

        let idom = reverse_dominators(&alive, &["r".into()], &reverse);
        assert_eq!(idom.get("c"), Some(&"r".to_string())); // r is idom of c (both paths)
        assert_eq!(idom.get("a"), Some(&"r".to_string()));
        assert_eq!(idom.get("b"), Some(&"r".to_string()));
    }
}
