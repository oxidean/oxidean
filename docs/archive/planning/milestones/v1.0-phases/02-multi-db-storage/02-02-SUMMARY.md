---
phase: 02-multi-db-storage
plan: "02"
subsystem: database
tags: [sqlx, postgres, mysql, sqlite, migrations, diagnostics]
requires:
  - phase: 02-multi-db-storage
    provides: "02-01 Dialect/DbPool/Database connection layer"
affects: [02-03 API startup + RPC, 02-04 dialect switch tooling, 02-05 CI matrix + docs]
tech-stack:
  added: [sqlx migrate feature, sqlx macros feature (migrate! only, no query!/query_as!)]
  patterns: [per-dialect hand-synced migration files, concrete-pool SQL branching in probe.rs, sqlite busy_timeout for concurrent-connection tests]
key-files:
  created:
    - crates/oxidean-db/migrations/postgres/0001_init.sql
    - crates/oxidean-db/migrations/mysql/0001_init.sql
    - crates/oxidean-db/migrations/sqlite/0001_init.sql
    - crates/oxidean-db/src/migrate.rs
    - crates/oxidean-db/src/probe.rs
    - crates/oxidean-db/src/bin/migrate.rs
    - crates/oxidean-db/tests/dialect_probe.rs
    - crates/oxidean-db/tests/sqlite_paths.rs
  modified:
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-db/src/pool.rs
    - crates/oxidean-db/Cargo.toml
    - crates/oxidean-core/src/lib.rs
key-decisions:
  - "Hand-synced parallel SQL per dialect (no templating/rewriter) — locked by CONTEXT D-05 discretion"
  - "instances columns: id, dialect, probe_count, probed_at — locked per RESEARCH open item 4"
  - "sqlite busy_timeout(5s) added to pool.rs so two connections against one file retry instead of erroring — needed once dialect_probe.rs tests share one DATABASE_URL/file"
  - "dialect_probe.rs tests serialize via a static tokio::sync::Mutex since they share one target across postgres/mysql/sqlite CI legs"
patterns-established:
  - "migration_parity unit test (no DB) catches per-dialect migration drift mechanically"
  - "Database::migrate/is_empty/probe are the only entry points; all dialect SQL branching stays inside oxidean-db"
requirements-completed: [PLAT-07, PLAT-08]
duration: 45min
completed: 2026-09-09
---

# Phase 2 Plan 02: Migrations + Probe Summary

**Three lockstep sqlx migration sets create an `instances` table on Postgres/MySQL/SQLite, with a shared `probe()` upsert-then-read and an explicit `migrate --assert-empty` CLI, both DATABASE_URL-gated and proven Docker-free on SQLite.**

## Performance

- **Duration:** ~45 min
- **Tasks:** 3/3
- **Files modified:** 12 (4 new + 4 modified for 02-02 proper, plus a deviation fix that landed 5 files 02-01 had left uncommitted)

## Accomplishments

- `migrations/{postgres,mysql,sqlite}/0001_init.sql` create `instances (id, dialect, probe_count, probed_at)`, each dialect using its native PK/timestamp/default syntax
- `migrate::run_migrations` runs `sqlx::migrate!` per dialect (own `_sqlx_migrations` bookkeeping per target); `migration_parity` unit test asserts identical filename sets across all three dirs with no DB required
- `migrate::is_empty` counts user tables excluding `_sqlx_migrations`, per dialect — the D-16 empty-target guard
- `oxidean-core::DbProbeResponse` is the one shared response shape; `probe::probe()` does a dialect-correct single-row upsert on `id=1` then read-back (`ON CONFLICT`/`ON DUPLICATE KEY UPDATE`/`ON CONFLICT`), decoded via `sqlx::Row::try_get`, no `query!`/`query_as!` macros anywhere
- `Database::migrate()` / `Database::is_empty()` / `Database::probe()` expose the shared path
- `crates/oxidean-db/src/bin/migrate.rs`: `cargo run -p oxidean-db --bin migrate -- [--assert-empty] [--force-empty]` prints `migrated: <dialect>` on success, refuses a non-empty target unless forced, and only ever prints `redact_url()` output
- `tests/dialect_probe.rs` (DATABASE_URL-gated, the file CI drives per dialect) and `tests/sqlite_paths.rs` (Docker-free, always runs) prove migrate+probe end-to-end

