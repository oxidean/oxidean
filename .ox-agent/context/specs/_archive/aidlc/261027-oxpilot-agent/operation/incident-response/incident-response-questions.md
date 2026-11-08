# Incident Response — Clarifying Questions

## Sources

Consumes `observability-setup/dashboards.md` (**dashboards**), `observability-setup/alarms.md` (**alarms**), per-unit `nfr-design/reliability-design.md` (**reliability-design**) and `nfr-design/security-design.md` (**security-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
## Q1. Oxpilot-specific incident classes?

**Answer:** (a) provider outage/misconfig → turns fail `error`; (b) connector misuse/consent abuse → revoke grants; (c) wedged queue/turn (single-replica in-memory); (d) redaction breach → P1 security incident; (e) budget misconfig → mass `budget_exceeded`.

## Q2. First containment lever?

**Answer:** `OXIDEAN_OXPILOT_ENABLED=false` — one env var disables the whole feature instantly; second lever is per-target grant revocation (`oxpilot.target_revoke`).

## Q3. Escalation path?

**Answer:** On-call operator → repo maintainers; security-class incidents (redaction breach, authz bypass) escalate immediately per project security policy.

## Consolidated Summary Confirmation

- Five oxpilot incident classes mapped to existing ops posture (Q1).
- Flag env var is the global kill-switch; grant revoke is the targeted one (Q2).
- On-call → maintainers; security incidents fast-tracked (Q3).

Does this all look correct before I write the incident-response artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
