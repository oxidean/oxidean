---
phase: 13-branch-protection
plan: "06"
subsystem: api
tags: [dismiss-stale, conversations, enforce-admins, linear-history]
requires:
  - phase: 12-pull-requests
    provides: "reviews + thread resolved flag"
provides:
  - "Full merge flag evaluation for D-06..08, D-16..17"
affects: [13-08]
actuals:
  tokens: 5000
  tasks: 2
  commits: 0
plan_head_before: 2648d2b9c8d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4
tech-stack:
  added: []
  patterns: ["dismiss_stale recomputed at merge via review.commit_sha vs head"]
key-files:
  modified:
    - crates/oxidean-api/src/protection/mod.rs
    - crates/oxidean-api/src/pull/merge_ops.rs
key-decisions:
  - "dismiss_stale: merge-time ignore Approves whose commit_sha != head (persist-on-push deferred)"
  - "require_last_push_approval wired; last_head_pusher_id optional until push attribution lands"
requirements-completed: [ORG-06, PR-08]
coverage:
  - id: D1
    description: "enforce_admins + review extras in evaluate_merge"
    requirement: PR-08
    verification:
      - kind: integration
        ref: "branch_protect_merge tests use enforce_admins true"
        status: pass
    human_judgment: false
duration: 5min
completed: 2026-09-16
status: complete
---

# Phase 13 Plan 06: Review extras + admin/linear Summary

**Merge evaluate honors dismiss_stale (SHA match), conversation resolution, last-push approval, enforce_admins, linear_history, and draft.**

## Deviations from Plan

**1. [Rule 2] dismiss_stale uses merge-time SHA filter** rather than persisting dismiss-on-push (CONTEXT discretion prefer persist — deferred polish).

## Self-Check: PASSED