## Task Commits

Each task was committed atomically:

1. **Deviation fix — commit missing 02-01 source files** - `faf8b96` (fix) — see Deviations below
2. **Task 1: Per-dialect migration sets + run_migrations + parity test** - `6750972` (feat)
3. **Task 2: instances probe write/read + core response type** - `8c5adde` (feat)
4. **Task 3: DATABASE_URL-gated integration tests + migrate binary** - `5e3a5fe` (feat)

## Files Created/Modified

- `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0001_init.sql` - per-dialect `instances` DDL
- `crates/oxidean-db/src/migrate.rs` - `run_migrations`, `is_empty`, `migration_parity` test
- `crates/oxidean-db/src/probe.rs` - dialect-branched upsert + read-back
- `crates/oxidean-db/src/bin/migrate.rs` - operator/CI migrate CLI
- `crates/oxidean-db/tests/dialect_probe.rs` - DATABASE_URL-gated round-trip + idempotency tests
- `crates/oxidean-db/tests/sqlite_paths.rs` - parent-dir + probe proof, no env gating
- `crates/oxidean-db/src/lib.rs` - wires `migrate`/`probe` modules onto `Database`
- `crates/oxidean-db/src/pool.rs` - added `busy_timeout(5s)` for SQLite (deviation, see below)
- `crates/oxidean-db/Cargo.toml` - added `macros` sqlx feature (needed for `sqlx::migrate!`)
- `crates/oxidean-core/src/lib.rs` - `DbProbeResponse { dialect, probe_count, probed_at }`

## Decisions Made

- Hand-synced parallel SQL files per dialect (CONTEXT D-05 discretion) — no templating engine or rewriter; a unit test (`migration_parity`) catches drift instead of relying on memory
- `instances` column set locked to `id, dialect, probe_count, probed_at` per RESEARCH's proposed shape
- Added SQLite `busy_timeout(5s)` to `pool.rs` — not explicitly in the plan text, but required once two integration tests intentionally share one `DATABASE_URL`/file (see Deviations)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Committed 02-01's connection-layer files that were never staged**
- **Found during:** Pre-Task-1 `git status` check
- **Issue:** `02-01-SUMMARY.md` and the ROADMAP checkbox claimed the connection layer (dialect resolution, `DbPool`, multi-dialect `Database`) was committed, but `git show HEAD:crates/oxidean-db/src/lib.rs` still showed the Phase-1 Postgres-only stub. The 02-01 commit (`4e55b9e`) only touched `.planning/ROADMAP.md` and `02-01-SUMMARY.md` — the actual `dialect.rs`, `pool.rs`, rewritten `lib.rs`, `Cargo.toml` sqlx-feature changes, and `Cargo.lock` were left sitting uncommitted in the working tree.
- **Fix:** Verified the working-tree code matched exactly what 02-01-SUMMARY.md described (5 `oxidean-db` unit tests passing, workspace green), then committed those files as-is with no functional changes, crediting them to 02-01.
- **Files modified:** `crates/oxidean-db/Cargo.toml`, `crates/oxidean-db/src/lib.rs`, `crates/oxidean-db/src/dialect.rs` (new), `crates/oxidean-db/src/pool.rs` (new), `Cargo.lock`
- **Verification:** `cargo test -p oxidean-db --lib` — 5 passed; `cargo test --workspace` — 13 passed, matching 02-01-SUMMARY.md's reported numbers
- **Committed in:** `faf8b96`

