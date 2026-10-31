# NFR Validation Matrix — oxpilot

| Target | Expected | Actual | Verdict |
|---|---|---|---|
| First SSE frame < 250 ms p99 | <250ms p99 | — (flag-on deploy pending) | Pending — owned by first flag-on deploy measurement (L1) |
| `session_list` < 150 ms p99 | <150ms p99 | — (flag-on deploy pending) | Pending — owned by first flag-on deploy measurement (L2) |
| Transcript newest-first ≤128 KiB | bound enforced | tests pass | Met (CI) |
| Tool summaries ≤2 KiB | cap enforced | tests pass | Met (CI) |
| Tool timeout 60 s | enforced | tests pass | Met (CI) |
| Max 8 tool rounds / 10 min turn | enforced | tests pass | Met (CI) |
| Single-replica honesty | documented caveat | in NFR design + runbooks | Met |
| No creds/content in logs | redaction | `oxidean-core` redact tests pass | Met (CI) |
| Per-user/instance budgets | enforced | budget tests pass | Met (CI) |

Two `Pending` rows remain by design — they are the only targets that require a live flag-on environment, and the load plan + this matrix carry them forward explicitly (they do not block the flag-off merge; they gate the post-enable SLO sign-off).

## Ownership

Pending rows are owned by the first flag-on deployment measurement loop defined in `load-test-plan.md`; all other rows carry CI or design evidence.

## Notes

All verdict language mirrors the build-and-test matrix conventions; pending rows are not defects — they are targets requiring a live flag-on environment.
