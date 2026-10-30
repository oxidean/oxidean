# Code Summary — connector-gateway (U6)

The complete in-process connector gateway for Oxpilot computer-use:
`0088_oxpilot_connector` migrations on all three dialects, the
dialect-dispatched connector store behind `Database`, the
`ConnectorGateway` trait + DTO seam in api-core, the concrete
`OxpilotConnectorGateway` in api-oxpilot (pairing, verifier auth,
consent grants, claim/report/heartbeat, lifecycle, session-delete
hook), the four C3 connector HTTP routes, eight owner RPC procedures,
boot wiring, the regenerated typed client, and the assembled-router
integration suite. No new deployable service and no background
sweeper — expiry is lazy and decision-path based.

## Data layer

- **`crates/oxidean-db`** — `0088_oxpilot_connector.sql` x3 dialects:
  `oxpilot_pairings` (`UNIQUE(code_hash)`, single-use `pending`/`claimed`
  /`expired`), `oxpilot_targets` (`verifier_hash` partial-unique so NULL
  tombstones never collide — MySQL documents its functional-unique
  divergence), `oxpilot_grants` (connect/action kinds, `session_id`
  nullable — NULL ⇒ connect grant that survives session delete), and
  `oxpilot_actions` (`queued→claimed→running→terminal` + `cancel_requested`
  /`cancelled`). Session-scoped rows FK `ON DELETE CASCADE`; indexes
  cover the `(target_id, state, created_at)` FIFO dequeue and owner
  reads.
- `src/oxpilot.rs` — row structs + store fns per dialect: pairing
  mint/lookup-by-hash/lazy-expire/`claim_pairing_tx` (one tx: pairing
  CAS + target insert + connect-grant insert), target CRUD + verifier
  lookup + `last_seen_at` touch + `disconnect_target_tx`/`revoke_target_tx`
  (tombstone vs verifier-preserving close paths + bulk grant/action
  transitions in one tx), grant mint/find/list/live-session-lookup/
  pending dedupe/lazy-expire/deny CAS/`allow_*_tx` (the connect-allow
  tx also gates on the target's `pending_consent` CAS landing —
  `Ok(false)` rolls the grant flip back so a dead target can never
  record `connected`)/session-grant revoke tx, action insert/find/FIFO
  candidates/claim CAS/queued
  cancel/report close/`claimed→running` flip/cancel-requested ids/
  session-cascade cancel tx.
- `src/lib.rs` — `Database` facade methods over the same set.
- `tests/dialect_oxpilot.rs` — 0088 assertions: schema columns,
  pairing mint/lookup/expire, atomic claim tx + verifier uniqueness,
  grant state machine, FIFO dequeue + report idempotency,
  disconnect/revoke close paths, session-delete cancel-then-cascade;
  `docs/TESTING.md` db-dialects row names the 0088 proofs. SQLite leg
  23/23 green.

## API-core seam (`src/oxpilot.rs`)

- `trait ConnectorGateway` — the C7 surface plus the C3/proc backing
  verbs; every verb takes `&ClientMeta` for audit IP/UA (internal
  callers pass `ClientMeta::default()`). One trait object installed
  beside `sessions` on `OxpilotRuntime` — set-once `OnceLock`
  (`install_gateway`/`gateway()`), survives slot rebuilds; one impl,
  one state machine for routes, procs, U5 dispatch, and the C8 hook.
- DTOs: `ConnectorIdentity{target_id,user_id,caps,status}` (the sole
  scoping source on connector routes), `PairingView` (shown-once
  `pairing_code`), `TargetView{status,lifecycle,pairing_id,granted_caps}`
  (never `verifier_hash`/`last_seen_at`), `GrantView` (`kind` collapsed
  `connect|action`; `args_summary` only — `action_args` never
  serializes), `ActionView`, `RedeemedPairing{connector_id==target_id,
  verifier}`, `ClaimAction`, `ClaimOutcome::{Batch,Timeout}`,
  `HeartbeatOutcome{revoked,cancel}`.
- `denied(reason, grant_id)` mints the C7-pinned refusal — an
  `AppError{code:"oxpilot.action_denied", data:{denied:{reason,grant_id?}}}`;
  `denied_reason::{NOT_CONNECTED,OFFLINE,CAPABILITY,CONSENT_REQUIRED}`.
- Pinned constants: `PAIRING_CODE_TTL_SECS=600`,
  `ACTION_GRANT_TTL_SECS=300`, `CONNECT_GRANT_TTL_SECS=600`,
  `CLAIM_WAIT_MAX_SECS=60`, `HEARTBEAT_LIVENESS_WINDOW_SECS=90`,
  `CLAIM_DEQUEUE_MAX=50`, `ACTION_ARGS_MAX`/`RESULT_MAX`=2KiB,
  `SUMMARY_MAX`=512; `CONNECTOR_CAPABILITIES` closed enum
  `{exec_shell,fs_read,fs_write}`.

