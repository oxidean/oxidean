# Dashboards — oxpilot

## Sources

Consumes per-unit `nfr-design/performance-design.md` (**performance-design**), `nfr-design/security-design.md` (**security-design**), `nfr-design/reliability-design.md` (**reliability-design**), `infrastructure-design/monitoring-design.md` (**monitoring-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
No dedicated dashboard service exists in this deployment; observability rides Railway service logs + existing API metrics (NFR6 — reuse). Recommended operator views (log-derived):

## API service log view (Railway)

- Filter `oxpilot` in api logs: session create/list, chat_post accept/refuse codes, turn terminal statuses.
- Key fields logged (content-free): session id, owner, proc name, duration ms, refusal code, tool-call count per turn.

## Health/availability view

- `api`/`web` `/health` probe status (existing Railway healthchecks) — already the deploy gate signal.

## Provider view

- Provider call latency + error codes only (redacted; never request/response bodies) — used to separate provider faults from runtime faults.
