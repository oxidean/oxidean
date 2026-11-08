---
phase: 18-webhooks
plan: "00"
subsystem: testing
tags: [wave0, nextest, vitest, webhooks, hmac]
requires:
  - phase: 15-releases-transfer
    provides: "Wave 0 #[ignore] + it.fails discoverability patterns"
provides:
  - "Discoverable ignored nextest stubs for webhook_* + dialect_webhooks"
  - "Vitest it.fails stub for Settings Webhooks UI"
affects: [18-01, 18-02, 18-03, 18-04]
actuals:
  tokens: 1523
  tasks: 2
  commits: 1
plan_head_before: 98369571e8b4b1c593ca63b1f16e353018ed2f39
tech-stack:
  added: []
  patterns: ["Wave 0 #[ignore] stubs discoverable via nextest list --run-ignored all"]
key-files:
  created:
    - crates/oxidean-api/tests/webhook_rpc.rs
    - crates/oxidean-api/tests/webhook_delivery.rs
    - crates/oxidean-db/tests/dialect_webhooks.rs
    - apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts
  modified: []
key-decisions:
  - "Wave 0 stubs use #[ignore]/it.fails so CI stays green until later plans turn them green"
patterns-established:
  - "Phase 18 test filters: webhook_create/list/update/delete/admin, webhook_issues_/hmac/ssrf/push/pull_request, dialect_webhooks"
requirements-completed: [HOOK-01, HOOK-02, HOOK-03]
coverage:
  - id: D1
    description: "Wave 0 discoverable stubs for webhook RPC, delivery/HMAC, dialect_webhooks, and Settings Webhooks UI"
    requirement: HOOK-01
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(webhook)' --run-ignored all"
        status: pass
    human_judgment: false
duration: 16min
completed: 2026-09-16
status: complete
---

# Phase 18 Plan 00: Wave 0 Stubs Summary

**Ignored nextest + Vitest stubs for webhook CRUD, delivery/HMAC, dialect migrations, and Settings Webhooks UI (HOOK-01..03 discoverability).**

## Performance

- **Duration:** 16 min
- **Tasks:** 2/2
- **Commits:** 1

## Accomplishments

- Scaffolded `webhook_rpc.rs` covering create/list/update/delete/Admin denial/inactive/deliveries/ping/redeliver stubs
- Scaffolded `webhook_delivery.rs` covering issues, HMAC, SSRF, timeout, retry, push, pull_request stubs
- Scaffolded `dialect_webhooks.rs` for next-free `00NN_webhooks` parity
- Scaffolded Vitest `it.fails` stub for Settings Webhooks heading/CRUD/history

## Task Commits

| Task | Commit | Files |
|------|--------|-------|
| 1 Rust Wave 0 stubs | 8b6fdcf | webhook_rpc.rs, webhook_delivery.rs, dialect_webhooks.rs |
| 2 Web Wave 0 stubs | 8b6fdcf | settings.webhooks.integration.test.ts |

## Deviations from Plan

None - plan executed exactly as written (single commit for both stub tasks).

## Auth Gates

None.

## Known Stubs

| Stub | File | Reason |
|------|------|--------|
| All webhook_* RPC tests #[ignore] | webhook_rpc.rs | Green in 18-01/18-02 |
| All webhook_* delivery tests #[ignore] | webhook_delivery.rs | Green in 18-01..18-03 |
| dialect_webhooks #[ignore] | dialect_webhooks.rs | Green in 18-01 |
| Settings Webhooks Vitest it.fails | settings.webhooks.integration.test.ts | Green in 18-04 |

## Self-Check: PASSED

- FOUND: crates/oxidean-api/tests/webhook_rpc.rs
- FOUND: crates/oxidean-api/tests/webhook_delivery.rs
- FOUND: crates/oxidean-db/tests/dialect_webhooks.rs
- FOUND: apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts
- FOUND: 8b6fdcf