## Gateway (`oxidean-api-oxpilot/src/connector/`)

- `gateway.rs` — `OxpilotConnectorGateway::new(db, sessions)`; process-
  local per-target `Mutex` map serializes every decision-sensitive
  operation and a per-target `Notify` map closes the claim
  lost-wakeup window (armed BEFORE the first dequeue — see the pinned
  `tokio::pin!` + `enable()` order); shared `FailedAuthLimiter` feeds
  both the connector-auth IP bucket (20/15min — checked BEFORE the
  hash lookup, miss records + audits `connector_auth_failed`) and the
  owner mint user bucket (10/15min, check+record under ONE guard).
  Post-commit audits inside decision paths queue as `DeferredAudit`
  and emit via `emit_deferred` after `drop(_g)` — the critical
  section never awaits an audit write. `ensure_live_connect_grant`
  re-verifies `pending_consent` under the held lock before minting
  (BR6.3: a dead target never gains a dangling connect grant).
- Pairing: `mint_pairing` validates caps ⊆ enum + target-name bound,
  records the mint limiter, persists only `sha256(code)`;
  `redeem_pairing` orders expiry-first (`pairing_expired` + lazy
  stamp) then `pairing_used` then caps ⊆ minted → atomic claim tx —
  a forbidden attempt never consumes the code.
- `resolve_verifier` — sha256 → `verifier_hash` lookup → identity;
  absent/tombstoned ⇒ `unauthorized` (same limiter path); `revoked`
  still resolves so heartbeat can answer `revoked:true`.
- `dispatch_action` (BR4.1) under the per-target lock: session
  ownership passthrough (`not_found`) → target user-match/lifecycle →
  liveness (`offline`) → caps → live `action_session` grant → pending
  dedupe on the redacted+bounded stored args (identical args join the
  same card) → mint action grant → `Denied{consent_required,grant_id}`;
  a live grant inserts `queued` + notifies waiters. Consent is always
  the `Denied` error channel, never an Ok variant.
- `respond_grant` (BR3.5): `pending` → `allowed` (connect CAS flips the
  target `connected` + audits `target_connected`; action mints the
  `Action{queued}` in one tx, scope stamps `action_once`/`action_session`)
  or `denied`; `allowed action_session + deny` is the BR6.4 revoke arm
  (`revoked` + queued→`cancelled` + in-flight→`cancel_requested`);
  everything else `grant_not_pending`. `scope` ∈ `{once,session}`.
- `claim` (BR5.1): revoked ⇒ `{error:revoked}`; every call flips prior
  `claimed→running` + touches `last_seen_at` FIRST (touch-proof of
  delivery); dequeue under lock (dead grant ⇒ cancel+audit
  `grant_expired`, caps miss ⇒ skip); empty + pending cancel set ⇒
  immediate batch; else park on the armed `Notify` up to the 60s
  clamp → `204`/`Timeout{retry_after}` or a re-dequeued batch.
- `report` (BR5.2): validates status, bounds+redacts output into
  `result_summary`; terminal close is CAS-idempotent (re-report on a
  closed row ⇒ same `{ack:true}`); `cancel_requested` + report ⇒
  `cancelled`; unknown/cross-target ⇒ `not_found` (kill-local).
- `heartbeat` (BR5.3): liveness touch + `claimed→running` flip +
  `{revoked, cancel:[ids]}` re-announcing `cancel_requested` rows.
- `disconnect_target`/`revoke_target` (BR6.1/BR6.2): the two close
  paths with their pinned verifier semantics; idempotent on terminal
  targets (BR6.7); every cancelled row owes its own audit event.
- Owner reads fold absent and foreign ids to `not_found` (BR7.1
  anti-enumeration); `list_targets`/`get_target` run BR3.2's lazy
  pending-connect re-mint on `pending_consent` rows; `list_grants`
  lazy-stamps past-due `pending` grants `expired` on observation.
- `on_session_delete` (BR6.5): `cancel_oxpilot_session_connector_rows_tx`
  cancels session-scoped grants/actions (`reason:"session_deleted"`
  audits); connect grants (`session_id NULL`) survive (BR6.6);
  idempotent; `Err` aborts the delete.
- `hook.rs` — `ConnectorDeleteHook` wraps the same `Arc` into the
  `SessionDeleteHook` shape; registered in `main.rs` AFTER
  `OxpilotTurnAbortHook` so deletion order is U3 abort → U6 connector
  cancel → session cascade (C8).
- Audit events (post-commit `audit::record_with`): `pairing_created/
  pairing_claimed/pairing_expired/pairing_used`, `target_connected/
  connect_grant_allowed|denied|expired`, `action_grant_pending/
  action_grant_allowed|denied|expired`, `action_session_revoked`,
  `action_dispatched/action_cancelled/action_claimed/action_result`,
  `target_disconnected/target_revoked`, `grant_cancelled`,
  `connector_auth_failed`, `session_deleted`-reasoned cancels — never
  carrying verifier/code/hash material or unredacted args/output.

