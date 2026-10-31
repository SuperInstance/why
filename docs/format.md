# The `because` format — v0.2

`why` reads a two-file format. Both files are plain text, line-oriented,
hand-writable, git-diffable, and co-located with the artifacts they describe.

## graph.txt — the justification graph

```
# comment lines start with # (a `# discovered: N` header is meaningful)
child-id -> parent-id (explicit|inferred)
child-id -> all-of: parent-id parent-id [parent-id ...]
```

- One edge per line: `child because parent`.
- `(explicit)` — a human wrote it (or an adapter read it from an
  authoritative field such as a workflow's `needs:`).
- `(inferred)` — a heuristic proposed it. **Inferred edges never confer
  status.** They live in the candidate namespace: they can explain and
  suggest, but a node whose only path to any root — live or dead — runs
  through an inferred edge is `unknown`, never `alive` and never `dead`.
  Confirming a candidate means rewriting `(inferred)` as `(explicit)`.
- IDs are opaque strings to the core. Conventions live in adapters
  (`iam:role/analyst`, `workflow:deploy#approve`, `jira:MIG-4821`). IDs
  should be canonical and qualified within the graph's scope; `--strict`
  enforces the `prefix:id` shape and referential integrity.

### Conjunctive groups

```
orderset:icu-sepsis-v3 -> all-of: protocol:sepsis-v3 policy:condition-active
```

Default semantics are **disjunctive**: a node is alive if ANY parent path
reaches a live root (any live reason suffices). `all-of:` marks the
exception where a node requires ALL named parents alive (a standing order
needs the protocol alive AND the patient condition present).

Group semantics, in precedence order:

- all members alive → the node is `alive`
- any member unknown → the node is `unknown` (evidence incomplete)
- a dead member kills its group; if every group of the node contains a
  dead member and none has an unknown one → `dead`
- multiple groups are disjunctive between themselves: any fully-alive
  group confers liveness

### Coverage header

```
# discovered: 14
```

Written by `why extract` (the count of artifacts the adapter discovered,
e.g. workflow jobs). `why check` reports `coverage: wired/discovered` —
the fraction of discovered artifacts that carry at least one justification
edge. **An unwired repository must never read as "healthy."** `--fail-under
<PCT>` turns a coverage shortfall into a CI failure.

## att.txt — root attestations

```
# root-id  alive|dead  [since]  [by]  [until=YYYY-MM-DD]  [review-by=YYYY-MM-DD]
jira:MIG-4821   dead   2023-04-12   ci
reg:SOC2-CC6    alive  2024-01-01   security   review-by=2027-01-01
role:oncall     alive  2026-06-01   sre        until=2026-12-31
```

A root is a declared living necessity: a project, a regulation, a contract,
a service commitment. Its liveness is **external truth recorded here**, not
computed by the graph.

`since` and `by` keep their legacy positional slots. The v0.2 mortality
fields are `key=value` tokens so they can never be confused with dates or
authors:

- `until` — hard expiry. After this date the root is `dead`. No grace.
- `review-by` — soft expiry. After this date + 30 days of grace the root
  degrades to `unknown` until re-attested. **A stale attestation is never
  silently alive.**

Roots are mortal by convention: a root with neither `until` nor `review-by`
and no automated oracle should be reviewed at least annually. Perpetuity by
silence is the failure mode this format exists to prevent.

The evaluation date defaults to today; `--as-of YYYY-MM-DD` makes it
deterministic (tests, golden pairs, historical diffs). `--no-clock`
ignores expiry fields entirely.

## Inline form

The same edge, co-located as a comment inside the artifact it justifies:

```yaml
# because: jira:MIG-4821 @ 2023-04-12 by ci
permissions:
  contents: read
```

`why extract` reads `# because: <parent-id>` comments as explicit edges
(attached to the enclosing job when one is open, else to the workflow).
Co-location is the point: the reason lives inside the artifact, travels in
the same diff, and dies in the same commit.

## Status semantics

Given the graph and the attestations, every node gets exactly one status:

| Status | Meaning |
|---|---|
| `alive` | A path of explicit edges to a root currently attested alive (or an `all-of` group whose members are all alive) |
| `dead` | Certainty that no live reason exists: an explicit path to a dead or `until`-expired root, or every conjunctive group contains a dead member |
| `unknown` | The evidence is incomplete: an unattested or stale root is reachable, or the only path runs through an inferred edge (candidate justification) |
| `absent` | The node never appears in any justification edge (never wired) |

`unknown` is first-class. Incomplete evidence is never forced into a binary
lie — and a candidate justification, even one pointing at a dead root, is
never allowed to produce a removal recommendation. That is what makes the
tool trustworthy on real organizational data.

Every status carries a derivation chain. There is no "trust me" output path.

## Extraction rules (GHA adapter)

- `needs:` (inline and block form) and `environment:` produce **explicit**
  edges — they are read from authoritative fields.
- Ticket-like tokens become **inferred** candidate edges only when they are
  anchored: 2+ uppercase alphanumeric prefix, dash, digits (`MIG-4821`,
  `GH-7`). Version strings, OS names, and lowercase words (`ubuntu-22`,
  `Python-3`, `v2`) never match.
- `# discovered: N` counts the artifacts (jobs) seen, powering coverage.

## Strict mode

`why check --strict` enforces referential integrity and exits `2` on
violation:

- every id is namespaced (`prefix:id`)
- no duplicate edges
- `all-of:` groups have at least two members
- no attestation for a node absent from the graph

## Exit codes

- `0` — clean (no dead findings)
- `1` — findings present (at least one `dead`, or coverage below
  `--fail-under`)
- `2` — error (bad files, strict violations)

`unknown` does not fail the build by default; CI policy decides via
`--fail-under` once coverage is reported.
