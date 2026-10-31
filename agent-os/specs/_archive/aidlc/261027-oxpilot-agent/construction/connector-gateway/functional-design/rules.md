# Business Rules — connector-gateway (U6)

## Rule set

### BR1 — Pairing lifecycle

- **BR1.1** `oxpilot.pairing_create{target_name, caps}` (owner-scoped,
  gate-checked) validates `caps ⊆ {exec_shell, fs_read, fs_write}`
  (FR5.3 — a minted pairing can never carry a GUI cap), mints a random
  code (≥ 160 bits), stores `sha256(code)`, inserts
  `Pairing{state:pending, expires_at: now+PAIRING_CODE_TTL(10 min),
  requested_caps: caps}`, audits `pairing_created`, and returns
  `{pairing_id, pairing_code, expires_at}` — the ONLY response that
  ever carries plaintext code material (AC4.1.4); `pairing_id` is
  the pairing→target correlation handle the connect dialog polls
  `target_list`/`target_get` against (C1 additive amendment).
- **BR1.2** `POST /connector/pair{pairing_code, device_label,
  requested_caps}` (unauthenticated — the code is the credential,
  RFC-8628 precedent): looks up by `sha256(pairing_code)`; checks
  `expires_at` FIRST (lazily flips `expired`, answers
  `pairing_expired`, audited) — then `state≠pending` → `pairing_used`
  (audited); `requested_caps ⊄ pairing.requested_caps` → `forbidden`.
  Order matters: an expired-then-retried code must always read
  `pairing_expired`, never degrade to `pairing_used`. On pass, in ONE
  transaction: `Pairing→claimed`, INSERT `Target{status:
  pending_consent, verifier_hash: sha256(fresh token), user_id:
  pairing.user_id, name: device_label fallback target_name, caps:
  intersection}`, audit `pairing_claimed`, return `{connector_id,
  target_id, verifier}` — verifier shown once. `connector_id` is the
  same value as `target_id` in v1 (the C3 sketch carries both names —
  U7 emits one value for both).
- **BR1.3** Neither code nor verifier is ever returned again by any
  proc, log line, or audit detail — hashes only (FR5.4, C1 notes).
- **BR1.4** Pairing expiry is lazy — `expires_at` evaluated at redeem
  and at user-facing read time; no sweeper exists.
- **BR1.5** `pairing_create` is rate-limited per user (the
  credential-mint pattern) — abuse guard on the only user-facing
  mint. The `/pair` redeem route relies on ≥160-bit code entropy
  rather than a rate limit (a limiter can't distinguish a retrying
  legitimate connector; the code space makes guessing infeasible) —
  failed redeems still audit.

### BR2 — Connector authentication

- **BR2.1** `/api/oxpilot/connector/*` (except `/pair`) requires
  `Authorization: Bearer <verifier>` → `sha256_hex` →
  `oxpilot_targets.verifier_hash` lookup → `ConnectorIdentity`.
  Absent/unknown hash or `status=disconnected` target → 401
  `{error:"unauthorized"}`; the attempt feeds `FailedAuthLimiter`
  keyed on IP (credential-endpoint precedent — BR1.5 covers the user
  side).
- **BR2.2** A `revoked` target's verifier STILL authenticates —
  heartbeat answers `{revoked:true}` (the connector learns to stop;
  C3's claim shape has no revoked field so claim/report answer
  `{error:"revoked"}` instead). ONE exception: `report` on a
  claimed/running/cancel_requested action of a revoked target is
  ACCEPTED for terminal-close — C3 pins "its report still closes the
  row"; it lands `cancelled` (or the reported terminal status) +
  audit. `disconnected` targets authenticate nothing — the verifier
  is tombstoned (BR6.1).
- **BR2.3** These routes never see session cookies or PATs — the
  extractor accepts ONLY the connector verifier class (C3). The
  session-auth middleware does not wrap this subtree.
- **BR2.4** `ConnectorIdentity` is the sole scoping source — a body
  `target_id` must equal the resolved identity's `target_id` (else
  `forbidden`); `user_id` in every row write/audit comes from the
  identity, never the request body (AC5.5.4).
