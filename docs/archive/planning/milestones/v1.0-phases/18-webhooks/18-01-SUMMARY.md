---
phase: 18-webhooks
plan: "01"
subsystem: api
tags: [webhooks, hmac, rpc, issues, delivery]
requires:
  - phase: 18-webhooks
    provides: "Wave 0 webhook stub filters"
  - phase: 11-issues
    provides: "issue.create/update/close/reopen mutation hooks"
provides:
  - "Admin webhook.create/list/get/update/delete RPC"
  - "WebhookDispatcher.emit + HMAC delivery for issues"
  - "0017_webhooks tri-dialect schema + DB helpers"
  - "generated webhook.* api-client"
affects: [18-02, 18-03, 18-04]
actuals:
  tokens: 27628
  tasks: 3
  commits: 1
plan_head_before: e31d0a18d628a3bca3b9600eb7a3d09245879579
tech-stack:
  added: []
  patterns:
    - "WebhookDispatcher fire-and-forget spawn after domain mutate"
    - "HMAC-SHA256 via sha2 (no new crates.io dep)"
    - "secret one-time reveal on create/rotate; masked elsewhere"
key-files:
  created:
    - crates/oxidean-core/src/webhook_types.rs
    - crates/oxidean-db/src/webhooks.rs
    - crates/oxidean-db/migrations/sqlite/0017_webhooks.sql
    - crates/oxidean-api/src/webhook/mod.rs
    - crates/oxidean-api/src/webhook/dispatch.rs
    - crates/oxidean-api/src/webhook/deliver.rs
  modified:
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - packages/api-client/src/index.ts
key-decisions:
  - "RPC namespace webhook.* (not repo.webhook.*)"
  - "HMAC implemented with existing sha2 to avoid hmac crates.io legitimacy gate"
  - "Tracer delivery is fire-and-forget spawn; retries land in 18-02"
patterns-established:
  - "validate_webhook_url https + loopback-http in non-production"
requirements-completed: [HOOK-01, HOOK-02, HOOK-03]
coverage:
  - id: D1
    description: "Admin webhook create/list with masked secret and Admin denial"
    requirement: HOOK-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(webhook_create) | test(webhook_list) | test(webhook_admin)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "issue.create delivers signed issues opened POST with attempt status"
    requirement: HOOK-02
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(webhook_issues_deliver) | test(webhook_hmac)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Issue lifecycle emits edited/closed/reopened; update/delete/inactive"
    requirement: HOOK-02
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(webhook_issues_) | test(webhook_update) | test(webhook_delete) | test(webhook_inactive)'"
        status: pass
    human_judgment: false
duration: 20min
completed: 2026-09-16
status: complete
---

# Phase 18 Plan 01: Issues Tracer Summary

**Admin-gated webhook CRUD with HMAC-signed async issues delivery and tri-dialect schema — HOOK-01..03 vertical slice.**

## Performance

- **Duration:** 20 min
- **Tasks:** 3/3
- **Commits:** 1

## Accomplishments

- Landed `0017_webhooks` across sqlite/postgres/mysql with deliveries + attempts
- Implemented `webhook.create/list/get/update/delete` Admin RPC with secret reveal/mask
- `WebhookDispatcher.emit` + `deliver_once` (Hookshot headers, `X-Hub-Signature-256`)
- Wired issue create/update/close/reopen emitters; green nextest + dialect_webhooks + rpc-gen

## Task Commits

| Task | Commit | Notes |
|------|--------|-------|
| 1 Tracer create→issues delivery | a37c90d | schema + RPC + dispatcher + tests |
| 2 update/delete/inactive | a37c90d | same commit (interleaved with tracer) |
| 3 issue lifecycle actions | a37c90d | same commit |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Critical] HMAC via sha2 without new `hmac` crate**
- **Found during:** Task 1
- **Issue:** Adding direct `hmac` dep would require legitimacy checkpoint (T-18-SC)
- **Fix:** RFC 2104 HMAC-SHA256 using existing `sha2`
- **Files modified:** `deliver.rs`
- **Commit:** a37c90d

## Auth Gates

None.

## Known Stubs

| Stub | File | Reason |
|------|------|--------|
| webhook_ssrf/timeout/retry ignored | webhook_delivery.rs | Green in 18-02 |
| webhook_push_*/pull_request_* ignored | webhook_delivery.rs | Green in 18-03 |
| deliveries.list/ping/redeliver RPC | — | 18-02 |
| Settings Webhooks Vitest it.fails | settings.webhooks.integration.test.ts | 18-04 |

## Self-Check: PASSED

- FOUND: crates/oxidean-api/src/webhook/mod.rs
- FOUND: crates/oxidean-db/migrations/sqlite/0017_webhooks.sql
- FOUND: a37c90d
