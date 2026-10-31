use why_core::*;

#[test]
fn alive_nodes_are_detected() {
    let g = parse_graph(include_str!("corpus/basic_alive.graph.txt")).unwrap();
    let a = parse_attestation(include_str!("corpus/basic_alive.att.txt")).unwrap();
    let eval = g.evaluate(&a);

    assert_eq!(eval.status["role:analyst"], Status::Alive);
    assert_eq!(eval.status["workflow:deploy"], Status::Alive);
    assert_eq!(eval.status["project:active-migration"], Status::Alive);
}

#[test]
fn dead_nodes_are_detected() {
    let g = parse_graph(include_str!("corpus/basic_dead.graph.txt")).unwrap();
    let a = parse_attestation(include_str!("corpus/basic_dead.att.txt")).unwrap();
    let eval = g.evaluate(&a);

    assert_eq!(eval.status["role:legacy-export"], Status::Dead);
    assert_eq!(eval.status["workflow:legacy-step"], Status::Dead);
    assert_eq!(eval.status["project:old-migration"], Status::Dead);
}

#[test]
fn unknown_when_no_attestation() {
    let g = parse_graph(include_str!("corpus/unknown.graph.txt")).unwrap();
    let a = parse_attestation(include_str!("corpus/unknown.att.txt")).unwrap();
    let eval = g.evaluate(&a);

    assert_eq!(eval.status["service:billing"], Status::Unknown);
    assert_eq!(eval.status["ticket:BILL-99"], Status::Unknown);
}

#[test]
fn chain_reconstruction() {
    let g = parse_graph(include_str!("corpus/basic_dead.graph.txt")).unwrap();
    let a = parse_attestation(include_str!("corpus/basic_dead.att.txt")).unwrap();
    let eval = g.evaluate(&a);
    let chain = g.chain(&eval, "role:legacy-export");
    // Because the root is dead we do not get a pred link from the alive pass.
    // Chain will at least contain the node itself.
    assert!(!chain.is_empty());
    assert_eq!(chain[0], "role:legacy-export");
}
