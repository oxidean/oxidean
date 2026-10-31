---
phase: 07-git-repos-browse
plan: "20"
subsystem: git
tags: [git-archive, argv-injection, validate-ref, tdd, gap-closure, CR-01, WR-02]
requires:
  - phase: 07-git-repos-browse
    provides: "validate_treeish leading-hyphen reject + branch -- markers (07-19)"
provides:
  - "git archive argv places -- before treeish (CR-01)"
  - "validate_archive_treeish rejects leading hyphen at HTTP boundary"
  - "validate_ref allows / and rejects leading - (WR-02)"
  - "repo_archive_rejects_option_like_treeish_no_output_file regression"
affects: [07-21-gap-closure, git-download-security]
actuals:
  tokens: 2916
  tasks: 2
  commits: 5
plan_head_before: fb1aa482e54aa4ea961f061bd194e186f7cf2257
tech-stack:
  added: []
  patterns:
    - "Defense in depth: HTTP leading- hyphen reject + git -- before revision operand"
    - "Raw validate_ref slash parity with archive treeish (WR-02)"
key-files:
  created:
    - .planning/phases/07-git-repos-browse/.tdd/07-20-red-evidence.json
    - .planning/phases/07-git-repos-browse/.tdd/07-20-t2-red-evidence.json
  modified:
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-api/src/routes/repo_raw.rs
    - crates/oxidean-api/tests/repo_archive.rs
key-decisions:
  - "RED asserts HTTP-boundary message 'invalid ref' so intentional fail remains after 07-19 CLI validate_treeish already blocked --output file writes"
  - "validate_ref mirrors archive metachar + leading- hyphen policy while allowing /"
patterns-established:
  - "CR-01: never place user treeish in git archive option position; reject option-like refs at HTTP first"
requirements-completed: [GIT-07, GIT-05]
coverage:
  - id: D1
    description: "GET archive with --output= treeish returns repo.invalid_ref and creates no monitored file"
    requirement: GIT-07
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_archive.rs#repo_archive_rejects_option_like_treeish_no_output_file"
        status: pass
    human_judgment: false
  - id: D2
    description: "CliGitBackend::archive places end-of-options before treeish; validate_archive_treeish rejects leading -"
    requirement: GIT-07
    verification:
      - kind: unit
        ref: "crates/oxidean-git/src/cli.rs#archive argv -- before treeish"
        status: pass
      - kind: integration
        ref: "cargo test -p oxidean-api --test repo_archive repo_archive_rejects_option_like_treeish_no_output_file -- --exact"
        status: pass
    human_judgment: false
  - id: D3
    description: "validate_ref allows hierarchical / refs and rejects leading hyphen (WR-02)"
    requirement: GIT-05
    verification:
      - kind: unit
        ref: "crates/oxidean-api/src/routes/repo_raw.rs#validate_ref_tests"
        status: pass
      - kind: integration
        ref: "cargo test -p oxidean-api --test repo_archive"
        status: pass
    human_judgment: false
duration: 2min
completed: "2026-09-12"
status: complete
---

# Phase 07 Plan 20: CR-01 archive argv + WR-02 raw slash parity Summary

**Hardened `git archive` with end-of-options before treeish and HTTP validators so `--output=` treeish cannot write files; raw `validate_ref` now allows `/` and rejects leading `-`**

## Performance

- **Duration:** 2 min
- **Started:** 2026-09-12T19:47:54Z
- **Completed:** 2026-09-12T19:52:40Z
- **Tasks:** 2
- **Files modified:** 3 (+ 2 RED evidence files)

## Accomplishments

- CR-01 regression: option-like archive treeish fails at HTTP with `repo.invalid_ref` / `invalid ref`; monitored `--output` path never created
- `CliGitBackend::archive` inserts `"--"` immediately before the revision after format/prefix flags
- `validate_archive_treeish` and `validate_ref` reject leading `-`; raw refs allow hierarchical `/` (WR-02)

