---
phase: 20-packages-registry
plan: "09"
subsystem: api
tags: [packages, quota, gc]
requires:
  - phase: 20-packages-registry
    provides: store + formats + packages RPC
provides:
  - Owner quota enforcement on upload
  - packages.adminUsage / packages.adminSetQuota
  - Periodic package blob GC job
affects: [20-11, 20-12]
actuals: { tokens: 28000, tasks: 3, commits: 1 }
tech-stack: { added: [], patterns: [refcount GC with grace, Admin sys-admin gate] }
key-files:
  created: [crates/oxidean-api/src/packages/quota.rs, crates/oxidean-api/tests/package_gc.rs, crates/oxidean-api/tests/package_quota.rs]
  modified: [crates/oxidean-api/src/packages/rpc.rs, crates/oxidean-api/src/jobs/schedule.rs, packages/api-client/src/index.ts]
key-decisions:
  - "Admin package RPCs require Role::SysAdmin (same as admin.auth.*)"
  - "GC interval OXIDEAN_PACKAGES_GC_INTERVAL_SECS default 24h; grace 7d"
requirements-completed: [PKG-01, PKG-02, PKG-03]
coverage:
  - id: D1
    description: Over-quota / max-blob rejects
    requirement: PKG-01
    verification: [{ kind: integration, ref: "cargo nextest run -p oxidean-api -E test(package_quota)", status: pass }]
    human_judgment: false
  - id: D2
    description: Admin usage/setQuota RPC
    requirement: PKG-01
    verification: [{ kind: integration, ref: "cargo nextest run -p oxidean-api -E test(package_rpc_admin)", status: pass }]
    human_judgment: false
  - id: D3
    description: Refcount-safe blob GC
    requirement: PKG-01
    verification: [{ kind: integration, ref: "cargo nextest run -p oxidean-api -E test(package_gc)", status: pass }]
    human_judgment: false
plan_head_before: dc6c8d03abc1dac6f71427285a902cbb2de3c8aa
duration: 35min
completed: 2026-09-14
status: complete
---
# Phase 20 Plan 09: Quotas and GC Summary

**Per-owner package quotas with Admin override RPC and refcount-safe blob GC (D-PKG-09).**

## Task Commits

| Task | Commit |
|------|--------|
| 1–3 | `44625d8` |

## Deviations from Plan

None - plan executed as written.

## Self-Check: PASSED
