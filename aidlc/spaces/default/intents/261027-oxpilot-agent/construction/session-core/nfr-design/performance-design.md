# Performance Design — session-core (U2)

Consumes: `nfr-requirements/performance-requirements.md` (NFR3.1–3.9),
`functional-design/rules.md` (BR2.x, BR4.x, BR6.x),
`nfr-design-questions.md` (Q3, Q4, Q6).

## Query plan per hot path

Every session-scoped verb first runs `require_owned_session`
(`find_owned_session(user_id, session_id)` — one indexed read,
`not_found` for missing/foreign) before its session-keyed store
call (R-18) — each plan below carries that pre-read implicitly.

| path | query shape | index |
|------|-------------|-------|
| `transcript` | `list_messages` (reused unfiltered — shared with the snapshot path that needs `streaming` rows) + in-service `status IN ('complete','error')` filter — one scan, full set (BR7.2); the pin describes the returned SET, not a literal SQL predicate | `UNIQUE(session_id, seq)` covers order |
| `list_sessions` | `SELECT … WHERE user_id=? AND status != 'deleted' ORDER BY last_activity_at DESC` | shipped `idx_oxpilot_sessions_user_activity (user_id, last_activity_at DESC)` |
| `queue_list` | `SELECT … WHERE session_id=? ORDER BY queue_seq` | `UNIQUE(session_id, queue_seq)` |
| `enqueue` dedupe | `find_message_by_client_msg_id` then `find_queued_by_cmid` — two point reads | `UNIQUE(session_id, client_msg_id)` on both tables |
| `record_usage` | TWO upserts in one tx — `INSERT … ON CONFLICT(scope,scope_key,window) DO UPDATE SET requests=requests+1, tokens=tokens+?` for `(user, user_id, today)` AND `(instance, '__instance__', today)` — both scopes must be written or the instance cap never engages | counter UNIQUE is the upsert target |
| `check_budget` | `read_counters(user_id)` — one SELECT fetching both scope rows + in-memory compare vs caller-passed `BudgetCaps` (slot-read, R-01) | `(scope, scope_key, window)` UNIQUE |

No N+1 anywhere: drain is one tx (Q3), transcript is one query
(NFR3.1), dedupe is two point reads (Q4), counters one row-pair read
(Q6).

## Seq minting detail (NFR3.4/NFR3.5)

```rust
// oxidean-db::oxpilot — same shape as shipped next_message_seq
pub async fn next_queue_seq(db: &Db, session_id: &str) -> Result<i64> {
    // SELECT COALESCE(MAX(queue_seq),0)+1 FROM oxpilot_queued_messages
    // WHERE session_id=?  — one indexed read on the session's rows
}
// enqueue loop: mint → INSERT; on UNIQUE(session_id,queue_seq)
// violation → re-mint; bound = 8 attempts (NFR10.7)
```

The mint+insert pair serializes on the session's UNIQUE keys — two
sessions never contend (NFR11.1); within a session the collision loop
is the only contention point and stays bounded.

## Drain tx shape (Q3)

```rust
// postgres/mysql/sqlite variants inside oxidean-db — one tx:
//   pg/sqlite: rows = DELETE FROM oxpilot_queued_messages
//              WHERE session_id=? RETURNING *      // no ORDER BY on
//              DELETE — service sorts RETURNING rows by queue_seq
//   mysql:     SELECT … FOR UPDATE ORDER BY queue_seq (locks ordered),
//              then DELETE by id list
//   for r in sorted_rows { INSERT INTO oxpilot_messages
//                          (… seq=MAX(seq)+1 …) }   // minted per row
//   COMMIT — claim+delete+append atomic (NFR4.2)
```

`seq` mints inside the tx via `SELECT MAX(seq)+1` evaluated per row —
correct because a transaction sees its OWN writes (each insert raises
the next row's MAX), not because of snapshot stability. Single-writer
per session per BR4.2's invariant means no competing tx contends on
`UNIQUE(session_id, seq)` anyway.

## Budget compare (Q6)

`check_budget` = `read_counters` (1 SELECT) + in-memory compare vs the
caller-passed `BudgetCaps` (read once from `slot.budget_caps` by the
dispatch handler / U3 turn-start — zero writes on the read path, no
lock held across the DB call; the over-admit window is the documented
NFR4.10 bound, not a lock).
