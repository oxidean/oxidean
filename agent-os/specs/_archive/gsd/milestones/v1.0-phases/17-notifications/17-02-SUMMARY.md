---
phase: 17-notifications
plan: "02"
subsystem: api
tags: [notifications, issues, mentions, fanout]
requires:
  - phase: 17-notifications
    provides: notify::fanout + notification RPC
provides:
  - Issue D-01 emitters (open/close/reopen/assign/comment)
  - @mention recipient resolution
affects: [17-03, 17-04]
actuals:
  tokens: 8000
  tasks: 2
  commits: 1
tech-stack:
  added: []
  patterns: [issue_participant_ids helper, mention parse via @username]
key-files:
  created: []
  modified:
    - crates/oxidean-api/src/notify/mod.rs
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/tests/notification_rpc.rs
key-decisions:
  - "Mentions who are already participants get issue_comment only; non-participants get issue_mention"
requirements-completed: [NOTF-01]
coverage:
  - id: D1
    description: Issue close/assign/mention emit; reactions/labels silent
    requirement: NOTF-01
    verification:
      - kind: integration
        ref: crates/oxidean-api/tests/notification_rpc.rs
        status: pass
    human_judgment: false
duration: 15min
completed: 2026-09-16
status: complete
plan_head_before: 681d99b12f6901c7c3be731f08cd989572b9eac7
commits: 1
---

# Phase 17 Plan 02: Issue emitters Summary

**Full issue-side notification fan-out: lifecycle, assignees, @mentions; reactions/labels stay silent.**

## Accomplishments
- Wired create/close/reopen/assignees.set/comments.create through `notify::fanout`
- `@username` extraction + `find_user_by_username` resolution
- Nextest covers close, assign, mention, and D-04 silence

## Task Commits

1. **Tasks 1–2: Issue lifecycle + mentions** - `2ce77f1` (feat)

## Deviations from Plan

None - plan executed as written.

## Self-Check: PASSED
