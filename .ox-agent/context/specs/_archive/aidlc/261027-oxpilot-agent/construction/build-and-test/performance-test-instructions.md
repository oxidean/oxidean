# Performance Test Instructions — oxpilot

NFR performance targets (session-core nfr-requirements + nfr-design) with measurable checks:

| Target | Expected | Check |
|---|---|---|
| First SSE frame latency | < 250 ms p99 | e2e-stack subscribe→first-frame timing; **owned by `performance-validation` (operation phase)** — needs deployed stack |
| `session_list` latency | < 150 ms p99 | proc timing under load; **owned by `performance-validation`** |
| Prompt transcript bound | newest-first, ≤ 128 KiB | `cargo nextest run -p oxidean-api-oxpilot` (assemble_prompt bound tests) |
| Tool summary bound | ≤ 2 KiB per tool_call summary | same suite — summarizer cap tests |
| Tool timeout | 60 s per call | runtime invoke_call tests (timeout path) |
| Max tool rounds | 8 per turn | runtime loop tests |
| Turn max duration | 10 min | turn lifecycle/timeout tests |
| Single-replica caveat | in-memory active-turn state documented | doc/comment check — verified in code reviews |

## Locally executable (permitted — light Rust nextest invocations are still CI-owned per user directive; run in CI)

CI `rust-build`/`db-dialects`/`e2e-stack` jobs execute the bound/timeout/round tests above. The two latency targets require a deployed, instrumented environment → deferred to operation-phase `performance-validation` with evidence recorded there.

## Regression detection

- `coverage-weighted` CI job guards test-coverage regression.
- Any future perf harness lands under `e2e/` with a manifest row.