- **BR2.5** The connector subtree is enablement-gated fail-closed:
  flag-off or provider-absent refuses pair/claim/report/heartbeat
  with `{error:"disabled"}` — queued work must not execute while the
  feature is nominally off (NFR4).

### BR3 — Grant state machine

- **BR3.1** `Grant.state ∈ {pending, allowed, denied, expired,
  revoked, cancelled}`; `pending→allowed|denied` via `grant_respond`;
  `pending→expired` lazily at ANY observation — read, dispatch,
  claim-check, `grant_respond`, or any write touching the row (a
  past-due `pending` grant is `expired` before the write evaluates,
  never `allow`able past its window); live states →`revoked`
  (target_revoke / session-grant revoke) or `cancelled` (disconnect /
  session-delete). `denied|expired|revoked|cancelled` are terminal.
  WHENEVER a grant leaves `pending`/`allowed` for a terminal state —
  any path — its `queued` Actions transition `cancelled` + audit in
  the same write (a `queued` row never outlives its authorization).
- **BR3.2** Connect grant (`session_id NULL`, `kind:connect`): minted
  `pending` when pairing completes (the connect dialog's server side —
  AC4.2.3). `grant_respond(allow)` → `allowed` + `Target.status:
  connected`; `deny` → `denied` + target stays `pending_consent`.
  Re-consent: ANY owner read of a `pending_consent` target —
  `target_list`, `target_get`, `grant_list` — lazily mints a fresh
  `pending` connect grant when none is live (pending+unexpired),
  audited `connect_grant_pending`; the mint is a state-conditional
  insert under the per-target lock (insert-if-none-live) so
  concurrent reads can never mint duplicate pending grants. A denied
  or expired consent is never a dead end. AC4.2.2: denied is
  auditable.
- **BR3.3** Action grants (`session_id` set): minted `pending` ONLY
  by `dispatch_action`'s Denied path (BR4.1), carrying
  `action_kind`, bounded redacted `action_args` (≤ 2 KiB),
  `action_summary` (≤ 512 B), `expires_at: now+ACTION_GRANT_TTL
  (5 min)`, and kind `action` — UNFLAVORED: the once-vs-session
  choice is the user's at consent time (interaction-spec's Allow
  once/session), so `grant_respond{allow, scope:once|session}` stamps
  `action_once` (consumed by the single Action it materializes —
  `resolved_at` set, thereafter covers nothing) or `action_session`
  (covers subsequent dispatches on that target+session until
  expiry/revoke).
- **BR3.4** No `always` flavor exists — kind is a closed four-member
  enum (`connect`, `action` = pending-unflavored, `action_once`,
  `action_session`); cross-session authorization is inexpressible
  (FR5.2, AC4.3.2).
- **BR3.5** `grant_respond{grant_id, allow, scope?}` requires
  owner match (`not_found` on cross-user — BR7.1's anti-enumeration
  convention) THEN one of: `state=pending` → `allowed` (action
  grants stamp kind from scope, default `once`) or `denied`;
  `state=allowed AND kind=action_session AND allow=false` → the
  BR6.4 session-grant revoke arm; every other state/kind →
  `grant_not_pending`. It is the ONLY proc that resolves grants;
  there is no grant-delete.

### BR4 — Dispatch and Action lifecycle

