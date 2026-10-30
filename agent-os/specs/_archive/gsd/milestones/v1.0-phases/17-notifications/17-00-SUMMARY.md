---
phase: 17-notifications
plan: "00"
subsystem: testing
tags: [notifications, wave0, nextest, vitest]
requires:
  - phase: 11-issues
    provides: issue RPC test patterns
provides:
  - Discoverable Wave 0 stubs for notification RPC, dialect migrations, chrome bell, /notifications
affects: [17-01, 17-02, 17-03, 17-04]
actuals:
  tokens: 400
  tasks: 2
  commits: 2
tech-stack:
  added: []
  patterns: [ignored nextest Wave 0 stubs, Vitest raw-source discoverability stubs]
key-files:
  created:
    - crates/oxidean-api/tests/notification_rpc.rs
    - crates/oxidean-db/tests/dialect_notifications.rs
    - apps/web/src/components/chrome.notifications.integration.test.ts
    - apps/web/src/routes/notifications.integration.test.ts
  modified:
    - .planning/phases/17-notifications/17-VALIDATION.md
key-decisions:
  - "Target migration number 0017_notifications (after 0016_pull_requests)"
  - "Wave 0 Rust stubs use #[ignore] so default nextest stays green"
patterns-established:
  - "notification nextest filter + dialect_notifications binary for later plans"
requirements-completed: [NOTF-01, NOTF-02]
coverage:
  - id: D1
    description: Wave 0 notification RPC stubs discoverable
    requirement: NOTF-01
    verification:
      - kind: unit
        ref: crates/oxidean-api/tests/notification_rpc.rs
        status: pass
    human_judgment: false
  - id: D2
    description: Wave 0 web notification stubs on disk
    requirement: NOTF-02
    verification:
      - kind: unit
        ref: apps/web/src/routes/notifications.integration.test.ts
        status: pass
    human_judgment: false
duration: 8min
completed: 2026-09-16
status: complete
plan_head_before: 197d64cc7256d918ebd5ed48c6d94193ad511bad
commits: 2
---

# Phase 17 Plan 00: Wave 0 notification stubs Summary

**Discoverable RED stubs for notification RPC, dialect migrations, header bell, and `/notifications` before implementation plans turn them green.**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-09-16T13:26:44Z
- **Completed:** 2026-09-16T13:42:00Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Added ignored nextest stubs covering list/unread/mark, comment→author, actor exclusion, and IDOR
- Added dialect_notifications migration presence stub targeting `0017_notifications.sql`
- Added Vitest stubs for SiteHeader bell and `/notifications` Unread|All / Mark all as read
- Marked Wave 0 checklist present in `17-VALIDATION.md`

## Task Commits

1. **Task 1: Rust Wave 0 stubs** - `ba90f7d` (test)
2. **Task 2: Web Wave 0 stubs + validation** - `ca99d7a` (test)

## Files Created/Modified
- `crates/oxidean-api/tests/notification_rpc.rs` — NOTF-01/02 RPC stubs
- `crates/oxidean-db/tests/dialect_notifications.rs` — migration parity stub
- `apps/web/src/components/chrome.notifications.integration.test.ts` — bell stub
- `apps/web/src/routes/notifications.integration.test.ts` — page stub
- `.planning/phases/17-notifications/17-VALIDATION.md` — Wave 0 checklist

## Decisions Made
- Migration number foreshadowed as **0017** (after Phase 12 `0016_pull_requests`)
- Ignored stubs so default nextest remains green until 17-01

## Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

- FOUND: crates/oxidean-api/tests/notification_rpc.rs
- FOUND: crates/oxidean-db/tests/dialect_notifications.rs
- FOUND: apps/web/src/components/chrome.notifications.integration.test.ts
- FOUND: apps/web/src/routes/notifications.integration.test.ts
- FOUND: ba90f7d, ca99d7a
