---
phase: 02-multi-db-storage
plan: "03"
subsystem: api
tags: [rpc, startup, dialect, migrate, typed-client]
requires:
  - phase: 02-multi-db-storage
    provides: "02-01 Dialect/DbPool/Database connection layer; 02-02 migrate/probe on oxidean-db"
provides:
  - API startup fail-fast dialect resolve + auto-migrate
  - system.db_probe RPC procedure
  - generated TS client system.dbProbe + systemDbProbeQueryOptions
affects: [02-04 dialect switch tooling/Compose, 02-05 CI matrix + docs]
tech-stack:
  added: [tempfile dev-dependency in oxidean-api]
  patterns: [db.probe_failed detail stays server-side only (T-02-06), unauthenticated bounded probe accepted (T-02-07)]
key-files:
  created:
    - crates/oxidean-api/tests/rpc_db_probe.rs
  modified:
    - crates/oxidean-api/src/main.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/Cargo.toml
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - packages/api-client/src/index.test.ts
    - Cargo.lock
key-decisions:
  - "When DATABASE_URL is set, both dialect mismatch and connect failure now fail-fast (exit 1) instead of warn-and-continue — a deliberate behavior change from Phase 1 for that case only"
  - "system.db_probe error branch never interpolates the raw sqlx/db error into the RPC response; detail goes to tracing::error! only, response carries the stable db.probe_failed code"
patterns-established:
  - "system.* RPC arms may call Database methods only — no dialect branching leaks into oxidean-api (D-08 held)"
requirements-completed: [PLAT-07, PLAT-08]
duration: 25min
completed: 2026-09-09
---

# Phase 2 Plan 03: API startup + system.db_probe RPC Summary

**API now fails fast on a bad `DATABASE_URL`/`OXIDEAN_DB_DIALECT` pairing or dead database, auto-migrates on boot by default, and answers `system.db_probe` over RPC with a matching generated TypeScript client surface.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 3/3
- **Files modified:** 7 (1 new test file + 6 modified)

## Accomplishments

- `main.rs`: when `DATABASE_URL` is set, resolves dialect via `resolve_dialect_from_env`, exits 1 with an operator-readable, credential-redacted message on mismatch or connect failure; logs the resolved dialect. When unset, preserves the Phase 1 `Database::skipped()` + warn behavior so Docker-free `cargo test`/`make dev` keep working.
- `OXIDEAN_AUTO_MIGRATE` (default `true`) runs `db.migrate()` before `axum::serve` starts when a pool exists; `false` logs the `make db-migrate` hint instead. Migration failure exits 1.
- `system.db_probe` dispatch arm in `rpc.rs` calls the shared `Database::probe()` path: `db.not_configured` when no pool, `db.probe_failed` (generic message, detail only in `tracing::error!`) on any other error — matching threat mitigation T-02-06.
- Three new integration tests in `rpc_db_probe.rs`: not-configured error shape, version-header enforcement on the new procedure, and a SQLite tempdir round-trip proving `probe_count` increments across two calls.
- `rpc_gen.rs` now emits `DbProbeResponse`, `system.dbProbe()`, and `systemDbProbeQueryOptions` (registered as `queryOptions.systemDbProbe`); `packages/api-client/src/index.ts` regenerated via `make rpc-gen`, verified clean via `make rpc-sync-check`. Added a client test that asserts the `system.db_probe` procedure name and version header on the new call.
- No dialect branching added to `oxidean-api` (`! grep -q 'Dialect::Postgres' crates/oxidean-api/src` — clean) and `apps/web` untouched (no diagnostics UI, per CONTEXT D-12).

## Task Commits

Each task was committed atomically:

1. **Task 1: Startup dialect resolution, fail-fast, and auto-migrate** - `991df53` (feat)
2. **Task 2: system.db_probe RPC procedure + API tests** - `58b852d` (feat)
3. **Task 3: Regenerate the typed client with system.dbProbe** - `ea3babb` (feat)

## Files Created/Modified

- `crates/oxidean-api/src/main.rs` - dialect resolve + fail-fast + auto-migrate before listener bind
- `crates/oxidean-api/src/rpc.rs` - `system.db_probe` dispatch arm with stable error codes
- `crates/oxidean-api/Cargo.toml` - added `tempfile = "3"` dev-dependency for the SQLite round-trip test
- `crates/oxidean-api/tests/rpc_db_probe.rs` - 3 new integration tests
- `crates/oxidean-api/src/bin/rpc_gen.rs` - emits `DbProbeResponse` type, `dbProbe()` client method, `systemDbProbeQueryOptions`
- `packages/api-client/src/index.ts` - regenerated (never hand-edited)
- `packages/api-client/src/index.test.ts` - added a `dbProbe` header/procedure-name test
- `Cargo.lock` - `tempfile` added to `oxidean-api`'s dependency graph

