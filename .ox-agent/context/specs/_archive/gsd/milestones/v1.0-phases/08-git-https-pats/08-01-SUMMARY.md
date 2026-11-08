---
phase: 08-git-https-pats
plan: "01"
subsystem: testing
tags: [wave0, nyquist, pat, git-02, git-11, vitest, tokens-ui, clone-box]

requires:
  - phase: 07-git-repos-browse
    provides: CloneBox + integration harness patterns for HTTPS clone UI
provides:
  - "Wave 0 RED tokens.integration.test.ts stubs (list/empty/Generate/verify/revoke/reveal)"
  - "Wave 0 RED clone-box.pat.integration.test.ts stubs (how-to + /settings/tokens CTA)"
affects:
  - 08-09 tokens list/nav/revoke greens
  - 08-10 classic create/reveal greens
  - 08-12 CloneBox PatHowTo greens

actuals:
  tokens: 1445
  tasks: 1
  commits: 6

plan_head_before: 9647dceb98b9b6f3619b2dbf91b32bcf9863ecab

tech-stack:
  added: []
  patterns:
    - "Wave 0 Vitest stubs with variable @vite-ignore dynamic import while route .tsrx absent"
    - "CloneBox PAT how-to stubs assert against existing CloneBox until 08-12 embeds PatHowTo"

key-files:
  created:
    - apps/web/src/routes/settings/tokens.integration.test.ts
    - apps/web/src/components/repo/clone-box.pat.integration.test.ts
  modified: []

key-decisions:
  - "Wave 0 is RED-only — no production /settings/tokens or PatHowTo components"
  - "tokens stubs use runtime-variable import(/* @vite-ignore */) so Vitest collects while ./tokens is missing (static path fails Vite transform)"
  - "Threat stubs encode T-08-01 (no plaintext on list) and T-08-03 (unverified Generate disabled + hint) in discoverable expectations"

patterns-established:
  - "Nyquist Wave 0 for Phase 8 web: failing Vitest paths exist before tokens UI + CloneBox how-to implementation waves"

requirements-completed: []  # Wave 0 scaffolds only; greens land in later 08-xx plans

coverage:
  - id: D1
    description: "Web Wave 0 stubs for /settings/tokens list/empty/Generate/verify/revoke/reveal (GIT-11)"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "bunx vitest run src/routes/settings/tokens.integration.test.ts"
        status: fail
    human_judgment: false
  - id: D2
    description: "Web Wave 0 stubs for CloneBox Authenticate with a personal access token how-to + CTA (GIT-02)"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "bunx vitest run src/components/repo/clone-box.pat.integration.test.ts"
        status: fail
    human_judgment: false

duration: 3min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 01: Wave 0 Tokens UI + CloneBox How-to Stubs Summary

**Failing Vitest stubs for `/settings/tokens` list/create/revoke UI and CloneBox HTTPS PAT how-to before Phase 8 UI implementation waves**

## Performance

- **Duration:** 3 min
- **Started:** 2026-09-13T18:00:41Z
- **Completed:** 2026-09-13T18:03:46Z
- **Tasks:** 1
- **Files modified:** 2

## Accomplishments

- Tokens Wave 0 stubs encode Personal access tokens list title, empty hero No personal access tokens, Generate new token / Classic / Fine-grained, unverified Generate disabled + Verify your email to create a token., revoke Revoke token? / Keep token, and one-time reveal Make sure to copy your personal access token now (GIT-11 / D-14 / D-15 / D-17 / D-24 / T-08-01 / T-08-03)
- CloneBox PAT how-to stubs assert Authenticate with a personal access token, username aliases git/token/oauth2, password=PAT not account password, and Create a personal access token → `/settings/tokens` (GIT-02 / D-13)
- Variable `@vite-ignore` dynamic import lets Vitest collect tokens suite while production routes are absent

## Task Commits

Each task was committed atomically:

1. **Task 1: Web Wave 0 stubs for tokens settings and CloneBox how-to** - `fb8975d` (test)

**Plan metadata:** `a30e17e` (docs: complete plan)

_Note: Wave 0 is RED-only by design — GREEN belongs to 08-09–08-12._

## Files Created/Modified

- `apps/web/src/routes/settings/tokens.integration.test.ts` - GIT-11 UI Nyquist RED stubs
- `apps/web/src/components/repo/clone-box.pat.integration.test.ts` - GIT-02 how-to Nyquist RED stubs

## Decisions Made

- Kept Wave 0 RED-only; no production tokens routes or PatHowTo
- Used runtime-variable `import(/* @vite-ignore */)` because a static `./tokens` path fails Vite transform before tests collect
- Encoded T-08-01 / T-08-03 expectations in stub messages for later greens

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Static `@vite-ignore ./tokens` still failed Vite transform**
- **Found during:** Task 1 (tokens stubs)
- **Issue:** Vite import-analysis resolves string-literal `./tokens` even with `@vite-ignore`, so the suite could not collect (0 tests / transform error)
- **Fix:** Load via runtime variable `const rel = "./tokens"; import(/* @vite-ignore */ rel)` so Vitest discovers 4 failing tests
- **Files modified:** `apps/web/src/routes/settings/tokens.integration.test.ts`
- **Verification:** `bunx vitest run src/routes/settings/tokens.integration.test.ts` → 4 failed (RED)
- **Committed in:** `fb8975d` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Required for Wave 0 discoverability; no scope creep.

## Issues Encountered

None beyond the Vite transform fix above.

## Known Stubs

Wave 0 intentional RED stubs (expected until later plans):

| File | Reason |
|------|--------|
| `tokens.integration.test.ts` | Fails until `/settings/tokens` (+ create/reveal) land in 08-09/08-10 |
| `clone-box.pat.integration.test.ts` | Fails until PatHowTo embeds in CloneBox in 08-12 |

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Web Wave 0 paths named by VALIDATION.md / later plans are discoverable and RED
- Ready for parallel Wave 1+ implementation plans (08-02+)

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/settings/tokens.integration.test.ts`
- FOUND: `apps/web/src/components/repo/clone-box.pat.integration.test.ts`
- FOUND: commit `fb8975d`

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*
