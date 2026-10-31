# Performance Design — connector-gateway (U6)

Consumes: `nfr-requirements/performance-requirements.md`
(NFR3.1–3.13), `functional-design/rules.md` (BR5.x, BR3.2),
`nfr-design-questions.md` (Q3).

## Long-poll design

```rust
// POST /connector/claim — verifier-authenticated
let wait = min(body.wait_seconds, CLAIM_WAIT_MAX /* 60s */);
// (0) connector-touch proof FIRST: flip prior `claimed` rows
//     `running` BEFORE this request's dequeue (BR4.3/NFR3.4 — a
//     claim response alone never marks running; the flip covers
//     work delivered by EARLIER responses, never this batch).
store.flip_claimed_to_running(target_id);
store.touch_last_seen(target_id);
// (1) arm the waiter BEFORE the first dequeue — a `Notified` only
//     registers on first poll, and `notify_waiters` stores no permit
//     for unregistered futures; arming ahead of the check closes the
//     lost-wake window between the empty dequeue's lock release and
//     subscription (tokio `Notified::enable()` exists for exactly
//     this). Then: dequeue-check — work queued since the last poll
//     is dequeuable at arrival (NFR3.2).
let notified = gateway.notify(target_id).notified();
tokio::pin!(notified);
notified.as_mut().enable();          // registered before any check
let mut batch = gateway.dequeue(identity, body.capabilities).await?;
if batch.is_empty() {
    // (2) park on the already-armed waiter up to `wait`; on wake
    //     re-dequeue under the lock.
    match timeout(Duration::from_secs(wait), notified).await {
        Err(_) => return (Status::NO_CONTENT,
                          header Retry-After: hint),      // 204
        Ok(_)  => batch = gateway.dequeue(identity,
                                          body.capabilities).await?,
    }
}
let cancels = store.cancel_set(target_id);
(200, {actions: batch, cancel: cancels, retry_after: hint})
```

Wake sources (all `notify_waiters(target_id)`): dispatch INSERT
`queued`; any transition into `cancel_requested`; disconnect/revoke/
session-delete transitions that change the cancel set. A 204 carries
NO body — `retry_after` rides the `Retry-After` header (C3-pinned
shape: 200 `{actions, cancel, retry_after}`).

## Dequeue algorithm (locked, state-conditional)

```rust
// inside lock(target_id):
let cands = SELECT * FROM actions
   WHERE target_id=? AND state='queued'
   ORDER BY created_at, action_id
   LIMIT CLAIM_ACTIONS_MAX /* 50, NFR-introduced bound */;
for row in cands {
    // BR5.1 re-check, TWO distinct arms:
    // (a) grant dead — `allowed`+past-`expires_at` (the ONLY
    //     reachable dead state at dequeue: pending grants never own
    //     `queued` rows — BR4.2's materialize flips pending→allowed
    //     atomically; and every synchronous death path —
    //     revoke/disconnect/delete/BR6.4 — already cancels the
    //     grant's queued rows in its own tx): flip the row
    //     `cancelled`, audit `action_cancelled{reason:
    //     grant_expired}` — a bounded-enum extension over NFR5.15's
    //     four, flagged here with basis (BR5.1's paired rule). The
    //     grant itself is NOT stamped: `allowed→expired` is outside
    //     entities' closed transitions (lazy expiry applies to
    //     `pending` grants only; live states exit to
    //     revoked/cancelled) — the failed `unexpired` live-test is
    //     the cancel trigger, the reason names the cause.
    // (b) `kind ⊄ claim.capabilities` or `kind ⊄ target.caps` —
    //     SKIP ONLY; the row stays `queued` (NFR1.6).
    if grant_dead(row) {
        UPDATE actions SET state='cancelled'
          WHERE action_id=? AND state='queued';
        audit(action_cancelled, reason=grant_expired);
        continue;
    }
    if !caps_match(row) { continue; }   // stays queued
    let n = UPDATE actions SET state='claimed'
            WHERE action_id=? AND state='queued'; // CAS
    if n == 1 { batch.push(row) }
}
```

`ORDER BY (created_at, action_id)` rides the same-migration index
`(target_id, state, created_at, action_id)` — O(limit) index scan;
only the per-row CAS write serializes (inside the lock).

## Liveness + re-mint costs

- `last_seen_at` write: one `UPDATE targets SET last_seen_at=now`
  piggybacked on every authenticated route (`/pair` excluded —
  pre-credential). No separate job.
- `status` derivation at read: `lifecycle=connected` → `online` iff
  `now - last_seen_at < HEARTBEAT_LIVENESS_WINDOW (90s)` — computed,
  not stored; revoked → `revoked`; other lifecycles → `offline`.
- BR3.2 re-mint: on any owner read of a `pending_consent` target,
  `INSERT … SELECT WHERE NOT EXISTS(live connect grant)` under the
  per-target lock — O(1), idempotent, audited `connect_grant_pending`.

## Bounds summary

| bound | value | enforced at |
|-------|-------|-------------|
| `CLAIM_WAIT_MAX` | 60 s | claim clamp |
| `CLAIM_ACTIONS_MAX` | 50 (NFR-introduced) | dequeue LIMIT |
| `ACTION_ARGS_MAX` | 2 KiB | dispatch arg ingest |
| `SUMMARY_MAX` | 512 B | summaries on grant/action views + audit |
| `RESULT_MAX` | 2 KiB | report output tail → `result_summary` |
| `HEARTBEAT_LIVENESS_WINDOW` | 90 s | status derivation |
| pairing entropy | ≥160 bits | mint |
| verifier entropy | ≥256 bits | redeem |

## Contention profile

- Per-target lock keys only mutating decisions on that target;
  candidate scans + owner reads run lock-free (except the BR3.2
  re-mint write path).
- One parked future per idle claim; wake is O(1) Notify.
- `action_status`/`target_get`/`grant_*` reads are indexed owner-scoped
  lookups — no scans on the hot path.
