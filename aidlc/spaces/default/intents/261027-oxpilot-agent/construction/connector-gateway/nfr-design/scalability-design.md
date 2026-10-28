# Scalability Design — connector-gateway (U6)

Consumes: `nfr-requirements/scalability-requirements.md`
(NFR-S1–S6), `functional-design/rules.md` (BR4.1, BR5.1),
`nfr-design-questions.md` (Q1, Q3).

## Concurrency structures

```rust
// in the ConnectorGateway impl (api-oxpilot):
struct GatewayState {
    locks:  Mutex<HashMap<TargetId, Arc<Mutex<()>>>>,   // keyed locks
    notify: Mutex<HashMap<TargetId, Arc<Notify>>>,      // claim wakes
}
// lock(target_id): get-or-create Arc<Mutex<()>> — per-target
// serialization only; entries evict on terminal lifecycle
// (disconnected/revoked) once their cancel sets close.
```

- Lock keying is `TargetId` — two targets never contend; a user with
  N targets runs N independent queues.
- `Notify` per target: `notify_waiters()` on every state change that
  could produce a dequeable action or a new cancel entry; claim
  holds `notified()` futures — O(1) wake, O(1) memory per waiter.
- No global mutex, no cross-target iteration on the hot path.

## Index and query plan

| table | index | serves |
|-------|-------|--------|
| `actions` | `(target_id, state, created_at, action_id)` | FIFO dequeue scan (LIMIT CLAIM_ACTIONS_MAX) |
| `actions` | `(session_id)` | session-delete hook + owner `action_status` join |
| `grants` | `(target_id, state)` | live-grant lookup at dispatch, re-mint check |
| `grants` | `(session_id)` | session-delete hook |
| `grants` | `(user_id, state, created_at)` | `grant_list` owner read — NFR-S4's stable ordering: `pending` first (by `created_at` asc), then resolved by `resolved_at` desc (functional-spec pin) |
| `targets` | `PRIMARY (target_id)`, `(user_id)` | identity bind (`connector_id` on the wire IS `target_id` — no separate column) + owner list |
| `targets` | `(verifier_hash)` UNIQUE-where-not-null | find-by-hash auth lookup |
| `pairings` | `(code_hash)` UNIQUE, `(user_id)` | redeem + owner reads |
| `audit_events` | existing (no FK) | reuse |

All migrations ship tri-dialect in the same change; `dialect_oxpilot`
test extends to prove parity (NFR9).

## Holding cost per resource

| resource | idle cost | active cost |
|----------|-----------|-------------|
| queued action | 1 row | CAS write + audit row |
| parked claim | 1 `Notified` future | wake + LIMIT-50 index scan |
| pending grant | 1 row (≤2 KiB args + ≤512 B summary) | expires lazily — no timer |
| live target | 1 row + Notify entry | touch on each request |

## Single-replica boundary (documented)

In-process `Notify` + keyed mutexes are correct ONLY on one replica —
the documented deployment model (NFR4 constraint, shared with the
runtime units). A second replica would strand claim wake-ups and
split the admission locks; no cross-node primitives are introduced in
v1, and the caveat is stated, not hidden.
