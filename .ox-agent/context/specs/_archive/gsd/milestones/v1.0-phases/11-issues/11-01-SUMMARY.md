---
phase: 11-issues
plan: "01"
subsystem: testing
tags: [wave0, nyquist, vitest, issues, markdown, iss-01, iss-02, iss-03, iss-04, d-iss-10, d-iss-13]

requires:
  - phase: 11-issues
    provides: Wave 0 Rust nextest stubs (11-00) + VALIDATION web gap list
  - phase: 08-git-https-pats
    provides: Wave 0 Vitest runtime-variable @vite-ignore stub pattern
provides:
  - "Wave 0 RED Issues tab/list/new/detail Vitest integration stubs (ISS-01..04 UI)"
  - "Wave 0 RED markdown.issues #N / owner/repo#N autolink stubs (ISS-04 / D-ISS-13)"
  - "Sanitize-last XSS regression coverage retained in markdown.issues unit suite (D-ISS-10 / T-11-03)"
affects:
  - 11-03 Issues tab + list/new/detail greens
  - 11-04 detail lifecycle / history / Admin delete greens
  - 11-05 Write|Preview + comments greens
  - 11-06 label settings greens
  - 11-08 reaction bar greens
  - 11-09 Linked PRs panel greens
  - 11-10 remark-github autolink greens

actuals:
  tokens: 3596
  tasks: 2
  commits: 7

plan_head_before: 206f9b32d1a2b378f59e51a8fff0efbdd192539e

tech-stack:
  added: []
  patterns:
    - "Wave 0 Vitest it.fails stubs so RED cases stay discoverable without failing the suite"
    - "Runtime-variable import(/* @vite-ignore */) while Issues .tsrx routes absent"
    - "markdown.issues.test.ts included explicitly in unit Vitest project (alongside markdown.test.ts)"

key-files:
  created:
    - apps/web/src/routes/$owner.$repo.issues.integration.test.ts
    - apps/web/src/lib/markdown.issues.test.ts
  modified:
    - apps/web/vitest.config.ts

key-decisions:
  - "Wave 0 is RED-only — no production Issues Octane routes and no remark-github install"
  - "Prefer it.fails over bare failing expects so later plans can green cases incrementally"
  - "Mention/commit non-autolink assertions are green now (Q1); #N autolink cases stay it.fails until 11-10"

patterns-established:
  - "Phase 11 web Nyquist Wave 0 mirrors Phase 08/10 stub discoverability with it.fails for suite-stable RED"

requirements-completed: []  # Wave 0 scaffolds only; greens land in later 11-xx plans

coverage:
  - id: D1
    description: "Vitest Wave 0 stubs for Issues tab, list/new/detail, Write|Preview, Linked PRs, lifecycle, reactions, label settings"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "bunx vitest list src/routes/$owner.$repo.issues.integration.test.ts"
        status: pass
      - kind: integration
        ref: "bunx vitest run --project integration src/routes/$owner.$repo.issues.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D2
    description: "Vitest Wave 0 stubs for markdown #N / owner/repo#N autolink + sanitize-last XSS + mention/commit disabled"
    requirement: ISS-04
    verification:
      - kind: unit
        ref: "bunx vitest list src/lib/markdown.issues.test.ts"
        status: pass
      - kind: unit
        ref: "bunx vitest run --project unit src/lib/markdown.issues.test.ts"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 01: Web Wave 0 Issues Vitest Stubs Summary

**Nyquist/Wave 0 Vitest stubs for Issues chrome/routes and markdown `#N` autolink (ISS-01..04, D-ISS-10/13/16/19) using `it.fails` so the suite stays green while UI/markdown plans turn cases over.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-14T14:19:37Z
- **Completed:** 2026-09-14T14:23:36Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Discoverable Issues integration suite covering chrome tab, Open/Closed/All list, can_write New issue gate, detail shells, Write|Preview, lifecycle/history/delete, reactions, and can_admin Labels entry
- Discoverable `markdown.issues` unit suite with RED `#N` / `owner/repo#N` stubs plus green sanitize-last and mention/commit non-autolink gates
- Unit Vitest project include extended for `markdown.issues.test.ts` without installing remark-github

## Task Commits

Each task was committed atomically:

1. **Task 1: Issues routes + chrome Wave 0 stubs** - `5f7fe71` (test)
2. **Task 2: markdown.issues autolink Wave 0 stub** - `658ae3c` (test)

**Plan metadata:** `608b87e` (docs: complete plan) + SUMMARY actuals fix commit

## Files Created/Modified

- `apps/web/src/routes/$owner.$repo.issues.integration.test.ts` — Issues UI Wave 0 `it.fails` integration stubs
- `apps/web/src/lib/markdown.issues.test.ts` — ISS-04 autolink Wave 0 stubs + XSS / mention gates
- `apps/web/vitest.config.ts` — include `src/lib/markdown.issues.test.ts` in unit project

## Decisions Made

- Used `it.fails` (not bare failing expects) so remaining Wave 0 cases do not exit the suite non-zero
- Left production `.tsrx` and `remark-github` for later plans (11-03+, 11-10)
- Encode T-11-03 sanitize-last as a green regression in the same markdown.issues file

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Nested comment delimiters inside block comment broke Oxc parse**
- **Found during:** Task 1 (vitest list/run)
- **Issue:** Comment text describing `@vite-ignore` import syntax terminated the file's block comment early → PARSE_ERROR / suite unloadable
- **Fix:** Rewrote the comment without nested comment delimiters
- **Files modified:** `apps/web/src/routes/$owner.$repo.issues.integration.test.ts`
- **Verification:** `bunx vitest list` + `vitest run --project integration` → 10 expected fail, file pass
- **Committed in:** `5f7fe71`

**2. [Rule 3 - Blocking] Plan verify used `bun --cwd apps/web exec vitest` which is unavailable**
- **Found during:** Task 1 verify
- **Issue:** `bun exec` script not found in this environment
- **Fix:** Used `bunx vitest list|run` from `apps/web` (equivalent discoverability check)
- **Files modified:** none (verify command only)
- **Verification:** list/run succeeded for both stub files
- **Committed in:** n/a

---

**Total deviations:** 2 auto-fixed (1 Rule 1, 1 Rule 3)
**Impact on plan:** Necessary for collectable stubs; no scope creep and no production UI.

## Known Stubs

Intentional Wave 0 RED (`it.fails`) until later greens — plan goal is stubs only:

| File | Stub | Greened by |
|------|------|------------|
| `$owner.$repo.issues.integration.test.ts` | Issues tab / list / new / detail / Write\|Preview / lifecycle / reactions / Labels | 11-03..11-09 |
| `markdown.issues.test.ts` | `#N` and `owner/repo#N` href autolink | 11-10 |

## Issues Encountered

None beyond the parse/comment and bun-exec verify deviations above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Web Wave 0 VALIDATION gaps for Issues UI + markdown.issues are closed (files exist + vitest-discoverable)
- Ready for 11-02 schema and 11-03+ UI/RPC greens; do not treat `it.fails` as production coverage

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/$owner.$repo.issues.integration.test.ts`
- FOUND: `apps/web/src/lib/markdown.issues.test.ts`
- FOUND: `.planning/phases/11-issues/11-01-SUMMARY.md`
- FOUND: commits `5f7fe71`, `658ae3c`, `608b87e` (+ SUMMARY fix)

---
*Phase: 11-issues*
*Completed: 2026-09-14*
