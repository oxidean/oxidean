---
phase: 11-issues
plan: "00"
subsystem: testing
tags: [wave0, nyquist, issues, nextest, dialect, iss-01, iss-02, iss-03, iss-04]

requires:
  - phase: 10-orgs-permissions
    provides: Wave 0 assert!(false) nextest stub pattern + repo_private_404 harness to extend
provides:
  - "Wave 0 RED issue_* lifecycle/delete/comments/labels/assignees/links/reactions stubs (ISS-01..04)"
  - "Wave 0 dialect_issues stub for 0011_issues tri-dialect"
  - "Wave 0 private issue soft not-found stub names (D-ISS-20)"
  - "Wave 0 factory_reset_issues stub for cascade wipe"
affects:
  - 11-02 issue schema migration 0011_issues
  - 11-03+ issue RPC greens
  - 11-xx factory reset + private ACL greens

actuals:
  tokens: 4465
  tasks: 2
  commits: 7

plan_head_before: 40d6cd5cee78211cc51e66e1b6c9170910c90fc9

tech-stack:
  added: []
  patterns:
    - "Wave 0 intentional RED stubs with assert!(false) until issue RPC/schema land"
    - "issue_ / dialect_issues / factory_reset_issues / issue_private nextest filters mirror 11-VALIDATION.md Wave 0"
    - "Migration number 0011_issues (after 0010_orgs_acl)"

key-files:
  created:
    - crates/oxidean-api/tests/issue_lifecycle.rs
    - crates/oxidean-api/tests/issue_delete.rs
    - crates/oxidean-api/tests/issue_comments.rs
    - crates/oxidean-api/tests/issue_labels.rs
    - crates/oxidean-api/tests/issue_assignees.rs
    - crates/oxidean-api/tests/issue_links.rs
    - crates/oxidean-api/tests/issue_reactions.rs
    - crates/oxidean-db/tests/dialect_issues.rs
    - crates/oxidean-db/tests/factory_reset_issues.rs
  modified:
    - crates/oxidean-api/tests/repo_private_404.rs
    - crates/oxidean-api/tests/git_ssh.rs

key-decisions:
  - "Wave 0 is RED-only — no production issue RPC, migrations, or UI"
  - "Use 0011_issues after 0010_orgs_acl"
  - "Private issue ACL stubs encode soft not_found (D-ISS-20 / T-11-01)"
  - "Admin hard-delete + label defs stubs encode T-11-02 capability split"

patterns-established:
  - "Nyquist Wave 0 for Phase 11: failing nextest paths exist before issue implementation waves"

requirements-completed: []  # Wave 0 scaffolds only; greens land in later 11-xx plans

coverage:
  - id: D1
    description: "API Wave 0 stubs for create/edit/close/reopen + history"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(issue_lifecycle) | test(issue_history)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "API Wave 0 stubs for Admin hard-delete + no #N reuse"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(issue_delete)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "API Wave 0 stubs for comments CRUD/moderation/history"
    requirement: ISS-02
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(issue_comments)'"
        status: pass
    human_judgment: false
  - id: D4
    description: "API Wave 0 stubs for labels + assignees"
    requirement: ISS-03
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(issue_labels) | test(issue_assignees)'"
        status: pass
    human_judgment: false
  - id: D5
    description: "API Wave 0 stubs for link stubs + reactions"
    requirement: ISS-04
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(issue_links) | test(issue_reactions)'"
        status: pass
    human_judgment: false
  - id: D6
    description: "dialect_issues expects tri-dialect 0011_issues"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-db -E 'test(dialect_issues)'"
        status: pass
    human_judgment: false
  - id: D7
    description: "Private issue soft not-found + factory_reset_issues stubs"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(repo_private) | test(issue_private)'; cargo nextest list -p oxidean-db -E 'test(factory_reset)'"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 00: Issues Wave 0 Nyquist Stubs Summary

**Discoverable RED nextest stubs for issue lifecycle, comments, labels, assignees, links, reactions, dialect 0011_issues, private ACL, and factory_reset cascade — no production issue behavior.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-14T14:12:49Z
- **Completed:** 2026-09-14T14:16:38Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments

