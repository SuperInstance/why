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
why check graph.txt --alive att.txt --strict --fail-under 100   # CI gate
why why graph.txt role:legacy-export --alive att.txt
why impact graph.txt project:MIG-4821 --alive att.txt
why fragile graph.txt --alive att.txt
why att att.txt project:MIG-4821 dead --since 2023-04-12 --by ci
why att att.txt reg:SOC2 alive --review-by 2027-01-01 --by security
why roots att.txt
why extract . --out graph.txt
```

## Format

**graph.txt**

```
child-id -> parent-id (explicit|inferred)
child-id -> all-of: parent-id parent-id
```

**att.txt**

```
root-id  alive|dead  [since]  [by]  [until=YYYY-MM-DD]  [review-by=YYYY-MM-DD]
```

The same edge, co-located inside the artifact it justifies (read by `extract`):

```yaml
# because: jira:MIG-4821 @ 2023-04-12 by ci
permissions:
  contents: read
```

Full spec — four-state semantics, the inferred-edge candidate namespace,
conjunctive `all-of:` groups, root mortality (`until` / `review-by`),
coverage, strict mode: [docs/format.md](docs/format.md).

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

## v0.2 — honesty fixes

v0.1 could lie in both directions. v0.2 closes the six confirmed blockers:

1. **Coverage reporting** — `check` reports `wired/discovered` and
   `--fail-under` fails CI on a shortfall. An unwired repo exits with a
   visible 0%, never a silent green.
2. **Inferred edges never confer status** — a heuristic ticket match
   produces a *candidate* (`unknown`), never a verdict. The engine refuses
   to recommend removal on a guessed link.
3. **Root mortality** — `until=` (hard expiry, dead after the date) and
   `review-by=` (soft expiry, unknown after +30d grace). A stale
   attestation is never silently alive.
4. **Strict mode** — `--strict` enforces namespaced ids, no duplicate
   edges, no orphan attestations, well-formed `all-of:` groups.
5. **Anchored ticket detection** — only `KEY-123`-shaped tokens become
   inferred edges; `ubuntu-22` and `Python-3` never do. `extract` also
   reads inline `# because:` comments as explicit edges and block-form
   `needs:` lists.
6. **Golden corpus pairs** — the public equivalence suite: each pair is a
   graph + attestations + expected statuses + a surprise note
   (`why-core/tests/corpus/`). Changing a pair is a semantic decision, not
   a refactor.

## Verbs

| Verb       | Purpose |
|------------|---------|
| `check`    | Status of every node + coverage (exit 1 if any dead or coverage below `--fail-under`) |
| `why`      | Justification chain for one node, with root expiry/staleness annotations |
| `impact`   | What becomes dead or unknown if this node/root dies |
| `fragile`  | Nodes with exactly one justifying path |
| `att`      | Add or update a root attestation (mutates file, preserves mortality fields) |
| `roots`    | List attestations with their effective status (`EXPIRED` / `STALE`) |
| `extract`  | Build a graph from `.github/workflows/` (needs, environments, because-comments, anchored tickets) |

All read verbs accept `--as-of YYYY-MM-DD` (deterministic clock) and
`--no-clock` (ignore expiry fields).

## Exit codes

- `0` — clean
- `1` — findings present (`check`: dead nodes, or coverage below threshold)
- `2` — error (including strict-mode violations)

## Demo

```bash
cd demo
why extract .                     # workflows -> graph.txt (coverage header included)
why check graph.txt --alive att.txt
why why graph.txt workflow:deploy#build --alive att.txt
why impact graph.txt project:MIG-4821 --alive att.txt
```

## CI

See `examples/ci-why.yml` for a minimal GitHub Action; this repo runs its
own build + test + golden-corpus suite on every push (`.github/workflows/ci.yml`).

## Status

v0.2 — core reachability + candidate namespace + mortality + coverage +
strict mode + golden corpus (45 tests). Next on the lab's docket: the
`--decide` disposition layer (annotate-only, calibrated retire/investigate/
keep) and the agent/MCP adapter. Research and deliberation behind these
decisions: the private `why-lab` library.

## License

MIT OR Apache-2.0.

## Contributing

Adapters are the contribution surface: an adapter is a script that reads a
directory and writes `graph.txt` lines to stdout (plus a `# discovered: N`
header). The core stays small — no domain nouns in the kernel.
