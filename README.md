# why

**Does this still deserve to exist?**

`why` is a small, single-binary CLI that answers one question over a two-file format.

## Install

```bash
cargo install --path why-cli
```

## Quick start

```bash
why check graph.txt --alive att.txt
why why graph.txt role:legacy-export --alive att.txt
why impact graph.txt project:MIG-4821 --alive att.txt
why fragile graph.txt --alive att.txt
why att att.txt project:MIG-4821 dead --since 2023-04-12 --by ci
why roots att.txt
why extract . --out graph.txt
```

## Format

**graph.txt**
```
child-id -> parent-id (explicit|inferred)
```

**att.txt**
```
root-id  alive|dead  [since]  [by]
```

The same edge, co-located inside the artifact it justifies:

```yaml
# because: jira:MIG-4821 @ 2023-04-12 by ci
```

Full spec, including four-state semantics (`alive`/`dead`/`unknown`/`absent`),
trust rules for inferred edges, and draft v0.2 fields (`until`, `review-by`,
`all-of:`): [docs/format.md](docs/format.md).

## Why "why"?

Systems accumulate structures whose reasons have decayed. Permissions outlive
projects. Approval gates outlive the risks they controlled. Agent tool grants
outlive the tasks that justified them. Existing tools answer *what exists*,
*what happened*, *what is allowed*, and *what is true*. None answers whether
the reason is still alive.

`why` treats justification as a graph with living roots and computes
reachability. Every answer carries its derivation chain. `unknown` is
first-class: incomplete evidence is never forced into a binary lie. The tool
never deletes anything — it makes the absence of a live reason cheap to see.

Prior art and positioning: `why` is a necessity-reachability engine for
*organizational* artifacts (permissions, approval gates, agent grants,
workflows). For justification-liveness over code decisions, see
[rationale-guard](https://github.com/lurui1997/rationale-guard).

## Verbs

| Verb       | Purpose |
|------------|---------|
| `check`    | Status of every node (exit 1 if any dead) |
| `why`      | Justification chain for one node |
| `impact`   | What depends on / becomes dead with this node |
| `fragile`  | Nodes with exactly one parent edge |
| `att`      | Add or update a root attestation (mutates file) |
| `roots`    | List current root attestations |
| `extract`  | Build a graph from `.github/workflows/` |

## Exit codes

- `0` — clean
- `1` — findings present (`check`)
- `2` — error

## Demo

```bash
cd demo
why check graph.txt --alive att.txt
why why graph.txt role:legacy-export --alive att.txt
why impact graph.txt project:MIG-4821 --alive att.txt
why extract .
```

## CI

See `examples/ci-why.yml` for a minimal GitHub Action.

## Status

v0.1 — core reachability + check, why, impact, fragile, att, roots, extract.
Tests green (7 unit + 4 corpus). v0.2 blockers: coverage reporting (an unwired
repo must never read as "healthy"), inferred edges in a candidate namespace,
stale attestations → `unknown`, anchored ticket detection in `extract`.

## License

MIT OR Apache-2.0.

## Contributing

Adapters are the contribution surface: an adapter is a script that reads a
directory and writes `graph.txt` lines to stdout. The core stays small — no
domain nouns in the kernel.
