---
phase: 18-webhooks
plan: "04"
subsystem: ui
tags: [webhooks, octane, settings, tanstack-query]
requires:
  - phase: 18-webhooks
    provides: "webhook.* CRUD + deliveries RPC"
provides:
  - "Admin Settings Webhooks CRUD UI"
  - "Delivery history with ping/redeliver"
affects: []
actuals:
  tokens: 8293
  tasks: 2
  commits: 2
plan_head_before: ea15d2a8d04937770b4aad1512c32319aa069e96
tech-stack:
  added: []
  patterns:
    - "Admin-gated Settings panel + one-time secret reveal (PAT pattern)"
    - "Inline deliveries under selected webhook row"
key-files:
  created:
    - apps/web/src/components/repo/webhooks-panel.tsrx
    - apps/web/src/components/repo/webhook-form.tsrx
    - apps/web/src/components/repo/webhook-deliveries.tsrx
  modified:
    - apps/web/src/routes/$owner.$repo.settings.tsrx
    - apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts
key-decisions:
  - "Inline Settings section (not nested /settings/hooks routes)"
  - "Secret reveal after create/rotate before returning to list"
requirements-completed: [HOOK-01, HOOK-03]
coverage:
  - id: D1
    description: "Admin Webhooks CRUD section on repo settings"
    requirement: HOOK-01
    verification:
      - kind: integration
        ref: "bun --filter @oxidean/web test -- src/routes/$owner.$repo.settings.webhooks.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D2
    description: "Delivery history with ping/redeliver for Admin"
    requirement: HOOK-03
    verification:
      - kind: integration
        ref: "bun --filter @oxidean/web test -- src/routes/$owner.$repo.settings.webhooks.integration.test.ts"
        status: pass
    human_judgment: false
duration: 5min
completed: 2026-09-16
status: complete
---

# Phase 18 Plan 04: Settings Webhooks UI Summary

**Admin Settings Webhooks section: CRUD, event subscriptions, one-time secret reveal, and delivery history with ping/redeliver (HOOK-01/03).**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-09-16T14:20:55Z
- **Completed:** 2026-09-16T14:26:00Z
- **Tasks:** 2/2
- **Files modified:** 5

## Accomplishments

- `WebhooksPanel` Admin-only section on repo settings (D-HOOK-02, D-HOOK-19)
- `WebhookForm` create/edit with URL, secret, push/PR/issues checkboxes, active toggle, PAT-style secret reveal (D-HOOK-16)
- `WebhookDeliveries` recent attempts + Ping/Redeliver (D-HOOK-20, D-HOOK-21)
- Vitest settings.webhooks integration tests green; `make web-lint` and `make web-format-check` pass

## Task Commits

| Task | Commit |
|------|--------|
| 1 Webhooks panel CRUD | 68ea220 |
| 2 Delivery history + ping/redeliver | fd79c16 |

## Files Created/Modified

- `apps/web/src/components/repo/webhooks-panel.tsrx` — list, toggle, edit/delete, select for deliveries
- `apps/web/src/components/repo/webhook-form.tsrx` — create/edit + secret reveal
- `apps/web/src/components/repo/webhook-deliveries.tsrx` — history table + ping/redeliver
- `apps/web/src/routes/$owner.$repo.settings.tsrx` — mounts `WebhooksPanel` when `can_admin`
- `apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts` — Wave 0 stubs greened

## Decisions Made

- Inline Settings section (ASSUME discretion vs nested routes)
- Button variants use `secondary`/`ghost` (no `outline`/`sm` in Button cva)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Button variant/size**
- **Found during:** Task 2
- **Issue:** Draft deliveries UI used `variant="outline"` / `size="sm"` not in Button cva
- **Fix:** Switched to `secondary`/`ghost` + height classes
- **Files modified:** `webhook-deliveries.tsrx`
- **Commit:** fd79c16

## Self-Check: PASSED

- FOUND: apps/web/src/components/repo/webhooks-panel.tsrx
- FOUND: apps/web/src/components/repo/webhook-form.tsrx
- FOUND: apps/web/src/components/repo/webhook-deliveries.tsrx
- FOUND: 68ea220
- FOUND: fd79c16