- **BR4.1** `dispatch_action(user_id, session_id, target_id, kind,
  args)` (C7 amended signature — user context, since C5 has no
  unscoped session read) checks IN ORDER under the gateway's
  per-target lock (the same lock serializes grant_respond,
  target_revoke, and claim dequeue for that target):
  (1) `SessionService.get_session(user_id, session_id)` — not_found/
  forbidden propagates (session actions only);
  (2) `Target` exists + `target.user_id == user_id` +
  `status=connected` + liveness-fresh → else `Denied{reason:
  not_connected|offline}`;
  (3) `kind ∈ target.caps` → else `Denied{reason: capability}`;
  (4) LIVE grant = an `action_session` grant `allowed`+unexpired for
  (target,session) — the ONLY shape that passes. A PENDING action
  grant covering (target,session,action_kind) with IDENTICAL stored
  args joins the existing row → `Denied{consent_required, grant_id:
  existing}` (BR4.5's dedupe); distinct args → INSERT a NEW pending
  action grant → `Denied{consent_required, grant_id}` — a second
  dispatch is never silently absorbed into the first's consent.
  `action_once` grants never satisfy (4): an allowed one covers
  exactly the Action it materialized (BR4.2) and is consumed —
  pending action grants always carry the unflavored `action` kind, so
  `action_once` only ever exists post-consent. On pass: INSERT
  `Action{state:queued, grant_id}` + audit `action_dispatched` +
  return the Action.
- **BR4.2** `grant_respond(allow, scope?)` on a pending action grant
  stamps kind from scope (`once` default → `action_once`; `session`
  → `action_session`) and materializes `Action{state:queued}` from
  the grant's stored args in the same transaction. `action_once`
  consumption is `resolved_at`-stamped at materialize — the grant
  stays `allowed` for audit but covers nothing further (BR4.1(4)
  excludes it by kind). `deny` → grant `denied`, NO Action row is
  ever minted (AC4.3.5).
- **BR4.3** `Action.state` machine: `queued→claimed` (BR5.1 dequeue)
  →`running` — C3 carries no per-action progress message, so the
  connector's next authenticated touch on that target (heartbeat or
  claim) flips all its `claimed` rows `running` — proof the
  delivered work is executing — →`done|failed` (report); any live
  state →`cancel_requested` (revoke/disconnect/delete) →`cancelled`.
  Every transition audits.
- **BR4.4** `action_status(user_id, action_id)` returns `ActionView`
  — `{action_id, kind, state, created_at, args_summary,
  result_summary?}`; owner-checked via the action's `user_id`
  (`not_found` on cross-user). `Denied` is the pinned shape
  `{reason, grant_id?}` — C7 amendment, entities.md §ConnectorGateway.
  Error channel: `dispatch_action`'s `Result<Action, Denied>` is an
  `AppError` domain error — `Denied` serializes into the error's
  `data` payload (`data:{denied:{reason, grant_id?}}`) while C5
  passthroughs (`not_found`/`forbidden` from step 1) ride the same
  `AppError` channel as their own codes. U5 distinguishes consent
  refusals from hard errors by the `denied` marker, never by type
  sniffing.
- **BR4.5** Pending-grant dedupe joins ONLY on identical content —
  a second dispatch matching (target, session, action_kind,
  action_args) exactly joins the existing pending grant
  (`Denied{consent_required, grant_id}` of the existing row — model
  retries and duplicate sends share one card); distinct args mint a
  separate pending grant. A same-kind/different-args dispatch is
  NEVER silently absorbed — the second action's args must run under
  their own consent or not at all (NFR4).

### BR5 — Claim / report / heartbeat protocol

