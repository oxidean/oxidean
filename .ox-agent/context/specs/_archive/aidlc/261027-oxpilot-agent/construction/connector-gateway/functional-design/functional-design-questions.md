# Functional Design Questions — connector-gateway

Unit: `connector-gateway` (U6, kind `service`) — owns `ConnectorGateway`:
device-grant-style pairing (single-use codes, shown once), target
registry + liveness + per-user scoping, the Grant state machine
(pending/allowed/denied/expired/revoked/cancelled; connect + action
grants; lazy expiry), the Action lifecycle
(queued→claimed→running→done/failed/cancel_requested→cancelled) with
cancel sets in claim/heartbeat responses, the C3 connector HTTP routes
(`pair`, `claim`, `report`, `heartbeat`) under verifier-class bearer
auth, the C7 dispatch seam it provides to U5, the C8 delete hook
(revoke/cancel, second after U3's abort), and the C1-pinned user procs
`oxpilot.target_*` / `oxpilot.grant_*`. depends_on: U2 (`SessionService`
+ delete-hook registry). U5 consumes C7; U7 implements the connector
client against C3; U8 renders the consent/status UI against C1.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U6.
- [contract] `inception/contract-design/contract-summary.md` — C1
  (`oxpilot.target_*`: target_list, pairing_create, target_disconnect,
  target_revoke; `oxpilot.grant_*`: grant_list, grant_respond,
  grant_get), C3 (connector HTTP protocol — pair/claim/report/heartbeat
  under verifier-class Bearer, 204+`Retry-After`, `wait_seconds` clamp),
  C5 (`SessionService` verbs — `get_session` scoping, delete-hook
  registry), C7 (action-queue dispatch — `dispatch_action`,
  `action_status`, `ActionView` public trim), C8 (hook ordering: U3
  abort first, U6 revoke second).
- [req] FR5.1 (outbound dial-out, no inbound listener), FR5.2 (explicit
  per-action/per-session consent, no cross-session "always"), FR5.3
  (caps bound: exec_shell|fs_read|fs_write), FR5.4 (AES-256-GCM at rest /
  hashed verifier; no agent-reachable secret return), FR5.5 (audited,
  secrets-redacted), FR5.6 (immediate revoke terminates pending
  grants), FR6.6 (3-dialect persistence), NFR4 (fail-closed, no silent
  stalls), NFR5 (auditability).
- [stories] US4.1 (AC4.1.1–4 pairing + target rows), US4.2 (AC4.2.1–3
  connect-grant consent, server-enforced), US4.4 (AC4.4.1–3 revoke +
  cancel propagation), US5.3 contribution (consent lifecycle audit),
  US5.5 contribution (connector-route ACs).
- [domain] `inception/domain-design/components.md` §ConnectorGateway —
  entities Target/Grant/Action/Pairing, grant shapes, lazy expiry,
  cancel propagation, per-user scoping (AC5.5.4).
- [shipped] `crates/oxidean-api-oxpilot/src/routes.rs` (`router()`
  mounts `/api/oxpilot/sessions/*` under session auth — connector
  routes need a different auth layer), `crates/oxidean-api/src/main.rs`
  (`register_delete_hook` call ordering — U3 abort first),
  `crates/oxidean-api-core/src/pat/bearer.rs` (sha256_hex +
  find-by-hash credential precedent),
  `crates/oxidean-api-core/src/pat/rate_limit.rs` (`FailedAuthLimiter`),
  `crates/oxidean-api-core/src/oxpilot.rs` (`SessionDeleteHook` trait,
  `SessionService` port). No connector code ships yet — U6 is
  greenfield.

## Q1. Scope split — what does U6 build, and where does the seam live?

- A. Greenfield: new `oxpilot_targets` / `oxpilot_pairings` /
  `oxpilot_grants` / `oxpilot_actions` tables (3 dialects) + a
  `ConnectorGateway` port trait in `oxidean-api-core::oxpilot` beside
  `SessionService`/`ToolExecutor`/`LlmProvider` (the established port
  crate for cross-unit seams — C5/C6 precedent) + its impl in
  api-oxpilot, installed on `RpcCtx.oxpilot` (`gateway` slot field) so
  U5's tools crate consumes C7 without depending on the gateway impl.
  The C1 user procs (`target_*`, `grant_*`) and the C3 connector routes
  land in api-oxpilot dispatch/routes; U6 registers its revoke hook
  second at boot.
- B. `ConnectorGateway` as a concrete api-oxpilot struct U5 imports —
  couples U5 to the gateway's module internals, inverts the port
  direction C5/C6 established (the cross-unit seam type lives in
  api-core), and makes U5's executor untestable against a stub.
- C. The C7 seam on `SessionService` — muddles the session port with
  connector vocabulary (targets, grants, claims); `SessionService` is
  already oversized.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — greenfield tables + `ConnectorGateway` port trait in
api-core beside the other oxpilot seams, impl in api-oxpilot, slot
field on `RpcCtx.oxpilot`. Conductor-decided under standing instruction
(2026-10-27); ratify at PR review.

## Q2. Pairing flow — how does a user-confirmable code become credentials?

- A. `oxpilot.pairing_create` (user proc) mints a `Pairing` row — random
  code (stored `sha256` hashed, never plaintext at rest), `state:
  pending`, `expires_at` ≈ 10 min, per-user bound — and returns the
  plaintext code ONCE (AC4.1.4: shown once, never in transcripts/audit).
  The connector POSTs `/connector/pair` `{pairing_code, device_label,
  requested_caps}` unauthenticated (the code IS the credential,
  RFC-8628-style); the gateway checks unused+unexpired+caps ⊆
  FR5.3's bound, mints a `Target` row (`status: pending_consent`) +
  random verifier token (returned once, stored sha256-hashed), marks
  the pairing `claimed`, and returns `{connector_id, target_id,
  verifier}`. Errors: `pairing_expired` | `pairing_used` | `forbidden`
  (bad caps). A liveness sweep is NOT needed — expiry is checked lazily
  at pair/claim/read time.
- B. Code stored plaintext for display-retry — violates AC4.1.4 (shown
  once) and FR5.4 (verifier-class material hashed).
- C. Long-lived reusable pairing links — a leaked link mints targets
  forever; AC4.1.2/AC4.1.4 pin single-use + short-lived.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — hashed single-use code shown once, `pending_consent`
target + hashed verifier minted on exchange, lazy expiry throughout.
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Q3. Connector auth — how do the C3 routes authenticate?

- A. A dedicated router subtree `/api/oxpilot/connector/*` mounted
  beside the session-authed routes, fronted by a connector-auth
  extractor: `Authorization: Bearer <verifier>` → `sha256_hex` →
  target lookup by `verifier_hash` (the PAT `find_by_hash` precedent —
  never session, never PAT, per C3). A revoked/absent target ⇒ 401
  typed body `{error: revoked|unauthorized}`. `/pair` is the ONE
  unauthenticated route (its credential is the pairing code). Failed
  verifier attempts feed `FailedAuthLimiter` keyed on source IP (the
  repo's credential-endpoint pattern); a revoked target's heartbeat
  still authenticates (so it can receive `revoked: true` and shut
  down) — the credential is invalidated only by re-pairing or
  `target_disconnect`.
- B. Reuse the session-auth extractor — violates C3's "never session or
  PAT" pin and would let stolen session cookies drive a connector.
- C. Reject revoked credentials outright (401) — the connector then
  can't learn it was revoked (heartbeat 401 ≠ `{revoked:true}`) and
  retries forever against an undifferentiated error.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — dedicated subtree + verifier-hash extractor, `/pair`
unauthenticated, `FailedAuthLimiter` on failures, revoked targets keep
authenticating to receive `revoked:true`. Conductor-decided under
standing instruction (2026-10-27); ratify at PR review.

## Q4. Grant model — what are the grant shapes and state machine?

- A. `Grant` rows: `{grant_id, target_id, session_id (NULL = connect
  grant), kind: connect|action, action_kind?, action_summary?,
  action_args?, state, expires_at, resolved_at}`. State machine:
  `pending → allowed|denied|expired` (lazy expiry — `expires_at`
  checked at read/dispatch/claim, no timer); terminal
  `revoked`/`cancelled` from revoke/delete paths. Connect grants come
  from the connect dialog (US4.2 — the target stays `pending_consent`
  until a connect grant is `allowed`; AC4.2.3 server-enforced). Action
  grants come from `dispatch_action`'s Denied path and are
  session-scoped, kind `action`; `action_args` holds the bounded,
  redacted dispatch args (≤ 2 KiB) so `grant_respond(allow)` can
  materialize the Action without a model round-trip. Two action-grant
  flavors per AC4.3.2: one-shot (consumed by its materialized action)
  and session (covers subsequent actions on that target in that
  session until expiry/revoke). No cross-session "always allow" — no
  `always` flavor exists at all.
- B. Grant state machine owns expiry via a background sweeper — a
  timer thread for lazy-checkable state adds a failure mode; the
  components.md pin is lazy expiry.
- C. Single grant kind with flags — collapses connect-vs-action
  semantics into field presence; the two lifecycles (activate target
  vs authorize one dispatch) differ in scope, UI, and revocation
  cascade.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — connect (session_id NULL) + action (session-scoped,
one-shot|session flavors) grants; lazy expiry; pending
action-grant rows carry bounded redacted args. Conductor-decided under
standing instruction (2026-10-27); ratify at PR review.

## Q5. `dispatch_action` semantics — the C7 grant check and Denied path?

- A. `dispatch_action(user_id, session_id, target_id, kind, args)`
  under the
  gateway's per-target admission lock: (1) session ownership via
  `SessionService.get_session` (session-scoped actions only);
  (2) target owner == session user, target `connected` (connect grant
  `allowed`, not expired) — else `Denied{reason: not_connected}` /
  `offline`; (3) kind ∈ the granted caps; (4) live action grant =
  an `action_session` grant allowed+unexpired for (target,session) —
  the ONLY passing shape; a pending grant with IDENTICAL stored args
  joins it (`Denied{consent_required, grant_id}`), distinct args mint
  a new pending grant; else INSERT the pending action grant
  (`action_kind`, bounded redacted `action_args`, `action_summary`,
  expires ≈ 5 min) and return `Denied{consent_required, grant_id}`
  (the consent card renders from `grant_get`).
  On pass: INSERT `Action{state: queued, grant_id}` + audit `dispatch`
  and return it. `grant_respond(allow, scope)` stamps the unflavored
  grant `action_once`/`action_session` and materializes the queued
  Action from the stored args (`action_once` consumed at materialize
  via `resolved_at`; `action_session` stays `allowed` until expiry);
  `deny` resolves `denied` + audit — no
  Action row is ever minted for a denied action (AC4.3.5).
- B. Create the Action row `queued` at dispatch even when the grant is
  pending — mixes unauthorized intent into the action stream; a claim
  must then re-filter on grant state at dequeue AND the queue fills
  with rows that may never run.
- C. Denied carries no grant — the UI would need a side channel to find
  the pending consent card; C1 pins `grant_get` as the card ref target.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — ordered checks (session → target connected → caps →
live grant); Denied{consent_required, grant_id} mints/joins the
pending grant carrying bounded args; `grant_respond(allow, scope)`
stamps flavor + materializes the queued Action; action_once consumed
at materialize. [Iter-3 sync: amended per review R-02/R-11 — only
action_session passes live-grant; flavor stamps at respond.]
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Q6. Claim/report/heartbeat — the long-poll protocol mechanics?

- A. `claim{target_id, wait_seconds ≤ 60 (clamped, not rejected),
  capabilities}`: atomically transitions matching `queued` Actions →
  `claimed` — re-checking each action's grant is still `allowed` AND
  unexpired AND its requested caps ⊆ the claim's capabilities (C3's
  "honoured before starting") — then returns `{actions:
  [{action_id, kind, args, grant_id}], cancel: [action_id of
  cancel_requested rows], retry_after}`. Long-poll waits up to
  `wait_seconds` for newly-queued rows, else `204` + `Retry-After`
  header (C3). `report{action_id, status: done|failed, exit_code?,
  output}` is idempotent by action_id — output bounded + secrets-
  redacted server-side too (FR5.5; client redaction is defense in
  depth, not the only barrier) → action `done|failed`, audit `result`;
  a report on a `cancel_requested` action still closes the row (C3:
  "its report still closes the row" → terminal `cancelled` honors the
  reported exit_code in audit). `heartbeat{target_id}` → `{revoked:
  bool, cancel: [action_id]}` and touches `last_seen_at`.
- B. Claim dequeues by grant-state-at-dispatch only — a grant revoked
  between dispatch and claim lets the action start; violates C3's
  "honoured before starting".
- C. Report on cancelled rows 404s — the connector's result for a
  killed action vanishes; audit loses the outcome (FR5.5 audits
  results).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — claim re-checks grant+caps at dequeue, long-poll with
60 s clamp + 204/`Retry-After`, report idempotent + bounded/redacted +
closes cancelled rows, heartbeat carries revoked+cancel. Conductor-
decided under standing instruction (2026-10-27); ratify at PR review.

## Q7. `target_disconnect` vs `target_revoke` — the two C1 procs?

- A. `target_disconnect` (recoverable detach): invalidates the verifier
  (claims/heartbeat 401 — C3's revoked-credential path), target →
  `disconnected`, pending grants → `cancelled`, in-flight actions →
  `cancel_requested` (never delivered — the channel is dead; audit
  records them cancelled-on-disconnect). Re-pair mints a fresh
  verifier; the row + history survive. `target_revoke` (consent
  withdrawal, FR5.6): verifier stays valid so heartbeat answers
  `{revoked: true}`, target → `revoked`, ALL grants (pending AND
  allowed) → `revoked` + audit, in-flight actions → `cancel_requested`
  carried on the next claim/heartbeat cancel set (AC4.4.2 — including
  a grant covering an in-flight action). Both behind user confirm
  (UI-side, AC4.4.1), both audited, both refuse new actions
  (AC4.4.3).
- B. One proc aliased twice — C1 pins both names; aliasing loses the
  disconnect-detach vs revoke-consent distinction FR5.6 draws.
- C. Revoke also invalidates the verifier — then the connector can
  never learn `revoked: true` (its heartbeat 401s); it retries
  forever instead of self-uninstalling.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — disconnect kills the credential + closes in-flight
`cancelled` directly (re-pair = new row); revoke keeps the credential
for `revoked:true` propagation while revoking every grant +
cancel-requesting in-flight. [Iter-3 sync per R-03/R-06.]
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Q8. C8 hook — what does U6's session-delete hook do?

- A. `ConnectorDeleteHook` (api-oxpilot) registered SECOND at boot
  (after U3's turn-abort hook, per C8 ordering): on
  `on_session_delete(session_id, user_id)` → resolve the session's
  action grants + the actions dispatched under them (session-scoped
  rows only — connect grants outlive sessions) → grants pending/allowed
  → `cancelled`; ALL actions (queued AND claimed/running) →
  `cancelled` directly — the rows cascade-delete with the session so
  a cancel set could never be delivered; the connector's post-delete
  report answers `not_found` which C3/U7 treats as kill-locally →
  audited transitions, then `Ok(())` letting
  the cascade proceed. Idempotent: a second call finds no live rows ⇒
  `Ok`. Failure aborts the delete (C8 pin).
- B. Delete the grant/action rows with the cascade — loses audit
  history (FR5.5 audits transitions; a deleted row can't show "this
  action was cancelled because the session went away") and C8 pins
  revoke/cancel semantics, not deletion.
- C. Hook hard-cancels the connector in-flight — the gateway has no
  inbound channel; only claim/heartbeat propagation can reach the
  connector (C3 is outbound-only).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — second hook; session-scoped grants+actions →
cancelled (all actions direct — cascade makes delivery impossible),
audited, idempotent, failure aborts delete. [Iter-3 sync per R-27.]
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Q9. User procs — the C1 `target_*`/`grant_*` surface?

- A. Exactly C1's pinned vocabulary, all SessionOnly + gate-checked
  (except `status`): `target_list` (owner-scoped `TargetView` —
  `{id, label, status: online|offline|revoked, granted_caps[]}`, never
  verifier/hash fields — C1 "secret material never in payload"),
  `pairing_create` (Q2's mint → `{pairing_id, pairing_code,
  expires_at}` once), `target_get` (the connect dialog's poll —
  additive proc), `target_disconnect`, `target_revoke` (Q7),
  `grant_list` (owner+target
  scoped pending/recent grants), `grant_get`, `grant_respond{grant_id,
  allow: bool, scope?: once|session}` (connect grant → target
  activation; action grant → Q5's flavor-stamp + materialize/deny).
  `TargetView.status` derives `online` from
  `last_seen_at < liveness window` (any connector touch), not a
  stored flag; `lifecycle`/`pairing_id` ride the flagged additive
  amendment fields.
- B. Add wait-for-dispatch or raw-action procs — U5 consumes C7's seam
  in-process; user-facing action introspection isn't pinned in C1 and
  Action internals (verifier refs, claim cursors) must stay off the
  wire anyway (C7's public-trim rule).
- C. Return grant `action_args` verbatim in `grant_get` — the consent
  card needs `action_summary` (bounded/redacted), not raw args; C1's
  payload is `args_summary`.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — exact C1 vocabulary; public-trim `TargetView`/`Grant`
payloads, liveness-derived online status, no action introspection
procs. Conductor-decided under standing instruction (2026-10-27);
ratify at PR review.

## Q10. Audit surface — which transitions emit `audit_events` rows?

- A. Consent + connector lifecycle (FR5.5, US5.3's U6 half):
  `pairing_created`, `pairing_claimed` (target minted),
  `pairing_expired`/`pairing_used` denials (security-relevant),
  `connect_grant_allowed`/`_denied`, `action_grant_allowed`/`_denied`/
  `_expired`, `action_dispatched`, `action_result{done|failed}`,
  `action_cancelled` (revoke/delete/disconnect paths), `target_revoked`,
  `target_disconnected`, connector auth failures (rate-limited).
  Detail fields: actor user_id, target_id, grant_id/action_id where
  applicable, kind, bounded args_summary/result_summary — NEVER
  codes, verifiers, raw args, or unredacted output (FR5.4/FR5.5).
- B. Audit only consent answers — loses the "every connector action is
  audited (who/what/when/result)" FR5.5 pin (claim/report/revoke are
  connector actions too).
- C. Audit raw args/output for forensics — direct FR5.5 violation:
  audit detail is secrets-redacted BEFORE write.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — pinned transition set covering consent, dispatch,
results, revocation, and auth failures; bounded redacted detail only.
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Assumptions & Open Questions

- [assumption] `action_args` on pending action grants is bounded
  (≤ 2 KiB) and redacted at insert — the consent card needs enough
  detail to be informed (AC4.2.1) but args like exec commands can
  carry secrets-in-error-paths; U5's dispatch site already redacts
  tool output by the same rule.
- [assumption] Pairing code TTL ≈ 10 min, action-grant expiry ≈ 5 min,
  connect-grant expiry ≈ session-length — concrete values pinned in
  rules; tunable in nfr-design without redesign.
- [assumption] `TargetView`/`ActionView`/`Grant` wire shapes are public
  trims — verifier_hash, claim cursors, raw args never serialized
  (C1/C7 pins).
- [Q1] Whether `claim` delivers `args` verbatim or a re-derived payload
  — pinned here as verbatim stored args (the connector needs the full
  command to run; redaction happened at insert); nfr-design may
  revisit per-target field sizes.
- [Q2] Whether `grant_respond` on a session-flavor action grant can be
  revoked mid-session — answered yes via `target_revoke` and a
  grant-level `revoke` (deny on an allowed session grant ⇒
  `revoked`, audited); per-action re-consent is the escape hatch.

## Consolidated Summary Confirmation

The shape I intend to write into the functional-design artifacts for
`connector-gateway`:

- Scope: greenfield `ConnectorGateway` — 4 tables, port trait in
  api-core, impl in api-oxpilot on `RpcCtx.oxpilot.gateway`; C1 user
  procs + C3 connector routes + C7 seam + C8 hook #2 (Q1).
- Pairing: hashed single-use code shown once → `pending_consent`
  target + hashed verifier on exchange; lazy expiry (Q2).
- Auth: verifier-Bearer subtree (sha256 lookup, `FailedAuthLimiter`),
  `/pair` unauthenticated, revoked targets authenticate to learn
  `revoked:true` (Q3).
- Grants: connect (session_id NULL) + action (session-scoped,
  one-shot|session) kinds; lazy expiry; pending grants carry bounded
  redacted args (Q4).
- Dispatch: session → connected → caps → live-grant checks; Denied
  mints the pending grant + consent card ref; allow materializes the
  queued Action (Q5).
- Protocol: claim re-checks grant/caps at dequeue + long-poll 60 s
  clamp + 204/`Retry-After`; report idempotent, bounded, closes
  cancelled rows; heartbeat revoked+cancel (Q6).
- Disconnect vs revoke: credential-kill vs consent-withdrawal with
  `revoked:true` propagation (Q7).
- C8 hook #2: session-scoped grants+actions cancelled, audited,
  idempotent, failure aborts delete (Q8).
- Procs: exact C1 vocabulary, public-trim payloads only (Q9).
- Audit: consent + lifecycle transition set, redacted detail (Q10).

Does this all look correct before I write the functional-design
artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
