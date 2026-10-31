# Log Queries — oxpilot

## Sources

Consumes per-unit `nfr-design/performance-design.md` (**performance-design**), `nfr-design/security-design.md` (**security-design**), `nfr-design/reliability-design.md` (**reliability-design**), `infrastructure-design/monitoring-design.md` (**monitoring-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
Railway log queries over the `api` service (operator-side; all fields are content-free by redaction design):

```
# All oxpilot activity
oxpilot

# Refusals (budget/flag/permission)
oxpilot AND (budget_exceeded OR flag_off OR denied OR forbidden)

# Turn lifecycle
oxpilot AND (turn_started OR turn_terminal OR finish_turn)

# Computer use
oxpilot AND (target OR consent OR grant OR revoked)

# Slow path hunting — session_list
oxpilot.session_list AND duration_ms>150
```

## Redaction contract

Logs MUST NOT contain provider credentials, user message content, tool inputs/outputs, or connector secrets — enforced by `oxidean-core` redaction (unit-tested) and SEC pins in the UI suite. Any query hit containing them is a P1 defect, not a logging choice.
