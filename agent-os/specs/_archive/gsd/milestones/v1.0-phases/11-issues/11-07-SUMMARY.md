---
phase: 11-issues
plan: "07"
subsystem: api
tags: [issues, assignees, rpc, acl, octane, iss-03, d-iss-06, d-iss-08, d-iss-07, d-iss-20]

requires:
  - phase: 11-issues
    provides: "0011_issues schema with issue_assignees (11-02)"
  - phase: 11-issues
    provides: "issue lifecycle + detail UI (11-04)"
  - phase: 11-issues
    provides: "labels Write+ assign pattern (11-06)"
  - phase: 11-issues
    provides: "Wave 0 issue_assignees RED stubs (11-00)"
provides:
  - "issue.assignees.set Write+ multi-assignee replace"
  - "issue.assigneeCandidates Read+-eligible profiles"
  - "IssueAssigneesPanel multi-select on issue detail"
affects:
  - 11-09 / 11-11 list filters by assignee
  - ISS-03 complete for assignees half

actuals:
  tokens: 10819
  tasks: 2
  commits: 3

plan_head_before: 4dca78f92926f3895e6045ce0076f20d589ee1a2

tech-stack:
  added: []
  patterns:
    - "Assignee eligibility via effective_capability ≥ Read (not user.lookup alone)"
    - "Write+ for set + candidates; soft repo.not_found for Read-only mutators"
    - "IssueAssigneesPanel mirrors IssueLabelsPanel checkbox save UX"

key-files:
  created:
    - apps/web/src/components/repo/issue-assignees-panel.tsrx
  modified:
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-api/tests/issue_assignees.rs
    - crates/oxidean-core/src/issue_types.rs
    - crates/oxidean-db/src/issue_labels.rs
    - crates/oxidean-db/src/lib.rs
    - packages/api-client/src/index.ts
    - apps/web/src/routes/$owner.$repo.issues.$n.tsrx
    - apps/web/src/routes/$owner.$repo.issues.integration.test.ts

key-decisions:
  - "Eligibility = Capability coalesce Read+ for each user_id (D-ISS-08 / T-11-12)"
  - "assigneeCandidates pools owner + org members + collaborators (+ prefix lookup), then filters Read+"
  - "UI loads candidates via issue.assigneeCandidates only — never user.lookup alone"
  - "Detail route file remains $owner.$repo.issues.$n.tsrx (plan path said $number)"

patterns-established:
  - "Nested issue.assignees.set + top-level issue.assigneeCandidates like labels.set"
  - "list_issue_assignees joins users for IssuePublic.assignees"
  - "Write+ sidebar multi-select with Save assignees"

requirements-completed: [ISS-03]

coverage:
  - id: D1
    description: "Write+ multi-assign + clear; Read cannot set; outsider without Read+ rejected"
    requirement: ISS-03
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_assignees)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "assigneeCandidates lists only Read+ eligible profiles"
    requirement: ISS-03
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_assignees_multi_assign)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Issue detail Write+ multi-assignee picker saves via assignees.set"
    requirement: ISS-03
    verification:
      - kind: integration
        ref: "apps/web vitest $owner.$repo.issues.integration.test.ts — Write+ multi-assignee picker"
        status: pass
    human_judgment: false

duration: 9min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 07: Multi-assignees + Read+ eligibility Summary

**Write+ can set multiple Read+-eligible assignees; `issue.assigneeCandidates` drives the detail sidebar picker with server-side eligibility (ISS-03 / D-ISS-06 / D-ISS-08).**

## Performance

- **Duration:** 9 min
- **Started:** 2026-09-14T15:55:34Z
- **Completed:** 2026-09-14T16:04:46Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments

- Greened `issue.assignees.set` / `issue.assigneeCandidates` with Capability Read+ checks (T-11-12)
- Populated `IssuePublic.assignees` from `issue_assignees` join
- Shipped `IssueAssigneesPanel` on issue detail (Write+ gated)

## Task Commits

1. **Task 1 RED:** `2a44394` — test(11-07): add failing tests for multi-assignee eligibility
2. **Task 1 GREEN:** `b6690f7` — feat(11-07): implement multi-assignee RPC with Read+ eligibility
3. **Task 2:** `5b096c7` — feat(11-07): add multi-assignee picker on issue detail

## Files Created/Modified

- `crates/oxidean-api/tests/issue_assignees.rs` — multi-assign / reject / unassign + candidates
- `crates/oxidean-api/src/issue/mod.rs` — assignees_set + assignee_candidates; assignees on to_public
- `crates/oxidean-db/src/issue_labels.rs` — `list_issue_assignees`
- `packages/api-client/src/index.ts` — generated client types/procedures
- `apps/web/src/components/repo/issue-assignees-panel.tsrx` — multi-select panel
- `apps/web/src/routes/$owner.$repo.issues.$n.tsrx` — wire panel

## Decisions Made

- Server eligibility via `effective_capability` ≥ Read for every assignee id (D-ISS-08)
- Candidates from access graph (owner/org members/collaborators) plus optional username prefix, still filtered by Read+
- Picker must call `issue.assigneeCandidates` — not `user.lookup` alone

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] DB helper lives in `issue_labels.rs`, not `issues.rs`**
- **Found during:** Task 1
- **Issue:** Plan listed `crates/oxidean-db/src/issues.rs`; assignees helpers already live beside labels
- **Fix:** Extended `issue_labels.rs` + `Database::list_issue_assignees` (existing pattern)
- **Files modified:** `crates/oxidean-db/src/issue_labels.rs`, `crates/oxidean-db/src/lib.rs`
- **Commit:** `b6690f7`

**2. [Rule 3 - Blocking] Detail route path is `$n`, not `$number`**
- **Found during:** Task 2
- **Issue:** Plan named `$owner.$repo.issues.$number.tsrx`
- **Fix:** Wired panel into existing `$owner.$repo.issues.$n.tsrx`
- **Files modified:** `apps/web/src/routes/$owner.$repo.issues.$n.tsrx`
- **Commit:** `5b096c7`

## Threat Flags

None — T-11-12 mitigated by Read+ validation on set; no new packages (T-11-SC).

## Known Stubs

None that block this plan. Linked PRs sidebar shell remains for later plans.

## Self-Check: PASSED

- FOUND: `apps/web/src/components/repo/issue-assignees-panel.tsrx`
- FOUND: `2a44394`, `b6690f7`, `5b096c7`
- FOUND: `cargo nextest` issue_assignees green; web build + issues integration tests green
