---
phase: 08-git-https-pats
plan: "03"
subsystem: database
tags: [pat, migration, oxidean-db, pat_types, git-https, d-08, d-10, git-11]

requires:
  - phase: 08-git-https-pats
    provides: 08-02 locked D-08/D-18/D-21 + Wave 0 dialect_pats stubs
provides:
  - "Tri-dialect 0008_pats (personal_access_tokens + personal_access_token_repos)"
  - "pat_types with oxidean_pat_/oxidean_fg_ and classic/FG DTOs (no secret on list)"
  - "Database PAT CRUD: create/find/list/revoke/touch"
  - "RESERVED_USERNAMES includes git/token/oauth2"
affects:
  - 08-04 mint + Smart HTTP tracer
  - 08-05+ pat.* RPC and settings UI

actuals:
  tokens: 10940
  tasks: 2
  commits: 7

plan_head_before: 530d04cdbdda2fa731a95367705647123dee3eff

tech-stack:
  added: []
  patterns:
    - "PAT hash-at-rest mirrors sessions (token_hash CHAR(64) UNIQUE; soft revoke)"
    - "FG selected repos via JOIN table with CASCADE FKs"
    - "Token prefixes locked to oxidean_* (not ona_*)"

key-files:
  created:
    - crates/oxidean-db/migrations/postgres/0008_pats.sql
    - crates/oxidean-db/migrations/mysql/0008_pats.sql
    - crates/oxidean-db/migrations/sqlite/0008_pats.sql
    - crates/oxidean-db/src/pats.rs
    - crates/oxidean-core/src/pat_types.rs
    - .planning/phases/08-git-https-pats/08-03-SUMMARY.md
  modified:
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-db/tests/dialect_pats.rs
    - crates/oxidean-core/src/auth_types.rs
    - crates/oxidean-core/src/lib.rs
    - .planning/phases/08-git-https-pats/08-DISCUSSION-LOG.md

key-decisions:
  - "08-03-T0 proceed_locked with oxidean_* prefixes (not plan checkpoint ona_*)"
  - "find_pat_by_token_hash excludes soft-revoked rows; list orders created_at DESC, id DESC"
  - "PatListItem has no token field; CreatePatResponse carries one-time plaintext"

patterns-established:
  - "pats.rs dialect match CRUD mirrors sessions.rs + repositories nullable timestamp SELECT"
  - "CLASSIC_PAT_PREFIX / FINE_GRAINED_PAT_PREFIX constants in pat_types for mint (08-04)"

requirements-completed: []  # GIT-11 persistence only; full create/list/revoke UX completes in 08-04+

coverage:
  - id: D1
    description: "Tri-dialect 0008_pats with personal_access_tokens (token_hash UNIQUE) and personal_access_token_repos"
    requirement: GIT-11
    verification:
      - kind: unit
        ref: "crates/oxidean-db --lib#migration_parity"
        status: pass
      - kind: unit
        ref: "crates/oxidean-db/tests/dialect_pats.rs#dialect_pats_tri_dialect_files"
        status: pass
    human_judgment: false
  - id: D2
    description: "PAT create/find/list/revoke/touch round-trip on sqlite migrate"
    requirement: GIT-11
    verification:
      - kind: unit
        ref: "crates/oxidean-db/tests/dialect_pats.rs#dialect_pats_migrate_0008_schema_presence"
        status: pass
    human_judgment: false
  - id: D3
    description: "pat_types classic/FG DTOs with oxidean_pat_/oxidean_fg_; list item has no secret"
    requirement: GIT-11
    verification:
      - kind: unit
        ref: "crates/oxidean-core --lib#pat_types::tests"
        status: pass
    human_judgment: false
  - id: D4
    description: "RESERVED_USERNAMES blocks git/token/oauth2 Basic aliases"
    requirement: GIT-11
    verification:
      - kind: unit
        ref: "crates/oxidean-core/src/auth_types.rs#git_token_oauth2_are_reserved_for_basic_aliases"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 03: PAT Schema & Types Summary

**Tri-dialect `0008_pats` + `pat_types`/`pats` CRUD with `oxidean_pat_`/`oxidean_fg_` prefixes and reserved Basic aliases — ready for 08-04 mint/Smart HTTP tracer**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-09-13T18:22:06Z
- **Completed:** 2026-09-13T18:25:55Z
- **Tasks:** 2 (Task 0 checkpoint + Task 1 TDD)
- **Files modified:** 11

## Accomplishments

