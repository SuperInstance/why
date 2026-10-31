# Response to the external audit — 2026-10-31

*Respondent: the `why` maintainers. The audit
([audit-2026-10-30.md](audit-2026-10-30.md)) reviewed v0.1 at `4c00872`.
This response ships with v0.2.*

## Finding 1 — Inferred edges confer liveness

**Accepted; fixed in v0.2.** Pass 1 now propagates aliveness over explicit
edges only. Inferred edges form the candidate namespace: a node whose only
path to any root runs through an inferred edge is `unknown` — never
`alive`, and never `dead` either (the conservative mirror the audit did
not ask for, but the same logic demands: no removal recommendations on a
guessed link). Golden pairs: `inferred_live`, `inferred_dead`,
`mixed_verdicts`.

## Finding 2 — Deadness does not propagate past one hop

**Accepted; fixed.** This was not on the lab's blocker list and the audit
is right to call it a correctness bug, not roadmap. Pass 3 now seeds
`unknown` only from *dangling* parents — unattested nodes with no
justification chain of their own. A parent that carries its own chain
derives its status from that chain, so deadness flows through it at any
depth. A fully dead graph now exits 1.

Two golden pairs pin it, per the audit's suggestion:
`dead_chain_depth2` (the audit's minimal reproduction) and
`dead_vs_dangling` (a certain dead reason plus an unresolved branch
resolves to `unknown` — incomplete evidence still beats a removal
recommendation).

One refinement to the audit's demo remark: `workflow:deploy#build` and
`#test` read `unknown` in the v0.2 demo *by design* — `build`'s edge to
the dead `project:MIG-4821` is `(inferred)`, i.e. a candidate, so
`unknown` is the intended verdict, not the one-hop bug. The all-explicit
case is the one that was broken and is now fixed.

## Minor observations

- **Block-style `needs:`** — fixed in v0.2 (`extract` handles inline and
  block lists).
- **Positional attestation fields** — fixed in v0.2: `until=` and
  `review-by=` are `key=value` tokens, validated as dates; unknown fields
  are a parse error, and `--strict` rejects the legacy ambiguity outright.
- **`chain()` first-parent walk** — improved: the walk now prefers an
  explicit parent; conjunctive nodes print their full `all-of` group.
  Full multi-path explanation remains open (tracked in why-lab).
- **why-lab visibility** — acknowledged as the owner's call. The lab
  stays private for now because it contains digests of private ideation;
  the audit trail that can be public (this exchange, the golden corpus,
  the decision rationale summarized in README and format docs) is public.

## Verdict on the verdict

The audit's framing — "correctness, not roadmap" — is adopted as policy:
both findings were fixed before v0.2 shipped, and the corpus now carries
both probes so they cannot regress. The distinction between the two
failure directions is now explicit in the format spec: `unknown` never
fails a build, so anything that can silently convert `dead` into
`unknown` is the dangerous direction and gets golden pairs first.
