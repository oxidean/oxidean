# Monitoring Design — u1-skeleton-slice

Consumes: `nfr-design/observability-design.md`,
`nfr-requirements/observability-requirements.md` (NFR5, NFR6).
Questions: Q3 — no new monitoring infrastructure (conductor-decided).

U1 adds **no monitoring infrastructure**: existing `tracing` stdout on
the api service is carried by Railway platform logs; there are no new
metrics, exporters, dashboards, or alert rules (NFR6.1). Tables below
record the signals that exist and their operator meaning — thresholds
are observational guidance, not configured alerts.

## Metrics & KPIs (tracing signals; no registry)

| Metric | Source | Threshold | Why it matters |
|--------|--------|-----------|----------------|
| Gate-denial count by reason | `oxpilot`-targeted tracing events | >0 sustained = feature misconfigured vs. flag-off | distinguishes `flag_off`/`misconfigured`/`no_provider` without status access (NFR6.2) |
| Turn lifecycle events | turn span (claim → terminal) with `session_id`/`turn_id`/`turn_ms`/`frames` | any turn > minutes-class duration on the stub = wedge signal | stub is instant; a long open span means a stuck task (NFR6.2) |
| `turn_active` (409) events | claim-denial event | any sustained rate = client double-send bug | races should be rare (NFR6.2) |
| Orphan-sweep count | startup event | >0 after a crash deploy = expected; >0 on every boot = unhealthy | proves the sweep ran and bounds phantom-live exposure (NFR6.2) |
| Lagged-resync count | SSE reader span events | occasional = slow client; sustained = capacity symptom | 256-frame broadcast buffer pressure signal (NFR6.2) |
| Session-create events | audit row + create event | baseline volume only | adoption signal (NFR6.2) |

## Alerts

| Alert | Condition | Severity | Routes to |
|-------|-----------|----------|-----------|
| none configured | — | — | — |

No alert rules are designed: there is no alert pipeline in the repo and
NFR6 forbids new observability infrastructure. Recorded as operational
debt alongside the missing performance harness.

## SLIs / SLOs

| SLI | SLO target | Measurement window |
|-----|-----------|--------------------|
| Stub round-trip integrity (POST → frames → terminal) | 100% success in stack test | per CI run (N=1) |
| First-frame/snapshot latency | < 250 ms single-shot ceiling | per CI run, warm-up excluded |
| `session_list`/`status`/`session_create` | < 150 ms single-shot ceiling | per CI run |

These are smoke-level pass/fail ceilings asserted by a
`*.stack.test.ts`, not monitored SLOs (measurement protocol in
`performance-requirements.md`).

## Logs & Tracing

`tracing` spans carry `session_id`/`turn_id`/`message_id`/`repo_id` as
span fields for correlation; log lines are metadata-only (NFR6.3 — no
content, no credential-shaped values). Audit rows (`oxpilot_session.
create`, optional denial audits) land in the existing `audit_events`
table fire-and-forget (NFR5). No distributed-trace propagation: SSE
fetches carry cookies only and the surface is single-process.
Dashboards: none — `oxpilot.status` + DB/audit queries are the operator
surface (NFR6.5).