- **BR5.1** `claim{wait_seconds, capabilities}`: waits up to
  `min(wait_seconds, 60)` (clamp per C3) for `queued` actions on the
  identity's target in `created_at` FIFO order; dequeue is a
  state-conditional update inside the per-target lock and RE-CHECKS —
  grant still `allowed`+unexpired AND `action.kind ⊆
  claim.capabilities` AND `action.kind ⊆ target.caps` — before
  `queued→claimed` (C3's 'honoured before starting'). A `queued` row
  whose grant died (lazy expiry, revoke, deny) is flipped
  `cancelled` + audit at dequeue observation and SKIPPED — `queued`
  never outlives authorization (BR3.1's paired rule). Response:
  `{actions:[{action_id, kind, args, grant_id}], cancel:[action_id of
  cancel_requested], retry_after}`; timeout → `204` + `Retry-After`
  header (no body). Claim touches `last_seen_at` AND flips the
  target's `claimed` rows `running` (BR4.3 — any authenticated touch
  is execution proof). At-most-once delivery: dequeue is
  dequeue-at-once with no requeue — a claim response lost in transit
  strands the action `claimed`/`running` until a server-side close
  lands — revoke/disconnect/session-delete paths, or BR6.4's
  session-grant revoke (`cancel_requested` → report) — the strand
  stays visible via `action_status` and can never self-close
  connector-side (a connector that never got the response holds no
  action_id). Accepted residual — C3 pins no ack for claim receipts,
  and a requeue would break the at-most-once execution semantics
  the consent model relies on.
- **BR5.2** `report{action_id, status, exit_code?, output}`:
  touches `last_seen_at`, resolves the action and requires
  `action.target_id ==
  identity.target_id` — cross-target or unknown ids both answer 404
  `not_found` (no existence leak into another connector's audit
  space). Idempotent by action_id — a second report on a terminal
  action answers `{ack:true}` without rewriting. Output is bounded
  (≤ 2 KiB tail) + secrets-redacted SERVER-side (FR5.5; the client's
  own redaction is defense in depth, not the barrier). `done|failed`
  → terminal + audit `action_result`. A report on `cancel_requested`
  closes the row `cancelled` (C3's 'its report still closes the
  row') — the reported exit_code/result rides the audit detail. On a
  `revoked` target this close-out path is the ONLY accepted report
  (BR2.2) — new work is refused but in-flight outcomes still land.
- **BR5.3** `heartbeat{target_id}` touches `last_seen_at`, flips
  the target's `claimed` rows `running` (BR4.3 — the touch proves
  execution), and returns `{revoked: bool, cancel: [action_id]}` —
  the cancel set is ALL `cancel_requested` rows on this target,
  re-announced on every heartbeat/claim until they close: kill
  semantics are idempotent (the connector de-duplicates), and a
  dropped response can never strand a cancel.
- **BR5.4** Liveness derives from `last_seen_at <
  now-HEARTBEAT_LIVENESS_WINDOW` — `TargetView.status: online|offline`
  is computed at read; no stored flag, no sweeper.
- **BR5.5** The connector-facing surface is outbound-only (FR5.1):
  the gateway never initiates a connection; claim's long-poll IS the
  push channel, and `running` flips on the connector's own next touch
  (report or a claim that returns the action again is not required —
  heartbeat suffices to mark delivered actions still alive).

### BR6 — Disconnect, revoke, delete

- **BR6.1** `target_disconnect{target_id}` (owner, confirm UI-side):
  verifier tombstoned, `Target→disconnected{disconnected_at}`,
  pending grants→`cancelled`, queued actions→`cancelled`,
  claimed/running→`cancelled` directly — the channel is dead, no
  cancel can ever be delivered, so `cancel_requested` would be a
  permanent non-terminal state (audit records `action_cancelled`
  with reason `target_disconnected` for each). Audited
  `target_disconnected`. Re-pair mints a NEW Target row (one pairing
  mints ≤ 1 target — there is no row rotation; terminal rows persist
  in `target_list` as the audit-visible history the user can
  disconnect-again only insofar as `revoked` is terminal).
- **BR6.2** `target_revoke{target_id}` (owner, FR5.6): verifier STAYS
  valid (heartbeat must deliver `revoked:true`), `Target→revoked
  {revoked_at}` — terminal for the row. ALL grants on the target
  (pending AND allowed, connect AND action) →`revoked`; queued →
  `cancelled`; claimed/running →`cancel_requested` carried on the
  next heartbeat's cancel set — claim refuses `revoked` on a revoked
  target, so heartbeat alone delivers (AC4.4.2 — an in-flight
  action's covering grant is revoked too). Audited `target_revoked`.
- **BR6.3** `revoked`/`disconnected` targets refuse everything:
  dispatch → `Denied{not_connected}`; claim → `{error:revoked}` /
  disconnected → 401 `{error:unauthorized}`; `grant_respond` on a
  grant whose target died mid-consent → `grant_not_pending` with the
  grant already resolved (AC4.4.3). The ONE exception is report's
  terminal-close arm (BR2.2/BR5.2) — a revoked target may still close
  its claimed/running/cancel_requested rows because C3 pins that the
  killed action's report lands.
- **BR6.4** `grant_respond{allow:false}` on an ALLOWED
  `action_session` grant is the session-grant revoke path (carved out
  of BR3.5's pending-only rule) → `revoked` + queued under it →
  `cancelled` + in-flight (claimed/running) → `cancel_requested`. A
  pending grant with `deny` is plain `denied`; an `allowed`
  `action_once` or connect grant denies → `grant_not_pending`.
  Connect grants do not revoke this way — target revoke is the
  connect-grant withdrawal.
- **BR6.5** C8 hook #2 `ConnectorDeleteHook` (registered after U3's
  abort hook): `on_session_delete(session_id, user_id)` resolves the
  session's action grants + actions — grants pending/allowed
  →`cancelled`; actions queued/claimed/running →`cancelled`
  directly. NOT `cancel_requested`: the rows cascade-delete with the
  session (AC1.5.2), so a cancel set built from them could never be
  delivered — `cancelled` is the honest terminal, audited
  `action_cancelled`/`grant_cancelled` with reason
  `session_deleted`. The audit_events rows are the durable evidence
  and SURVIVE the cascade; the grant/action rows cascade-delete via
  `ON DELETE CASCADE` FKs (the 0087 convention). A connector holding
  a killed action discovers it at report time → `not_found` —
  C3/U7's client pins report-not_found as kill-locally, consistent
  with anti-enumeration. Idempotent — second call finds no live rows
  → `Ok`. A failure aborts the delete (C8).
- **BR6.6** Connect grants survive session delete (`session_id NULL`
  rows are untouched) — a target's activation outlives any session.
- **BR6.7** `target_disconnect`/`target_revoke` on an already-terminal
  target (`disconnected`/`revoked`) answer `{ok}` idempotently — no
  error vocabulary exists for "already dead" and the user's intent
  (it stays dead) is already satisfied.

### BR7 — Scoping, secrecy, audit

- **BR7.1** Every read/write is owner-scoped: proc paths scope by the
  session user's id; connector paths scope by the resolved
  `ConnectorIdentity`. Cross-user proc hits answer `not_found` —
  identical to an absent row, the shipped U2 anti-enumeration
  convention (a `forbidden`-vs-`not_found` split leaks existence);
  connector body-mismatch answers `forbidden` (BR2.4 — different
  domain: the identity IS the target, nothing to hide). (AC5.5.4)
- **BR7.2** Audit set (FR5.5, US5.3's U6 half):
  `pairing_created|pairing_claimed|pairing_expired|pairing_used`,
  `connect_grant_pending|connect_grant_allowed|connect_grant_denied|
  connect_grant_expired` (pending covers the lazy re-mints; the first
  mint rides `pairing_claimed`),
  `action_grant_pending|action_grant_allowed|action_grant_denied|
  action_grant_expired`, `grant_revoked|grant_cancelled` (the
  target-revoke/disconnect/delete paths),
  `action_dispatched|action_result{done|failed}|action_cancelled`,
  `target_connected|target_disconnected|target_revoked`,
  `connector_auth_failed` (rate-limited). Detail: actor user_id +
  target_id + grant_id/action_id + kind + `reason` (bounded enum —
  e.g. `session_deleted`, `target_disconnected`, `grant_revoked`;
  the field BR6.1/BR6.5 cite) + bounded args_summary/result_summary
  — NEVER codes, verifiers, raw args, unredacted output
  (FR5.4/FR5.5/AC5.3.2).
- **BR7.3** Secret material is write-only everywhere: pairing code
  once in the mint response, verifier once in the pair response;
  `verifier_hash`/`code_hash` never serialize (C1 notes, AC4.1.3).
  Logs carry hashes or ids, never tokens.
- **BR7.4** Field bounds, all redacted before write (NFR1/FR5.5):
  `action_args`/`args` ≤ 2 KiB; `action_summary`/`args_summary`
  ≤ 512 B; `result_summary` ≤ 2 KiB (the report excerpt that lands
  on the Action row — one bound, one field).

## Summary

- BR1 pairing: hashed single-use code, shown once, lazy expiry.
- BR2 auth: verifier-hash Bearer, `/pair` unauthenticated, revoked
  authenticates to learn `revoked:true`, identity-scoped bodies.
- BR3 grants: connect + action (once|session), lazy expiry, closed
  kind enum — no 'always'.
- BR4 dispatch: ordered checks, Denied mints the pending grant,
  allow materializes the queued Action.
- BR5 protocol: grant/caps re-check at atomic dequeue, 60 s clamp +
  204/`Retry-After`, idempotent bounded report closing cancelled
  rows, liveness derived.
- BR6 endings: disconnect kills the credential, revoke keeps it for
  `revoked:true`, C8 hook #2 cancels session-scoped work.
- BR7 rails: owner-scoped everything, pinned audit transitions,
  write-only secrets, bounded fields.
