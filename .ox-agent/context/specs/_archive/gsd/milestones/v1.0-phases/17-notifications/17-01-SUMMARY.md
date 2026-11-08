---
phase: 17-notifications
plan: "01"
subsystem: api
tags: [notifications, rpc, migrations, fanout]
requires:
  - phase: 17-notifications
    provides: Wave 0 stubs
  - phase: 11-issues
    provides: issue.comments.create hook site
provides:
  - notifications table + notification.* RPCs
  - notify::fanout helper
  - comment→author tracer path
affects: [17-02, 17-03, 17-04]
actuals:
  tokens: 25000
  tasks: 3
  commits: 2
tech-stack:
  added: []
  patterns: [soft-fail notify fanout after domain write, own-rows-only notification RPC]
key-files:
  created:
    - crates/oxidean-db/migrations/postgres/0017_notifications.sql
    - crates/oxidean-db/src/notifications.rs
    - crates/oxidean-core/src/notification_types.rs
    - crates/oxidean-api/src/notification/mod.rs
    - crates/oxidean-api/src/notify/mod.rs
  modified:
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - docs/API.md
key-decisions:
  - "RPC namespace notification.* with camelCase unreadCount/markRead/markAllRead"
  - "Migration 0017 after Phase 12 0016_pull_requests"
  - "Soft-fail fanout after successful domain write"
patterns-established:
  - "notify::fanout(actor, recipients, reason, subject) excludes actor"
requirements-completed: [NOTF-01, NOTF-02]
coverage:
  - id: D1
    description: Comment on another's issue creates unread for author; list/mark work
    requirement: NOTF-01
    verification:
      - kind: integration
        ref: crates/oxidean-api/tests/notification_rpc.rs#notification_issue_comment_creates_unread_for_author
        status: pass
    human_judgment: false
  - id: D2
    description: Own-rows markRead IDOR + unauthenticated fail-closed
    requirement: NOTF-02
    verification:
      - kind: integration
        ref: crates/oxidean-api/tests/notification_rpc.rs#notification_cannot_mark_another_users_notification
        status: pass
    human_judgment: false
duration: 25min
completed: 2026-09-16
status: complete
plan_head_before: 1bf393eabb91fa35752c9885c4441bd67f25f942
commits: 2
---

# Phase 17 Plan 01: Notification tracer Summary

**End-to-end in-app notifications: schema + `notification.*` RPCs + issue comment→author fan-out with own-rows list/mark.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 3 (T1 tracer + T2 IDOR/auth covered in same suite + T3 API docs)
- **Files modified:** 17

## Accomplishments
- Dialect-identical `0017_notifications` with `read_at` null = unread
- Session-bound `notification.list` / `unreadCount` / `markRead` / `markAllRead`
- Soft-fail `notify::fanout` wired from `issue.comments.create` (author only for tracer)
- Regenerated `@oxidean/api-client`; `make rpc-sync-check` clean

## Task Commits

1. **Task 1–2: Tracer + IDOR/auth** - `e9c3c89` (feat)
2. **Task 3: API docs deep-link fields** - `1e973f0` (docs)

## Deviations from Plan

None material — T2 hardening shipped inside the tracer commit with dedicated tests.

## Self-Check: PASSED
