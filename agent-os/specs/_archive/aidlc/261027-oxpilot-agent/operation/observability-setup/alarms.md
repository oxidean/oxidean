# Alarms — oxpilot

## Sources

Consumes per-unit `nfr-design/performance-design.md` (**performance-design**), `nfr-design/security-design.md` (**security-design**), `nfr-design/reliability-design.md` (**reliability-design**), `infrastructure-design/monitoring-design.md` (**monitoring-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
Alarm delivery rides the platform's existing alerting (Railway deploy health + any operator-configured log alerts). Recommended thresholds:

| Signal | Threshold | Rationale |
|---|---|---|
| api healthcheck fail | immediate | existing deploy gate |
| oxpilot refusal-rate spike (`budget_exceeded`, `flag_off`) | >50% of oxpilot requests over 15m | indicates misconfig, not load |
| SSE route 5xx | any sustained (>5m) | streaming path broken |
| turn `error` terminal rate | >10% of turns over 15m | provider or runtime fault |
| migration failure on boot | immediate | additive migrations should never fail |

No new alarm infrastructure is provisioned — operators wire these into their existing log-alert sink.

## Scope

Existing-surface reuse only — see Sources; no new observability infrastructure is provisioned by this intent.