- Scaffolded Wave 0 `assert!(false)` API stubs matching `test(issue_)` filters in 11-VALIDATION.md
- Added `dialect_issues` tri-dialect expectation for `0011_issues` (issues, counters, comments, revisions, labels, assignees, reactions, links)
- Extended `repo_private_404` with unauthorized private `issue.list` / `issue.get` soft not-found stubs (D-ISS-20 / T-11-01)
- Added `factory_reset_issues` stubs for repository-cascade wipe of issue domain tables

## Task Commits

Each task was committed atomically:

1. **Task 1: Issue domain Wave 0 API + dialect stubs** - `007956d` (test)
2. **Task 2: Private ACL + factory_reset issue stub names** - `035208d` (test)

**Plan metadata:** `eb41dc6` … `fdc8b59`+ (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-api/tests/issue_lifecycle.rs` — create/edit/close/reopen + history stubs
- `crates/oxidean-api/tests/issue_delete.rs` — Admin hard-delete + confirmNumber + no reuse
- `crates/oxidean-api/tests/issue_comments.rs` — comment CRUD/moderation/history
- `crates/oxidean-api/tests/issue_labels.rs` — Admin defs + Write+ assign
- `crates/oxidean-api/tests/issue_assignees.rs` — multi-assignee + Read+ eligibility
- `crates/oxidean-api/tests/issue_links.rs` — stub links + no closing-keyword enforcement
- `crates/oxidean-api/tests/issue_reactions.rs` — GitHub eight content values
- `crates/oxidean-db/tests/dialect_issues.rs` — 0011_issues tri-dialect stub
- `crates/oxidean-db/tests/factory_reset_issues.rs` — cascade wipe stub
- `crates/oxidean-api/tests/repo_private_404.rs` — private issue soft not-found stubs
- `crates/oxidean-api/tests/git_ssh.rs` — Rule 3 arity fix for `insert_repository` owner_type

## Decisions Made

- Wave 0 RED-only; production issue RPC/migrations/UI deferred to later 11-xx plans
- Migration id locked as `0011_issues` after `0010_orgs_acl`
- Soft not-found stubs for unauthorized private issue get/list (D-ISS-20 / T-11-01)
- Capability stubs: Admin for label defs + hard-delete; Write+ for assign/comment/react (T-11-02)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed git_ssh insert_repository arity for nextest list**
- **Found during:** Task 1 verification (`cargo nextest list -p oxidean-api`)
- **Issue:** Pre-existing `git_ssh.rs` called `insert_repository` with 6 args after Phase 10 added `owner_type`, blocking compile of all `oxidean-api` test targets and thus Wave 0 discovery
- **Fix:** Pass `"user"` as `owner_type` at three call sites
- **Files modified:** `crates/oxidean-api/tests/git_ssh.rs`
- **Commit:** `007956d`

## Known Stubs

Intentional Wave 0 RED stubs (`assert!(false)` until later plans green them):

| File | Pattern | Reason |
|------|---------|--------|
| `crates/oxidean-api/tests/issue_*.rs` | `assert!(false, "Wave 0: …")` | Nyquist stubs; greens in 11-02+ |
| `crates/oxidean-db/tests/dialect_issues.rs` | empty migration + final `assert!(false)` | 0011_issues not shipped yet |
| `crates/oxidean-db/tests/factory_reset_issues.rs` | `assert!(false)` | cascade wipe not wired yet |
| `crates/oxidean-api/tests/repo_private_404.rs` (new issue_* cases) | `assert!(false)` | private issue ACL greens later |

## Threat Flags

None beyond plan register — stubs only encode T-11-01 / T-11-02 expectations; no new network endpoints.

## Self-Check: PASSED

- All Wave 0 stub files present
- Commits `007956d` and `035208d` present on `feat/forge-core`
- nextest list discovers `issue_`, `dialect_issues`, `factory_reset`, `repo_private` / `issue_private`
