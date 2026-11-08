# Functional Spec — connector-gateway (U6)

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U6.
- [contract] `inception/contract-design/contract-summary.md` — C1
  (`oxpilot.target_*`/`oxpilot.grant_*` families), C3 (connector HTTP
  protocol), C5 (`SessionService` + delete-hook registry), C7
  (action-queue dispatch), C8 (hook order: U3 abort first, U6 second).
- [req] FR5.1–FR5.6, FR6.6, NFR1, NFR4, NFR5, NFR9.
- [stories] US4.1, US4.2, US4.4 (+contributing US5.3 consent-audit
  half, US5.5 connector-route + scoping ACs).
- [domain] `inception/domain-design/components.md` §ConnectorGateway.
- [shipped] `crates/oxidean-api-oxpilot/src/routes.rs` (`router()`
  session-auth pattern), `crates/oxidean-api/src/main.rs`
  (`register_delete_hook` order), `crates/oxidean-api-core/src/
  pat/bearer.rs` (sha256 find-by-hash precedent),
  `crates/oxidean-api-core/src/pat/rate_limit.rs` (FailedAuthLimiter),
  `crates/oxidean-api-core/src/oxpilot.rs` (`SessionDeleteHook`,
  `SessionService`). No connector code ships — U6 is greenfield.

## Scope

