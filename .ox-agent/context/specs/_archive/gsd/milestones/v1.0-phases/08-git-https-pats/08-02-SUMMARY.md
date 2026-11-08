---
phase: 08-git-https-pats
plan: "02"
subsystem: planning
tags: [reversibility-gates, pat, git-https, d-08, d-18, d-21, d-01, checkpoints]

requires:
  - phase: 08-git-https-pats
    provides: 08-00/08-01 Wave 0 stubs + 08-CONTEXT decisions for PAT/Smart HTTP
provides:
  - "Locked D-08 token prefixes (oxidean_pat_ / oxidean_fg_)"
  - "Locked D-18 HTTPS clone URL /{owner}/{repo}.git"
  - "Locked D-21 git private unauth 401 + D-01 PAT HTTPS-only (not RPC Bearer)"
affects:
  - 08-03 PAT schema
  - 08-04 mint + Smart HTTP tracer
  - 08-05+ Smart HTTP status mapping and Traefik .git routing

actuals:
  tokens: 2273
  tasks: 3
  commits: 7

plan_head_before: 9700d3688f3292f131ddfe7fd8a9d9fc3b540f07

tech-stack:
  added: []
  patterns:
    - "REVERSIBILITY_GATES plan records human option IDs in DISCUSSION-LOG before schema/tracer ship"
    - "D-08 prefix lock may deviate from plan option id when human chooses full-brand prefixes"

key-files:
  created:
    - .planning/phases/08-git-https-pats/08-02-SUMMARY.md
  modified:
    - .planning/phases/08-git-https-pats/08-DISCUSSION-LOG.md
    - .planning/phases/08-git-https-pats/08-CONTEXT.md
    - .planning/STATE.md

key-decisions:
  - "D-08: oxidean_pat_ / oxidean_fg_ (option oxidean_prefixes; deviation from plan ona_prefixes)"
  - "D-18: https://{host}/{owner}/{repo}.git on public origin (option owner_repo_git)"
  - "D-21/D-01: git private unauth → 401+WWW-Authenticate Basic; PATs HTTPS-git-only not RPC Bearer (option git_401_pat_https_only)"

patterns-established:
  - "One-way/costly public contracts confirmed via blocking-human checkpoints before 08-03/08-04 implement them"
  - "DISCUSSION-LOG 08-02 section is the audit trail for locked option IDs"

requirements-completed: []  # Gates only; GIT-02/GIT-11 greens land in later 08-xx plans

coverage:
  - id: D1
    description: "Human locked PAT string prefixes to oxidean_pat_ / oxidean_fg_ (D-08)"
    requirement: GIT-11
    verification: []
    human_judgment: true
    rationale: "One-way public token format — requires explicit human option selection"
  - id: D2
    description: "Human locked HTTPS clone URL to /{owner}/{repo}.git on public origin (D-18)"
    requirement: GIT-02
    verification: []
    human_judgment: true
    rationale: "One-way public clone URL scheme — requires explicit human option selection"
  - id: D3
    description: "Human locked git private unauth 401 + PAT HTTPS-only not RPC Bearer (D-21/D-01)"
    requirement: GIT-02
    verification: []
    human_judgment: true
    rationale: "Costly credential/error contract — requires explicit human option selection"

duration: 1min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 02: Reversibility Gates Summary

**Human-locked D-08/D-18/D-21/D-01 public contracts (oxidean_* prefixes, `/{owner}/{repo}.git`, git 401 + PAT HTTPS-only) before schema and Smart HTTP tracer**

## Performance

- **Duration:** ~1 min (continuation after Task 3 human reply; Tasks 1–2 earlier in session)
- **Started:** 2026-09-13T18:17:37Z
- **Completed:** 2026-09-13T18:18:20Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Locked PAT prefixes to `oxidean_pat_` / `oxidean_fg_` (option `oxidean_prefixes`) — D-08 one-way; never github/gh* prefixes
- Locked HTTPS clone URL to `https://{host}/{owner}/{repo}.git` on public origin (option `owner_repo_git`) — D-18 one-way; Smart HTTP only on `.git` suffix
- Locked git private/no-access unauth → 401 + WWW-Authenticate Basic and PAT HTTPS-git-only (not typed RPC Bearer) (option `git_401_pat_https_only`) — D-21 / D-01
- DISCUSSION-LOG records all three locked option IDs for 08-03/08-04 proceed signal

## Task Commits

Each task was committed atomically:

1. **Task 1: Confirm PAT prefixes (D-08 one-way)** - `693df1a` (docs)
2. **Task 2: Confirm HTTPS clone URL (D-18 one-way)** - `9964f08` (docs)
3. **Task 3: Confirm git private unauth → 401 + PAT-not-RPC (D-21/D-01)** - `18086a7` (docs)

**Follow-ups:** `c03e98e` (STATE decision dedupe), `5ea4ee3` (session stop for Task 3 gate)

**Plan metadata:** `bcf32c8` (docs: complete plan)

## Files Created/Modified

- `.planning/phases/08-git-https-pats/08-DISCUSSION-LOG.md` — 08-02 reversibility gate checkpoint outcomes
- `.planning/phases/08-git-https-pats/08-CONTEXT.md` — D-08/D-18/D-01/D-21 annotated with locked option IDs
- `.planning/STATE.md` — Phase 08 decisions for all three locks
- `.planning/phases/08-git-https-pats/08-02-SUMMARY.md` — this summary

## Decisions Made

- **D-08:** Human chose `oxidean_prefixes` over plan-recommended `ona_prefixes` — full-brand token format for secret-scanning contract
- **D-18:** `owner_repo_git` confirmed — matches Phase 7 clone box; Traefik must not let SPA steal `.git`
- **D-21/D-01:** `git_401_pat_https_only` confirmed — intentional git 401 vs web 404 split; PATs never authenticate typed RPC in Phase 8

## Deviations from Plan

### Auto-fixed Issues

None during Task 3 continuation.

### Documented human deviations (Tasks 1–2)

**1. [Human decision] D-08 prefix branding**
- **Found during:** Task 1 (PAT prefixes)
- **Issue:** Plan offered `ona_prefixes` (`ona_pat_` / `ona_fg_`); human locked full-brand `oxidean_pat_` / `oxidean_fg_`
- **Fix:** Recorded as option `oxidean_prefixes` in DISCUSSION-LOG + CONTEXT D-08; schema/tracer must mint `oxidean_*`
- **Files modified:** `08-DISCUSSION-LOG.md`, `08-CONTEXT.md`
- **Committed in:** `693df1a`

---

**Total deviations:** 1 human option deviation (no Rule 1–3 auto-fixes)
**Impact on plan:** Correct — one-way door locked to human preference; threat T-08-04 still mitigated (no github/gh* prefixes)

## Issues Encountered

None — Task 3 continuation after human `git_401_pat_https_only` reply.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Reversibility gates closed — executor may proceed to **08-03** (PAT schema) and **08-04** (mint + Smart HTTP tracer)
- Implementers must use `oxidean_pat_` / `oxidean_fg_`, `/{owner}/{repo}.git`, and 401+WWW-Authenticate for private git unauth
- Do not wire PAT Bearer for typed RPC in Phase 8

## Known Stubs

None — docs/decision plan only.

## Self-Check: PASSED

- FOUND: `08-02-SUMMARY.md`, `08-DISCUSSION-LOG.md`, `08-CONTEXT.md`
- FOUND commits: `693df1a` (T1), `9964f08` (T2), `18086a7` (T3)
- Locked options present: `oxidean_prefixes`, `owner_repo_git`, `git_401_pat_https_only`

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*
