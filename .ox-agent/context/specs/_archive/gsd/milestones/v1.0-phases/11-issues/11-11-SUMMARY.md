---
phase: 11-issues
plan: "11"
subsystem: api
tags: [issues, filters, pagination, octane, sqlx]

requires:
  - phase: 11-issues
    provides: "issue.list ACL + Open/Closed/All shell UI (11-07/09/10)"
provides:
  - "issue.list author/label/assignee/q filters + offset pagination"
  - "Issues list UI Apply filters + Previous/Next"
affects:
  - 11-12
  - forge issues discovery

actuals:
  tokens: 12059
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "IssueListFilters struct in oxidean-db; username→id resolve in API"
    - "Dialect LIKE/ILIKE text search stays in oxidean-db only"
    - "Offset Prev/Next (page size 25), not infinite scroll"

key-files:
  created: []
  modified:
    - crates/oxidean-db/src/issues.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/tests/issue_lifecycle.rs
    - apps/web/src/components/repo/issues-list.tsrx
    - apps/web/src/routes/$owner.$repo.issues.tsrx
    - apps/web/src/routes/$owner.$repo.issues.integration.test.ts
    - apps/web/src/lib/ssr-repo.ts

key-decisions:
  - "author/assignee filters accept usernames (optional @); label filter is label id"
  - "Unknown author/assignee username → empty page (total 0), not rpc error"
  - "Default page size 25 (plan ASSUME ~25)"

patterns-established:
  - "List filter draft inputs + Apply; query key includes all applied filters + offset"
  - "Parameterized optional filters via NULL OR predicates in dialect SQL"

requirements-completed: [ISS-01, ISS-03]

coverage:
  - id: D1
    description: "Issues list defaults Open; Closed and All available (D-ISS-16)"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/issue_lifecycle.rs#issue_list_filters_and_offset_pagination"
        status: pass
      - kind: integration
        ref: "apps/web/src/routes/$owner.$repo.issues.integration.test.ts#list defaults Open with Closed and All controls"
        status: pass
    human_judgment: false
  - id: D2
    description: "Filters author, label, assignee, and text search; sort newest-updated; offset pagination (D-ISS-17, D-ISS-18)"
    requirement: ISS-03
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/issue_lifecycle.rs#issue_list_filters_and_offset_pagination"
        status: pass
      - kind: integration
        ref: "apps/web/src/routes/$owner.$repo.issues.integration.test.ts#author/label/assignee/text filters and Apply"
        status: pass
      - kind: integration
        ref: "apps/web/src/routes/$owner.$repo.issues.integration.test.ts#offset Previous/Next pagination controls"
        status: pass
    human_judgment: false

plan_head_before: ca7b6786d083de48ad3379017d69214a01bae8f1
duration: 12min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 11: List filters + pagination Summary

**issue.list Open/Closed/All with author/label/assignee/text filters, newest-updated sort, and offset Prev/Next UI (page size 25)**

## Performance

- **Duration:** 12 min
- **Started:** 2026-09-14T16:30:15Z
- **Completed:** 2026-09-14T16:42:00Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Extended `list_for_repo` with parameterized author/label/assignee/q filters across Postgres/MySQL/SQLite
- Wired `issue.list` to resolve usernames and pass `IssueListFilters`; default limit 25
- Shipped Issues list Apply filters + Previous/Next; kept New issue empty-state IA

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: failing filter/pagination coverage** - `f4524b1` (test)
2. **Task 1 GREEN: issue.list filters + offset** - `1ac0911` (feat)
3. **Task 2: List UI filters + pagination** - `82c0763` (feat)

## Files Created/Modified

- `crates/oxidean-db/src/issues.rs` — `IssueListFilters` + dialect filtered list/count
- `crates/oxidean-db/src/lib.rs` — `Database::list_issues_for_repo` takes filters
- `crates/oxidean-api/src/issue/mod.rs` — username resolve + filter passthrough
- `crates/oxidean-api/tests/issue_lifecycle.rs` — `issue_list_filters_and_offset_pagination`
- `apps/web/src/components/repo/issues-list.tsrx` — filter form + pagination nav
- `apps/web/src/routes/$owner.$repo.issues.tsrx` — filter state + query keys
- `apps/web/src/routes/$owner.$repo.issues.integration.test.ts` — D-ISS-17/18 UI coverage
- `apps/web/src/lib/ssr-repo.ts` — SSR list accepts filters; default limit 25

## Decisions Made

- Author/assignee filter strings are usernames; label filter is label id from `label.listForRepo`
- Unknown username yields empty result set rather than `rpc.bad_input`
- Page size default 25 per plan ASSUME

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Comment-thread waitFor race**
- **Found during:** Task 2 verify (full integration suite)
- **Issue:** Sync `getByTestId("issue-comment")` raced after longer list tests; passed in isolation
- **Fix:** Assert comment row inside `waitFor` with the comments shell
- **Files modified:** `apps/web/src/routes/$owner.$repo.issues.integration.test.ts`
- **Verification:** 15/15 integration tests pass
- **Committed in:** `82c0763` (Task 2)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Stabilized suite; no scope creep.

## Issues Encountered

- SQLite second-resolution `updated_at` required a 1s sleep in the list sort assertion setup

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- List IA (D-ISS-16..18) ready for 11-12 polish / remaining Issues phase work
- STATE/ROADMAP intentionally not updated (executor instruction for this run)

## TDD Gate Compliance

- **RED:** `issue_list_filters_and_offset_pagination` failed on author filter (`total=3` vs expected `1`) — commit `f4524b1`
- **GREEN:** Filters + offset wired — commit `1ac0911`; full `issue_lifecycle | issue_list` nextest green
- **REFACTOR:** Skipped (no cleanup needed beyond GREEN)

## Self-Check: PASSED

- Found: `crates/oxidean-db/src/issues.rs`, `apps/web/src/components/repo/issues-list.tsrx`, `11-11-SUMMARY.md`
- Found commits: `f4524b1`, `1ac0911`, `82c0763`

---
*Phase: 11-issues*
*Completed: 2026-09-14*
