---
phase: 09-git-ssh
plan: "01"
subsystem: testing
tags: [ssh, vitest, wave0, nyquist, clone-box, settings]

requires:
  - phase: 08-git-https-pats
    provides: Wave 0 web integration stub pattern (@vite-ignore dynamic import; tokens + CloneBox PAT suites)
  - phase: 09-git-ssh
    provides: 09-00 Rust Wave 0 stubs + 09-UI-SPEC copy locks
provides:
  - "Wave 0 RED /settings/ssh-keys Vitest stubs (list/add/revoke, verify wall, fingerprint, SettingsNav)"
  - "Wave 0 RED CloneBox SSH Vitest stubs (scp-style git@ URL, Port 2222 hint, add-key CTA, user git)"
affects: [09-07-ssh-keys-ui, 09-08-clonebox-ssh]

actuals:
  tokens: 2801
  tasks: 1
  commits: 1

tech-stack:
  added: []
  patterns:
    - "Web Wave 0 RED via missing-route @vite-ignore import + absent CloneBox SSH panel assertions"
    - "Mirror tokens.integration.test.ts / clone-box.pat.integration.test.ts for GIT-04 / GIT-03 UI paths"

key-files:
  created:
    - apps/web/src/routes/settings/ssh-keys.integration.test.ts
    - apps/web/src/components/repo/clone-box.ssh.integration.test.ts
  modified: []

key-decisions:
  - "Wave 0 web stubs only — no production /settings/ssh-keys or CloneBox SSH panel (09-07/09-08)"
  - "requirements-completed left empty; GIT-03/GIT-04 greens land in later UI plans"

patterns-established:
  - "Phase 9 web Nyquist Wave 0: discoverable Vitest stubs that fail until .tsrx greening plans"

requirements-completed: []  # Wave 0 scaffolds only; greens land in 09-07 / 09-08

coverage:
  - id: D1
    description: "GIT-04 settings SSH keys UI Wave 0 stubs (title, nav, empty, Add, verify wall, revoke, fingerprint)"
    requirement: GIT-04
    verification:
      - kind: integration
        ref: "bun run --filter @oxidean/web test -- src/routes/settings/ssh-keys.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D2
    description: "GIT-03 CloneBox SSH Wave 0 stubs (scp-style URL, Port hint, add-key CTA, user git)"
    requirement: GIT-03
    verification:
      - kind: integration
        ref: "bun run --filter @oxidean/web test -- src/components/repo/clone-box.ssh.integration.test.ts"
        status: pass
    human_judgment: false

duration: 3min
completed: 2026-09-13
status: complete
plan_head_before: bdf29e715ba9f79b47fd2b46bdc57b860ba73e03
commits: 1
---

# Phase 09 Plan 01: Web Wave 0 SSH Keys + CloneBox SSH Stubs Summary

**Discoverable Vitest Wave 0 stubs for `/settings/ssh-keys` and CloneBox SSH that fail until 09-07/09-08 green the production UI.**

## Performance

- **Duration:** 3 min
- **Started:** 2026-09-13T23:36:02Z
- **Completed:** 2026-09-13T23:38:47Z
- **Tasks:** 1/1
- **Files modified:** 2

## Accomplishments

- `/settings/ssh-keys` stubs encode SSH keys title, SettingsNav sibling, empty hero, Add SSH key CTA, unverified Add disabled + verify copy, revoke AlertDialog (Revoke SSH key? / Keep key / Revoke key), and SHA256 fingerprint rows (GIT-04 / D-SSH-05 / D-SSH-06 / T-09-03)
- CloneBox SSH stubs encode scp-style `git@host:owner/repo.git` primary (not `ssh://`), Port 2222 hint, compact Add an SSH key → `/settings/ssh-keys`, and login user `git` (GIT-03 / D-SSH-02 / D-SSH-06)
- Vitest collects both files; suite exits 1 with 9 failing tests while production routes/panel absent (Wave 0 RED by design)

## Task Commits

Each task was committed atomically:

1. **Task 1: Web Wave 0 stubs for ssh-keys settings and CloneBox SSH** - `1f8362b` (test)

_Note: Wave 0 is RED-only by design — GREEN belongs to 09-07 / 09-08._

## Files Created/Modified

- `apps/web/src/routes/settings/ssh-keys.integration.test.ts` - GIT-04 Wave 0 settings SSH keys UI stubs
- `apps/web/src/components/repo/clone-box.ssh.integration.test.ts` - GIT-03 Wave 0 CloneBox SSH stubs

## Decisions Made

- Kept Wave 0 RED-only; no production `.tsrx` for ssh-keys or live SSH CloneBox panel
- Did not mark GIT-03 / GIT-04 complete in REQUIREMENTS (same as 09-00 Wave 0)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Cursor Write landed stubs in main checkout**
- **Found during:** Task 1
- **Issue:** Relative Write paths resolved to `/home/jesse/wsl-projects/personal/typescript/oxidean` instead of the phase worktree
- **Fix:** Copied stubs into `oxidean-wt-09` and removed the leaked main-tree files before commit
- **Files modified:** worktree copies of the two stub files (main cleaned)
- **Verification:** `test -f` in worktree; main paths absent
- **Committed in:** `1f8362b`

**2. [Rule 3 - Blocking] Plan verify command `bun --cwd apps/web exec vitest` unavailable**
- **Found during:** Task 1 verify
- **Issue:** `bun exec` script not found; worktree lacked `node_modules`
- **Fix:** `bun install` in worktree; ran `bun run --filter @oxidean/web test -- <paths>`
- **Files modified:** none (local install only)
- **Verification:** 2 files / 9 tests failed (exit 1) — Wave 0 RED
- **Committed in:** n/a (tooling only)

**Total deviations:** 2 auto-fixed (Rule 3 × 2)
**Impact on plan:** No scope creep; stubs match plan acceptance criteria.

## Issues Encountered

None beyond the path/install fixes above.

## Known Stubs

Wave 0 intentional RED stubs (expected until later plans):

| File | Stub | Reason |
|------|------|--------|
| `apps/web/src/routes/settings/ssh-keys.integration.test.ts` | all 5 tests fail (route missing) | RED until 09-07 ssh-keys.tsrx |
| `apps/web/src/components/repo/clone-box.ssh.integration.test.ts` | all 4 tests fail (SSH panel absent) | RED until 09-08 CloneBox SSH |

## Threat Flags

None — stubs only assert verify-wall / revoke copy (T-09-03); no new packages or network endpoints (T-09-SC).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Web Wave 0 discoverability complete alongside 09-00 Rust stubs. Next: 09-02 schema / later waves; UI greening in 09-07 / 09-08.

## Self-Check: PASSED

- FOUND: apps/web/src/routes/settings/ssh-keys.integration.test.ts
- FOUND: apps/web/src/components/repo/clone-box.ssh.integration.test.ts
- FOUND: 1f8362b
