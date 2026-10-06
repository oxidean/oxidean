---
phase: 19-actions-runners
plan: "04"
subsystem: api
tags: [actions, dispatch, runner-protocol, tracer]

requires:
  - phase: 19-actions-runners
    provides: workflow parse + Actions schema
provides:
  - Push→enqueue dispatcher hooked after HTTPS/SSH receive-pack
  - /api/actions Register+FetchTask tracer mount
  - Job log append/read under ACTIONS_LOG_DIR
affects: [19-05, 19-06, 19-07, 19-09]

actuals:
  tokens: 18000
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns: [async spawn after pack success, JSON runner protocol stub, label subset match]

key-files:
  created:
    - crates/oxidean-api/src/actions/dispatch.rs
    - crates/oxidean-api/src/actions/runner_proto.rs
    - crates/oxidean-api/src/actions/logs.rs
  modified:
    - crates/oxidean-api/src/routes/git_smart_http.rs
    - crates/oxidean-api/src/ssh/server.rs
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-db/src/actions.rs

key-decisions:
  - "Tracer uses JSON Register/FetchTask; 19-05 may deepen Connect/prost without remount"
  - "SSH Actions dispatch uses CliGitBackend + env gate (SshState stays lean)"

patterns-established:
  - "notify_push_actions never fails the git push"

requirements-completed: [ACT-01, ACT-02, ACT-06, ACT-07]

coverage:
  - id: D1
    description: Push enqueue + gates + queued policy
    requirement: ACT-02
    verification:
      - kind: integration
        ref: cargo nextest run -E 'test(actions_triggers)|test(actions_dispatch_policy)'
        status: pass
    human_judgment: false
  - id: D2
    description: FetchTask + cookie ignored
    requirement: ACT-06
    verification:
      - kind: integration
        ref: cargo nextest run -E 'test(actions_runner_protocol)'
        status: pass
    human_judgment: false
  - id: D3
    description: Log bytes round-trip
    requirement: ACT-03
    verification:
      - kind: integration
        ref: cargo nextest run -E 'test(actions_rpc)'
        status: pass
    human_judgment: false

plan_head_before: c62a6ca
duration: 20min
completed: 2026-09-16
status: complete
---

# Phase 19 Plan 04: Push→FetchTask tracer Summary

**End-to-end control-plane tracer: push dispatch enqueues queued jobs, registered runners FetchTask by label match, logs round-trip under ACTIONS_LOG_DIR — no in-process step execution.**

## Task Commits

1. **Task 1: End-to-end push dispatch tracer** - `c352e5e` (feat)
2. **Task 2: Minimal runner FetchTask mount** - `4045ff4` (feat)
3. **Task 3: Tracer log append path** - `b9fc512` (feat)

## Deviations from Plan

**1. [Rule 3] SSH state lean** — Actions SSH hook uses `CliGitBackend::new()` + env `OXIDEAN_ACTIONS_ENABLED` instead of extending `SshState`.

**2. [Rule 3] JSON protocol stub** — Per plan assumption; full prost/Connect deferred to 19-05.

## Self-Check: PASSED

- 13 nextest cases green for triggers/dispatch/runner/rpc filters
