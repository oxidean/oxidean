---
phase: 17-notifications
plan: "04"
subsystem: ui
tags: [notifications, octane, tanstack-query, chrome]
requires:
  - phase: 17-notifications
    provides: notification.* RPC client
provides:
  - SiteHeader bell + unread badge
  - /notifications inbox page
affects: []
actuals:
  tokens: 9000
  tasks: 2
  commits: 1
tech-stack:
  added: []
  patterns: [notification-queries poll pattern, mark-read-on-navigate]
key-files:
  created:
    - apps/web/src/lib/notification-queries.ts
    - apps/web/src/routes/notifications.tsrx
  modified:
    - apps/web/src/components/chrome.tsrx
    - apps/web/src/routeTree.gen.ts
    - apps/web/src/lib/session-queries.ts
key-decisions:
  - "Bell navigates to page only (no dropdown preview)"
  - "Deep links use window.location.assign for issue/PR paths"
requirements-completed: [NOTF-01, NOTF-02]
coverage:
  - id: D1
    description: Signed-in bell with unread badge
    requirement: NOTF-02
    verification:
      - kind: automated_ui
        ref: apps/web/src/components/chrome.notifications.integration.test.ts
        status: pass
    human_judgment: false
  - id: D2
    description: Inbox list filters, mark all, mark+navigate
    requirement: NOTF-02
    verification:
      - kind: automated_ui
        ref: apps/web/src/routes/notifications.integration.test.ts
        status: pass
    human_judgment: false
duration: 25min
completed: 2026-09-16
status: complete
plan_head_before: 96b9612
commits: 1
---

# Phase 17 Plan 04: Notifications UI Summary

**Octane SiteHeader bell with Query-polled unread badge and `/notifications` Unread|All inbox with mark read + subject navigation.**

## Task Commits

1. **Tasks 1–2: Bell + inbox** - `7dc5c63` (feat)

## Deviations from Plan

None material.

## Self-Check: PASSED
