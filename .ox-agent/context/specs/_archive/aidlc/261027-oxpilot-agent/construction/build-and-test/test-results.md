# Test Results — Build and Test (oxpilot)

Executed locally under the user's directive (no local Bun/Cargo builds — CI is the authoritative executor for heavyweight jobs). Locally-permitted lightweight checks were run for real; CI-owned checks carry the in-flight run as evidence and remain `Unverified` until the run completes.

## Executed locally — all passed

| Command | Result |
|---|---|
| `echo "local unit verification deferred to CI per user directive" && git diff --check` | exit 0 — clean |
| `bunx vitest run src/routes/oxpilot.integration.test.ts src/lib/oxpilot-stream.unit.test.ts` (cwd `apps/web`) | **73/73 passed** (53 integration + 20 stream unit), 2 files, 8.35s |
| `make web-lint` | exit 0 — type-aware oxlint + DOM-race heuristic clean |
| `make web-format-check` | exit 0 — oxfmt + prettier clean (559 files) |

Note: running vitest from the repo root fails module-alias resolution (`@/…`); the suite must run with `apps/web` as cwd so its vitest config applies. Documented in integration-test-instructions.md.

## CI-owned executions (in flight on `aa7ff9b3`)

`cargo nextest` suites (api-oxpilot, api --test oxpilot, api-core oxpilot, db dialect_oxpilot, cli pilot/pilot_connector), `cargo fmt/clippy`, `cargo build`, `make rpc-gen`/`rpc-sync-check`, `make dead-code-check`, `make route-coverage-check`/`browser-coverage-check`, `bun run --filter @oxidean/web build`, browser tests, `e2e-stack`, `coverage-weighted`, `commit-lint` — executed by `ci.yml`/`images.yml`/`ox-cli.yml` run 40793221440 (in progress at authoring). Evidence: GitHub Actions run artifacts on PR #203.

## Target Verification Matrix

| Target ID | Source | Expected | Actual | Evidence | Owning Stage | Verdict |
|---|---|---|---|---|---|---|
| BT-1 | unit-test-instructions (all 8 units) | per-unit test commands green | Rust/bun heavyweight suites — in flight | CI run 40793221440 | CI pipeline | Unverified |
| BT-2 | session-core nfr-requirements | transcript newest-first ≤128 KiB; tool summaries ≤2 KiB | covered by api-oxpilot bound tests | CI run 40793221440 (`rust-build`) | CI pipeline | Unverified |
| BT-3 | session-core nfr-requirements | tool timeout 60s; ≤8 rounds; turn ≤10min | covered by runtime tests | CI run 40793221440 | CI pipeline | Unverified |
| BT-4 | session-core nfr-requirements | first SSE frame <250ms p99 | needs deployed stack | — | performance-validation | Unverified |
| BT-5 | session-core nfr-requirements | session_list <150ms p99 | needs deployed stack | — | performance-validation | Unverified |
| BT-6 | nfr security requirements | isolation, PAT-denial, flag-off denial, consent-gated targets | covered by `oxidean-api --test oxpilot` + connector tests | CI run 40793221440 | CI pipeline | Unverified |
| BT-7 | nfr security requirements | no creds/content in logs | `cargo test -p oxidean-core redact` + raw-sink audit | CI run 40793221440 | CI pipeline | Unverified |
| BT-8 | code-generation-plans | generated client in sync | `make rpc-gen && rpc-sync-check` verified at commit time (byte-identical template check) | commit `aa7ff9b3` tree; CI `rpc-sync` leg | CI pipeline | Unverified |
| BT-9 | oxpilot-ui unit tests | UI integration suite green | 53/53 passed locally | vitest output above | build-and-test (local) | Met |
| BT-10 | oxpilot-ui unit tests | stream-lib unit suite green | 20/20 passed locally | vitest output above | build-and-test (local) | Met |
| BT-11 | repo gates | web lint clean | passed | `make web-lint` output | build-and-test (local) | Met |
| BT-12 | repo gates | web format clean | passed | `make web-format-check` output | build-and-test (local) | Met |
| BT-13 | construction verification command | clean diff | passed | `git diff --check` exit 0 | build-and-test (local) | Met |
| BT-14 | cross-unit coverage gate (Step 10) | every FR/NFR/AC covered `OK` | 122/122 enumerated ids covered (OK / via-children / mapped / Deferred-with-owner) | cross-unit-traceability.md | build-and-test | Met |
| BT-15 | e2e stack contract | oxpilot.stack.test.ts green (append-or-snapshot contract) | in flight | CI run 40793221440 (`e2e-stack`) | CI pipeline | Unverified |

No `Pending` verdicts remain. `Unverified` rows are CI/deployment-owned per the recorded user directive and the operation-phase validation stage — they do not represent in-stage defects; no in-stage fix exists for them (the user directive prohibits local execution), so the failure ladder's rung-1/rung-2 do not apply and no loop-back fix is available (rung 3 inapplicable — nothing to fix in generated code).

## Loop-Back Log

None — no loop-back fired; no diagnosable in-stage or upstream defect found.