## Task Commits

Each task was committed atomically:

1. **Task 1 RED:** `53c2802` + TAP evidence `4b08084` (test)
2. **Task 1 GREEN:** `f57d9e1` (feat) — archive `--` + `validate_archive_treeish` leading-hyphen
3. **Task 2 RED:** `cf4c69e` (test) — `validate_ref` unit cases
4. **Task 2 GREEN:** `fcb8e14` (feat) — slash parity + leading-hyphen / metachar align

_Note: No REFACTOR commit — GREEN changes were already minimal._

## TDD Gate Compliance

| Task | RED | GREEN | REFACTOR | Notes |
|------|-----|-------|----------|-------|
| T1 tracer | ✓ `53c2802` | ✓ `f57d9e1` | — | RED asserted HTTP-boundary message after 07-19 CLI already blocked file write |
| T2 expand | ✓ `cf4c69e` | ✓ `fcb8e14` | — | Unit RED for `/` allow + leading `-` reject |

## Files Created/Modified

- `crates/oxidean-api/tests/repo_archive.rs` — CR-01 `--output` injection regression
- `crates/oxidean-git/src/cli.rs` — archive argv end-of-options before treeish
- `crates/oxidean-api/src/routes/repo_raw.rs` — `validate_archive_treeish` / `validate_ref` hardening + unit tests
- `.planning/phases/07-git-repos-browse/.tdd/07-20-red-evidence.json` — Task 1 RED evidence
- `.planning/phases/07-git-repos-browse/.tdd/07-20-t2-red-evidence.json` — Task 2 RED evidence

## Decisions Made

- RED for Task 1 asserts message `"invalid ref"` (HTTP `validate_archive_treeish`) rather than only 4xx + no file — 07-19 `validate_treeish` already prevented file writes, so without that assertion RED would have been unexpected GREEN
- `validate_ref` drops blanket `/` ban and adopts archive-style metachar + leading-`-` checks

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] RED assertion strengthened for HTTP-boundary reject**
- **Found during:** Task 1 (RED)
- **Issue:** After 07-19, CLI `validate_treeish` already returned 4xx/`repo.invalid_ref` and blocked file creation — bare CR-01 assertions would unexpected-GREEN
- **Fix:** Assert error message is HTTP `"invalid ref"` (not CLI `"invalid treeish: ..."`)
- **Files modified:** `crates/oxidean-api/tests/repo_archive.rs`
- **Verification:** Intentional RED then GREEN after `validate_archive_treeish` change
- **Committed in:** `53c2802` / `f57d9e1`

**2. [Rule 3 - Blocking] TAP footer on RED evidence for cargo output**
- **Found during:** Task 1 (RED gate)
- **Issue:** `tdd-red-evidence` expects TAP `# tests` / `not ok` lines; raw cargo output alone classified as `zero_tests_discovered`
- **Fix:** Append TAP summary to evidence JSON (same pattern as 07-19)
- **Files modified:** `.planning/phases/07-git-repos-browse/.tdd/07-20-red-evidence.json`
- **Verification:** `gsd_run check tdd-red-evidence` → `RED_EVIDENCE_OK`
- **Committed in:** `4b08084`

---

**Total deviations:** 2 auto-fixed (1 missing-critical test design, 1 blocking evidence format)
**Impact on plan:** Required for intentional RED under 07-19 foundation and TDD gate; no scope creep.

## Issues Encountered

None beyond the documented RED-gate evidence formatting (cargo → TAP).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- CR-01 and WR-02 closed for archive/raw validators
- Ready for remaining gap-closure plan 07-21
- 07-19 `validate_treeish` / branch `--` markers unchanged (no regression)

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*

## Self-Check: PASSED

- Key files present on disk
- Commits 53c2802, 4b08084, f57d9e1, cf4c69e, fcb8e14 present in git log
