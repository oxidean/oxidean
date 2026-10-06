---
phase: 07-git-repos-browse
plan: "08"
subsystem: api
tags: [git, archive, zip, tar.gz, clone-box, axum, nextest]

requires:
  - phase: 07-git-repos-browse
    provides: "repo_raw ACL + bare layout; Code home stub clone placeholder"
provides:
  - "GET archive zip/tar.gz after browse ACL (GIT-07 / D-29)"
  - "Code Clone/Download dropdown with HTTPS copy + archives (D-22)"
affects:
  - 07-git-repos-browse
  - phase-08-https-pat

actuals:
  tokens: 8360
  tasks: 2
  commits: 3

plan_head_before: 0d54c79ea5eb1e4458e44b953ce48b804950fe98

tech-stack:
  added: []
  patterns:
    - "git archive via CliGitBackend with ARCHIVE_TIMEOUT; empty refs → GitError::NotFound"
    - "HTTP archive path /api/repos/{owner}/{repo}/archive/{*file} (.zip|.tar.gz) after resolve_repo_for_read"

key-files:
  created:
    - crates/oxidean-api/tests/repo_archive.rs
    - apps/web/src/components/repo/clone-box.tsrx
    - .planning/phases/07-git-repos-browse/.tdd/07-08-red-evidence.json
  modified:
    - crates/oxidean-git/src/backend.rs
    - crates/oxidean-git/src/cli.rs
    - crates/oxidean-api/src/routes/repo_raw.rs
    - crates/oxidean-api/src/app.rs
    - apps/web/src/routes/$owner.$repo.index.tsrx

key-decisions:
  - "Archive bytes collected under 120s timeout (no new streaming crates; T-07-SC)"
  - "Empty/unborn ref → 404 repo.archive_unavailable structured JSON"
  - "Clone box archives disabled when repo empty; HTTPS copy always available"

patterns-established:
  - "ArchiveFormat enum on GitBackend; HTTP never wraps archives in RPC JSON"

requirements-completed: [GIT-07]

coverage:
  - id: D1
    description: "ZIP and tar.gz archives downloadable for seeded ref after ACL"
    requirement: GIT-07
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_archive.rs#repo_archive_zip_and_tar_gz_nonempty_for_seeded_ref"
        status: pass
      - kind: unit
        ref: "crates/oxidean-git/src/cli.rs#git_archive_formats_zip_and_tar_gz"
        status: pass
    human_judgment: false
  - id: D2
    description: "Private non-owner archive → repo.not_found (no existence leak)"
    requirement: GIT-07
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_archive.rs#repo_archive_private_non_owner_not_found"
        status: pass
    human_judgment: false
  - id: D3
    description: "Empty repo archive → structured 404, not 500"
    requirement: GIT-07
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/repo_archive.rs#repo_archive_empty_repo_structured_failure"
        status: pass
      - kind: unit
        ref: "crates/oxidean-git/src/cli.rs#git_archive_empty_repo_returns_not_found"
        status: pass
    human_judgment: false
  - id: D4
    description: "Code Clone/Download box: HTTPS + Copy, SSH placeholder, ZIP/tar.gz"
    requirement: GIT-07
    verification:
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: true
    rationale: "E12 copy success/overflow and visual clone menu need browser confirmation"

duration: 7min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 08: Git archive + clone box Summary

**Streaming zip/tar.gz archives over ACL’d HTTP plus Code Clone/Download dropdown (GIT-07, D-22, D-29).**

## Performance

- **Duration:** 7 min
- **Started:** 2026-09-12T18:29:50Z
- **Completed:** 2026-09-12T18:37:00Z
- **Tasks:** 2
- **Files modified:** 10

## TDD Gate Compliance

| Gate | SHA | Evidence |
|------|-----|----------|
| RED | `de7e212` | `test(07-08): add failing archive format and HTTP tests` — `.tdd/07-08-red-evidence.json` → `RED_EVIDENCE_OK` (`git_archive_formats_zip_and_tar_gz`) |
| GREEN | `528d683` | `feat(07-08): stream zip/tar.gz archives via ACL'd HTTP` — nextest `git_archive` + `repo_archive` green |
| REFACTOR | — | skipped (no cleanup needed) |

## Accomplishments

- `CliGitBackend::archive` runs `git archive --format=zip|tar.gz --prefix=name/` with 120s timeout; empty refs map to `NotFound`
- `GET /api/repos/{owner}/{repo}/archive/{ref}.zip|.tar.gz` after same browse ACL as raw blobs (not RPC JSON)
- Code home Clone/Download: HTTPS + Copy HTTPS URL (E12), SSH placeholder, Download ZIP / tar.gz (disabled when empty)

## Task Commits

1. **Task 1 RED: failing archive tests** - `de7e212` (test)
2. **Task 1 GREEN: archive HTTP** - `528d683` (feat)
3. **Task 2: Clone / Download box** - `4d2d205` (feat)

## Files Created/Modified

- `crates/oxidean-git/src/backend.rs` — `ArchiveFormat` + `archive` on `GitBackend`
- `crates/oxidean-git/src/cli.rs` — `git archive` implementation + unit tests
- `crates/oxidean-api/src/routes/repo_raw.rs` — `serve_archive`
- `crates/oxidean-api/src/app.rs` — archive route registration
- `crates/oxidean-api/tests/repo_archive.rs` — seeded / private / empty cases
- `apps/web/src/components/repo/clone-box.tsrx` — clone dropdown
- `apps/web/src/routes/$owner.$repo.index.tsrx` — wire CloneBox into Code toolbar

## Decisions Made

- Collect archive bytes under timeout rather than adding `tokio-util` stream crates (threat T-07-SC: no new packages)
- Empty archive → `404` + `repo.archive_unavailable` (structured, not panic)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Octane adjacent JSX in `@if` branches**
- **Found during:** Task 2 (clone box build)
- **Issue:** Multi-child `@if`/`@else` branches failed `bun run build` with adjacent JSX error
- **Fix:** Wrapped ZIP/tar.gz menu items (and empty Code home toolbar+QuickSetup) in fragments
- **Files modified:** `clone-box.tsrx`, `$owner.$repo.index.tsrx`
- **Committed in:** `4d2d205`

**Total deviations:** 1 auto-fixed (Rule 3)
**Impact on plan:** Build-only; no scope change.

## Issues Encountered

None beyond the Octane fragment fix above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

GIT-07 archive download path is live for browse ACL; HTTPS clone URL is placeholder until Phase 8 PAT auth. Remaining phase plans can consume archive URLs from tags/commits UI.

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/routes/repo_raw.rs`
- FOUND: `apps/web/src/components/repo/clone-box.tsrx`
- FOUND: `crates/oxidean-api/tests/repo_archive.rs`
- FOUND: `de7e212`, `528d683`, `4d2d205`

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
