# Security Requirements — connector-gateway (U6)

Consumes: `construction/connector-gateway/functional-design/rules.md`
(BR1–BR7), `functional-design/entities.md`,
`functional-design/functional-spec.md`,
`inception/requirements-analysis/requirements.md` (NFR1, NFR2, NFR5),
`inception/contract-design/contract-summary.md` (C1 +amendments, C3, C5,
C7, C8), `codekb/oxidean-app/technology-stack.md`.

The gateway holds the only secret-capable material in the connector
plane (pairing codes at mint, verifier material at redeem) and the only
path by which an agent-dispatched intent reaches a user machine — every
pin below is a server-side enforcement point, never UI policy.

## Authentication & authorization boundaries

| id | requirement | basis |
|----|-------------|-------|
| NFR1.1 | Every `oxpilot.target_*`/`oxpilot.grant_*`/`oxpilot.target_get` proc is SessionOnly + `oxpilot.disabled`-gated; a PAT/bearer refusal answers `auth.pat_scope`/`auth.unauthenticated`; a foreign `target_id`/`grant_id` answers `not_found` — never `forbidden` (BR7.1's anti-enumeration: a forbidden-vs-not_found split leaks existence) | NFR1, C1, BR7.1 |
| NFR1.2 | C3's `/pair` route is the SOLE unauthenticated connector route — the pairing code IS the credential (BR1.2). `/claim`, `/report`, `/heartbeat` accept ONLY verifier-class Bearer credentials — never session cookies, never PATs (BR2.3). The check is the shipped find-by-hash model (`sha256_hex` lookup, bearer.rs precedent); failed auth is keyed on IP through `FailedAuthLimiter` (BR2.1) and audited `connector_auth_failed` (BR7.2) | BR1.2, BR2.1, BR2.3, BR7.2, NFR1 |
| NFR1.3 | `ConnectorIdentity` is the sole scoping source: a body `target_id` that disagrees with the credential's binding answers `forbidden` (BR2.4 — the identity IS the target, nothing to hide); a missing/unknown/tombstoned credential answers 401 `unauthorized` (BR2.1, BR6.3) — `unauthenticated` is the RPC-side code, not C3's | BR2.1, BR2.4, BR6.3 |
| NFR1.4 | Dispatch authorization is server-side ordered inside the per-target admission lock (BR4.1): (1) session ownership via `SessionService.get_session`; (2) target exists + owner + `status=connected` (lifecycle) + liveness-fresh → `Denied{not_connected}` (pending_consent/revoked/disconnected) or `Denied{offline}` (stale `last_seen_at`); (3) `kind ∈ target.caps` → `Denied{capability}`; (4) live `action_session` grant allowed+unexpired — else pending-grant dedupe/mint → `Denied{consent_required, grant_id}` (BR4.1/BR4.5) | BR4.1, BR4.5, C7 |
| NFR1.5 | `grant_respond{grant_id, allow, scope?}` mutates a `pending` grant — PLUS the BR6.4 carve-out: `allow:false` on an ALLOWED `action_session` grant revokes it (the session-consent withdrawal path). Every other state/kind answers `grant_not_pending`; resolved-on-dead-target also `grant_not_pending` (BR6.3) | BR3.5, BR6.3, BR6.4 |
| NFR1.6 | The claim dequeue itself re-enforces consent: inside the per-target lock, each candidate `queued` row re-checks grant `allowed`+unexpired AND `action.kind ⊆ claim.capabilities` AND `⊆ target.caps` before `queued→claimed`; a row whose grant died flips `cancelled`+audit and is skipped — `queued` never outlives authorization (BR5.1, BR3.1's paired rule) | BR5.1, BR3.1 |

## Secret custody

| id | requirement | basis |
|----|-------------|-------|
| NFR1.7 | `pairing_code` exists in plaintext exactly once — in the `pairing_create` response `{pairing_id, pairing_code, expires_at}`; stored only as hash on the pairing row; never in audit detail, transcripts, logs, or errors (BR7.3 — write-only, `code_hash` never serializes) | BR1.3, BR7.3, FR5.4 |
| NFR1.8 | `verifier` exists in plaintext exactly once — in the `/pair` response `{connector_id, target_id, verifier}`; stored only as `SHA-256(verifier)`; re-pair mints a NEW Target row — never rotates a verifier on an existing row (BR6.1) | BR1.3, BR6.1, BR7.3 |
| NFR1.9 | Pairing redeem is atomic single-use in one transaction: `pending→claimed` (the enum is `pending|claimed|expired` — no `redeemed`/`redeemed_at`); expiry is evaluated BEFORE state — an expired-then-retried code always answers `pairing_expired`, never degrades to `pairing_used`; caps-superset answers `forbidden`; malformed shape answers `bad_input` (BR1.2, BR1.4) | BR1.2, BR1.4, C3 |
| NFR1.10 | Entropy floors are pinned: pairing code ≥ 160 bits, verifier ≥ 256 bits (spec Pins) — unguessability is a stated bound, not an assumption | spec Pins, BR1.1, BR1.2 |
| NFR1.11 | No RPC/HTTP/store surface returns `verifier_hash`, `code_hash`, `verifier`, or `pairing_code` — `TargetView` carries `{id, label, status, lifecycle, pairing_id, granted_caps[]}` only; a store-level pin test asserts the secret columns never join a projection (entities TargetView) | BR7.3, entities, NFR9 |
| NFR1.12 | `pairing_create` is rate-limited PER USER (session-auth'd proc — BR1.5); `/pair` redeem carries NO limit — code entropy is the barrier (BR1.5's explicit carve-out); connector auth failures are limited PER IP via `FailedAuthLimiter` (BR2.1) | BR1.5, BR2.1 |

## Consent & isolation enforcement

| id | requirement | basis |
|----|-------------|-------|
| NFR1.13 | An `action_once` grant authorizes exactly one materialization: `resolved_at` + flavor stamp + `Action{queued}` insert happen in one transaction (BR4.2); `allowed` state remains for audit but covers nothing further — BR4.1(4) treats it as consumed | BR4.1, BR4.2 |
| NFR1.14 | An `action_session` grant authorizes dispatches only for its stamped (target_id, session_id) and only while `allowed`+unexpired — the live test at dispatch/claim (BR4.1(4)); lazy `expired` stamping fires on the `pending` arm per BR3.1's closed transition set (live states exit only to `revoked`/`cancelled`); BR6.4's deny-on-allowed arm is the only respond-side (session-consent) withdrawal path — `target_revoke` revokes ALL grants (BR6.2) and session-delete cancels them (BR6.5); a session-scoped grant can never cover a sibling session (BR3.3) | BR3.3, BR6.4 |
| NFR1.15 | Pending action grants store `action_kind` + `action_args` (≤ `ACTION_ARGS_MAX = 2 KiB`, redacted) + `action_summary` (≤ `SUMMARY_MAX = 512 B`); the consent card renders `args_summary` — NEVER the stored args (entities GrantView; the stored args exist to materialize, not to display) | BR3.3, BR7.4, entities GrantView |
| NFR1.16 | Approval always materializes atomically: `grant_respond(allow)` stamps flavor AND inserts `Action{queued}` from the stored args in one transaction (BR4.2) — there is no "approved but never minted" state; a dead grant's queued rows cancel at dequeue observation (BR5.1) | BR4.2, BR5.1 |
| NFR1.17 | Every read/mutation is owner-scoped: proc paths scope by session user_id, connector paths by `ConnectorIdentity`; `action_status(user_id, action_id)` and all grant/target reads answer `not_found` on foreign ids — no existence leak either direction (BR7.1, BR4.4) | BR7.1, BR4.4 |

## Auditability & data minimization

| id | requirement | basis |
|----|-------------|-------|
| NFR5.1 | Audit fires on the BR7.2 transition set — `pairing_created/claimed/expired/used`, `connect_grant_*`/`action_grant_*` lifecycle, `grant_revoked`/`grant_cancelled`, `action_dispatched`/`action_result{done|failed}`/`action_cancelled`, `target_connected/disconnected/revoked`, `connector_auth_failed` — emitted at each transition via `audit::record`; detail is bounded JSON: actor user_id, target_id, grant/action ids, kind, `reason` (bounded enum), `args_summary`/`result_summary` ≤ `SUMMARY_MAX`/`RESULT_MAX` — NEVER codes, verifiers, raw args, unredacted output (BR7.2, BR7.4) | BR7.2, BR7.4, NFR5 |
| NFR5.2 | Emission strength is honest: the shipped `audit::record` is fire-and-forget (swallows write failures) — these rows pin the vocabulary, not a stronger delivery guarantee than the existing helper gives; the in-state transitions themselves are transactional and remain the authoritative record (BR7.2, audit.rs precedent) | BR7.2 |
| NFR5.3 | `audit_events` rows carry NO foreign key to session/target/grant/action tables — they survive session-delete cascades (AC1.5.2) and target revocation as the sole durable evidence (entities observations §session_id FK posture, BR6.5) | entities obs, BR6.5, NFR5 |
| NFR5.4 | Report output is bounded ≤ `RESULT_MAX = 2 KiB` tail AND secrets-redacted SERVER-side before storage/audit — the connector's own redaction is defense in depth, not the barrier (BR5.2, FR5.5) | BR5.2, BR7.4 |

## Threat considerations

| threat | mitigation |
|--------|------------|
| Stolen verifier | Hash-only storage; `target_revoke` keeps the credential authenticating (heartbeat returns `{revoked:true}` + cancel set); `target_disconnect` tombstones it; re-pair mints a fresh target row |
| Pairing-code replay/guessing | ≥160-bit entropy; atomic `pending→claimed` single-use; lazy expiry-first; per-USER limit on `pairing_create` (BR1.5); explicit NO limit on `/pair` — `pairing_expired`/`pairing_used`/`forbidden`/`bad_input` |
| Cross-user queue snoop | `ConnectorIdentity`-bound claim resolution; `not_found` on all foreign proc ids; `forbidden` on body/identity mismatch; report cross-target → 404 `not_found` |
| Capability escalation at redeem | `requested_caps ⊄ pairing.requested_caps` → `forbidden` (BR1.2) — the mint-time caps are the ceiling |
| Consent bypass via dequeue | BR5.1's grant+caps re-check inside the per-target lock — a queued row with a dead grant cancels instead of running |
| Post-delete zombie exec | BR6.5 transitions → `cancelled` before cascade; late report → `not_found`; C3 pins kill-local on 404 |
| Consent evidence forgery | Card renders server-stored `args_summary` (bounded, redacted) — the user approves what the server recorded, not a client-rendered payload |
