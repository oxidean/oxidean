---
phase: 08-git-https-pats
plan: "10"
subsystem: ui
tags: [pat, classic-token, one-time-reveal, octane, auth-shell, git-11]

requires:
  - phase: 08-git-https-pats
    provides: 08-08 pat.createClassic RPC + CreatePatResponse
  - phase: 08-git-https-pats
    provides: 08-09 /settings/tokens list + Generate → Classic link
provides:
  - "/settings/tokens/new classic create form (D-05 / D-07 / D-16)"
  - "pat-reveal one-time plaintext panel (D-15)"
  - "AuthShell verify wall on create when email unverified (D-24 / D-25)"
affects:
  - 08-11 fine-grained create (reuse PatReveal + verify wall)
  - 08-12 CloneBox how-to CTA

actuals:
  tokens: 7063
  tasks: 1
  commits: 4

plan_head_before: 5ecd7cc3861b706aa648adcb4811fd6e499803aa

tech-stack:
  added: []
  patterns:
    - "Classic create uses local useState + apiClient.pat.createClassic; plaintext only in ephemeral page state"
    - "Unverified create = AuthShell + /verify (mirror /new); verified = form then PatReveal replace"

key-files:
  created:
    - apps/web/src/routes/settings/tokens.new.tsrx
    - apps/web/src/components/settings/pat-classic-form.tsrx
    - apps/web/src/components/settings/pat-reveal.tsrx
    - .planning/phases/08-git-https-pats/08-10-SUMMARY.md
  modified:
    - apps/web/src/routes/settings/tokens.integration.test.ts
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "Reuse TokensLoaderData-shaped session loader + client auth.me fallback from list page"
  - "PatReveal Back to tokens is a link (clears ephemeral secret by leaving create route)"
  - "repo checkbox defaults checked; Generate disabled when unchecked"

patterns-established:
  - "PatReveal shared for classic (08-10) and fine-grained (08-11)"
  - "Fragment wrap for adjacent Rivet siblings inside @else (Octane single-root)"

requirements-completed: [GIT-11]

coverage:
  - id: D1
    description: "Classic create form New classic token + Note + repo checkbox + No expiration"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "apps/web/src/routes/settings/tokens.integration.test.ts#title New classic token + Note + Full control checkbox + No expiration + Generate token"
        status: pass
    human_judgment: false
  - id: D2
    description: "Empty Note shows Note is required. without calling createClassic"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "apps/web/src/routes/settings/tokens.integration.test.ts#empty Note submit shows Note is required."
        status: pass
    human_judgment: false
  - id: D3
    description: "Unverified create shows AuthShell Verify your email + /verify (D-25)"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "apps/web/src/routes/settings/tokens.integration.test.ts#unverified shows Verify your email AuthShell — not the create form"
        status: pass
    human_judgment: false
  - id: D4
    description: "One-time reveal Make sure to copy… + Copy token + Back to tokens (D-15)"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "apps/web/src/routes/settings/tokens.integration.test.ts#success shows Make sure to copy… + Copy token + Back to tokens"
        status: pass
    human_judgment: false

duration: 14min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 10: Classic PAT create + reveal Summary

**/settings/tokens/new classic create with verify wall, optional expiry, and one-time plaintext reveal (D-05 / D-07 / D-15 / D-25)**

## Performance

- **Duration:** 14 min
- **Started:** 2026-09-13T19:26:22Z
- **Completed:** 2026-09-13T19:40:00Z
- **Tasks:** 1
- **Files modified:** 5

## Accomplishments
- Classic create route: Note, required `repo` checkbox, No expiration / Expires on date, Generate token → `pat.createClassic`
- One-time reveal: warning copy, monospace token, Copy token (Copied 2s / fail message), Back to tokens
- Unverified users get AuthShell **Verify your email** + link to `/verify` — form not rendered

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: failing classic create/reveal assertions** - `f059d1b` (test)
2. **Task 1 GREEN: classic create + reveal + verify wall** - `1785dec` (feat)

**Plan metadata:** _(see final docs commit)_

## Files Created/Modified
- `apps/web/src/routes/settings/tokens.new.tsrx` — route, AuthShell wall, form↔reveal switch
- `apps/web/src/components/settings/pat-classic-form.tsrx` — classic fields + createClassic
- `apps/web/src/components/settings/pat-reveal.tsrx` — D-15 one-time reveal
- `apps/web/src/routes/settings/tokens.integration.test.ts` — greened classic create/reveal/verify
- `apps/web/src/routeTree.gen.ts` — `/settings/tokens/new` registration

## Decisions Made
- Session loader mirrors tokens list (`kind` union + client `auth.me` fallback) for test harness compatibility
- Plaintext lives only in page `useState` until Back to tokens navigates away (T-08-01)
- Generate stays disabled when repo scope unchecked (UI-SPEC)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fragment wrap for adjacent Rivet siblings**
- **Found during:** Task 1 GREEN (Octane transform)
- **Issue:** `@else { <h1/> <PatClassicForm/> }` failed with "Adjacent JSX elements must be wrapped"
- **Fix:** Wrap title + form in `<>…</>`
- **Files modified:** `tokens.new.tsrx`
- **Verification:** vitest 9 passed + `bun run build`
- **Committed in:** `1785dec`

---

**Total deviations:** 1 auto-fixed (blocking)
**Impact on plan:** Required for Octane compile; no scope change.

## Known Stubs

None — D-15 reveal `it.skip` removed; fine-grained create route remains 08-11.

## Issues Encountered
None beyond documented deviation.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Ready for 08-11 fine-grained create at `/settings/tokens/new/fine-grained` reusing `PatReveal` + AuthShell wall

## Self-Check: PASSED

- FOUND: `apps/web/src/routes/settings/tokens.new.tsrx`
- FOUND: `apps/web/src/components/settings/pat-classic-form.tsrx`
- FOUND: `apps/web/src/components/settings/pat-reveal.tsrx`
- FOUND commits: `f059d1b`, `1785dec`

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*
