---
phase: 11-issues
plan: "12"
subsystem: database
tags: [factory-reset, issues, cascade, docs, validation, ISS-01, ISS-02, ISS-03, ISS-04]

requires:
  - phase: 11-issues-08
    provides: "Issue domain schema + RPC surface"
  - phase: 11-issues-10
    provides: "Markdown autolink + UI"
  - phase: 11-issues-11
    provides: "Prior wave closeouts before ops/docs gate"
provides:
  - "factory_reset_instance CASCADE coverage for issue domain tables"
  - "issue.* / label.* operator docs + VALIDATION nyquist_compliant"
  - "Phase 11 automated gate green (ISS-01..04)"
affects: [verify-work, ship, phase-12-prs]

actuals:
  tokens: 7203
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Prove factory reset via seed + COUNT(*) reconnect after cascade wipe"
    - "Docs closeout pairs API table + ARCHITECTURE section + CONFIGURATION no-new-env note"

key-files:
  created: []
  modified:
    - crates/oxidean-db/tests/factory_reset_issues.rs
    - crates/oxidean-api/tests/factory_reset_scope.rs
    - crates/oxidean-db/src/lib.rs
    - docs/API.md
    - docs/ARCHITECTURE.md
    - docs/CONFIGURATION.md
    - .planning/phases/11-issues/11-VALIDATION.md

key-decisions:
  - "Existing ON DELETE CASCADE from repositories/orgs is sufficient — no explicit DELETE order for issue tables"
  - "Org-scoped labels survive hard_delete_repository; wiped on factory_reset via organizations DELETE"

patterns-established:
  - "factory_reset_issues seeds full domain graph then asserts zero row counts per table"
  - "VALIDATION Wave 0 checklist flipped with File Exists? ✅ when suites green"

requirements-completed: [ISS-01, ISS-02, ISS-03, ISS-04]

coverage:
  - id: D1
    description: "factory_reset_instance wipes issues/comments/labels/assignees/reactions/links/counters"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "crates/oxidean-db/tests/factory_reset_issues.rs#factory_reset_issues_wipes_issue_domain_tables"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/factory_reset_scope.rs#factory_reset_wipes_issue_domain_rows"
        status: pass
    human_judgment: false
  - id: D2
    description: "Repository hard-delete cascades issue child rows (no orphans)"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "crates/oxidean-db/tests/factory_reset_issues.rs#factory_reset_issues_repository_cascade"
        status: pass
    human_judgment: false
  - id: D3
    description: "Docs describe issue.*/label.*, #N, ACL, autolink, stubs, D-ISS-15 deferral; VALIDATION Wave 0 closed"
    requirement: ISS-04
    verification:
      - kind: other
        ref: "docs/API.md#issues-issue--labels-label"
        status: pass
      - kind: other
        ref: ".planning/phases/11-issues/11-VALIDATION.md nyquist_compliant:true"
        status: pass
    human_judgment: false
  - id: D4
    description: "Phase gate green: issue_ + migration_parity + dialect_issues + rpc-sync + web build"
    requirement: ISS-02
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_)'"
        status: pass
      - kind: unit
        ref: "cargo test -p oxidean-db --lib migration_parity"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p oxidean-db -E 'test(dialect_issues)'"
        status: pass
      - kind: other
        ref: "make rpc-sync-check"
        status: pass
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false

plan_head_before: 7f1872cc039015394656b85203e5f75cbeff9cf5
duration: 5min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 12: Factory Reset + Docs Gate Summary

**Factory reset CASCADE-wipes the issue domain; operator docs and VALIDATION close Phase 11 with a green automated gate.**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-09-14T16:43:00Z
- **Completed:** 2026-09-14T16:47:30Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- Replaced Wave 0 stub asserts with seed/reset proofs for all issue-domain tables plus repository CASCADE
- Extended `admin.instance.factory_reset` API coverage to assert issue rows disappear
- Documented `issue.*` / `label.*`, numbering, ACL, autolink, stubs, and D-ISS-15 deferral; marked VALIDATION nyquist-compliant

## Task Commits

Each task was committed atomically:

1. **Task 1: Factory reset issue tables + tests** - `3d7e33e` (feat)
2. **Task 2: Docs + VALIDATION phase gate** - `b64317d` (docs)

## Files Created/Modified

- `crates/oxidean-db/tests/factory_reset_issues.rs` — full domain seed + wipe/cascade assertions
- `crates/oxidean-api/tests/factory_reset_scope.rs` — RPC factory_reset wipes issue rows
- `crates/oxidean-db/src/lib.rs` — factory_reset_instance docs mention issue CASCADE
- `docs/API.md` — issue/label RPC table + section + error codes
- `docs/ARCHITECTURE.md` — Issues & labels abstraction + reset cascade
- `docs/CONFIGURATION.md` — no new env vars; reset wipes issue domain
- `.planning/phases/11-issues/11-VALIDATION.md` — Wave 0 closed, nyquist_compliant true

## Decisions Made

- Relied on existing FK `ON DELETE CASCADE` (no extra explicit DELETEs in `factory_reset_instance`)
- Cascade test asserts org-scoped labels remain after repo hard-delete; factory reset still clears them via orgs wipe

## Deviations from Plan

None - plan executed exactly as written.

### Threat Mitigations Applied

**T-11-05:** Factory reset / repo delete CASCADE proven green for issue domain tables.

**T-11-SC:** No new packages introduced in closeout.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 11 issues are ship-ready for verification. Closing keywords (D-ISS-15) remain deferred to Phase 12 PR merge.

## Known Stubs

None — Linked PR `pr_stub` rows are intentional Phase 11 product stubs (documented; not incomplete implementations blocking the plan goal).

## Self-Check: PASSED

- FOUND: `crates/oxidean-db/tests/factory_reset_issues.rs`
- FOUND: `docs/API.md` issues section
- FOUND: `.planning/phases/11-issues/11-VALIDATION.md` with `nyquist_compliant: true`
- FOUND: commits `3d7e33e`, `b64317d`
