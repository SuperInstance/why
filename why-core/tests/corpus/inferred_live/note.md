# inferred_live

**Surprise:** `app:legacy` is *unknown* while its sibling `app:exporter`
is alive with the same root. The format spec line "a node whose only path
to a live root runs through an inferred edge is unknown, not alive" is
exactly this pair. The fix for the maintainer: confirm the edge — rewrite
`(inferred)` as `(explicit)`.
