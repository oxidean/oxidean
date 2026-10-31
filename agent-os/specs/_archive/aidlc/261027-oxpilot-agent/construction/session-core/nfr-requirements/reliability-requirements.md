# Reliability Requirements — session-core (U2)

Consumes: `construction/session-core/functional-design/rules.md`
(BR2.x, BR4.x, BR5.x), `functional-design/functional-spec.md`,
`inception/requirements-analysis/requirements.md` (NFR4),
`inception/contract-design/contract-summary.md` (C2, C8).

## Exactly-once & crash windows

| id | requirement | basis |
|----|-------------|-------|
| NFR4.1 | `client_msg_id` dedupe is a UNION across `oxpilot_messages` AND `oxpilot_queued_messages` (BR2.2): a resend whose original persisted pre-turn RE-ATTACHES via `resend_matches` (or resolves from the prior-turn snapshot) — never a duplicate, never a 409; a resend whose original is still QUEUED answers `409 turn_active` on the chat POST (the SSE route has no queued-position return shape) but returns the existing `{queued_id, position}` on `message_queue` | BR2.2, C2 |
| NFR4.2 | Queue drain is claim+delete+append inside one transaction — a crash between claim and append cannot lose the message: the row still exists (claim uncommitted ⇒ row never deleted) and the next-turn-start residual drain picks it up | BR4.2, AC1.8.1 |
| NFR4.3 | The residual drain at turn START is the crash-recovery path — a crash between the end-of-turn drain point and process exit leaves rows that the NEXT turn drains before prompt assembly; no orphaned queued rows are silently skipped | BR4.2 |
| NFR4.4 | Concurrent `queue_seq` mints that collide on `(session_id, queue_seq)` retry the mint — a UNIQUE violation on enqueue is retried (bounded: 8 attempts), not surfaced as a 500 | BR4.1 |
| NFR4.12 | A `client_msg_id` UNIQUE collision on ENQUEUE (the dedupe read raced the original's commit) re-reads the queue and returns the existing `{queued_id, position}` — idempotent like the resend path, never a 500 | BR2.2 |

## Delete integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR4.5 | `session_delete` runs hooks in registration order; ANY hook failure aborts the delete — the session row and all children stay intact; the error propagates as the proc's typed error (no silent half-delete) | C8, BR5.2 |
| NFR4.6 | The cascade itself is `DELETE FROM oxpilot_sessions WHERE …` relying on `ON DELETE CASCADE` for messages/tool_calls/queue/proposals — one statement ⇒ no partial-delete window; usage counters are NOT cascade children (they key on scope, not session) | entities, BR5.3 |
| NFR4.7 | A delete on a session with an ACTIVE turn still runs hooks first — the U3 abort hook is what terminates the live turn; U2 must invoke hooks even when the turn map has an entry, never short-circuit "active ⇒ refuse" at the service layer | C8, BR5.2 |

## Budget & refusal semantics

| id | requirement | basis |
|----|-------------|-------|
| NFR4.8 | `check_budget` refusal is typed `BudgetExceeded` → `budget_exceeded` — a refusal never drops the message silently; the queued/enqueued attempt surfaces the typed error to the caller | BR6.2, C1 |
| NFR4.9 | Budget counters keyed `(scope, scope_key, day_utc)` upsert atomically — concurrent `record_usage` calls never lose a delta (ON CONFLICT increment, not read-modify-write) | BR6.1 |
| NFR4.10 | `record_usage` AFTER `check_budget` is not transactional — the check/act window can over-admit under concurrency; documented as the accepted bound for the single-process limiter (see scalability NFR11.x) | BR6.2, FR6.7 |

## Orphan sweep interplay

| id | requirement | basis |
|----|-------------|-------|
| NFR4.11 | U2 does NOT extend the U1 boot-time `sweep_streaming_messages` — a `streaming` row stranded mid-flight already flips to `error` at boot; new U2 tables carry no streaming statuses needing sweep | U1 shipped sweep (oxidean-db oxpilot.rs), BR2.3 |
