# Scalability Requirements — connector-gateway (U6)

Consumes: `construction/connector-gateway/functional-design/rules.md`,
`functional-design/functional-spec.md`,
`inception/requirements-analysis/requirements.md`,
`inception/contract-design/contract-summary.md` (C3, C7).

The connector plane is per-user by construction — scale obligations are
about NOT serializing across targets/users, and about holding many
idle long-polls cheaply on a single replica.

## Concurrency model

| id | requirement | basis |
|----|-------------|-------|
| NFR-S1 | Serialization is per-target only: the admission lock is keyed `target_id`; dispatch on target A never blocks dispatch on target B; no process-global mutex exists anywhere in the gateway (BR4.1) | BR4.1 |
| NFR-S2 | Long-poll claims are held as notification waiters, not worker threads: an idle connector contributes one parked future + the dequeue write on wake; the single-replica caveat (the documented single-replica caveat shared across the runtime units) is stated, not hidden (NFR4 constraint, BR5.1) | BR5.1, NFR4 |
| NFR-S3 | Queue depth per target is implicitly bounded by consent: actions exist only where a live grant authorized them; backlog beyond `CLAIM_ACTIONS_MAX` stays `queued` for the next poll — never dropped (NFR3.12, BR5.1) | BR5.1, NFR3.12 |

## Growth edges

| id | requirement | basis |
|----|-------------|-------|
| NFR-S4 | All list surfaces are bounded: `target_list`/`grant_list` are owner-scoped with stable ordering; claim payloads are capped per response; stored/audit fields are bounded by BR7.4 — no unbounded scan or blob exists on the hot path | BR7.4, entities |
| NFR-S5 | Indexes ride the same migration: `(target_id, state, created_at, action_id)` for FIFO dequeue, `(owner_user_id)` for target/grant lists, equality lookups on `code_hash`/`verifier_hash` — no sequential scan on any documented access pattern (BR5.1, NFR9) | BR5.1, NFR9 |
| NFR-S6 | N/A beyond the above — the single-replica deployment model (documented caveat) means no cross-node coordination, no distributed-lock primitives, and no horizontal-scaling NFRs are claimed in v1 | NFR4 constraint |
