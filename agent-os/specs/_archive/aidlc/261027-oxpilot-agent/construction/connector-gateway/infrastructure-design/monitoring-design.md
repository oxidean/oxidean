# Monitoring Design — connector-gateway (U6)

Consumes: `nfr-design/observability-design.md`, `nfr-requirements/
observability-requirements.md` (NFR5.5–5.17), `nfr-requirements/
security-requirements.md` (NFR1.x custody), `functional-design/
rules.md` (audit vocabulary).

NFR6 forbids a new observability stack — monitoring rides the
existing `tracing` pipeline, the existing audit-events table, and the
existing `FailedAuthLimiter` trip surface. This file names the
operator-visible signals U6 adds.

## Operator signals

| signal | emission | reads |
|--------|----------|-------|
| connector auth failure | `warn!` `{ip_class, reason}` (bounded; NO verifier, NO hash) | brute-force pressure on the verifier lookup |
| limiter trip | `warn!` `{ip_class}` + 429 response | sustained auth hammering — limiter doing its job |
| pairing mint | `debug!` `{user_id}` (+ `pairing_created` audit) | code-generation rate |
| claim park/wake | `debug!` `{target_id, parked, woke}` | long-poll health; parked>>woke shows dead connectors |
| dequeue | `debug!` `{target_id, count}` | dispatch throughput; count=50 sustained shows backlog |
| grant resolution | `info!` `{grant_id, kind, state}` (+ `connect_grant_*`/`action_grant_*` audit per observability-design's `info` pin) | consent latency/denial pressure |
| action close | `info!` for `done`/`failed` (`action_result` pin), `debug!` for `cancelled` `{action_id, state, reason?}` (+ `action_*` audit) | cancel/done/failed flow |
| target lifecycle | `info!` `{target_id, status}` (+ `target_connected/disconnected/revoked` audit) | connector fleet state |

Every line carries bounded ids only — NFR5.1/6.2 forbid verifier,
verifier hash, pairing code/hash, raw action args, environment
values, transcript content, or unredacted output in any operator
surface. Persisted rows and audit entries are pre-redacted by
`oxidean_core::redact` + `SUMMARY_MAX` bounds.

## Audit-events table coverage

The NFR5.5–5.17 event vocabulary is emitted via
`audit::record(ctx,…)` on proc paths and `audit::record_with(db,
client,…)` on C3 routes and ctx-less hooks — fire-and-forget; an
audit write failure must never fail the operation (NFR5.2). The
table is the durable query surface for: pairing lifecycle, auth
failures, grant lifecycle per kind, action lifecycle with bounded
reasons, and target lifecycle.

## What's absent by design

- No metrics registry — same posture as U3: log observability only;
  counters are an ops-stage concern.
- No sweeper/cron — lazy expiry means stale rows surface as
  `expired` outcomes on next touch; there is nothing to poll.
- No cross-node delivery state — single-replica boundary (NFR-S6):
  replicas >1 breaks the Notify map; operators see this as
  `claim` timeouts + stranded `claimed` actions, which is the
  documented residual.

## Failure-shape reading

- Rising auth-failure warns + limiter trips ⇒ verifier probing —
  the limiter and 429 surface are the pinned response.
- `claim` parked with no dequeue across heartbeats ⇒ dispatch stall
  (Notify registration bug) — pin: the armed future MUST precede the
  first dequeue (nfr-design performance, R-29).
- `cancel_requested` actions that never report ⇒ dead connector —
  the close paths (disconnect/revoke/cascade) own termination, not
  a timeout.
- `denied` grant resolutions clustering on one user ⇒ consent-flow
  confusion or scope misunderstanding — the bounded `reason` field
  distinguishes `user_denied` vs `expired`.
