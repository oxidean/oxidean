# Performance Requirements — session-core (U2)

Consumes: `construction/session-core/functional-design/rules.md`
(BR2.x, BR4.x, BR6.x), `functional-design/entities.md`,
`inception/requirements-analysis/requirements.md` (NFR3),
`codekb/oxidean-app/technology-stack.md`.

Provider latency is external; this unit bounds OUR persistence and
sequencing cost on the chat hot path.

## Hot-path reads

| id | requirement | basis |
|----|-------------|-------|
| NFR3.1 | `transcript(session_id)` returns the FULL non-streaming set ordered by `seq` (BR7.2 pins `Vec<Message>` — no service-layer pagination; windowing is the U3 caller's concern). The NFR3 proxy binds the QUERY SHAPE: one index-covered scan per session, O(session length), no N+1 — a ~500-message session reads trivially inside the interactive budget | NFR3, BR7.2, C5 |
| NFR3.2 | `list_sessions_for_user` is index-covered by `(user_id, last_activity_at DESC)` — the shipped U1 index; the sessions list on `/oxpilot` must not scan other users' rows or sort unindexed | NFR3, entities OxpilotSession |
| NFR3.3 | `queue_list` reads `WHERE session_id = $1 ORDER BY queue_seq` — index-covered, O(queued depth) only; no join to messages | NFR3, entities OxpilotQueuedMessage |

## Write-path cost

| id | requirement | basis |
|----|-------------|-------|
| NFR3.4 | `seq` minting is per-session — `next_message_seq` is a `SELECT MAX(seq)+1` (indexed read on the session's rows) immediately followed by the INSERT under `UNIQUE(session_id, seq)`; two statements serialized per session, no global counter table, no cross-session serialization | BR2.1, entities OxpilotMessage |
| NFR3.5 | `queue_seq` minting is per-session the same way — `MAX(queue_seq)+1` read then INSERT under `UNIQUE(session_id, queue_seq)`; concurrent enqueues collide on the unique key and retry, bounded at 8 attempts (concurrency degree is realistically 2–3) | BR2.1, BR4.1 |
| NFR3.6 | `redact()` on writes is an O(content) pattern scan — bounded linear cost per row, no regex backtracking risk beyond the compiled pattern table | BR3.1 |
| NFR3.7 | `record_usage` is an upsert (`INSERT … ON CONFLICT … SET count = count + delta`) — one statement, O(1) per axis per scope | BR6.1, entities OxpilotUsageCounter |

## Queue-drain cost

| id | requirement | basis |
|----|-------------|-------|
| NFR3.8 | `drain_queue` claims + deletes + appends in ONE transaction bounded by queue depth — rows are claimed `FOR UPDATE`-equivalent (SQLite: single-tx delete-returning semantics) so a second drainer sees zero rows; no per-row round trip outside the tx | BR4.2, AC1.8.1 |
| NFR3.9 | The turn-start residual drain runs the same single-tx claim — crash-window leftovers cost one extra transaction at the next turn, never a background sweep task | BR4.2 |
