---
phase: 11-issues
plan: "04"
subsystem: api
tags: [issues, rpc, acl, octane, lifecycle, history, hard-delete, iss-01, d-iss-02, d-iss-03, d-iss-04, d-iss-20]

requires:
  - phase: 11-issues
    provides: "issue.create/get/list tracer + Issues UI shells (11-03)"
  - phase: 11-issues
    provides: "0011_issues schema with issue_revisions (11-02)"
provides:
  - "issue.update / close / reopen / history / delete RPC"
  - "Full title/body revision trail (D-ISS-04)"
  - "Admin hard-delete with confirmNumber; #N not reclaimed"
  - "Detail UI edit/close/reopen + history panel + delete dialog"
affects:
  - 11-05 comments
  - 11-08 reactions
  - 11-11 advanced list filters

actuals:
  tokens: 20406
  tasks: 3
  commits: 7

plan_head_before: c577bf9a00b89e3a33498e5e1f91a8e38943e688

tech-stack:
  added: []
  patterns:
    - "Prior title/body snapshots in issue_revisions on each content edit"
    - "confirmNumber typed confirm mirrors repo.softDelete confirmName"
    - "Author OR Write+ edit ACL; Write+ close/reopen; Admin delete"

key-files:
  created:
    - apps/web/src/components/repo/issue-history.tsrx
    - apps/web/src/components/repo/issue-delete-dialog.tsrx
  modified:
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/src/issue/acl.rs
    - crates/oxidean-db/src/issues.rs
    - crates/oxidean-core/src/issue_types.rs
    - crates/oxidean-api/tests/issue_lifecycle.rs
    - crates/oxidean-api/tests/issue_delete.rs
    - packages/api-client/src/index.ts
    - apps/web/src/routes/$owner.$repo.issues.$n.tsrx
    - apps/web/src/routes/$owner.$repo.issues.integration.test.ts

key-decisions:
  - "Revision rows store pre-edit title/body; history returns oldest-first chronologic trail"
  - "SQLite revision order uses rowid tiebreak (second-precision created_at)"
  - "Detail route remains $n (not $number); Write|Preview appears after Edit click"

patterns-established:
  - "issue.confirm_mismatch for typed confirm failures"
  - "Cascade delete via FK ON DELETE CASCADE; counters never decremented"

requirements-completed: [ISS-01]

coverage:
  - id: D1
    description: "Author/Write+ can update title/body; Read soft-denied; close/reopen open↔closed"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_lifecycle) | test(issue_history)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "issue.history returns full prior title/body revision trail (D-ISS-04)"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_history_full_title_body_trail)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Admin hard-delete requires confirmNumber; Write denied; #N not reused"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_delete)'"
        status: pass
    human_judgment: false
  - id: D4
    description: "Detail UI exposes edit Write|Preview, close/reopen, history, Admin delete confirm"
    requirement: ISS-01
    verification:
      - kind: automated_ui
        ref: "bunx vitest run src/routes/$owner.$repo.issues.integration.test.ts"
        status: pass
    human_judgment: false

duration: 13min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 04: Issue edit/close/reopen/history/Admin hard-delete Summary

**ISS-01 lifecycle complete: Author/Write+ edit with full revision trail, Write+ close/reopen, Admin typed hard-delete that never reclaims `#N`.**

## Performance

- **Duration:** 13 min
- **Started:** 2026-09-14T14:54:57Z
- **Completed:** 2026-09-14T15:08:27Z
- **Tasks:** 3
- **Files modified:** 14

## Accomplishments

- Wired `issue.update` / `close` / `reopen` / `history` with Author|Write and Write+ ACL
- Admin `issue.delete` gated by `confirmNumber` matching `#N`; counters never reclaim numbers
- Detail page: Edit + Write|Preview, Close/Reopen, Edit history panel, typed delete dialog

## Task Commits

1. **Task 1: update/close/reopen + issue.history RPC** - `4caf22d` (feat)
2. **Task 2: Admin issue.delete with confirmNumber** - `bb22c1d` (feat)
3. **Task 3: Detail UI — edit/close/reopen/history/delete** - `967d4b3` (feat)

**Plan metadata:** `3097617` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-api/src/issue/mod.rs` — update/close/reopen/history/delete handlers
- `crates/oxidean-api/src/issue/acl.rs` — can_edit_issue + resolve_for_admin
- `crates/oxidean-db/src/issues.rs` — revisions + content/state mutations
- `crates/oxidean-core/src/issue_types.rs` — Update/Delete/History DTOs
- `crates/oxidean-api/tests/issue_lifecycle.rs` — greened edit/close/history
- `crates/oxidean-api/tests/issue_delete.rs` — greened Admin delete + no reuse
- `packages/api-client/src/index.ts` — generated client + mutation helpers
- `apps/web/src/routes/$owner.$repo.issues.$n.tsrx` — lifecycle UI
- `apps/web/src/components/repo/issue-history.tsrx` — revision trail panel
- `apps/web/src/components/repo/issue-delete-dialog.tsrx` — typed confirm dialog
- `apps/web/src/routes/$owner.$repo.issues.integration.test.ts` — greened 11-04 UI cases

## Decisions Made

- Store **pre-edit** title/body in `issue_revisions`; list oldest-first for a diff trail against current
- SQLite orders revisions by `created_at, rowid` to avoid UUID-order flapping at second precision
- Soft `repo.not_found` for insufficient capability (Read cannot edit; Write cannot delete)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] SQLite history order unstable under same-second edits**
- **Found during:** Task 1 (history test)
- **Issue:** Two revisions in the same second sorted by UUID `id`, so trail order flipped
- **Fix:** `ORDER BY created_at ASC, rowid ASC` for SQLite revision listing
- **Files modified:** `crates/oxidean-db/src/issues.rs`
- **Commit:** `4caf22d`

**2. [Rule 3 - Blocking] Plan file listed `$number` route; repo uses `$n`**
- **Found during:** Task 3
- **Issue:** Plan `files_modified` named `$owner.$repo.issues.$number.tsrx`
- **Fix:** Continued editing existing `$n` route (Wave 0 / 11-03 lock)
- **Files modified:** `apps/web/src/routes/$owner.$repo.issues.$n.tsrx`
- **Commit:** `967d4b3`

## Threat Flags

None — delete/update ACL and typed confirm match plan `<threat_model>` (T-11-08, T-11-09).

## Known Stubs

None that block this plan's goal. Comments/labels/assignees/Linked PRs remain empty shells for later plans; reactions `it.fails` deferred to 11-08.

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/issue/mod.rs`
- FOUND: `apps/web/src/components/repo/issue-delete-dialog.tsrx`
- FOUND: `apps/web/src/components/repo/issue-history.tsrx`
- FOUND: `.planning/phases/11-issues/11-04-SUMMARY.md`
- FOUND commits: `4caf22d`, `bb22c1d`, `967d4b3`
