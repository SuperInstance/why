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