**2. [Rule 1 - Bug] SQLite `database is locked` race in DATABASE_URL-gated tests**
- **Found during:** Task 3 verification (`cargo test -p oxidean-db --test dialect_probe`)
- **Issue:** `migrate_and_probe_round_trip` and `migrate_is_idempotent` intentionally point at the same `DATABASE_URL`/file (as CI does, one URL per matrix leg). Cargo runs tests in the same binary on separate threads by default, so both opened independent 1-connection SQLite pools against the same file concurrently; `CREATE TABLE`/upsert statements raced and one test failed with `(code: 5) database is locked` non-deterministically.
- **Fix:** (a) Added `.busy_timeout(Duration::from_secs(5))` to the shared `SqliteConnectOptions` in `pool.rs` — a sensible default per D-20 that also helps any future concurrent-writer scenario, not just tests. (b) Added a `static tokio::sync::Mutex<()>` guard in `dialect_probe.rs` so the two tests serialize instead of racing, since they share one target by design.
- **Files modified:** `crates/oxidean-db/src/pool.rs`, `crates/oxidean-db/tests/dialect_probe.rs`
- **Verification:** Re-ran the sqlite-targeted `dialect_probe` test 3 times in a row with fresh tempdirs — all green, no flake
- **Committed in:** `5e3a5fe` (part of Task 3 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking — completing 02-01's incomplete commit, 1 bug — SQLite lock race)
**Impact on plan:** Deviation 1 was pre-existing repo state, not caused by this plan, and was fixed as a prerequisite so 02-02 built on the connection layer that actually exists in git history (not just in the working tree). Deviation 2 was necessary for the plan's own integration tests to pass reliably; no scope creep beyond making the specified tests deterministic.

## Issues Encountered

None beyond the two deviations above, both resolved.

## User Setup Required

None - no external service configuration required. All verification in this plan ran against a local SQLite file; Postgres/MySQL legs are proven by the same test file once CI (`02-05`) wires `DATABASE_URL` per dialect.

## Next Phase Readiness

- `oxidean-db` now exposes `Database::migrate()`, `Database::is_empty()`, and `Database::probe() -> DbProbeResponse` — ready for `02-03` to call from API startup (auto-migrate) and wire a `system.db_probe` RPC handler around the same `probe()` path
- `crates/oxidean-db/src/bin/migrate.rs --assert-empty`/`--force-empty` is ready for `02-04`'s `make db-switch-dialect` to shell out to
- `tests/dialect_probe.rs` is the exact file `02-05`'s CI `db-matrix` job should run per dialect leg (`cargo test -p oxidean-db --test dialect_probe`)
- No blockers

## Self-Check: PASSED

- `cargo test -p oxidean-db --lib migration_parity` — 1 passed
- `cargo build -p oxidean-db && cargo test --workspace` — 18 passed (13 suites), no `DATABASE_URL` set
- `DATABASE_URL=sqlite:./target/tmp/verify/oxidean.db cargo test -p oxidean-db --test dialect_probe` — 2 passed, no skip message
- `cargo test -p oxidean-db --test sqlite_paths` — 2 passed, no `DATABASE_URL`, no Docker
- `! grep -rn 'sqlx::query!\|query_as!' crates/oxidean-db/src` — clean, no compile-time macros
- `DATABASE_URL=sqlite:./target/tmp/verify-cli/oxidean.db cargo run -q -p oxidean-db --bin migrate -- --assert-empty` — exit 0, `migrated: sqlite`, no credentials in output
- Re-ran `--assert-empty` against the now-populated same file — refused with exit 1 and the documented message, confirming D-16
- Acceptance-criteria greps for all three tasks (migrations, `probe.rs` upsert syntax per dialect, `bin/migrate.rs` flags, test files) — all pass

---
*Phase: 02-multi-db-storage*
*Completed: 2026-09-09*
