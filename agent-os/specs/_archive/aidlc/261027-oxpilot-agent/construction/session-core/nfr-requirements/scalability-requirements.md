# Scalability Requirements — session-core (U2)

Consumes: `construction/session-core/functional-design/rules.md`
(BR2.x, BR4.x, BR6.x), `functional-design/entities.md`,
`inception/requirements-analysis/requirements.md` (NFR3, NFR4, FR6.7).

v1 is single-process. These rows bound where the design holds as
replica count and session volume grow, and pin the documented limits.

## Per-session serialization

| id | requirement | basis |
|----|-------------|-------|
| NFR11.1 | `seq` and `queue_seq` minting serialize on the session's unique-key inserts — contention scope is ONE session's writers, not the table; two sessions never contend on minting | BR2.1, BR4.1 |
| NFR11.2 | Active-turn claim is process-local (`ActiveTurns` map) — multi-replica deployment would allow two replicas to each think they own the turn; v1 documents single-replica for oxpilot chat (same bound as the budget limiter) | BR4.2 single-writer invariant, FR6.7 |

## Counter semantics at scale

| id | requirement | basis |
|----|-------------|-------|
| NFR11.3 | Usage counters live in the DB (`oxpilot_usage_counters`) — they ARE shared across replicas, but `check_budget`→act is not atomic across processes; documented caveat: N replicas can over-admit up to N×the remaining budget in the check/act window | BR6.2, FR6.7 |
| NFR11.4 | Counter cardinality is bounded — `scope ∈ {user, instance}` ⇒ rows = users + 1 per day; the `window` field (UTC ISO day) rolls over at UTC midnight and old-window rows are never queried again — natural garbage collection, no trim job | entities OxpilotUsageCounter |

## Table growth

| id | requirement | basis |
|----|-------------|-------|
| NFR11.5 | Session tables grow unboundedly with use — no retention/trim in v1; `session_delete` is the only reclamation path. Indexes are sized for the access patterns (`list by user`, `transcript by session`, `queue by session`) so growth degrades reads gracefully, not nonlinearly | entities, BR2.x |
| NFR11.6 | `queue_list` and `drain_queue` are O(queue depth) — queue depth is bounded by user behavior (a user queueing 1000 follow-ups is the pathological case, accepted; no queue cap in v1 beyond the global budget gates) | BR4.x |
