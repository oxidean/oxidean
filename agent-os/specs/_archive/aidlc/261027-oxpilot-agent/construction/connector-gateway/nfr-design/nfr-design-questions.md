# NFR Design Questions — connector-gateway

Unit: `connector-gateway` (U6, kind `service` — greenfield) —
`ConnectorGateway` service: pairing mint/redeem, target registry,
verifier auth, consent grants, action queue, C3 HTTP surface,
disconnect/revoke, C8 hook, audit.

## Sources

- [nfr-req] construction/connector-gateway/nfr-requirements/*.md —
  NFR1.x–NFR5.x rows the designs must satisfy.
- [design] functional-design/{entities,rules,functional-spec}.md —
  BR1–BR7 entity/state-machine ground truth.
- [contract] contract-summary.md — C1 (+amendments), C3, C5, C7, C8.
- [code] crates/oxidean-api-core/src/{pat/bearer.rs,audit.rs,oxpilot.rs,
  rate_limit.rs}, crates/oxidean-db/src/{oxpilot.rs,audit_events.rs},
  crates/oxidean-api/src/main.rs — the shipped seams U6 plugs into.

## Q1. Gateway service shape and ownership of the per-target lock?

- A. `ConnectorGateway` in api-core — a set-once trait field on
  `OxpilotRuntime` beside `sessions` (the `OnceLock<Arc<dyn
  SessionService>>` precedent; NOT the rebuildable
  `OxpilotRuntimeSlot`) — backed by an impl in
  `oxidean-api-oxpilot` that owns: a `KeyedLocks` keyed-mutex map
  `HashMap<TargetId, Arc<Mutex<()>>>` (evicting on terminal targets),
  a per-target `tokio::sync::Notify` for claim wake-up, and the DB
  store via `OxpilotStore`-family traits. Procs delegate to the
  trait; C3 routes call the same impl — one code path, two auth
  entries.
- B. Gateway lives entirely in api-oxpilot with no api-core trait —
  U5's `ToolExecutor` needs `dispatch_action` on a shared type; an
  untyped cross-crate ref breaks the established slot pattern.
- C. DB-side locking via `SELECT … FOR UPDATE` — pins the single-
  replica caveat harder and serializes claim scans; keyed async
  mutex matches the admission-lock precedent.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — api-core trait on OxpilotRuntime (OnceLock set-once, not the slot) + api-oxpilot impl owning KeyedLocks + per-target Notify; one code path, two auth entries

## Q2. Verifier/pairing storage + lookup mechanism?

- A. `pairings` table: `{pairing_id, user_id, code_hash UNIQUE,
   target_name, requested_caps[], expires_at, state
   pending|claimed|expired, created_at}`;
   `targets` table: `{target_id, user_id, pairing_id, name, caps[],
   status pending_consent|connected|disconnected|revoked,
   verifier_hash NULLABLE (tombstone on disconnect — flagged
   deviation from entities' required:true), last_seen_at,
   created_at, disconnected_at?, revoked_at?}` — `connector_id` on
   the wire IS `target_id`, no separate column. Redeem = `UPDATE
   pairings SET state='claimed' WHERE code_hash=? AND state='pending'
   AND expires_at>now` in the same tx as target INSERT — atomic
   single-use via the state-conditional update's row count. Verifier
   lookup = `SELECT target WHERE verifier_hash = sha256(token)`
   (find-by-hash, bearer.rs precedent).
- B. Encrypt verifier at rest like FR5.4's AES target secrets —
  verifier is authentication not user-secret; hash-only is the
  pinned posture and nothing can re-derive it to the wire.
- C. Store pairing codes plaintext with a flag — violates BR7.3's
  write-only posture.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — pairings{code_hash UNIQUE,target_name,requested_caps,state pending|claimed|expired}+targets{user_id,name,caps,status,verifier_hash NULLABLE,last_seen_at}; connector_id IS target_id; atomic pending→claimed redeem in target-INSERT tx; find-by-hash lookup per bearer.rs

## Q3. Claim long-poll mechanism and wake sources?

- A. Per-target `Notify` in the gateway impl: `flip_claimed_to_running`
  + `last_seen_at` touch first (connector-touch proof covers EARLIER
  deliveries only), then ARM the waiter (`pin!` + `Notified::enable()`)
  BEFORE the first dequeue — `notify_waiters` stores no permit for
  unregistered futures, so arming ahead of the check closes the
  lost-wake window; dequeue-check, empty ⇒ `tokio::time::timeout(
  min(wait_seconds, 60), notified)`, wake ⇒ re-dequeue. Dispatch
  INSERT (queued), any cancel-path
  transition, and disconnect/revoke all `notify_waiters()`. 204 on
  timeout with `Retry-After` header. Dequeue = `SELECT … WHERE
  target_id=? AND state='queued' ORDER BY created_at, action_id LIMIT
  CLAIM_ACTIONS_MAX` → per-row state-conditional `queued→claimed`
  inside the per-target lock with the BR5.1 re-check split: dead grant
  ⇒ `cancelled`+audit; caps mismatch ⇒ skip only.
- B. `pg_notify`/cross-process channel — single-replica caveat is
  documented; in-process Notify is the honest mechanism and a second
  channel is dead code on this deployment.
- C. Spin-poll the table on an interval — wastes DB and adds latency
  the Notify wake removes.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — per-target tokio::sync::Notify; claim = flip+touch, arm waiter (enable()) BEFORE dequeue-check, then timeout(min(wait,60), notified) park + re-dequeue; locked state-conditional dequeue w/ split BR5.1 re-check (dead grant→cancel, caps miss→skip) + CLAIM_ACTIONS_MAX; 204+Retry-After on timeout

## Q4. Consent/grant materialization transaction shape?

- A. `grant_respond(allow, scope)` inside the per-target lock: (0)
  BR3.1 lazy expiry first — `pending` + past `expires_at` ⇒ stamp
  `expired`+audit then `grant_not_pending`; (1) `SELECT grant`
  (the lock serializes, so a plain read suffices); (2)
  state-conditional UPDATE `pending→allowed` stamping
  `kind=action_once|action_session` (absent scope defaults `once`,
  BR3.5/C1 amendment), `resolved_at` — non-pending ⇒
  `grant_not_pending`; (3) SAME tx: `INSERT action {queued,
  grant_id, kind, args (from grant), session_id}` — materialization
  is atomic with approval (BR4.2); (4) audit `action_grant_allowed`
  + `action_dispatched` post-commit (deny branches to
  `connect_grant_denied`/`action_grant_denied` by kind).
- B. Two transactions (stamp, then insert) — a crash between them
  leaves an allowed grant whose action never exists — the
  approved-but-never-minted state the fd forbids.
- C. Insert action at dispatch-time optimistically then cancel on
  deny — BR4.1 pins no action row until approval (consent evidence
  stays in the grant row only).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — lazy-expiry first, then single tx: state-conditional pending→allowed (non-pending ⇒ grant_not_pending) + kind stamp (default once) + Action{queued} INSERT; audits post-commit, kind-branched

## Q5. Dispatch check order and the Denied/pending-grant path?

- A. Inside the per-target lock, in order: (1)
  `SessionService.get_session(user_id, session_id)` → `not_found` on
  foreign; (2) target row: `user_id` match + `status=connected` +
  `last_seen_at` fresh → `Denied{not_connected}` /
  `Denied{offline}`; (3) `kind ∈ target.caps` → `Denied{capability}`;
  (4) live `action_session` grant (allowed+unexpired, scoped to
  target+session — NOT per action-kind, NFR1.14) → on hit INSERT
  `Action{queued}` + notify + `action_dispatched`; (5) miss →
  pending-grant dedupe on `(target_id, session_id, action_kind,
  action_args)` — a past-due match is lazily stamped `expired`+audit
  and a fresh pending minted instead of rejoining (BR3.1);
  identical-args pending grant rejoins it
  (`Denied{consent_required, grant_id}`), distinct args INSERT new
  pending grant (`ACTION_GRANT_TTL=5min`, bounded redacted args +
  summary) → `Denied{consent_required, grant_id}` +
  `action_grant_pending`. Return shape `Result<Action, Denied>` —
  C7's error-data pin, not an Ok-variant.
- B. Check grant first then session — the fd's order is pinned
  (session → target → caps → grant); reordering leaks target
  existence to a caller whose session doesn't own it.
- C. Mint the Action row queued at dispatch even when denied —
  unauthorized intent never reaches the queue per BR4.1.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — pinned order inside the lock: session-owner → target connected+liveness → caps → live session-grant → dedupe/mint pending → Denied{consent_required,grant_id}

## Q6. Session-delete hook mechanics?

- A. `ConnectorDeleteHook: SessionDeleteHook` registered in main.rs
  AFTER U3's abort hook: `on_session_delete(session_id, user_id)` →
  single tx: grants `WHERE session_id=? AND state IN (pending,
  allowed)` → `cancelled`; actions `WHERE session_id=? AND state IN
  (queued, claimed, running, cancel_requested)` → `cancelled` —
  direct terminal (dead channel), `action_cancelled`/
  `grant_cancelled` reason `session_deleted` audited per row;
  idempotent (second call hits no live rows → Ok); a failure aborts
  the whole delete (C8). Connect grants (session_id NULL) untouched.
  The FK cascade then removes the rows — audit_events survive.
- B. Transition to `cancel_requested` and let heartbeat deliver —
  the rows cascade-delete; a cancel set built from deleted rows is
  undeliverable (BR6.5's explicit reconciliation).
- C. Skip transitions and rely on cascade alone — audit evidence of
  the kill transitions is the FR5.5/US5.3 obligation.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — ConnectorDeleteHook after U3 abort: session grants+actions →cancelled directly in one tx, audited session_deleted, idempotent, failure aborts delete (C8)

## Q7. Observability sinks and levels?

- A. Audit per BR7.2's pinned set via `audit::record` on proc paths
  + `audit::record_with` on C3 routes and the delete hook (no
  `RpcCtx` there — the shipped sibling seam; fire-and-forget either
  way — honest, NFR5.2); `tracing` on `oxpilot_connector` target: `info` on
  `pairing_claimed`/`target_connected`/`action_dispatched`/
  `action_result`; `warn` on `connector_auth_failed` (limiter),
  `pairing_expired`/`pairing_used` redeem refusals, dequeue-batch
  counts, stranded-claim observations, session-delete hook counts;
  `debug` on claim wake/timeout cycles. Fields = ids + counts +
  reason only (BR7.3's hashes-or-ids rule).
- B. Emit metrics counters for every transition — NFR6 pins reuse,
  no new layer.
- C. Log args at debug for triage — violates BR7.3 never-fields.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — BR7.2 audit via fire-and-forget audit::record; oxpilot_connector tracing (info transitions, warn refusals/strands, debug poll cycles); ids+counts+reason only per BR7.3

## Consolidated Summary Confirmation

- Service shape: api-core trait + api-oxpilot impl owning KeyedLocks
  + Notify (Q1); find-by-hash custody, atomic pending→claimed redeem
  (Q2); Notify long-poll + locked state-conditional dequeue (Q3);
  single-tx allow-materialize (Q4); pinned 4-check dispatch + dedupe
  Denied path (Q5); direct-cancelled session-delete hook w/ audit
  (Q6); BR7.2 audit + oxpilot_connector tracing (Q7).

Does this all look correct before I write the NFR-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
