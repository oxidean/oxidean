---
phase: 08-git-https-pats
plan: "08"
subsystem: api
tags: [pat, rpc-gen, api-client, smart-http, git-11, d-01]

requires:
  - phase: 08-git-https-pats
    provides: 08-04/05 pat.* RPC + 08-06 Smart HTTP ACL
provides:
  - "@oxidean/api-client pat.createClassic/createFineGrained/list/revoke + DTOs"
  - "docs/API.md Smart HTTP + PAT auth contract (D-01 not RPC Bearer)"
affects:
  - 08-10/08-11 settings tokens UI
  - 08-09 CloneBox how-to

actuals:
  tokens: 4604
  tasks: 1
  commits: 5

plan_head_before: bafd6eaa8904b2b9ab8c1d237181f1aad422259a

tech-stack:
  added: []
  patterns:
    - "rpc-gen is source of truth for api-client; extend bin template then make rpc-gen"
    - "Token prefixes documented as oxidean_pat_ / oxidean_fg_ (not ona_*)"

key-files:
  created:
    - .planning/phases/08-git-https-pats/08-08-SUMMARY.md
  modified:
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - docs/API.md

key-decisions:
  - "PAT client surface lives in rpc_gen.rs template (no hand-edit of api-client)"
  - "API.md states D-01: session cookie for RPC; PAT Basic for Smart HTTP only"

patterns-established:
  - "pat.* client + Query/Mutation helpers mirror repo.* codegen style"
  - "Docs use redacted oxidean_pat_REDACTED examples only"

requirements-completed: [GIT-11]

coverage:
  - id: D1
    description: "make rpc-gen emits pat.createClassic/createFineGrained/list/revoke on @oxidean/api-client"
    requirement: GIT-11
    verification:
      - kind: other
        ref: "make rpc-sync-check"
        status: pass
      - kind: other
        ref: "rg createClassic|createFineGrained packages/api-client/src/"
        status: pass
    human_judgment: false
  - id: D2
    description: "docs/API.md documents Smart HTTP + PAT prefixes and D-01 (not RPC Bearer)"
    requirement: GIT-11
    verification:
      - kind: other
        ref: "rg 'not.*Bearer|Smart HTTP|oxidean_pat_' docs/API.md"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 08: rpc-gen + API.md PAT/Smart HTTP Summary

**Generated `@oxidean/api-client` `pat.*` methods via rpc-gen and documented Smart HTTP Basic+PAT auth with D-01 (PATs are not RPC Bearer).**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-13T18:53:05Z
- **Completed:** 2026-09-13T18:56:05Z
- **Tasks:** 1
- **Files modified:** 3

## Accomplishments
- Extended `rpc_gen.rs` so regeneration exports `pat.createClassic`, `pat.createFineGrained`, `pat.list`, `pat.revoke` plus DTOs and Query helpers
- Updated `docs/API.md` with PAT procedures, error codes, Smart HTTP paths, `oxidean_pat_` / `oxidean_fg_` prefixes, and explicit D-01 (session for RPC; PAT for HTTPS git only)
- `make rpc-sync-check` green after commit

## Task Commits

Each task was committed atomically:

1. **Task 1: make rpc-gen + API.md PAT/Smart HTTP docs** - `483daf5` (feat)

**Plan metadata:** `afb6522` (docs: complete plan)

## Files Created/Modified
- `crates/oxidean-api/src/bin/rpc_gen.rs` - PAT types, client methods, TanStack helpers
- `packages/api-client/src/index.ts` - regenerated client (do not hand-edit)
- `docs/API.md` - PAT RPC + Smart HTTP auth documentation

## Decisions Made
- Source of truth remains Rust + `rpc-gen` template; api-client was never hand-patched as lasting source
- Documented locked prefixes `oxidean_pat_` / `oxidean_fg_` (not `ona_*`) with redacted examples only

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Extended rpc-gen template for pat.***
- **Found during:** Task 1 (make rpc-gen + API.md)
- **Issue:** `make rpc-gen` alone did not emit `pat.*` — `rpc_gen.rs` had no PAT surface despite Rust RPC handlers existing
- **Fix:** Added PAT DTOs, `client.pat.*`, and Query/Mutation helpers to `rpc_gen.rs`, then regenerated
- **Files modified:** `crates/oxidean-api/src/bin/rpc_gen.rs`, `packages/api-client/src/index.ts`
- **Verification:** `make rpc-sync-check` exits 0; rg finds createClassic/createFineGrained
- **Committed in:** `483daf5` (part of task commit)

---

**Total deviations:** 1 auto-fixed (1 missing critical)
**Impact on plan:** Required for GIT-11 client unlock; no scope creep beyond plan goal.

## Issues Encountered
- Plan listed `packages/api-client/src/types.ts`; package only uses generated `index.ts` (no separate types file)

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Web UI plans can import `@oxidean/api-client` `pat.*` without hand patches
- Smart HTTP / PAT auth contracts documented for CloneBox and settings UI

## Self-Check: PASSED

- FOUND: `packages/api-client/src/index.ts` (pat methods present)
- FOUND: `docs/API.md` (Smart HTTP + D-01 Bearer note)
- FOUND: commit `483daf5`

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*
