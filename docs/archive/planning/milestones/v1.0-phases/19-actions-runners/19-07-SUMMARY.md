---
phase: 19-actions-runners
plan: "07"
subsystem: api
tags: [actions, commit-statuses, phase13]

requires:
  - phase: 19-actions-runners
    provides: runner protocol + enqueue
provides:
  - Actions job → commit_statuses publisher (D-ACT-15)
  - Query via repo.commitStatus.list (D-ACT-16)
affects: [19-09, phase-13]

actuals:
  tokens: 6000
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns: ["{workflow_name} / {job_key} context", job→pending/success/failure/error map]

key-files:
  created:
    - crates/oxidean-api/src/actions/statuses.rs
  modified:
    - crates/oxidean-api/src/actions/dispatch.rs
    - crates/oxidean-api/src/actions/runner_proto.rs
    - crates/oxidean-api/tests/commit_statuses.rs

key-decisions:
  - "Reuse Phase 13 repo.commitStatus.list — no new RPC procedure"
  - "cancelled jobs map to commit status error"

patterns-established:
  - "Enqueue publishes pending; UpdateTask refreshes terminal states"

requirements-completed: [ACT-03]

coverage:
  - id: D1
    description: Context format + lifecycle + list RPC
    requirement: ACT-03
    verification:
      - kind: integration
        ref: cargo nextest run -E 'test(commit_statuses)'
        status: pass
    human_judgment: false

plan_head_before: fb74011
duration: 20min
completed: 2026-09-16
status: complete
---

# Phase 19 Plan 07: Commit statuses Summary

**Actions jobs publish `{workflow_name} / {job_id}` commit statuses; Phase 13 reads them via existing `repo.commitStatus.list`.**

## Deviations from Plan

**1. [Rule 2] Reused existing commitStatus RPC instead of adding checks.list**
- No new core types / rpc-gen delta; sync-check clean.

## Self-Check: PASSED
