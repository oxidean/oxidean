# Tracing Config — oxpilot

## Sources

Consumes per-unit `nfr-design/performance-design.md` (**performance-design**), `nfr-design/security-design.md` (**security-design**), `nfr-design/reliability-design.md` (**reliability-design**), `infrastructure-design/monitoring-design.md` (**monitoring-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
No new tracing backend. The API's existing request-tracing/logging middleware already covers the `/api/oxpilot.*` procs and the SSE route (same Axum layers as every route).

## What is traceable today

- Per-proc request id + duration + status code (existing middleware fields).
- Turn correlation via session id + turn number fields in oxpilot log lines.
- Connector calls correlate via target id + grant id (ids only, never payload).

## Deferred

Distributed tracing (OTel/span export) across web→api→provider is a platform-level decision outside this intent's scope — no partial implementation was added.
