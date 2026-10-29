# Drift Report — oxpilot

## Spec → implementation drift (none outstanding)

Cross-unit traceability gate passed with 122/122 enumerated ids covered (see `construction/build-and-test/cross-unit-traceability.md`). Three `Deferred` labels are intra-unit ownership splits resolved at the cross-unit level — not drift.

## Known deliberate deferrals

| Item | Where deferred | Why |
|---|---|---|
| p99 latency SLO measurement | first flag-on deploy | needs live env — recorded in performance-validation matrix |
| Rust coverage gating | platform level | collect-only today (team.md debt) — not invented here |
| Full dialect parity in CI | platform level | `dialect_probe` per dialect is the CI contract; local-capable only (team.md) |
| Load-test harness | platform level | no bench infra exists (team.md debt) — plan documented instead |
| Multi-replica turn state | future | documented single-replica caveat — no hidden claim |

## Drift watch

The generated client (`@oxidean/api-client`) is the one true drift surface — `make rpc-sync-check` is the standing gate and stays green.
