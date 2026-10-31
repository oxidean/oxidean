# SLO Report — oxpilot

## Sources

Consumes `observability-setup/dashboards.md` (**dashboards**), `observability-setup/alarms.md` (**alarms**), `observability-setup/slo-config.md` (**slo-config**), and `deployment-execution/deployment-log.md` (**deployment-log**).

## Current posture

Flag-off at ship; two latency SLOs (`first SSE frame <250ms p99`, `session_list <150ms p99`) are defined and pending first flag-on measurement — see `operation/performance-validation/nfr-validation-matrix.md`.

## Resource-bound SLOs (Met via CI tests)

Transcript ≤128 KiB newest-first; tool summaries ≤2 KiB; tool timeout 60s; ≤8 rounds; turn ≤10min — all enforced and covered by CI nextest.

## Reporting cadence

At first flag-on deployment: run L1/L2 from `performance-validation/load-test-plan.md`, append actuals to `performance-validation/test-results.md`, and flip the pending matrix rows to Met/Not-Met with evidence.