U6 builds the entire computer-use server side: four tables
(`oxpilot_pairings`, `oxpilot_targets`, `oxpilot_grants`,
`oxpilot_actions` — all three dialects + the `dialect_oxpilot` `-E`
filter + TESTING.md row, per AC5.5.3), the `ConnectorGateway` port
trait in api-core + its api-oxpilot impl installed beside `sessions`
on `OxpilotRuntime` (set-once at boot — not the rebuildable slot, so
settings rebuilds can't drop it), the C3 connector route subtree
(verifier-auth, enablement-gated fail-closed), the C1-pinned user
procs (+ the amendment's `target_get`), the C7 seam U5 dispatches
through, and C8 hook #2. The new `target_*`/`grant_*` procs join the
pinned `OXPILOT_PROCEDURES` SessionOnly closed set (AC5.5.1's
U6-half). U7's connector client and U8's consent/status UI consume
this; U5's tools dispatch through it.

## Workflows

### WF1 — Pairing + activation (BR1, BR3.2)

1. User calls `oxpilot.pairing_create{target_name, caps}` → gate →
   owner → caps ⊆ bound (BR1.1) → `Pairing{pending, 10 min}` →
   `{pairing_id, pairing_code}` — code shown once; `pairing_id` is
   the correlation handle the dialog polls `target_list`/
   `target_get` against.
2. User runs `ox pilot pair <code>` on their machine (U7's client —
   this unit owns only the server side). Connector POSTs
   `/connector/pair{pairing_code, device_label, requested_caps}` →
   BR1.2 checks → transaction: pairing claimed + Target
   `pending_consent` + verifier minted → `{connector_id, target_id,
   verifier}` once + a `pending` connect grant minted for the
   connect dialog (BR3.2).
3. UI renders the connect-consent dialog naming the caps (AC4.2.1,
   U8). `grant_respond{connect_grant_id, allow}` → `allowed` +
   `Target→connected` + audit (AC4.2.3 server-enforced); `deny` →
   `denied` + audit, target stays `pending_consent`. Re-consent is
   never a dead end: ANY owner read (`target_list`, `target_get`,
   `grant_list`) lazily mints a fresh pending connect grant on a
   `pending_consent` target with no live one (BR3.2).

### WF2 — Dispatch with consent (BR4)

1. U5's exec/fs tool calls `dispatch_action(user_id, session_id,
   target_id, kind, args)` → BR4.1 ordered checks.
2. Live grant path → `Action{queued}` + audit → U5 polls
   `action_status` (or gets the result on a later turn — tool output
   tells the model the action is in flight; AC4.3.4's offline-target
   error is `Denied{offline}` at step (2)).
3. No grant → pending action grant minted (bounded redacted args) +
   `Denied{grant_id}` → U5's tool output = 'consent requested
   (grant_id)' → U8 renders the consent card from `grant_get`.
4. `grant_respond{grant_id, allow, scope}` — scope is the user's
   once-vs-session choice (interaction-spec's Allow once/session;
   C1 additive amendment, default `once`) → stamps `action_once`/
   `action_session` + materializes `Action{queued}` (BR4.2) + audit;
   `deny` → `denied` + audit, nothing runs (AC4.3.5 — U8 shows the
   denial; the next tool dispatch re-mints a pending grant if
   retried).

### WF3 — Connector poll/report loop (BR2, BR5)

1. Connector `POST /connector/claim{wait_seconds≤60, capabilities}`
   → BR2.1 auth → long-poll → atomic dequeue with grant+caps
   re-check (BR5.1) → `{actions[], cancel[], retry_after}` or
   `204`+`Retry-After`.
2. Executes locally (U7). `POST /connector/heartbeat{target_id}` →
   `last_seen_at` touch + `{revoked, cancel[]}` (BR5.3); cancel-set
   actions are killed locally.
3. `POST /connector/report{action_id, status, exit_code?, output}` →
   bounded + redacted + idempotent (BR5.2) → terminal row + audit.
4. Offline detection flows the other way: no heartbeat inside the
   liveness window → `TargetView.status: offline` at read (BR5.4) —
   dispatch to it answers `Denied{offline}` (AC4.3.4's visible error,
   rendered by U8).

### WF4 — Disconnect / revoke (BR6.1–BR6.3)

1. `target_disconnect` → tombstone verifier, `disconnected`, pending
   grants + queued cancelled, in-flight `cancelled` directly — the
   channel is dead, `cancel_requested` could never be delivered
   (BR6.1; audit marks each `action_cancelled`). Re-pair mints a NEW
   Target row (one pairing mints ≤ 1 target) → `pending_consent` —
   terminal rows persist as audit-visible history in `target_list`.
2. `target_revoke` → verifier kept (heartbeat answers
   `revoked:true`), `revoked` terminal, ALL grants `revoked`,
   queued `cancelled`, claimed/running `cancel_requested` on the
   next claim/heartbeat cancel set (AC4.4.2).
3. `grant_respond{allow:false}` on an allowed `action_session` grant
   → `revoked` + its in-flight `cancel_requested` (BR6.4).

### WF5 — Session delete (BR6.5, C8)

1. `SessionService.delete_session` → hook #1 (U3 abort) runs first;
   hook #2 `ConnectorDeleteHook` runs second → session's action
   grants pending/allowed →`cancelled`, ALL actions (queued AND
   claimed/running) →`cancelled` — the rows cascade-delete with the
   session so a cancel set could never reach the connector;
   `cancelled` is the honest terminal — audited with
   `session_deleted` reason → `Ok` → cascade proceeds. The
   connector's post-delete report answers `not_found`, which C3/U7
   treats as kill-locally.
2. Connect grants survive (BR6.6). Idempotent re-entry → `Ok`.
   Hook failure → delete aborts (C8).

### WF6 — Owner reads (BR7.1, Q9)

`target_list`, `target_get`, `grant_list`, `grant_get` —
owner-scoped `TargetView`/`GrantView` trims only; liveness-derived
`status` + the `lifecycle` amendment field; `pairing_id` correlation
for the connect dialog's poll loop; expired grants flip `expired`
lazily on read; `pending_consent` targets re-mint a pending connect
grant on read when none is live (BR3.2 re-consent).

## Contract deltas this unit carries

- **C1**: additive amendment (flagged per the deviation rule —
  consumer-proposed, consumer-visible): `Target` gains `lifecycle:
  pending_consent|connected|disconnected|revoked` (the `status` enum
  stays the pinned 3-value set — lifecycle states map onto it) +
  `pairing_id`; `pairing_create` returns `{pairing_id, pairing_code,
  expires_at}`; `grant_respond` gains optional `scope:
  once|session`; NEW proc `oxpilot.target_get{target_id} →
  TargetView` (the interaction-spec's connect dialog polls it).
  Otherwise the pinned `target_*`/`grant_*` vocabulary maps
  verbatim.
- **C3**: implemented verbatim + two documented deltas: the added
  error vocabulary beyond the pinned `/pair` trio (`unauthorized`,
  `revoked`, `not_found`, `disabled`, `bad_input`, limiter 429s —
  U7 must emit the full set in `openapi.yaml`), `connector_id :=
  target_id` in v1 (the sketch's two response fields carry one
  value), and the client semantic that a report answering 404
  `not_found` means the action row is gone → the connector kills it
  locally (BR6.5/WF5's session-delete close path depends on U7
  honouring this). The
  `wait_seconds>60 → clamp`, `204`+`Retry-After`, and
  report-closes-the-row rules are BR5.1/BR5.2's.
- **C7**: amendment (flagged — the pinned shape can't implement the
  pinned owner checks): `dispatch_action(user_id, session_id, …)`
  and `action_status(user_id, action_id)` — user context leads;
  `Denied{reason: not_connected|offline|capability|
  consent_required, grant_id?}` shape pinned; `ActionView` gains
  additive `result_summary`.
- **C8**: implemented verbatim — hook #2 registration after U3's
  abort hook; idempotent; failure aborts delete.
- **C5**: consumed, not changed — `get_session` (WF2 step 1 scoping)
  and the delete-hook registry (WF5).

## Procedure/route deltas

User procs (SessionOnly, gate-checked — `oxpilot.disabled` first
except `oxpilot.status`):

- `oxpilot.target_list` → `[TargetView]` (WF6 — includes
  `lifecycle` + `pairing_id` amendment fields)
- `oxpilot.target_get{target_id}` → `TargetView` (C1 additive
  amendment — the connect dialog's poll target)
- `oxpilot.pairing_create{target_name, caps}` → `{pairing_id,
  pairing_code, expires_at}` (WF1)
- `oxpilot.target_disconnect{target_id}` → `{ok}` (WF4)
- `oxpilot.target_revoke{target_id}` → `{ok}` (WF4)
- `oxpilot.grant_list{target_id?}` → `[GrantView]` — owner-scoped;
  pending-first then recent resolved (WF6)
- `oxpilot.grant_get{grant_id}` → `GrantView` — consent-card ref (C1)
- `oxpilot.grant_respond{grant_id, allow, scope?}` → `{state}`
  (WF1/WF2/WF4 — scope stamps action grant flavor, C1 amendment)

Connector routes (verifier-Bearer subtree, no session auth):

- `POST /api/oxpilot/connector/pair` — unauthenticated (BR1.2)
- `POST /api/oxpilot/connector/claim` — BR5.1
- `POST /api/oxpilot/connector/report` — BR5.2
- `POST /api/oxpilot/connector/heartbeat` — BR5.3

Seam verbs (api-core `ConnectorGateway` trait — C7 surface +
impl-side backing verbs listed in entities.md): `dispatch_action`,
`action_status`.

## Negative paths

- Pairing: `pairing_expired` (lazy-expired row), `pairing_used`
  (already claimed — no cancel state exists), `forbidden` (caps superset), `bad_input`
  (malformed code shape — no lookup).
- Auth: missing Bearer/unknown hash/disconnected target → 401
  `unauthorized` + `FailedAuthLimiter`; revoked → routes refuse
  `revoked` except heartbeat's `revoked:true` answer.
- Dispatch: `Denied{not_connected}` (pending_consent/revoked/
  disconnected), `Denied{offline}` (liveness-expired), `Denied{
  capability}` (kind ∉ caps), `Denied{grant_id}` (consent needed),
  session `not_found`/`forbidden` passthrough.
- Respond: resolved grant → `grant_not_pending`; missing OR
  non-owner → `not_found` (BR7.1's anti-enumeration — the
  resolved-vs-absent split never leaks existence);
  resolved-on-dead-target → `grant_not_pending` (BR6.3).
- Claim/report: `forbidden` on body target_id ≠ identity (BR2.4);
  report on unknown OR cross-target action_id → 404 `not_found`
  (BR5.2 anti-enumeration, not fabricated ack); report on terminal →
  `{ack:true}` idempotent; report on a `revoked` target still closes
  claimed/cancel_requested rows (BR2.2's C3-pinned exception) while
  everything else on it refuses `revoked`.
- Delete hook: failure → whole delete aborts (C8) — the session row
  stays, hooks may re-run (idempotent by BR6.5).

## Observability (BR7.2 pins)

- Audit rows per the BR7.2 transition set — `audit_events` with
  actor, target/grant/action ids, kind, bounded summaries.
- `tracing` lines (no new stack): `oxpilot_connector` target on
  pairing mint/redeem denials, auth limiter hits, dequeue batches
  (count only — never args), grant resolutions, cancel-set sizes.
- Metrics hooks where the repo already carries them (route latency,
  4xx/5xx counts) — no connector-specific metrics layer; NFR6's
  reuse-what-ships rule.

## Pins (constants)

- `PAIRING_CODE_TTL = 10 min` — BR1.1.
- `ACTION_GRANT_TTL = 5 min` — BR3.3.
- `CONNECT_GRANT_TTL = 10 min` — BR3.2's pending window.
- `CLAIM_WAIT_MAX = 60 s` — BR5.1/C3 clamp.
- `HEARTBEAT_LIVENESS_WINDOW = 90 s` — BR5.4 (≈3× the documented
  client interval U7 ships).
- `ACTION_ARGS_MAX = 2 KiB`, `SUMMARY_MAX = 512 B`,
  `RESULT_MAX = 2 KiB` — BR7.4.
- Pairing code entropy ≥ 160 bits; verifier entropy ≥ 256 bits —
  BR1.1/BR1.2.
