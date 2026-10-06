---
phase: 11-issues
plan: "05"
subsystem: api
tags: [issues, comments, rpc, acl, octane, write-preview, markdown, history, iss-02, d-iss-09, d-iss-10, d-iss-12, d-iss-20]

requires:
  - phase: 11-issues
    provides: "issue lifecycle CRUD + detail UI (11-04)"
  - phase: 11-issues
    provides: "0011_issues schema with issue_comments + comment_revisions (11-02)"
  - phase: 11-issues
    provides: "renderGfm sanitize-last pipeline (11-01)"
provides:
  - "issue.comments.list|create|update|delete|history RPC"
  - "Author edit/delete own; Write+ moderate-delete others (D-ISS-09)"
  - "Full comment body revision trail (D-ISS-12)"
  - "Shared MarkdownWritePreview + issue comment thread UI"
affects:
  - 11-08 reactions on comments
  - 11-10 remark-github autolink in Write|Preview

actuals:
  tokens: 21272
  tasks: 2
  commits: 3

plan_head_before: 9bada887d4462d3c4674fa68fa7f7bbeddb39772

tech-stack:
  added: []
  patterns:
    - "Comment revisions store pre-edit body; history oldest-first"
    - "Shared MarkdownWritePreview (Write|Preview) for new/edit/comment via renderGfm"
    - "Author-only comment edit; author OR Write+ delete (D-ISS-09)"

key-files:
  created:
    - apps/web/src/components/repo/markdown-write-preview.tsrx
    - apps/web/src/components/repo/issue-comments.tsrx
  modified:
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/src/issue/acl.rs
    - crates/oxidean-db/src/issues.rs
    - crates/oxidean-core/src/issue_types.rs
    - crates/oxidean-api/tests/issue_comments.rs
    - packages/api-client/src/index.ts
    - apps/web/src/routes/$owner.$repo.issues.$n.tsrx
    - apps/web/src/routes/$owner.$repo.issues.new.tsrx
    - apps/web/src/routes/$owner.$repo.issues.integration.test.ts

key-decisions:
  - "Comment edit is author-only; Write+ may only moderate-delete others (D-ISS-09)"
  - "Comment revision inserts use fractional created_at on SQLite/MySQL so rapid edits stay oldest-first"
  - "Detail route stays $n; MarkdownWritePreview extracted for new + edit + comments"

patterns-established:
  - "issue.comments.* nested client + Query/Mutation option helpers"
  - "issue.comment_not_found → HTTP 404"
  - "Comment history disclosure UI (Show/Hide edit history) per comment"

requirements-completed: [ISS-02]

coverage:
  - id: D1
    description: "Write+ create/list comments; author edit/delete own; Write+ moderate-delete others"
    requirement: ISS-02
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_comments)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "comments.history returns full body revision trail (D-ISS-12)"
    requirement: ISS-02
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_comments_edit_history_trail)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Write|Preview + comment thread UI on issue detail (D-ISS-10)"
    requirement: ISS-02
    verification:
      - kind: automated_ui
        ref: "apps/web/src/routes/$owner.$repo.issues.integration.test.ts#comment thread lists comments + Write|Preview compose"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 05: Comments + Write|Preview Summary

**Issue comments with author/Write+ moderation ACL, full body edit history, and shared Write|Preview markdown authoring (ISS-02 / D-ISS-09 / D-ISS-10 / D-ISS-12).**

## Performance

- **Duration:** 12 min
- **Started:** 2026-09-14T15:11:26Z
- **Completed:** 2026-09-14T15:23:30Z
- **Tasks:** 2
- **Files modified:** 15

## Accomplishments

- Shipped `issue.comments.list|create|update|delete|history` with soft ACL denies and full comment revision trail
- Extracted `MarkdownWritePreview` (renderGfm Preview) for new issue, edit, and comment compose
- Wired `IssueComments` thread (create/edit/delete + history disclosure) into issue detail

## Task Commits

1. **Task 1 RED: failing comment RPC tests** - `ce86304` (test)
2. **Task 1 GREEN: comments CRUD + history RPC** - `71f5838` (feat)
3. **Task 2: Comment thread + Write|Preview UI** - `4b75b4e` (feat)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Rapid comment edits ordered wrong under SQLite second-precision timestamps**
- **Found during:** Task 1 GREEN (history trail test)
- **Issue:** Two revisions in the same second sorted by UUID `id`, not insertion order
- **Fix:** Explicit fractional `created_at` on SQLite/MySQL comment revision inserts; SQLite `ORDER BY … rowid`
- **Files modified:** `crates/oxidean-db/src/issues.rs`
- **Commit:** `71f5838`

## Known Stubs

None — comment CRUD/history and Write|Preview/comment thread deliverables are wired (no placeholder UI stubs for this plan’s goal).

## Threat Flags

None beyond plan register (T-11-03, T-11-10, T-11-SC mitigated as designed).

## Self-Check: PASSED

- FOUND: `apps/web/src/components/repo/markdown-write-preview.tsrx`
- FOUND: `apps/web/src/components/repo/issue-comments.tsrx`
- FOUND: `crates/oxidean-api/tests/issue_comments.rs`
- FOUND commits: `ce86304`, `71f5838`, `4b75b4e`
