# dead_chain_depth2

**Surprise:** v0.1 (and the first v0.2 draft) read `mid:y` and `leaf:x` as
unknown here — the unattested-but-connected parent was treated as an
unknown source, flooding unknown before deadness could flow. An
all-explicit chain under an attested-dead root is dead at ANY depth, and a
fully dead graph must exit 1, not 0. Found by external probe (audit
2026-10-30), not by the lab's own rounds.