- Human `proceed_locked` recorded for D-08/D-18/D-21 with **oxidean_*** prefixes (not `ona_*`)
- Migrated `personal_access_tokens` + `personal_access_token_repos` on postgres/mysql/sqlite
- Core DTOs: `PatKind`, classic `repo`, FG `selected|all` + `contents` read/write; list never includes plaintext
- DB facade: `create_pat`, `find_pat_by_token_hash`, `list_pats_for_user`, `revoke_pat`, `touch_pat_last_used`
- Reserved usernames: `git`, `token`, `oauth2`

## Task Commits

Each task was committed atomically:

1. **Task 0: Confirm proceed_locked** - `1de5396` (docs)
2. **Task 1 RED: failing schema + reserved-alias tests** - `f52909a` (test) + `d38be96` (test evidence fix)
3. **Task 1 GREEN: schema, types, CRUD** - `803b54f` (feat)

**Plan metadata:** (this SUMMARY commit)

## Files Created/Modified

- `crates/oxidean-db/migrations/*/0008_pats.sql` — PAT tables + FG join
- `crates/oxidean-db/src/pats.rs` — dialect CRUD
- `crates/oxidean-db/src/lib.rs` — Database facade methods
- `crates/oxidean-db/tests/dialect_pats.rs` — schema + round-trip green
- `crates/oxidean-core/src/pat_types.rs` — enums/DTOs + prefix constants
- `crates/oxidean-core/src/auth_types.rs` — reserved aliases + unit test
- `crates/oxidean-core/src/lib.rs` — re-export pat_types
- `.planning/phases/08-git-https-pats/08-DISCUSSION-LOG.md` — proceed_locked audit

## Decisions Made

- Honored 08-02 human lock: mint prefixes are `oxidean_pat_` / `oxidean_fg_` everywhere (deviation from 08-03 checkpoint text that still said `ona_*`)
- Soft-revoked tokens excluded from hash lookup and list; list sort `created_at DESC, id DESC`
- No RPC / Smart HTTP wiring (deferred 08-04+)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Dialect match arm type mismatch in find/list**
- **Found during:** Task 1 GREEN compile
- **Issue:** Shared `match` returning sqlx row types across Postgres/MySQL/SQLite failed to unify
- **Fix:** Map to `PatRow` inside each dialect arm (same pattern as `sessions.rs`)
- **Files modified:** `crates/oxidean-db/src/pats.rs`
- **Committed in:** `803b54f`

### Documented human / plan text deviations

**1. [Human lock] Prefix branding vs plan checkpoint wording**
- **Found during:** Task 0
- **Issue:** Plan Task 0 / acceptance `rg` still mention `ona_pat_`/`ona_fg_`; 08-02 locked `oxidean_*`
- **Fix:** Implemented and tested `oxidean_pat_`/`oxidean_fg_` only
- **Files modified:** `pat_types.rs`, `dialect_pats.rs`, DISCUSSION-LOG
- **Committed in:** `1de5396`, `803b54f`

---

**Total deviations:** 1 auto-fix (Rule 1) + 1 documented human prefix lock
**Impact on plan:** Correct — matches locked CONTEXT; no scope creep

## Issues Encountered

- Initial RED evidence JSON used wrong schema (`exit_code` vs `exitCode` + missing TAP) → fixed with `d38be96` before GREEN

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Schema and validators ready for **08-04** mint + Smart HTTP tracer
- Do not wire `pat.*` RPC or CGI yet; keep dialect SQL in `oxidean-db` only

## Known Stubs

None — Wave 0 `dialect_pats` stubs turned green; no plaintext/list stubs remaining for this plan's goal.

## TDD Gate Compliance

- **RED:** `dialect_pats_migrate_0008_schema_presence` failed (missing 0008); evidence `RED_EVIDENCE_OK` via `.tdd/08-03-red-evidence.json`
- **GREEN:** migrations + `pats`/`pat_types` + reserved aliases; `cargo test -p oxidean-core --lib`, `migration_parity`, `dialect_pats` all pass
- **REFACTOR:** none required beyond Rule 1 match-arm fix inside GREEN

## Self-Check: PASSED

- FOUND: `0008_pats.sql` (postgres/mysql/sqlite), `pats.rs`, `pat_types.rs`, `08-03-SUMMARY.md`
- FOUND commits: `1de5396`, `f52909a`, `d38be96`, `803b54f`
- FOUND prefixes: `oxidean_pat_`, `oxidean_fg_` (no `ona_pat_`/`ona_fg_` in new code)
- FOUND reserved: `git`, `token`, `oauth2`
- NOTE: GIT-11 left Pending in REQUIREMENTS.md (`ready-ids` blocked) — persistence only; full UX in later plans

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*
