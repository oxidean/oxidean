---
phase: 08-git-https-pats
plan: "11"
subsystem: ui
tags: [pat, fine-grained, createFineGrained, octane, auth-shell, git-11, listMine]

requires:
  - phase: 08-git-https-pats
    provides: 08-05 pat.createFineGrained RPC + oxidean_fg_ mint
  - phase: 08-git-https-pats
    provides: 08-10 PatReveal + classic AuthShell verify wall pattern
  - phase: 08-git-https-pats
    provides: 08-09 Generate → Fine-grained link
provides:
  - "/settings/tokens/new/fine-grained FG create (D-05 / D-06 / D-07 / D-16)"
  - "pat-fg-form All/Selected repos + contents read/write + listMine checklist"
affects:
  - 08-13 phase verification / e2e

actuals:
  tokens: 8416
  tasks: 1
  commits: 2

plan_head_before: 80aaa82d4b250ed67334021039a55cf9b5368441

tech-stack:
  added: []
  patterns:
    - "FG create mirrors classic route: AuthShell wall → PatFgForm → PatReveal"
    - "Selected repos from repo.listMine only; server re-validates ownership (T-08-06)"

key-files:
  created:
    - apps/web/src/routes/settings/tokens.new.fine-grained.tsrx
    - apps/web/src/components/settings/pat-fg-form.tsrx
    - .planning/phases/08-git-https-pats/08-11-SUMMARY.md
  modified:
    - apps/web/src/routes/settings/tokens.integration.test.ts
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "Default repo access Only select repositories (GitHub FG empty selection)"
  - "Default contents Read-only; All repositories hides checklist"
  - "Reuse PatReveal; prefix oxidean_fg_ (D-08), not ona_fg_"

patterns-established:
  - "tokens.new.* sibling under SettingsTokensRoute with path /new/fine-grained"
  - "Fragment wrap for adjacent Rivet siblings on create success/empty-repos"

requirements-completed: [GIT-11]

coverage:
  - id: D1
    description: "FG create title + All/Selected repos + Contents Read-only / Read and write"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "apps/web/src/routes/settings/tokens.integration.test.ts#title New fine-grained token + All repositories / Only select repositories + Contents Read-only / Read and write"
        status: pass
    human_judgment: false
  - id: D2
    description: "Selected empty submit shows Select at least one repository. without createFineGrained"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "apps/web/src/routes/settings/tokens.integration.test.ts#selected empty submit shows Select at least one repository."
        status: pass
    human_judgment: false
  - id: D3
    description: "Zero owned repos → empty body + New repository link"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "apps/web/src/routes/settings/tokens.integration.test.ts#zero owned repos shows You don’t have any repositories yet. + New repository link"
        status: pass
    human_judgment: false
  - id: D4
    description: "Unverified FG create shows AuthShell Verify your email (D-25)"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "apps/web/src/routes/settings/tokens.integration.test.ts#unverified shows Verify your email AuthShell — not the FG create form"
        status: pass
    human_judgment: false
  - id: D5
    description: "FG success reveal via createFineGrained + oxidean_fg_ (D-15)"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "apps/web/src/routes/settings/tokens.integration.test.ts#success shows Make sure to copy… + Copy token + Back to tokens via createFineGrained"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 11: Fine-grained PAT create Summary

**/settings/tokens/new/fine-grained with All/Selected repos, contents read/write, verify wall, and shared one-time reveal (D-05 / D-06 / D-15 / D-25)**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-13T19:48:25Z
- **Completed:** 2026-09-13T19:54:49Z
- **Tasks:** 1
- **Files modified:** 5

## Accomplishments
- Fine-grained create route: Note, All/Selected repos (listMine checklist + visibility Badge + truncate title), Contents read/write, expiry, Generate → `pat.createFineGrained`
- Reused `PatReveal` for one-time `oxidean_fg_` plaintext; AuthShell verify wall when unverified
- Zero-repos and selected-empty validation copy per UI-SPEC

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: failing FG create/reveal assertions** - `61ab247` (test)
2. **Task 1 GREEN: FG create + reveal reuse** - `4adee2f` (feat)

**Plan metadata:** `a022bac` (docs: complete plan)

## Files Created/Modified
- `apps/web/src/routes/settings/tokens.new.fine-grained.tsrx` — route, AuthShell wall, form↔reveal switch
- `apps/web/src/components/settings/pat-fg-form.tsrx` — FG fields + listMine + createFineGrained
- `apps/web/src/routes/settings/tokens.integration.test.ts` — greened FG create/reveal/verify/empty
- `apps/web/src/routeTree.gen.ts` — `/settings/tokens/new/fine-grained` registration
- `.planning/phases/08-git-https-pats/.tdd/08-11-red-evidence.json` — RED TAP evidence

## Decisions Made
- Default **Only select repositories** with empty checklist (GitHub FG); **All repositories** hides list
- Default contents **Read-only**; token prefix **oxidean_fg_** (D-08 / api-client), not plan must_have `ona_fg_`
- Checklist fed only from `repo.listMine` (T-08-06)

## Deviations from Plan

None - plan executed as written (prefix wording in must_haves superseded by locked D-08 / critical_octane).

## Known Stubs

None — FG create route and reveal wired end-to-end.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- FG create UI green; ready for remaining phase plans / verification (08-13)

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/settings/tokens.new.fine-grained.tsrx`
- FOUND: `apps/web/src/components/settings/pat-fg-form.tsrx`
- FOUND: `apps/web/src/components/settings/pat-reveal.tsrx` (reused)
- FOUND commits: `61ab247`, `4adee2f`

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*
