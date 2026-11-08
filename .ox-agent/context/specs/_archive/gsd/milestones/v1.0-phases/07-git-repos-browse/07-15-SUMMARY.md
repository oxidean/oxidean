---
phase: 07-git-repos-browse
plan: "15"
subsystem: ui
tags: [octane, code-browse, tree, blob, README, syntax-highlight, D-15, D-17, D-25, GIT-05]
requires:
  - phase: 07-git-repos-browse
    provides: "ACL-safe repo.get/tree/blob/refs + raw (07-05); renderGfm + Shiki (07-14); empty Quick setup stub (07-13)"
provides:
  - "Code home with empty Quick setup vs dirs-first tree + sanitized README"
  - "/tree/{ref}/… and /blob/{ref}/… Octane splat routes"
  - "Identical Page not found for missing and private (D-25)"
  - "owner/repo integration coverage empty/tree/404"
affects:
  - 07-06-history-blame
  - 07-08-clone-download
  - 07-18-branches-tags
plan_head_before: 121fa98779450b5962a00e8334412ba0ad8aad95
actuals:
  tokens: 16740
  tasks: 1
  commits: 2
commits: 2
tech-stack:
  added: []
  patterns:
    - "Client browse phase machine: loading | not_found | empty | ready | error"
    - "Splat parseRefAndPath for /tree/$ and /blob/$ under /$owner/$repo"
    - "sortTreeEntries: tree/commit before blob"
key-files:
  created:
    - apps/web/src/routes/$owner.$repo.tree.$.tsrx
    - apps/web/src/routes/$owner.$repo.blob.$.tsrx
    - apps/web/src/routes/$owner.$repo.integration.test.ts
    - apps/web/src/components/repo/file-tree.tsrx
    - apps/web/src/components/repo/blob-viewer.tsrx
    - apps/web/src/components/repo/repo-not-found.tsrx
    - apps/web/src/lib/repo-browse.ts
  modified:
    - apps/web/src/routes/$owner.$repo.index.tsrx
    - apps/web/src/routeTree.gen.ts
    - packages/api-client/src/index.ts
    - apps/web/vite.config.ts
key-decisions:
  - "Soft-render RepoNotFound for repo.not_found (same copy as missing) instead of only throwing notFound()"
  - "Clone/Download remains stub copy until 07-08; Commits/Branches/Tags nav links reserved for later plans"
  - "No new npm packages — consume 07-14 markdown/highlight libs (T-07-SC)"
patterns-established:
  - "apps/web/src/components/repo/* shared chrome/tree/blob pieces for browse surfaces"
  - "repoGet/tree/blob/refs queryOptions helpers on api-client for future Query wiring"
requirements-completed: [GIT-05]
coverage:
  - id: D1
    description: "Empty repo Code home shows Quick setup (not tree)"
    requirement: GIT-05
    verification:
      - kind: integration
        ref: "apps/web/src/routes/$owner.$repo.integration.test.ts#empty repo Code home shows Quick setup (not tree)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Populated Code home lists directories first"
    requirement: GIT-05
    verification:
      - kind: integration
        ref: "apps/web/src/routes/$owner.$repo.integration.test.ts#tree route lists dirs first when commits exist"
        status: pass
    human_judgment: false
  - id: D3
    description: "Private/missing shows identical Page not found (D-25)"
    requirement: GIT-05
    verification:
      - kind: integration
        ref: "apps/web/src/routes/$owner.$repo.integration.test.ts#private non-owner (or missing) shows Not found — same copy"
        status: pass
    human_judgment: false
  - id: D4
    description: "Tree and blob splat routes register and web build is green"
    requirement: GIT-05
    verification:
      - kind: other
        ref: "bun --cwd apps/web run build + test -f tree/blob routes"
        status: pass
    human_judgment: false
  - id: D5
    description: "Blob highlight + README sanitize visual check"
    requirement: GIT-05
    verification: []
    human_judgment: true
    rationale: "Held-out visual — open seeded .ts/.tsrx blob and README with script attempt"
