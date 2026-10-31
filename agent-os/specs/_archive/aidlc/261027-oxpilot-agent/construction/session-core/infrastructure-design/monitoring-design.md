# Monitoring Design — session-core (U2)

Consumes: `nfr-design/observability-design.md`,
`nfr-requirements/observability-requirements.md` (NFR5.x, NFR6.x).
Questions: Q5 — no new monitoring infrastructure (conductor-decided).

U2 adds **no monitoring infrastructure**: `tracing` stdout on the api
service rides Railway platform logs; the audit trail is the existing
`audit_events` table written by `audit::record`. No metrics, exporters,
dashboards, or alert rules. The operator-facing surfaces are the audit
rows and the typed-error vocabulary.

## Metrics & KPIs (tracing/audit signals; no registry)

| Metric | Source | Threshold | Why it matters |
|--------|--------|-----------|----------------|
| `oxpilot.session_delete` audit rows | proc-layer `audit::record` on service Ok AND on the hook-failure/`not_found` Err returns — hook errors ride `AppError.data` up so the proc can write `detail={"result":"ok"\|"hook_failed"\|"not_found","hook_index"?}` (observability-design pins the three-value vocabulary) | `hook_failed` = a hook blocked a delete; `hook_index` identifies which one | audit is the deletion surface (NFR5.1/NFR5.3) |
| Drain completions | one `debug!` line per `drain_queue_tx`: `{session_id, drained_count}` | `drained_count > 0` at turn START = residual rows recovered post-crash | the pinned drain signal (NFR6.3) — crash-recovery visibility without new infra |
| Session-svc internal errors | `error!` on `oxpilot.internal` surfacing from a store error | any occurrence = a store call failed | `oxpilot.internal` is the session.rs:23 convention; store-failure logs use `tracing::error!` |
| Uninstalled service | `oxpilot.internal` "session service not installed" from `sessions()` | any occurrence = boot invariant violated | shell forgot `install_sessions` — loud, never silently unscoped (NFR1.x) |
| Budget refusals | `budget_exceeded` typed error count (proc error path) | sustained = real usage pressure or mis-sized caps | the refusal is loud by design (NFR4.8) |
| Turn lifecycle | existing `oxpilot` turn span (session_id/turn_id/message_id attrs — no user_id) | — | U1's span; U2 adds nothing to it (NFR6.1 keeps user_id off spans) |

## Alerts

| Alert | Condition | Severity | Routes to |
|-------|-----------|----------|-----------|
| none configured | — | — | — |

No alert rules are designed: there is no alert pipeline in the repo and
U2's observability requirements are satisfied by audit rows + warn logs
(NFR5.x/NFR6.x forbid new observability infrastructure).

## SLIs / SLOs

| SLI | SLO target | Measurement window |
|-----|------------|--------------------|
| `enqueue` | `require_owned_session` + dedupe pair + one insert + position count — a handful of indexed reads/writes | per call |
| `drain_queue` | one tx: DELETE…RETURNING (pg/sqlite) or FOR UPDATE+DELETE (mysql) + in-tx message inserts | per turn boundary |
| `transcript` | one `list_messages` scan (session_id index) + in-service status filter | per call |
| `check_budget` / `usage_view` | one `read_counters` pair read | per call |

## Logs & Tracing

Store failures map to `AppError{code:"oxpilot.internal"}` and log once
via the existing proc error path; `session_delete` audit rows carry
the full `detail` vocabulary — `{"result":"ok"}` /
`{"result":"hook_failed","hook_index":i}` / `{"result":"not_found"}` —
on the JSON detail column, since `insert_audit_event` has no result
column. Redacted payloads never appear in logs or audit detail
(redaction happens at service top, before the store call —
NFR1.5/1.6).
