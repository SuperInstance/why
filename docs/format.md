# The `because` format — v0.2 (draft)

`why` reads a two-file format. Both files are plain text, line-oriented,
hand-writable, git-diffable, and co-located with the artifacts they describe.

## graph.txt — the justification graph

```
# comment lines start with #
child-id -> parent-id (explicit|inferred)
```

- One edge per line: `child because parent`.
- `(explicit)` — a human wrote it (or an adapter read it from an authoritative
  field).
- `(inferred)` — a heuristic proposed it. **Inferred edges never confer
  liveness.** They live in a candidate namespace: they can explain and suggest,
  but a node whose only path to a live root runs through an inferred edge is
  `unknown`, not `alive`.
- IDs are opaque strings to the core. Conventions live in adapters
  (`iam:role/analyst`, `workflow:deploy#approve`, `jira:MIG-4821`).
- IDs should be canonical and qualified within the graph's scope. Two spellings
  of the same real thing are two nodes (referential integrity is the graph
  author's responsibility in v0.2; strict mode may follow).

Optional conjunctive marker (v0.2 draft):

```
orderset:icu-sepsis-v3 -> all-of: protocol:sepsis-v3 policy:condition-active
```

Default semantics are **disjunctive**: a node is alive if ANY parent path
reaches a live root (any live reason suffices). `all-of:` marks the exception
where a node requires ALL named parents alive (a standing order needs the
protocol alive AND the patient condition present).

## att.txt — root attestations

```
# root-id  alive|dead  [since]  [by]
jira:MIG-4821   dead   2023-04-12   ci
reg:SOC2-CC6    alive  2024-01-01   security
```

A root is a declared living necessity: a project, a regulation, a contract, a
service commitment. Its liveness is **external truth recorded here**, not
computed by the graph.

Optional fields (v0.2 draft):

```
root-id  alive|dead  [since]  [until]  [review-by]  [by]
```

- `until` — hard expiry. After this date the root is `dead`. No grace.
- `review-by` — soft expiry. After this date (+30d grace) the root degrades to
  `unknown` until re-attested. **A stale attestation is never silently alive.**

Roots are mortal by convention: a root with neither `until` nor `review-by` and
no automated oracle should be reviewed at least annually. Perpetuity by silence
is the failure mode this format exists to prevent.

## Inline form

The same edge, co-located as a comment inside the artifact it justifies:

```yaml
# because: jira:MIG-4821 @ 2023-04-12 by ci
permissions:
  contents: read
```

Co-location is the point: the reason lives inside the artifact, travels in the
same diff, and dies in the same commit.

## Status semantics

Given the graph and the attestations, every node gets exactly one status:

| Status | Meaning |
|---|---|
| `alive` | A path exists to a root currently attested alive (all edges explicit) |
| `dead` | Full traversal completes; no such path exists |
| `unknown` | The chain is incomplete, an attestation is stale/missing, or the only path uses an inferred edge |
| `absent` | The node never appears in any justification edge (never wired) |

`unknown` is first-class. Incomplete evidence is never forced into a binary
lie; that is what makes the tool trustworthy on real organizational data.

Every status carries a derivation chain. There is no "trust me" output path.

## Exit codes

- `0` — clean (no dead nodes)
- `1` — findings present (at least one `dead`)
- `2` — error

`unknown` does not fail the build by default; CI policy can decide otherwise
once coverage is reported.

## Coverage (v0.2 draft)

`check` reports coverage: nodes-with-justification-edges over total discovered
artifacts. An unwired repository exits clean **with a visible coverage number
of 0** — "never adopted" must never be mistaken for "audited and healthy."
