# Load Test Plan — oxpilot

Sources: per-unit `nfr-requirements` performance targets (**performance-requirements**, **scalability-requirements**), `nfr-design` (**performance-design**, **scalability-design**), `observability-setup/dashboards.md` (**dashboards**).

## Scope

No dedicated load-test harness exists in this repo (team.md records this as standing debt — no benches, no load infra). This plan defines what operators measure on a flag-on environment; it is executed post-deploy, not fabricated here.

## Plan

| # | Scenario | Method | SLO check |
|---|---|---|---|
| L1 | First SSE frame latency | `chat_post` → first frame timestamp (client-side) over N≥100 turns, stub + real provider separately | < 250 ms p99 |
| L2 | `session_list` latency | proc timing over N≥100 calls with varied session counts | < 150 ms p99 |
| L3 | Transcript assembly bound | drive sessions to the 128 KiB window edge; confirm newest-first truncation, no O(n²) | bound holds, no error |
| L4 | Sustained turns | serial turns across M sessions at single-replica expectation | error rate <1%, no queue wedge |

## Environment

Flag-on Railway env (staging first) with `stub` provider for isolate-runtime runs and the real provider for end-to-end runs. Record results in `test-results.md` (this file is updated with actuals at first flag-on deploy).
