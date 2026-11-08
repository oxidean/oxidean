# Performance Test Results — oxpilot

## Status

**Not measured against a live environment** — the feature ships flag-off and no flag-on deployed environment exists yet; fabricating numbers would falsify the record. The authoritative measurable evidence today is the CI-side and unit-level bound coverage:

| Bound | Evidence today | Result |
|---|---|---|
| Transcript ≤128 KiB newest-first | `oxidean-api-oxpilot` assemble_prompt tests (CI `rust-build`) | Pass in CI |
| Tool summaries ≤2 KiB | summarizer cap tests (CI) | Pass in CI |
| Tool timeout 60s / 8 rounds / 10min turn | runtime timeout/cap tests (CI) | Pass in CI |
| Queue drain seriality | drain_and_chain tests (CI) | Pass in CI |
| First-frame <250ms p99 | pending flag-on measurement (L1) | **Pending deploy** |
| `session_list` <150ms p99 | pending flag-on measurement (L2) | **Pending deploy** |

## Execution record

Load test scenarios L1–L4 are defined in `load-test-plan.md` and run on the first flag-on environment; results are appended here with measured percentiles before the SLO rows are called Met.
