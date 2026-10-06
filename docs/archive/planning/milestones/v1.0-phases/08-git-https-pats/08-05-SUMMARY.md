---
phase: 08-git-https-pats
plan: "05"
subsystem: api
tags: [pat, fine-grained, createFineGrained, oxidean_fg, git-11]

requires:
  - phase: 08-git-https-pats
    provides: 08-04 classic PAT RPC + Smart HTTP tracer; 08-03 schema/pat_types
provides:
  - "pat.createFineGrained with selected|all + contents read|write"
  - "FG ownership check on repository_ids (T-08-06)"
  - "oxidean_fg_ one-time mint + join rows for selected"
affects:
  - 08-06 FG/classic scope enforcement on Smart HTTP
  - 08-08 rpc-gen for createFineGrained client
  - 08-11 fine-grained settings UI

actuals:
  tokens: 3374
  tasks: 1
  commits: 2

plan_head_before: dc15370745af0c3f48b5c3a140428735242f87df

tech-stack:
  added: []
  patterns:
    - "FG create reuses create_pat + personal_access_token_repos; all mode passes empty ids"
    - "Foreign/missing selected repos → pat.invalid_scope (no cross-user binding)"

key-files:
  created:
    - .planning/phases/08-git-https-pats/08-05-SUMMARY.md
  modified:
    - crates/oxidean-api/src/pat/mod.rs
    - crates/oxidean-api/tests/pat_rpc.rs

key-decisions:
  - "D-08 mint uses FINE_GRAINED_PAT_PREFIX (oxidean_fg_), not plan-prose ona_fg_"
  - "Selected empty or non-owned repo ids → pat.invalid_scope; all ignores repository_ids"

patterns-established:
  - "createFineGrained mirrors createClassic (require_verified, note, one-time plaintext)"
  - "Ownership via find_repository_by_id.owner_id == session user before join insert"

requirements-completed: [GIT-11]

coverage:
  - id: D1
    description: "Verified createFineGrained all + write returns one-time oxidean_fg_ token; no join rows"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_create_fine_grained_all_returns_fg_token"
        status: pass
    human_judgment: false
  - id: D2
    description: "Selected with owned repos persists join rows; list shows fine_grained kind"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_create_fine_grained_selected_persists_repos"
        status: pass
    human_judgment: false
  - id: D3
    description: "Selected empty list and foreign repo rejected with pat.invalid_scope"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_create_fine_grained_selected_empty_rejected"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_create_fine_grained_foreign_repo_rejected"
        status: pass
    human_judgment: false
  - id: D4
    description: "Unverified createFineGrained → auth.email_unverified"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_create_fine_grained_unverified_email_unverified"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 05: Fine-grained PAT create Summary

**`pat.createFineGrained` mints `oxidean_fg_` tokens with selected/all repo access and contents read/write, enforcing owner-only selected bindings**

## Performance

- **Duration:** ~2 min
- **Started:** 2026-09-13T18:39:58Z
- **Completed:** 2026-09-13T18:42:05Z
- **Tasks:** 1
- **Files modified:** 2

## Accomplishments

- Implemented `pat.createFineGrained` (verified gate, required note, `repo_access` all|selected, `contents` read|write)
- Selected mode inserts `personal_access_token_repos` only for caller-owned repos; empty/foreign → `pat.invalid_scope`
- All mode stores no join rows; one-time plaintext uses `FINE_GRAINED_PAT_PREFIX`
- Green `pat_rpc` FG coverage alongside classic create/list/revoke

## Task Commits

1. **Task 1 RED: failing FG create tests** - `4a49518` (test)
2. **Task 1 GREEN: implement createFineGrained** - `5b0f405` (feat)

**Plan metadata:** `9e7c50e` (docs: complete plan); `a8090d4` (docs: ROADMAP/WINDOWS sync)

## TDD Gate Compliance

- RED commit present (`test(08-05): …`)
- GREEN commit present (`feat(08-05): …`)
- No REFACTOR commit (implementation stayed minimal)
- Note: `workflow.tdd_mode` is false; RED evidence checker rejected a hand-built record schema (`INVALID_RED` / `invalid_record`) — intentional assertion failures were observed via nextest before GREEN

## Files Created/Modified

- `crates/oxidean-api/src/pat/mod.rs` — `create_fine_grained` implementation
- `crates/oxidean-api/tests/pat_rpc.rs` — FG create/list validation tests
- `crates/oxidean-db/src/pats.rs` / `rpc.rs` — no code changes required (helpers + RPC arm already present from 08-03/08-04)

## Decisions Made

- Locked D-08 prefix `oxidean_fg_` via `FINE_GRAINED_PAT_PREFIX` despite plan prose `ona_fg_`
- Cross-user selected binding rejected with same `pat.invalid_scope` as empty selected (T-08-06)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Critical] Plan prose `ona_fg_` → `oxidean_fg_`**
- **Found during:** Task 1 (critical_deviation lock / D-08)
- **Issue:** Plan must_haves / verify still named `ona_fg_`
- **Fix:** Mint and assert `FINE_GRAINED_PAT_PREFIX` (`oxidean_fg_`) only
- **Files modified:** `pat/mod.rs`, `pat_rpc.rs`
- **Committed in:** `4a49518`, `5b0f405`

---

**Total deviations:** 1 auto-fixed (Rule 2)
**Impact on plan:** Required for locked branding; no scope creep. DB helper extension skipped (already shipped in 08-03).

## Issues Encountered

None beyond the prefix lock above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Classic + fine-grained create/list/revoke RPC surfaces are green
- 08-06 can enforce FG contents/selected vs Smart HTTP operations
- 08-08 should `make rpc-gen` for client types (not hand-edited here)

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/pat/mod.rs`
- FOUND: `crates/oxidean-api/tests/pat_rpc.rs`
- FOUND: `4a49518`, `5b0f405`

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*