duration: 10min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 15: Code / tree / blob Octane routes Summary

**GIT-05 Code browse UI: empty Quick setup, dirs-first tree + sanitized README, tree/blob splats, identical private/missing 404**

## Performance

- **Duration:** 10 min
- **Started:** 2026-09-12T18:00:41Z
- **Completed:** 2026-09-12T18:11:04Z
- **Tasks:** 1
- **Files modified:** 15

## Accomplishments

- Replaced empty-only Code stub with API-backed empty vs populated browse
- Added `/$owner/$repo/tree/$` and `/$owner/$repo/blob/$` with ref Select, breadcrumbs, highlight, large-file/raw links
- Anti-enumeration UI: `repo.not_found` → same **Page not found** copy as missing (D-25)

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: failing Code/tree/404 integration coverage** — `6421ea0` (test)
2. **Task 1 GREEN: Code/tree/blob browse UI** — `2ebd4f6` (feat)

_Note: TDD RED→GREEN; optional REFACTOR skipped (no cleanup needed)._

## TDD Gate Compliance

| Gate | Commit | Result |
|------|--------|--------|
| RED | `6421ea0` | Intentional fails: tree dirs-first + Page not found; evidence `RED_EVIDENCE_OK` |
| GREEN | `2ebd4f6` | All 3 integration tests pass; `bun run build` green |

## Files Created/Modified

- `apps/web/src/routes/$owner.$repo.index.tsrx` — Code home phase machine
- `apps/web/src/routes/$owner.$repo.tree.$.tsrx` — tree browse
- `apps/web/src/routes/$owner.$repo.blob.$.tsrx` — blob browse
- `apps/web/src/routes/$owner.$repo.integration.test.ts` — empty/tree/404
- `apps/web/src/components/repo/*` — chrome, tree, blob, README, Quick setup, 404
- `apps/web/src/lib/repo-browse.ts` — sort/path/hash/raw helpers
- `packages/api-client/src/index.ts` — repo get/tree/blob/refs queryOptions
- `apps/web/vite.config.ts` — proxy `/api/repos` for raw downloads
- `apps/web/src/routeTree.gen.ts` — register tree/blob children

## Decisions Made

- Soft `RepoNotFound` page for ACL misses so integration tests assert identical copy without full router notFound plumbing
- Clone/Download deferred to 07-08 (stub muted line on toolbar)
- Submodules shown as gitlink rows (Package icon + short oid); tree navigation attempted when path resolvable

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Adjacent Rivet siblings rejected by Octane compiler**
- **Found during:** Task 1 GREEN
- **Issue:** Multiple root nodes under `@else` failed compile (`Adjacent JSX elements must be wrapped`)
- **Fix:** Wrapped ready-state trees in `<>…</>` fragments on index/tree/blob routes
- **Files modified:** `$owner.$repo.index.tsrx`, `.tree.$.tsrx`, `.blob.$.tsrx`
- **Commit:** `2ebd4f6`

**2. [Rule 2 - Critical] Vite proxy missing for raw blob downloads**
- **Found during:** Task 1 GREEN
- **Issue:** Download/View raw links hit `/api/repos/.../raw/...` with no dev proxy
- **Fix:** Added `/api/repos` proxy entry in `vite.config.ts`
- **Commit:** `2ebd4f6`

## Known Stubs

| Location | Stub | Reason |
|----------|------|--------|
| Code toolbar | “Clone / Download arrives in a later phase.” | Plan allows stub until 07-08 |
| Submodule click | Links to tree path; may 404 if not resolvable | No submodule URL in API yet |

## Threat Flags

None — no new packages; browse reads reuse 07-05 ACL; README HTML via 07-14 sanitize.

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/$owner.$repo.tree.$.tsrx`
- FOUND: `apps/web/src/routes/$owner.$repo.blob.$.tsrx`
- FOUND: `apps/web/src/routes/$owner.$repo.integration.test.ts`
- FOUND: commit `6421ea0`
- FOUND: commit `2ebd4f6`
