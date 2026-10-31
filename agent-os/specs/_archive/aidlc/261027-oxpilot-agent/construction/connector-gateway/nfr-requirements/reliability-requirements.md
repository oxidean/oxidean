# Reliability Requirements — connector-gateway (U6)

Consumes: `construction/connector-gateway/functional-design/rules.md`,
`functional-design/entities.md`, `functional-design/functional-spec.md`,
`inception/requirements-analysis/requirements.md` (NFR4),
`inception/contract-design/contract-summary.md` (C3, C5, C7, C8).

The gateway owns the state machines whose integrity decides whether an
agent action runs zero, once, or wrongly — reliability here is
idempotency, atomic transitions, honest end-state semantics, and
graceful degradation that never lies about what the connector can do.

## Idempotency & terminal correctness

| id | requirement | basis |
|----|-------------|-------|
| NFR4.1 | `POST /connector/report` is idempotent keyed by `action_id`: a repeat terminal report answers `{ack:true}` without rewriting; unknown OR cross-target `action_id` answers 404 `not_found` (anti-enumeration — no existence leak into another connector's audit space) and C3 pins the connector kills local execution on 404 (BR5.2) | BR5.2, C3 |
| NFR4.2 | Terminal states are first-wins state-conditional updates: `queued→claimed→running→done|failed` plus the cancel branches (`cancel_requested→cancelled` via report-close; direct `→cancelled` on paths where delivery is impossible) — a report racing a cancel resolves deterministically and the loser observes the winner's state (BR4.3, BR5.2, BR6.x) | BR4.3, BR5.2 |
| NFR4.3 | `action_status(user_id, action_id)` exposes the owner-visible truth across the full C7 enum `queued|claimed|running|cancel_requested|done|failed|cancelled` + bounded `result_summary` ≤ `RESULT_MAX`; foreign `action_id` → `not_found` (BR4.4, C7) | BR4.4, C7 |
| NFR4.4 | Report on a `cancel_requested` row closes it `cancelled` — the reported exit_code/result rides the audit detail (C3's "its report still closes the row"); the cancel set re-announces on every heartbeat/claim until closed — idempotent kill delivery (BR5.2, BR5.3) | BR5.2, BR5.3 |

## Graceful degradation

| id | requirement | basis |
|----|-------------|-------|
| NFR4.5 | `oxpilot` disabled → proc side answers `oxpilot.disabled`; C3 side refuses pair/claim/report/heartbeat with `{error:"disabled"}` — fail-closed, NOT an auth failure (BR2.5); no target/grant/action row is minted while disabled | BR2.5, C1, C3 |
| NFR4.6 | Offline = stale `last_seen_at` ONLY: dispatch on a connected-but-stale target → `Denied{offline}` (BR4.1(2)). `disconnected`/`revoked`/`pending_consent` targets → `Denied{not_connected}` (BR6.3, spec negative paths). A `disconnected` target's verifier is TOMBSTONED — every connector route answers 401 `{error:"unauthorized"}`; there is no post-disconnect touch. At disconnect (BR6.1): verifier tombstoned, `Target→disconnected`, pending grants → `cancelled`, queued actions → `cancelled`, claimed/running → `cancelled` DIRECTLY — the channel is dead, `cancel_requested` could never be delivered — audited `action_cancelled` reason `target_disconnected` + `target_disconnected` (BR6.1, BR6.3) | BR4.1, BR6.1, BR6.3 |
| NFR4.7 | A `revoked` target's verifier STILL authenticates (BR2.2): `heartbeat` answers `{revoked:true}` + the cancel set; `claim`/`report` refuse `{error:"revoked"}` EXCEPT report's terminal-close arm — a revoked target may still close its claimed/running/cancel_requested rows (BR5.2's C3-pinned exception). At revoke: ALL grants → `revoked`, queued → `cancelled`, claimed/running → `cancel_requested` carried on the heartbeat cancel set (claim refuses, so heartbeat alone delivers). Dispatch → `Denied{not_connected}` (BR6.2, BR6.3) | BR2.2, BR6.2, BR6.3, BR5.2 |
| NFR4.8 | Session delete ordering is pinned (BR6.5, C8): (1) U3's abort hook runs FIRST; (2) `ConnectorDeleteHook` resolves the session's grants pending/allowed → `cancelled` and actions queued/claimed/running → `cancelled` DIRECTLY — never `cancel_requested`, because the rows cascade-delete and a cancel set could never be delivered; (3) FK cascade removes session-owned rows; (4) `audit_events` (no-FK) survive as the durable evidence, audited `action_cancelled`/`grant_cancelled` reason `session_deleted`; (5) a late report → `not_found` = kill-local. Connect grants (`session_id NULL`) survive untouched (BR6.6). Hook failure aborts the delete; the hook is idempotent | BR6.5, BR6.6, C8, AC1.5.2 |
| NFR4.9 | Pairing expiry before state ON EVERY REDEEM: any code past `expires_at` answers `pairing_expired` — never `pairing_used` — whatever its prior state (expiry is evaluated before the `state≠pending → pairing_used` arm, BR1.2/BR1.4); `pairing_expired` audit stamps lazily at read/redeem — no sweeper | BR1.2, BR1.4 |
| NFR4.10 | Dead-target idempotence: `target_disconnect`/`target_revoke` on an already-terminal target answers `{ok}` — no "already dead" vocabulary exists and the intent is satisfied (BR6.7) | BR6.7 |

## Concurrency integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR4.11 | The per-target admission lock serializes `dispatch_action`, `grant_respond`, dequeue writes, and the revoke/disconnect/delete close transitions ON that target — check→insert sequences cannot interleave two dispatches onto one grant or resurrect a dead target mid-flight; scans/reads run lock-free (BR4.1, BR5.1) | BR4.1, BR5.1 |
| NFR4.12 | FIFO claim order is index-enforced by `created_at` — no in-memory reordering; a claim crash mid-batch has already CAS'd the rows it returned (at-most-once), the remainder stay `queued` for the next poll (BR5.1) | BR5.1 |
