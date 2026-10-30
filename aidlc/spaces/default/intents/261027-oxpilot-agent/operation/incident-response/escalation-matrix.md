# Escalation Matrix — oxpilot

## Sources

Consumes `observability-setup/dashboards.md` (**dashboards**), `observability-setup/alarms.md` (**alarms**), per-unit `nfr-design/reliability-design.md` (**reliability-design**) and `nfr-design/security-design.md` (**security-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
| Level | Who | When | Channel |
|---|---|---|---|
| L1 | On-call operator | any Sev | existing on-call path |
| L2 | Repo maintainers | L1 can't contain in 30m, or any P2 | repo issue + maintainer ping |
| L3 | Security contact | any P1 (redaction, authz, secrets) | project security policy contact — immediate, parallel to containment |

## Ownership notes

- Provider-side faults → provider status page; keep `stub` provider as the diagnostic baseline (a stub turn succeeding isolates provider vs runtime).
- Railway-infra faults → Railway status + redeploy; not oxpilot scope.
- The feature being flag-gated means the default safe state is always one env var away — every escalation level can apply L1 containment.
