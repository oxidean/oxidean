---
phase: 19-actions-runners
plan: "02"
subsystem: database
tags: [actions, migrations, factory-reset, compose]

requires:
  - phase: 19-actions-runners
    provides: Wave 0 dialect_actions stub
provides:
  - 0021_actions dialect migrations + actions.rs accessors
  - OXIDEAN_ACTIONS_LOG_DIR / OXIDEAN_ACTIONS_ENABLED + Compose volume
  - Factory reset wipe for Actions domain + log dir
affects: [19-03, 19-04, 19-05, 19-07, 19-10]

actuals:
  tokens: 12000
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns: [dialect trio after latest id, AppState env absolutize, factory_reset wipe_actions_domain first]

key-files:
  created:
    - crates/oxidean-db/migrations/sqlite/0021_actions.sql
    - crates/oxidean-db/migrations/postgres/0021_actions.sql
    - crates/oxidean-db/migrations/mysql/0021_actions.sql
    - crates/oxidean-db/src/actions.rs
    - crates/oxidean-db/tests/factory_reset_actions.rs
  modified:
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-db/tests/dialect_actions.rs
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/auth/admin.rs
    - docker-compose.yml
    - docs/CONFIGURATION.md

key-decisions:
  - "Migration id 0021 (not 0016) — 0016–0020 already used by pulls/BP/notifications/webhooks/social"
  - "Reuse Phase 13 commit_statuses table; do not recreate in 0021"
  - "Config lives in app.rs (no separate config.rs module)"

patterns-established:
  - "Actions tables: action_runners, action_runner_tokens, action_runs, action_jobs, action_secrets + repositories.actions_enabled"

requirements-completed: [ACT-03, ACT-07]

coverage:
  - id: D1
    description: dialect_actions schema parity + sqlite migrate smoke
    requirement: ACT-03
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-db -E 'test(dialect_actions)'
        status: pass
    human_judgment: false
  - id: D2
    description: ACTIONS_LOG_DIR + ACTIONS_ENABLED in config/compose/docs
    requirement: ACT-03
    verification:
      - kind: other
        ref: rg OXIDEAN_ACTIONS_LOG_DIR app.rs compose CONFIGURATION.md
        status: pass
    human_judgment: false
  - id: D3
    description: factory_reset wipes Actions domain
    requirement: ACT-07
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-db -E 'test(factory_reset_actions)'
        status: pass
    human_judgment: false

plan_head_before: 716936cdc862d2f8aa6bafac8fac7fb80c36d445
duration: 15min
completed: 2026-09-16
status: complete
---

# Phase 19 Plan 02: Actions schema + LOG_DIR Summary

**Dialect `0021_actions` migrations, thin `actions.rs` accessors, Compose/docs for `OXIDEAN_ACTIONS_LOG_DIR` / `OXIDEAN_ACTIONS_ENABLED`, and factory-reset wipe for Actions metadata + logs.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 3
- **Files modified:** 13

## Accomplishments
- Synced sqlite/postgres/mysql migrations for runners, tokens, runs, jobs, secrets, `actions_enabled`
- Greened `dialect_actions` + `factory_reset_actions`
- Wired AppState log dir + instance gate; Compose volume; admin wipe of log dir on factory reset

## Task Commits

1. **Task 1: Actions migrations + db module** - `efc69ab` (feat)
2. **Task 2: ACTIONS_LOG_DIR config + Compose** - `c6cc541` (feat)
3. **Task 3: Factory reset wipe Actions** - `09bf82d` (test)

## Files Created/Modified
- `crates/oxidean-db/migrations/*/0021_actions.sql` — schema
- `crates/oxidean-db/src/actions.rs` — accessors + wipe_actions_domain
- `crates/oxidean-api/src/app.rs` — env parse (replaces planned config.rs)
- `docker-compose.yml` / `docs/CONFIGURATION.md` — volume + docs

## Decisions Made
- Renumbered migration to 0021 per plan assumption
- Reused existing `commit_statuses` from 0017
- Config in `app.rs` (project has no `config.rs`)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Migration id collision**
- **Found during:** Task 1
- **Issue:** Plan named `0016_actions` but 0016–0020 already exist
- **Fix:** Used `0021_actions` across all dialects
- **Commit:** efc69ab

**2. [Rule 3 - Blocking] config.rs path**
- **Found during:** Task 2
- **Issue:** Plan referenced nonexistent `crates/oxidean-api/src/config.rs`
- **Fix:** Extended `app.rs` + `RpcCtx` like LFS/packages
- **Commit:** c6cc541

**3. [Rule 2 - Critical] Factory reset log wipe**
- **Found during:** Task 2/3
- **Issue:** D-ACT-19 requires log dir wipe; plan task 3 only listed db tests
- **Fix:** Admin factory reset always wipes `actions_log_dir`
- **Commit:** c6cc541

## Self-Check: PASSED

- FOUND: crates/oxidean-db/migrations/sqlite/0021_actions.sql
- FOUND: crates/oxidean-db/src/actions.rs
- FOUND: crates/oxidean-db/tests/factory_reset_actions.rs
- FOUND: efc69ab, c6cc541, 09bf82d
