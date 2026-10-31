# conjunctive_short_circuit

**Surprise:** `orderset:x` is DEAD even though one of its required parents
is unknown. In all-of semantics a dead member definitively falsifies the
conjunction — the unknown sibling is irrelevant to that verdict. The naive
rule "any unknown member makes the node unknown" is the same
status-contamination bug class the external audit found in the one-hop
deadness propagation. Found by the round-3 deliberation probe.
