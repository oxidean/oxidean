---
phase: 11-issues
plan: "03"
subsystem: api
tags: [issues, rpc, acl, octane, tracer, iss-01, d-iss-01, d-iss-16, d-iss-19, d-iss-20]

requires:
  - phase: 11-issues
    provides: "0011_issues schema + issue_counters + IssuePublic DTOs (11-02)"
  - phase: 10-orgs-permissions
    provides: "Capability Read/Write/Admin + resolve_repo_for_read soft not_found"
provides:
  - "issue.create / issue.get / issue.list RPC with Write/Read ACL"
  - "Per-repo #N allocation via insert_issue txn"
  - "Issues tab + /issues list/new/$n Octane routes"
  - "Generated api-client issue.* + Query helpers"
affects:
  - 11-04 issue edit/close/reopen/history
  - 11-05+ labels/assignees/comments
  - 11-11 advanced list filters

actuals:
  tokens: 20171
  tasks: 3
  commits: 5

plan_head_before: 202adf9dc62c6deb2fea377304a88f7dfddfb5a7

tech-stack:
  added: []
  patterns:
    - "issue.* top-level RPC namespace (not repo.issues.*)"
    - "resolve_repo_for_read + meets(Write) for create; soft repo.not_found for private deny"
    - "TanStack Query + apiClient for Issues UI; SSR cookie-forward fetchIssueList/Get"

key-files:
  created:
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/src/issue/acl.rs
    - apps/web/src/components/repo/issues-list.tsrx
    - apps/web/src/routes/$owner.$repo.issues.tsrx
    - apps/web/src/routes/$owner.$repo.issues.new.tsrx
    - apps/web/src/routes/$owner.$repo.issues.$n.tsrx
  modified:
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-db/src/issues.rs
    - packages/api-client/src/index.ts
    - apps/web/src/components/repo/repo-chrome.tsrx
    - apps/web/src/routes/$owner.$repo.issues.integration.test.ts

key-decisions:
  - "Detail route param is $n (Wave 0 test lock) not $number from plan file list"
  - "List ships Open/Closed/All tabs now; advanced author/label/assignee/q filters still 11-11"
  - "Detail Comments/Labels/Assignees/Linked PRs are empty shells for later plans"

patterns-established:
  - "issue ACL mirrors repo: unauthorized private → identical repo.not_found"
  - "Title ≤1024 / body ≤65536 chars with rpc.bad_input"

requirements-completed: [ISS-01]

coverage:
  - id: D1
    description: "Write+ create allocates per-repo #1 then #2; second repo starts at #1; get/list return created issues"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_lifecycle_create) | test(issue_lifecycle_second)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Unauthorized private issue.list/get → soft repo.not_found (D-ISS-20 / T-11-01)"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(repo_private_404_issue) | test(issue_private)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Issues tab + Open list + New issue Write gate + detail title/shells"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "bun --cwd apps/web run test:integration -- src/routes/$owner.$repo.issues.integration.test.ts"
        status: pass
      - kind: other
        ref: "bun --cwd apps/web run build"
        status: pass
    human_judgment: false

duration: 16min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 03: Issue create/get/list tracer + Issues UI Summary

**End-to-end issue.create → per-repo `#N` → get/list with soft private ACL, plus Issues tab and list/new/detail Octane routes (ISS-01 tracer).**

## Performance

- **Duration:** ~16 min
- **Started:** 2026-09-14T14:37:20Z
- **Completed:** 2026-09-14T14:53:15Z
- **Tasks:** 3/3
- **Files modified:** 20

## Accomplishments

- Wired `issue.create` / `issue.get` / `issue.list` through ACL + `issue_counters` txn allocate
- Regenerated `@oxidean/api-client` via `make rpc-gen` (rpc-sync clean)
- Shipped Issues chrome tab and list (Open default) / new (Write|Preview) / detail routes with empty later-panel shells

## Task Commits

1. **Task 1 RED:** `20382a9` — test(11-03): RED issue.create/get/list + private soft-not-found
2. **Task 1 GREEN:** `38e32ef` — feat(11-03): issue.create/get/list RPC with ACL and rpc-gen
3. **Task 1 fix:** `8253efe` — fix(11-03): assert issue.create via JSON ok not HTTP status
4. **Task 2:** `802e755` — feat(11-03): Issues tab + list/new/detail Octane routes
5. **Task 3:** `99110c8` — test(11-03): green Issues tracer integration assertions

**Plan metadata:** (docs commit after this SUMMARY)

## Files Created/Modified

- `crates/oxidean-api/src/issue/mod.rs` — create/get/list handlers
- `crates/oxidean-api/src/issue/acl.rs` — Read/Write resolve helpers
- `crates/oxidean-db/src/issues.rs` — find_by_repo_number + list_for_repo
- `apps/web/src/routes/$owner.$repo.issues*.tsrx` — list/new/detail
- `apps/web/src/components/repo/repo-chrome.tsrx` — Issues tab
- `apps/web/src/components/repo/issues-list.tsrx` — Open/Closed/All + New issue

## Decisions Made

- Used `$n` for detail route param (matches Wave 0 integration import + useParams mock)
- Included Open/Closed/All state tabs in tracer UI; deferred author/label/assignee/q filters to 11-11
- Detail ships empty Comments/Labels/Assignees/Linked PRs shells so tracer detail assertions can pass without feature work

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] DB find/list helpers**
- **Found during:** Task 1
- **Issue:** Schema plan only shipped `insert_issue` / `find_by_id`; get/list need number + filtered list
- **Fix:** Added `find_by_repo_number` + `list_for_repo` (open|closed|all, newest-updated first) + Database wrappers
- **Files modified:** `crates/oxidean-db/src/issues.rs`, `crates/oxidean-db/src/lib.rs`
- **Committed in:** `38e32ef`

**2. [Rule 1 - Bug] Detail route filename `$n` vs plan `$number`**
- **Found during:** Task 2
- **Issue:** Plan listed `$owner.$repo.issues.$number.tsrx` but Wave 0 tests import `$owner.$repo.issues.$n`
- **Fix:** Implemented `$n` to match locked Wave 0 stubs
- **Files modified:** `apps/web/src/routes/$owner.$repo.issues.$n.tsrx`
- **Committed in:** `802e755`

**Total deviations:** 2 auto-fixed (Rule 2 ×1, Rule 1 ×1)
**Impact on plan:** Required for correctness / test lock; no scope creep beyond tracer.

## Issues Encountered

None beyond the deviations above. Wave 0 `issue_lifecycle` edit/close/history stubs remain intentionally RED for 11-04.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Ready for 11-04 (edit/close/reopen/history) and subsequent comments/labels plans. Tracer path is green.

## Known Stubs

| File | Stub | Reason |
|------|------|--------|
| `$owner.$repo.issues.$n.tsrx` | Comments/Labels/Assignees/Linked PRs empty shells | Intentional until 11-05+ / 11-09 |
| `issue_lifecycle.rs` | edit/close/history assert!(false) | Deferred to 11-04 |
| `issues.integration.test.ts` | it.fails labels admin, edit Write\|Preview, lifecycle, reactions | Deferred to later plans |

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/issue/mod.rs`
- FOUND: `apps/web/src/routes/$owner.$repo.issues.tsrx`
- FOUND: `apps/web/src/routes/$owner.$repo.issues.new.tsrx`
- FOUND: `apps/web/src/routes/$owner.$repo.issues.$n.tsrx`
- FOUND commits: `20382a9`, `38e32ef`, `8253efe`, `802e755`, `99110c8`
