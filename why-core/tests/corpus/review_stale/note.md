# review_stale (as_of=2026-06-01)

**Surprise:** an attested-alive root degrades to unknown after its
review-by date plus 30 days of grace — "a stale attestation is never
silently alive." The whole chain goes unknown, not dead: maybe the root
is still justified; the evidence is simply old. Re-attest to restore.
