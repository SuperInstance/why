//! Golden corpus pairs — the public equivalence suite.
//!
//! Each pair under `tests/corpus/<name>/` is:
//! - `graph.txt`     — the input justification graph
//! - `att.txt`       — the root attestations
//! - `expected.txt`  — expected statuses, one `id status` per line
//! - `note.md`       — the surprise note: what this pair pins down and why
//!
//! A pair that changes behavior is a semantic decision, not a refactor.
//! Adding a pair is cheap: drop the four files in and add one line below.

use std::path::Path;
use why_core::*;

fn run_pair(dir: &str, as_of: Option<&str>) {
    let base = Path::new("tests/corpus").join(dir);
    let gtext = std::fs::read_to_string(base.join("graph.txt"))
        .unwrap_or_else(|e| panic!("{dir}: missing graph.txt: {e}"));
    let atext = std::fs::read_to_string(base.join("att.txt"))
        .unwrap_or_else(|e| panic!("{dir}: missing att.txt: {e}"));
    let etext = std::fs::read_to_string(base.join("expected.txt"))
        .unwrap_or_else(|e| panic!("{dir}: missing expected.txt: {e}"));

    let g = parse_graph(&gtext).unwrap();
    let a = parse_attestation(&atext).unwrap();
    let eval = g.evaluate(&a, as_of);

    let mut failures = Vec::new();
    for line in etext.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let id = it.next().unwrap();
        let want = it.next().unwrap();
        let got = eval
            .status
            .get(id)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "(missing)".into());
        if got != want {
            failures.push(format!("{id}: want {want}, got {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "golden pair {dir} (as_of={as_of:?}):\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn pair_basic_alive() {
    run_pair("basic_alive", None);
}

#[test]
fn pair_basic_dead() {
    // v0.2 semantics: the inferred edge no longer confers dead — see the note.
    run_pair("basic_dead", None);
}

#[test]
fn pair_unknown_unattested() {
    run_pair("unknown", None);
}

#[test]
fn pair_inferred_live_is_unknown() {
    run_pair("inferred_live", None);
}

#[test]
fn pair_inferred_dead_is_unknown() {
    run_pair("inferred_dead", None);
}

#[test]
fn pair_until_expired() {
    run_pair("until_expired", Some("2026-04-01"));
}

#[test]
fn pair_until_active() {
    run_pair("until_active", Some("2026-02-01"));
}

#[test]
fn pair_review_stale() {
    run_pair("review_stale", Some("2026-06-01"));
}

#[test]
fn pair_review_grace() {
    run_pair("review_grace", Some("2026-01-15"));
}

#[test]
fn pair_conjunctive() {
    run_pair("conjunctive", None);
}

#[test]
fn pair_coverage_zero() {
    run_pair("coverage_zero", None);
}

/// Coverage numbers are part of the golden output.
#[test]
fn pair_coverage_zero_numbers() {
    let g = parse_graph(&std::fs::read_to_string("tests/corpus/coverage_zero/graph.txt").unwrap())
        .unwrap();
    let a =
        parse_attestation(&std::fs::read_to_string("tests/corpus/coverage_zero/att.txt").unwrap())
            .unwrap();
    let eval = g.evaluate(&a, None);
    let cov = eval.coverage.expect("coverage pair must declare discovered");
    assert_eq!(cov.wired, 1);
    assert_eq!(cov.discovered, 4);
    assert!((cov.pct() - 25.0).abs() < 1e-9);
}

/// Derivation chains are part of the contract: no "trust me" output.
#[test]
fn chain_reconstruction() {
    let g = parse_graph(&std::fs::read_to_string("tests/corpus/basic_dead/graph.txt").unwrap())
        .unwrap();
    let a =
        parse_attestation(&std::fs::read_to_string("tests/corpus/basic_dead/att.txt").unwrap())
            .unwrap();
    let eval = g.evaluate(&a, None);
    let chain = g.chain(&eval, "role:legacy-export");
    assert!(!chain.is_empty());
    assert_eq!(chain[0], "role:legacy-export");
    assert!(chain.contains(&"project:old-migration".to_string()));
}

/// The three-way surprise: explicit dead + inferred live -> unknown, not dead.
#[test]
fn pair_mixed_explicit_dead_inferred_live() {
    run_pair("mixed_verdicts", None);
}
