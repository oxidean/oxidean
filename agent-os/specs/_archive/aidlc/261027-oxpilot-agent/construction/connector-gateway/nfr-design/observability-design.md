# Observability Design — connector-gateway (U6)

Consumes: `nfr-requirements/observability-requirements.md`
(NFR2.1, NFR5.1–5.17, NFR6.x), `functional-design/rules.md` (BR7.2,
BR7.3), `nfr-design-questions.md` (Q7).

## Audit emission design

Emission point: post-commit, fire-and-forget audit (delivery strength
matches the existing surface; the state transitions themselves are
the authoritative record). Two shipped helpers by call site —
`audit::record(ctx, …)` on proc paths that hold an `RpcCtx`;
`audit::record_with(db, client, …)` on C3 routes (which hold
`AppState`, never `RpcCtx`) and inside `ConnectorDeleteHook` (whose
`on_session_delete(session_id, user_id)` signature carries no ctx).
Roughly half the pinned events originate off-proc — the split is
mandatory, not stylistic.

Redaction boundary (NFR5.4): `result_summary =
oxidean_core::redact(output).tail(RESULT_MAX)` is computed
SERVER-SIDE at the `/report` ingest boundary before any store or
audit write (same helper as `action_args`/`action_summary` at
dispatch mint) — the connector's own redaction is defense in depth,
never the barrier.

| transition site | event(s) | detail payload (bounded) |
|-----------------|----------|--------------------------|
| `pairing_create` | `pairing_created` | user_id, pairing_id, expires_at |
| `/pair` redeem | `pairing_claimed` (first connect-grant mint rides this) | target_id, connector_id, user_id |
| lazy expiry touch | `pairing_expired` | pairing_id, user_id, reason=expired |
| redeem on claimed | `pairing_used` | pairing_id, user_id |
| redeem refusal (expired/used/caps-superset `forbidden`) | `connector_auth_failed` — the credential-endpoint refusal event carries ALL /pair refusals (BR7.2's pairing set has no forbidden vocabulary; reason field distinguishes expired/used/caps) | ip-scoped, reason |
| BR3.2 re-mint | `connect_grant_pending` | user_id, target_id, grant_id |
| dispatch pending-grant mint | `action_grant_pending` | user_id, grant_id, kind (NFR5.11 — minted by dispatch's Denied path, also on lazy-expiry replacement) |
| `grant_respond` | `connect_grant_allowed|denied` or `action_grant_allowed|denied` | user_id, grant_id, scope, kind |
| lazy grant expiry | `connect_grant_expired`/`action_grant_expired` | user_id, grant_id, reason=expired |
| revoke paths | `grant_revoked` | user_id, grant_id, reason ∈ {target_revoked, grant_revoked (BR6.4)} |
| delete/disconnect paths | `grant_cancelled` | user_id, grant_id, reason ∈ {session_deleted, target_disconnected} |
| dispatch | `action_dispatched` | user_id, target_id, action_id, kind |
| terminal report | `action_result{done|failed}` | user_id, action_id, result, `result_summary` ≤ RESULT_MAX (server-redacted) |
| any cancel | `action_cancelled` | user_id, action_id, reason ∈ {session_deleted, target_disconnected, target_revoked, grant_revoked} |
| grant dead at dequeue | `action_cancelled` | user_id, action_id, grant_id, reason=`grant_expired` — bounded-enum extension over NFR5.15's four, flagged with basis in performance-design (BR5.1); NO `action_grant_expired` — the grant stays `allowed` (lazy expiry stamps `pending` only; the failed unexpired live-test is the trigger) |
| target lifecycle | `target_connected`/`target_disconnected`/`target_revoked` | user_id, target_id, reason |
| verifier-auth fail | `connector_auth_failed` | ip-scoped — NO credential material |

Detail never-fields (BR7.3/BR7.4): verifier, verifier_hash,
pairing_code, code_hash, raw `action_args` (only bounded redacted
`args_summary`/`result_summary` ≤ SUMMARY_MAX/RESULT_MAX), env values,
transcript content.

## Tracing design

`oxpilot_connector` target, ids+counts+reason only:

| level | when |
|-------|------|
| `info` | `pairing_claimed`, `target_connected`, grant resolutions (`connect_grant_allowed|denied`, `action_grant_allowed|denied`), `action_dispatched`, `action_result`, `target_disconnected`, `target_revoked` |
| `warn` | `connector_auth_failed` (limiter), `pairing_expired`/`pairing_used` refusals, dequeue dead-grant cancels, stranded claimed/running observed via `action_status`, session-delete hook counts, 204 cycles beyond expected poll cadence |
| `debug` | claim subscribe/wake/timeout cycle, dequeue batch sizes (count only), cancel-set sizes |

## Metric posture

Reuse existing route metrics where the repo carries them (latency,
4xx/5xx counts); no connector-specific counters — NFR6's
reuse-what-ships rule.

## Operator disclosure

`docs/CONFIGURATION.md` gains the connector-egress paragraph
(outbound-only dial, what C3 carries on each route) — U6's slice;
the connector-client README is U7's artifact.
