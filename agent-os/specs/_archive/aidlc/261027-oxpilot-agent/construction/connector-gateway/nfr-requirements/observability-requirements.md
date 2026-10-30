# Observability Requirements — connector-gateway (U6)

Consumes: `construction/connector-gateway/functional-design/rules.md`
(BR7.2's pinned vocabulary), `functional-design/entities.md`,
`functional-design/functional-spec.md` (Observability section),
`inception/requirements-analysis/requirements.md` (NFR2, NFR5, NFR6),
`inception/contract-design/contract-summary.md` (C3, C8).

## Audit vocabulary (pinned by BR7.2 — verbatim)

| id | event(s) | emitted when | detail fields (bounded, redacted) |
|----|----------|--------------|-----------------------------------|
| NFR5.5 | `pairing_created` | `pairing_create` mints the row | user_id, pairing_id, expires_at |
| NFR5.6 | `pairing_claimed` | `/pair` accepts the code (first-mint connect grant rides this event per BR7.2) | target_id, connector_id, user_id |
| NFR5.7 | `pairing_expired` | lazy-eval stamps expiry at read/redeem | pairing_id, user_id, reason=expired |
| NFR5.8 | `pairing_used` | redeem hits an already-`claimed` code | pairing_id, user_id |
| NFR5.9 | `connect_grant_pending` | a connect grant is (re-)minted on owner read — every BR3.2 re-mint after pairing; the FIRST mint rides `pairing_claimed` (NFR5.6), not this event | user_id, target_id, grant_id |
| NFR5.10 | `connect_grant_allowed` / `connect_grant_denied` / `connect_grant_expired` | respond / lazy-expire transitions | user_id, grant_id, scope where applicable |
| NFR5.11 | `action_grant_pending` / `action_grant_allowed` / `action_grant_denied` / `action_grant_expired` | dispatch-mint / respond / lazy-expire | user_id, grant_id, kind, scope, reason |
| NFR5.12 | `grant_revoked` / `grant_cancelled` | target-revoke/disconnect/delete paths + BR6.4's session-grant revoke + BR6.5's session-delete | user_id, grant_id, reason ∈ {target_revoked, target_disconnected, session_deleted, grant_revoked} |
| NFR5.13 | `action_dispatched` | queued INSERT lands | user_id, target_id, action_id, kind |
| NFR5.14 | `action_result{done|failed}` | terminal report accepted | action_id, result, `result_summary` ≤ RESULT_MAX redacted — never raw output |
| NFR5.15 | `action_cancelled` | any cancel path lands | action_id, reason ∈ {session_deleted, target_disconnected, target_revoked, grant_revoked} |
| NFR5.16 | `target_connected` / `target_disconnected` / `target_revoked` | lifecycle transitions | user_id, target_id, reason where applicable |
| NFR5.17 | `connector_auth_failed` | verifier-auth failure (rate-limited via BR2.1's FailedAuthLimiter) | ip-scoped, no credential material |

Detail contract (BR7.2): actor user_id + target_id + grant_id/action_id
+ kind + `reason` (bounded enum) + bounded `args_summary`/`result_summary`
— NEVER codes, verifiers, raw args, unredacted output (FR5.4/FR5.5/
AC5.3.2). Emission rides the shipped fire-and-forget `audit::record`
helper — the vocabulary is pinned, the delivery strength matches the
existing surface (NFR5.2, security-requirements.md).

## Tracing & logs

| id | requirement | basis |
|----|-------------|-------|
| NFR6.1 | Lifecycle logs ride the existing `tracing` surface on the `oxpilot_connector` target (spec Observability): pairing mint/redeem denials, auth limiter hits, dequeue batches (COUNT only — never args), grant resolutions, cancel-set sizes — no new observability stack (NFR6) | spec Observability, NFR6 |
| NFR6.2 | Log lines carry hashes or ids, never tokens (BR7.3): a `tracing` field carrying `verifier`, `pairing_code`, or raw args is a defect | BR7.3, NFR1 |
| NFR6.3 | Metrics hooks only where the repo already carries them (route latency, 4xx/5xx counts) — no connector-specific metrics layer (spec Observability, NFR6) | spec, NFR6 |

## Operator disclosure

| id | requirement | basis |
|----|-------------|-------|
| NFR2.1 | Connector egress (outbound-only dialing, what data leaves the host) is disclosed in `docs/CONFIGURATION.md`; the connector-client README is U7's artifact — U6's obligation is that the gateway itself emits no telemetry and the disclosed surface matches what C3 actually carries | NFR2, US4.6 |
