---
phase: 07-git-repos-browse
plan: "06"
subsystem: api
tags: [git, commits, blame, compare, diff, rpc, octane]

requires:
  - phase: 07-git-repos-browse
    provides: "ACL resolve_repo_for_read + tree/blob browse (07-05)"
  - phase: 07-git-repos-browse
    provides: "Repo chrome link-row + Code home UI patterns (07-15)"
provides:
  - "GitBackend log / show_commit / diff / blame via CLI argv"
  - "ACL RPC repo.commits / repo.commit / repo.compare / repo.blame"
  - "Octane routes for commits, commit detail, compare, blame (D-16 / GIT-05)"
affects:
  - 07-07-branch-crud
  - 07-18-branches-tags-ui

actuals:
  tokens: 23652
  tasks: 2
  commits: 3

plan_head_before: 25c8d3ecb301c828a5833075eaaf263b3c1dda55

tech-stack:
  added: []
  patterns:
    - "History RPCs reuse resolve_repo_for_read; soft DIFF/BLAME caps (D-20 / T-07-18)"
    - "Unified diff parsed into per-file patches; empty compare → empty:true not 500"
    - "Commits Load more via skip/limit; compare base...head splat"

key-files:
  created:
    - apps/web/src/routes/$owner.$repo.commits.$.tsrx
    - apps/web/src/routes/$owner.$repo.commit.$sha.tsrx
    - apps/web/src/routes/$owner.$repo.compare.$.tsrx
    - apps/web/src/routes/$owner.$repo.blame.$.tsrx
    - .planning/phases/07-git-repos-browse/.tdd/07-06-red-evidence.json
  modified:
    - crates/oxidean-git/src/backend.rs
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-core/src/repo_types.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - apps/web/src/lib/repo-browse.ts
    - apps/web/src/routes/$owner.$repo.blob.$.tsrx
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "RPC names repo.commits/commit/compare/blame (not repo.log/diff aliases)"
  - "Soft caps: 1 MiB unified patch bytes, 10k blame lines; no new crates"
  - "Compare empty copy exactly: Nothing to compare for these refs."

patterns-established:
  - "History surfaces: git lib unit tests (TDD) → ACL RPC → Octane route"
  - "Ref/sha validated before argv (reject NUL/.. / shell metacharacters)"

requirements-completed: [GIT-05]

coverage:
  - id: D1
    description: "Paged commit log for a ref via GitBackend + repo.commits"
    requirement: GIT-05
    verification:
      - kind: unit
        ref: "crates/oxidean-git/src/cli.rs#log_returns_paged_commit_summaries_for_ref"
        status: pass
    human_judgment: false
  - id: D2
    description: "Commit detail with unified per-file patches via show_commit + repo.commit"
    requirement: GIT-05
    verification:
      - kind: unit
        ref: "crates/oxidean-git/src/cli.rs#show_commit_returns_files_and_unified_patch"
        status: pass
    human_judgment: false
  - id: D3
    description: "Compare base...head returns empty result (not 500) when identical"
    requirement: GIT-05
    verification:
      - kind: unit
        ref: "crates/oxidean-git/src/cli.rs#diff_identical_refs_returns_empty_not_error"
        status: pass
    human_judgment: false
  - id: D4
    description: "Blame per-line meta for text files + UI blame route"
    requirement: GIT-05
    verification:
      - kind: unit
        ref: "crates/oxidean-git/src/cli.rs#blame_returns_per_line_meta_for_text_file"
        status: pass
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Long ref names in compare pickers truncate — held-out visual"
    requirement: GIT-05
    verification: []
    human_judgment: true
    rationale: "UI-SPEC marks long-text compare picker truncation as backstop visual"

duration: 10min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 06: History browse (commits/compare/blame) Summary

**Git history RPC + Octane routes: paged commits, commit diffs, compare, and blame with soft caps (remainder of D-16 / GIT-05)**

## Performance

- **Duration:** 10 min
- **Started:** 2026-09-12T18:12:20Z
- **Completed:** 2026-09-12T18:22:28Z
- **Tasks:** 2
- **Files modified:** 16

## Accomplishments

- `GitBackend::{log,show_commit,diff,blame}` on `CliGitBackend` with argv-only git and soft payload caps
- ACL-backed `repo.commits` / `repo.commit` / `repo.compare` / `repo.blame` + rpc-gen client types
- Browse UI: commits list (Load more), commit detail unified diffs, compare empty copy, blame gutter → commit; Blame link from blob

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: failing history backend tests** - `74d78b8` (test)
2. **Task 1 GREEN: log/show_commit/diff/blame RPC** - `0840f5b` (feat)
3. **Task 2: commits/commit/compare/blame UI** - `3f68732` (feat)

**Plan metadata:** `02b3c6b` (docs: complete plan)

_Note: TDD tasks may have multiple commits (test → feat → refactor)_

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `74d78b8` | `.tdd/07-06-red-evidence.json` — `cli::tests::log_returns_paged_commit_summaries_for_ref` assertion failure (`expected at least 2 commits, got 0`); `gsd_run check tdd-red-evidence` → `RED_EVIDENCE_OK` |
| GREEN | `0840f5b` | Same four history unit tests pass |

## Files Created/Modified

- `crates/oxidean-git/src/backend.rs` — Commit/Diff/Blame types + trait methods
- `crates/oxidean-git/src/cli.rs` — CLI implementations + unit tests
- `crates/oxidean-core/src/repo_types.rs` — History DTOs
- `crates/oxidean-api/src/repo/mod.rs` / `rpc.rs` — ACL handlers + dispatch
- `packages/api-client/src/index.ts` — Generated client
- `apps/web/src/routes/$owner.$repo.commits.$.tsrx` — Commits list
- `apps/web/src/routes/$owner.$repo.commit.$sha.tsrx` — Commit detail
- `apps/web/src/routes/$owner.$repo.compare.$.tsrx` — Compare view
- `apps/web/src/routes/$owner.$repo.blame.$.tsrx` — Blame view
- `apps/web/src/lib/repo-browse.ts` — History href helpers
- `apps/web/src/routes/$owner.$repo.blob.$.tsrx` — Blame entry link

## Decisions Made

- Exposed history as `repo.commits` / `repo.commit` / `repo.compare` / `repo.blame` (matches UI IA; satisfies plan acceptance regex)
- Soft caps aligned with blob: 1 MiB patch budget, 10k blame lines; no new packages (T-07-SC)
- Compare empty state uses UI-SPEC string “Nothing to compare for these refs.”

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Octane `@for` requires `const` binding**
- **Found during:** Task 2 (web build)
- **Issue:** `@for (c of …)` crashed the Octane compiler (`Cannot read properties of undefined`)
- **Fix:** Use `@for (const c of …; key …)` matching existing routes
- **Files modified:** four history route `.tsrx` files
- **Verification:** `bun run build` in `apps/web` exit 0
- **Committed in:** `3f68732`

---

**Total deviations:** 1 auto-fixed (blocking)
**Impact on plan:** Required for build; no scope creep.

## Issues Encountered

- Pre-existing failing `git_archive_formats_zip_and_tar_gz` and Wave 0 `repo_branch_soft_protect_*` stubs — out of scope; noted in `deferred-items.md`

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- History browse complete for GIT-05 beyond tree/blob
- Branch CRUD (07-07) and Branches/Tags UI (07-18) can proceed independently

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*

## Self-Check: PASSED
