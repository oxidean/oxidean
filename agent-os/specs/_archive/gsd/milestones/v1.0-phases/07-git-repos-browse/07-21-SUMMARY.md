---
phase: 07-git-repos-browse
plan: "21"
subsystem: git
tags: [repo-create, soft-delete, parseRefAndPath, hierarchical-refs, tdd, gap-closure, WR-01, WR-03, D-17]
requires:
  - phase: 07-git-repos-browse
    provides: "repo.create insert+init_bare/seed_commit (07-02/03); first-segment parseRefAndPath browse routes"
provides:
  - "create git failure soft-deletes row + removes partial bare path (WR-01)"
  - "parseRefAndPath longest-prefix knownRefs for slashy branches (WR-03 / D-17)"
  - "tree/blob/blame load refs before splat parse"
affects: [07-verify, browse-hierarchical-refs]
actuals:
  tokens: 3968
  tasks: 2
  commits: 4
plan_head_before: f7fe6f89700c69b32989738997a8c7bd955fb5fe
tech-stack:
  added: []
  patterns:
    - "Compensate failed create: soft_delete_repository + best-effort path remove"
    - "Client splat parse: longest known-ref prefix then path remainder; head() stays first-segment"
key-files:
  created:
    - apps/web/src/lib/repo-browse.unit.test.ts
    - .planning/phases/07-git-repos-browse/.tdd/07-21-t1-red-evidence.json
    - .planning/phases/07-git-repos-browse/.tdd/07-21-t2-red-evidence.json
  modified:
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/tests/repo_create.rs
    - apps/web/src/lib/repo-browse.ts
    - apps/web/src/routes/$owner.$repo.tree.$.tsrx
    - apps/web/src/routes/$owner.$repo.blob.$.tsrx
    - apps/web/src/routes/$owner.$repo.blame.$.tsrx
key-decisions:
  - "compensate_failed_create soft-deletes then remove_dir_all with remove_file fallback for blocking non-dir paths"
  - "Route bodies fetch repo.refs before parseRefAndPath; document.head() keeps first-segment-only (refs not loaded yet)"
patterns-established:
  - "WR-01: never leave a live name-blocking row after insert+git failure"
  - "WR-03/D-17: hierarchical browse URLs require knownRefs list at parse time"
requirements-completed: [GIT-01, GIT-05]
coverage:
  - id: D1
    description: "Failed create soft-deletes DB row so the same name can be recreated (WR-01)"
    requirement: GIT-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_create.rs#repo_create_git_failure_soft_deletes_row_allows_recreate"
        status: pass
    human_judgment: false
  - id: D2
    description: "parseRefAndPath longest-prefix hierarchical refs with knownRefs; first-segment fallback preserved"
    requirement: GIT-05
    verification:
      - kind: unit
        ref: "apps/web/src/lib/repo-browse.unit.test.ts#resolves longest matching hierarchical ref prefix (WR-03 / D-17)"
        status: pass
      - kind: other
        ref: "rg parseRefAndPath(..., knownRefs) in tree/blob/blame routes"
        status: pass
    human_judgment: false
duration: 4min
completed: "2026-09-12"
status: complete
---

# Phase 07 Plan 21: Create compensate + hierarchical parseRefAndPath Summary

**Soft-delete on create git failure so names are reusable (WR-01), and longest-prefix `parseRefAndPath` for slashy branch URLs (WR-03 / D-17)**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-12T19:54:46Z
- **Completed:** 2026-09-12T19:58:33Z
- **Tasks:** 2
- **Files modified:** 7 (+ RED evidence)

## Accomplishments

- Create path compensates `init_bare` / `seed_commit` failure with `soft_delete_repository` + best-effort bare-path cleanup so recreate succeeds
- `parseRefAndPath(splat, knownRefs?)` resolves longest matching hierarchical ref; empty splat and first-segment fallback unchanged
- Tree / blob / blame pages load `repo.refs` then parse splat with short ref names before tree/blob/blame RPC

## Task Commits

Each task was committed atomically (TDD RED → GREEN):

1. **Task 1 RED:** `cb07dea` (test) — create git-failure soft-delete recreate test
2. **Task 1 GREEN:** `d5d3de6` (feat) — compensate soft-delete on create git failure
3. **Task 2 RED:** `4f5a7cf` (test) — parseRefAndPath longest-prefix unit tests
4. **Task 2 GREEN:** `8090052` (feat) — longest-prefix parse + route wiring

**Plan metadata:** (docs commit after this SUMMARY)

## TDD Gate Compliance

- Task 1: RED evidence `07-21-t1-red-evidence.json` → `RED_EVIDENCE_OK`; GREEN test exact pass
- Task 2: RED evidence `07-21-t2-red-evidence.json` → `RED_EVIDENCE_OK`; vitest 6/6 pass
- Tracer (Task 1): `HUMAN_VERIFY_MODE=end-of-phase` + automated-only verify → re-ran exact cargo test, ⚡ expanding

## Files Created/Modified

- `crates/oxidean-api/src/repo/mod.rs` — `compensate_failed_create` on init/seed Err
- `crates/oxidean-api/tests/repo_create.rs` — WR-01 recreate integration test
- `apps/web/src/lib/repo-browse.ts` — optional `knownRefs` longest-prefix parse
- `apps/web/src/lib/repo-browse.unit.test.ts` — hierarchical + fallback unit coverage
- `apps/web/src/routes/$owner.$repo.tree.$.tsrx` — refs-first then `parseRefAndPath(splat, knownRefs)`
- `apps/web/src/routes/$owner.$repo.blob.$.tsrx` — same
- `apps/web/src/routes/$owner.$repo.blame.$.tsrx` — same

## Decisions Made

- **Head metadata stays first-segment:** `head()` on tree/blob/blame still calls `parseRefAndPath(splat)` without known refs because refs are not loaded in the head hook; page bodies re-parse after `repo.refs` for correct browse.
- **Compensate removes file or dir:** blocking non-directory at bare path is removed via `remove_file` fallback so recreate is not stuck on disk either.

## Deviations from Plan

None - plan executed exactly as written.

Minor test tweak after GREEN: tolerate blocker already removed by compensate before recreate (still asserts soft-delete + successful recreate).

---

**Total deviations:** 0 auto-fixed (1 small test hardening, not a Rule 1–3 deviation)
**Impact on plan:** None

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- WR-01 / WR-03 / D-17 closed for gap pass; phase 07 plans complete after this SUMMARY
- Ready for `/gsd-verify-work` / milestone closeout

## Self-Check: PASSED

- Key files present on disk
- Commits `cb07dea`, `d5d3de6`, `4f5a7cf`, `8090052` in git log
- `cargo test … repo_create_git_failure_soft_deletes_row_allows_recreate -- --exact` pass
- `bunx vitest run src/lib/repo-browse.unit.test.ts` pass; `knownRefs` wiring in all three routes

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
