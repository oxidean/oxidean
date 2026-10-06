---
phase: 08-git-https-pats
plan: "13"
subsystem: docs
tags: [pat, smart-http, configuration, architecture, validation, git-02, git-11, nyquist]

requires:
  - phase: 08-git-https-pats
    provides: PAT RPC + Smart HTTP + Traefik .git routing + tokens UI (plans 06–12)
provides:
  - "CONFIGURATION/ARCHITECTURE operator docs for Smart HTTP, PAT prefixes, cookie-vs-PAT"
  - "08-VALIDATION.md concrete 08-00…08-13 green task map + gate results"
affects:
  - gsd-validate-phase (nyquist)
  - gsd-verify-work / UAT

actuals:
  tokens: 5396
  tasks: 2
  commits: 4

plan_head_before: 529a4d242bb9fe7e920d82684a7e702b3f7a8867

tech-stack:
  added: []
  patterns:
    - "Operator docs use redacted oxidean_pat_ / oxidean_fg_ only; never claim PAT as RPC Bearer"
    - "VALIDATION map statuses filled from gate sweep; nyquist_compliant remains validate-phase owned"

key-files:
  created: []
  modified:
    - docs/CONFIGURATION.md
    - docs/ARCHITECTURE.md
    - .planning/phases/08-git-https-pats/08-VALIDATION.md

key-decisions:
  - "Docs examples use locked D-08 prefixes oxidean_pat_ / oxidean_fg_ (not plan draft ona_* wording)"
  - "Left nyquist_compliant: false for /gsd-validate-phase ownership"

patterns-established:
  - "CONFIGURATION documents Traefik PathRegexp + single-replica in-memory failed-auth limit"
  - "ARCHITECTURE documents git-http-backend CGI + hash-at-rest PAT + cookie/PAT credential split"

requirements-completed: [GIT-02, GIT-11]

coverage:
  - id: D1
    description: "CONFIGURATION/ARCHITECTURE document Smart HTTP .git routing, PAT prefixes, and cookie-vs-PAT split"
    requirement: GIT-02
    verification:
      - kind: other
        ref: "rg -n 'Smart HTTP|PathRegexp|oxidean_pat_|PUBLIC_ORIGIN' docs/CONFIGURATION.md docs/ARCHITECTURE.md"
        status: pass
    human_judgment: false
  - id: D2
    description: "CONFIGURATION documents OXIDEAN_PUBLIC_ORIGIN clone URLs and REPOS_DIR CGI without new env"
    requirement: GIT-02
    verification:
      - kind: other
        ref: "rg -n 'OXIDEAN_PUBLIC_ORIGIN|PathRegexp|REPOS_DIR' docs/CONFIGURATION.md"
        status: pass
    human_judgment: false
  - id: D3
    description: "Phase automated gates green: pat_/git_smart nextest, migration_parity, rpc-sync-check, web build"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(pat_) | test(git_smart)'"
        status: pass
      - kind: unit
        ref: "cargo test -p oxidean-db --lib migration_parity"
        status: pass
      - kind: other
        ref: "make rpc-sync-check"
        status: pass
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false
  - id: D4
    description: "08-VALIDATION.md Per-Task Verification Map lists 08-00…08-13 with green where verified"
    requirement: GIT-11
    verification:
      - kind: other
        ref: "rg -n '08-04|08-06|08-09' .planning/phases/08-git-https-pats/08-VALIDATION.md"
        status: pass
    human_judgment: false

duration: 3min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 13: Operator docs + phase gate sweep Summary

**CONFIGURATION/ARCHITECTURE Smart HTTP + PAT operator notes (oxidean_pat_/oxidean_fg_) and full automated gate sweep green with VALIDATION map refresh.**

## Performance

- **Duration:** 3 min
- **Started:** 2026-09-13T19:56:26Z
- **Completed:** 2026-09-13T19:58:51Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Documented Traefik `.git` PathRegexp, `OXIDEAN_PUBLIC_ORIGIN` clone URLs (D-19), CGI via `OXIDEAN_REPOS_DIR`, and single-replica failed-auth rate limits
- Documented `git-http-backend` Smart HTTP, hash-at-rest PATs, session cookies excluded from git auth (D-01/D-12), classic/FG scope summary with pointer to API.md
- Phase gates: 18 nextest `pat_`/`git_smart` pass, migration_parity, rpc-sync-check, web build; VALIDATION task map 08-00…08-13 marked green (`nyquist_compliant` still false)

## Task Commits

Each task was committed atomically:

1. **Task 1: CONFIGURATION + ARCHITECTURE Smart HTTP/PAT notes** - `766e94e` (docs)
2. **Task 2: Phase gate verify + VALIDATION map refresh** - `601e6c9` (test)

**Plan metadata:** `39e2965` (docs: complete plan) + ledger-align follow-up

## Files Created/Modified

- `docs/CONFIGURATION.md` — Smart HTTP & PAT operator section; PUBLIC_ORIGIN clone URL note
- `docs/ARCHITECTURE.md` — Smart HTTP & PATs subsection; Traefik priority 110 on diagram/data flow
- `.planning/phases/08-git-https-pats/08-VALIDATION.md` — concrete green map + 08-13-T2 gate table

## Decisions Made

- Used locked brand prefixes `oxidean_pat_` / `oxidean_fg_` (D-08) instead of plan draft `ona_*` wording — critical deviation from orchestrator
- Redacted examples only (`oxidean_pat_REDACTED`); explicitly stated PATs are not RPC Bearer
- Did not set `nyquist_compliant: true` — validate-phase owns that flip

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Docs use oxidean_* prefixes not ona_***
- **Found during:** Task 1 (operator docs)
- **Issue:** Plan action/verify text said `ona_pat_` / `ona_fg_`; D-08 and shipped code lock `oxidean_pat_` / `oxidean_fg_`
- **Fix:** Documented and verified against `oxidean_*` redacted examples only
- **Files modified:** `docs/CONFIGURATION.md`, `docs/ARCHITECTURE.md`
- **Verification:** `rg` finds `oxidean_pat_`; no `ona_pat_` / `ona_fg_` in those docs
- **Committed in:** `766e94e` (Task 1)

**2. [Rule 3 - Blocking] Web build via `cd apps/web && bun run build`**
- **Found during:** Task 2 gate sweep
- **Issue:** `bun --cwd apps/web run build` printed Bun CLI help (flag not accepted as package-script cwd in this Bun version)
- **Fix:** Ran `bun run build` from `apps/web` (same as prior phase summaries)
- **Files modified:** none (verify only)
- **Verification:** Vite build ✓
- **Committed in:** n/a (verify path only; VALIDATION notes gate results in `601e6c9`)

---

**Total deviations:** 2 auto-fixed (1 missing critical / correctness, 1 blocking verify path)
**Impact on plan:** Required for D-08 correctness and green gate; no scope creep.

## Issues Encountered

None beyond the Bun `--cwd` invocation quirk above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 8 automated gates and operator docs ready for `/gsd-verify-work` and `/gsd-validate-phase` (Nyquist sign-off)
- Manual UAT rows in VALIDATION.md still apply (reveal UX, revoke confirm, how-to readability)

## Self-Check: PASSED

- FOUND: `docs/CONFIGURATION.md`, `docs/ARCHITECTURE.md`, `.planning/phases/08-git-https-pats/08-VALIDATION.md`
- FOUND: commits `766e94e`, `601e6c9`

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*