## Decisions Made

- Fail-fast on connect failure only applies when `DATABASE_URL` is set (an explicit operator choice); the Phase 1 no-`DATABASE_URL` degraded-mode path is untouched, keeping `cargo test -p oxidean-api` and `make dev` Docker-free.
- `db.probe_failed` responses never carry the raw sqlx error text; it goes to `tracing::error!` only, matching the STRIDE mitigation for T-02-06 (information disclosure).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test file initially asserted `HTTP 200` for the `db.not_configured` error response**
- **Found during:** Task 2 verification (`cargo test -p oxidean-api --test rpc_db_probe`)
- **Issue:** `crates/oxidean-api/src/app.rs`'s existing `rpc_http` handler maps any `RpcResponse::Err` (other than `rpc.unknown_procedure`, which maps to 404) to HTTP 400 — this predates this plan and matches `system_health_requires_version`'s existing test pattern, but my first draft of `db_probe_without_database_returns_not_configured` assumed 200.
- **Fix:** Changed the assertion to `StatusCode::BAD_REQUEST`, matching the established handler behavior; no source changes needed.
- **Files modified:** `crates/oxidean-api/tests/rpc_db_probe.rs` (test-only)
- **Verification:** `cargo test -p oxidean-api --test rpc_db_probe` — 3 passed

**2. [Rule 3 - Blocking] `cargo fmt` reformatted unrelated pre-existing files**
- **Found during:** Pre-commit `cargo fmt -p oxidean-api` run
- **Issue:** `cargo fmt` also reformatted `crates/oxidean-api/src/app.rs` and `crates/oxidean-api/tests/rpc_ws.rs`, both outside this plan's file list and pre-existing (not touched by 02-03's tasks).
- **Fix:** `git checkout --` those two files after formatting, keeping only the plan-scoped files in the diff.
- **Files modified:** none (reverted the unintended reformat)
- **Committed in:** n/a — never staged

---

**Total deviations:** 2 auto-fixed (1 bug — test assumption vs. established handler behavior, 1 blocking — scope-creep from `cargo fmt` reverted before commit)
**Impact on plan:** Neither affected functionality; both were caught and resolved before the acceptance-criteria verification pass.

## Issues Encountered

None beyond the two deviations above, both resolved.

## User Setup Required

None — all verification ran Docker-free against a tempdir SQLite file. Postgres/MySQL legs are proven by the shared `Database::probe()`/`migrate()` path already covered by 02-02's `dialect_probe.rs`; 02-05's CI matrix exercises the same RPC arm per dialect.

## Next Phase Readiness

- API now refuses to boot on a bad dialect/URL pairing or unreachable database when `DATABASE_URL` is set, and migrates by default — ready for `02-04`'s Compose SQLite overlay and dialect-switch tooling to build on a predictable startup contract
- `system.db_probe` is reachable over HTTP and WS with a stable, generic error contract — ready for `02-04`'s dialect-asserting smoke script and `02-05`'s CI matrix to call directly
- Generated client exposes `client.system.dbProbe()` and `queryOptions.systemDbProbe` — ready for any future diagnostics UI (explicitly deferred) without further codegen work
- No blockers

## Self-Check: PASSED

- `cargo test -p oxidean-api --tests` — 10 passed (6 suites)
- `cargo test -p oxidean-api --test rpc_db_probe` — 3 passed
- `cargo test --workspace` (no `DATABASE_URL`) — 21 passed (14 suites)
- `DATABASE_URL=postgres://u:p@localhost/x OXIDEAN_DB_DIALECT=mysql cargo run -q -p oxidean-api --bin oxidean-api` — exit 1, prints `OXIDEAN_DB_DIALECT=mysql does not match DATABASE_URL scheme (detected postgres)`
- `make rpc-gen && make rpc-sync-check` — `rpc-sync-check: ok` on the committed tree
- `bun run --filter @oxidean/api-client test` — 3 passed
- `git status --porcelain apps/web` — empty (no diagnostics UI touched)
- Acceptance-criteria greps for all three tasks (`OXIDEAN_AUTO_MIGRATE`, `resolve_dialect_from_env`, `migration failed`, no `Dialect::Postgres` in API, `system.db_probe`/`db.probe_failed`/`db.not_configured` in `rpc.rs`, `DbProbeResponse`/`systemDbProbeQueryOptions` in generated client, no `format!` in the probe-failed error) — all pass

---
*Phase: 02-multi-db-storage*
*Completed: 2026-09-09*
