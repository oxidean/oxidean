# Reliability Design — session-core (U2)

Consumes: `nfr-requirements/reliability-requirements.md`
(NFR4.1–4.12), `functional-design/rules.md` (BR2.2, BR4.x, BR5.x),
`nfr-design-questions.md` (Q2, Q3, Q4).

## Dedupe decision table (Q4 — BR2.2's arms)

| original state | resend lands on | result |
|----------------|-----------------|--------|
| in `oxpilot_messages` — ANY status incl. `streaming` (the cmid lookup carries no status filter; attach keys on `prior.seq`) | chat POST | `resend_matches` (pure comparator — turn state + prior seq, no DB) decides attach; route streams/returns the EXISTING row (routes.rs:298–310) |
| in `oxpilot_messages` | `message_queue` | returns the pinned literal `{"queued_id": null, "position": null}` — the artifact is already persisted; nulls signal nothing was queued (BR2.2 "IF in messages THEN attach/return it") |
| in `oxpilot_queued_messages` | chat POST | `409 turn_active` — no queued-position return shape on the SSE route (fd R-14) |
| in `oxpilot_queued_messages` | `message_queue` | return existing `{queued_id, position}` — idempotent enqueue; position is the recomputed ordinal `COUNT(queue_seq < mine)+1`, not raw `queue_seq` |
| absent from both | either | normal insert path |

Collision handling: `UNIQUE(session_id, client_msg_id)` violation on
either insert ⇒ re-read + return the existing row (NFR4.12) — the
UNIQUE guarantees the winner is committed when the loser's insert
fails.

## Drain lifecycle (Q3)

```
turn start ──▶ drain_queue_tx(session)   # residual rule (NFR4.3)
   │              └ picks up rows orphaned by a prior crash
   ▼
[turn runs — queued cards visible via queue_list]
   │
turn end  ──▶ drain_queue_tx(session)   # normal delivery (NFR4.2)
```

Crash windows and their closures:

| crash point | state after restart | recovery |
|-------------|--------------------:|----------|
| mid-drain tx | tx uncommitted ⇒ queued rows intact, no transcript rows | next turn-start drain delivers them |
| after drain commit, before process exit | transcript rows exist, queue empty | none needed — already delivered |
| mid-turn before drain | queued rows persist | next turn-start drain |
| stranded `streaming` message row | U1 `sweep_streaming_messages` marks it `error` at boot | existing U1 sweep — U2 adds nothing (NFR4.11) |

## Delete integrity (Q2)

- Hook vec iterates in order; ANY `Err` propagates before the DELETE —
  the session row and children remain (NFR4.5, BR5.2).
- Cascade is one `DELETE FROM oxpilot_sessions … WHERE user_id=?` —
  FK `ON DELETE CASCADE` removes message/tool_call/queue/proposal
  children in the same statement; `oxpilot_usage_counters` is NOT a
  cascade child (scope-keyed, survives deletes — NFR4.6).
- A session with a live turn deletes normally once U3's abort hook
  unregisters the turn — U2's service does NOT short-circuit on
  `ActiveTurns` occupancy (NFR4.7); ordering comes from the hook vec.

## Budget refusal (NFR4.8–4.10)

- `check_budget` → `Err(AppError code `budget_exceeded` (data `{scope}`))`; proc
  maps to `budget_exceeded` typed error — the turn/enqueue call fails
  LOUDLY, never drops the message silently.
- `record_usage` upserts BOTH scope rows in one tx —
  `(user, user_id, today)` and `(instance, '__instance__', today)` —
  or the instance cap never engages (the counter UNIQUE is the upsert
  target for each).
- Counter upsert is atomic `ON CONFLICT DO UPDATE … +delta` —
  concurrent `record_usage` cannot lose deltas (NFR4.9).
- check→act is deliberately NOT transactional across the boundary —
  the over-admit window is the documented single-replica caveat
  (NFR4.10, NFR11.3); tightening it would need a DB-side claim the
  contract doesn't ask for.