## Routes + procs

- `connector/routes.rs` — `/pair` (unauthenticated carve-out — the
  code IS the credential) + `/claim` + `/report` + `/heartbeat`
  mounted under `/api/oxpilot/connector` in `src/routes.rs`.
  Enablement gates FIRST (`resolve_enablement_slot` →
  `{error:"disabled"}` 403 fail-closed); verifier-class Bearer only —
  no cookies/PATs; body `target_id` must equal the resolved identity
  (mismatch ⇒ 403); `ClaimOutcome::Timeout` ⇒ `204`+`Retry-After`;
  the `{error:code}` map covers `unauthorized/rate_limited/forbidden/
  revoked/disabled/not_found/bad_input/pairing_expired/pairing_used/
  internal`.
- `connector/procs.rs` — the eight owner procs (`pairing_create`,
  `target_list`, `target_get`, `target_disconnect`, `target_revoke`,
  `grant_list`, `grant_get`, `grant_respond`), `SessionOnly` at the
  gate + `require_enabled`/`require_verified` inside; registered in
  `PROCEDURES` + `dispatch` (the crate's `procedures_list_matches_
  dispatch` source-scan test enforces parity; `all_procs_are_session_
  only` covers the new names automatically).
- `crates/oxidean-api/src/main.rs` — one gateway `Arc` constructed
  from `state.db` + `oxpilot_sessions`, `install_gateway` once,
  `ConnectorDeleteHook` registered after `OxpilotTurnAbortHook`,
  session service installed last. Same Arc serves routes, procs,
  U5 dispatch, and the hook.

## Generated client

- `crates/oxidean-api/src/bin/rpc_gen.rs` — typed surface for the 8
  owner procs: `Oxpilot{PairingCreate,TargetRef,GrantRef,GrantList,
  GrantRespond}Request`, `{Pairing,Target,Grant}View`,
  `TargetList/GrantList/GrantRespond/Ok` responses, client methods,
  `oxpilotTargetList`/`oxpilotGrantList` query options and
  `pairingCreate`/`targetDisconnect`/`targetRevoke`/`grantRespond`
  mutation options; `packages/api-client/src/index.ts` regenerated +
  committed (deterministic; api-client vitest 6/6 green).

## Tests

- `crates/oxidean-api-oxpilot/tests/oxpilot_connector.rs` — 16
  assembled-router integration tests over fresh sqlite schemas: C3
  auth matrix (absent/unknown/tombstoned verifier, body-mismatch
  forbidden, malformed body), `{error:"disabled"}` fail-closed on all
  four routes, connector-auth 429 (20 IP misses trip), owner mint
  limiter (10 trips), pair single-use/expired/caps-bound + unknown
  folds, anti-enumeration across all six scoped procs, dispatch
  ordered-denial matrix (`not_found` session passthrough, foreign/
  absent target → `not_connected`, `offline`, `capability`),
  consent→`Denied` shape + pending dedupe + redacted summary,
  session-scope grant reuse + BR6.4 revoke arm + cancel-set flow,
  claim park/wake (204+Retry-After, armed-Notify wake on enqueue),
  disconnect tombstone + bulk cancel, revoke verifier-preserved
  `revoked:true`/claim-`revoked`/report-close paths, session-delete
  cascade keeping connect grants with `session_deleted` audits,
  report validation/cross-target 404/idempotent re-report, and the
  audit feed spy (events present; verifier/code/hashes/PAT material
  absent).
- Unit-side: `oxidean-api-oxpilot` 93-unit suite green (incl.
  `procedures_list_matches_dispatch` + `all_procs_are_session_only`
  covering the new names); `oxidean-api-core` 213-unit suite green;
  `oxidean-db::dialect_oxpilot` 23/23 sqlite green.

## Verification record

- `cargo fmt` — clean (all touched crates).
- `cargo clippy --all-targets` — clean on `oxidean-api-oxpilot`,
  `oxidean-api-core`, `oxidean-api`.
- `cargo test -p oxidean-api-oxpilot` — 93 unit + 16 connector +
  9 provider + 11 settings integration, all green.
- `cargo test -p oxidean-api-core` — 213 green.
- `cargo test -p oxidean-db --test dialect_oxpilot` — 23/23 (sqlite).
- `make rpc-gen` — deterministic output; `packages/api-client` diff
  is the committed generated surface (466 added lines, U2 backlog +
  U6 surface). `make rpc-sync-check` fails locally only because the
  regenerated file is an uncommitted change — it is the CI gate's
  expected diff and passes once committed. api-client vitest 6/6.
- Docs: `docs/TESTING.md` (db-dialects row + scoped commands),
  `docs/CONFIGURATION.md` (connector-egress paragraph, no new env).

## Sources

- `unit-of-work`: `connector-gateway` (U6) — the unit this summary
  covers; scope and ownership per the delivery plan.
