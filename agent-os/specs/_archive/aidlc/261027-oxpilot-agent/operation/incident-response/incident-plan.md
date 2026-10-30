# Incident Plan — oxpilot

## Sources

Consumes `observability-setup/dashboards.md` (**dashboards**), `observability-setup/alarms.md` (**alarms**), per-unit `nfr-design/reliability-design.md` (**reliability-design**) and `nfr-design/security-design.md` (**security-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
## Severity map

| Sev | Class | Examples |
|---|---|---|
| P1 | security | redaction breach (RB-4), authz bypass, connector secret leak |
| P2 | feature-down | all turns failing, SSE route 5xx, api crash-loop from migrations |
| P3 | degraded | provider flaky, elevated `budget_exceeded`, queue backlog |
| P4 | cosmetic | UI states wrong but RPC correct |

## Response flow

1. **Detect** — alarm thresholds (`observability-setup/alarms.md`) or user report.
2. **Contain** — flag-off env var (global) or `target_revoke` (scoped). The flag gate exists precisely so containment never requires a redeploy.
3. **Diagnose** — log queries (`log-queries.md`): refusal codes, turn terminals, provider errors. Content never logged — diagnose by ids/codes/counts.
4. **Remediate** — per runbook; rollback per `rollback-runbook.md` when code-level.
5. **Review** — incident note + regression test added to the owning suite.

## Data-handling rule during incidents

Never paste user message content or tool payloads into tickets/chat — ids, codes, counts only. The redaction contract applies to humans too.
