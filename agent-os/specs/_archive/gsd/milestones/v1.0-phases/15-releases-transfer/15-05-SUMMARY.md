---
phase: 15-releases-transfer
plan: "05"
subsystem: ui
tags: [settings, releases, factory-reset, octane, docs]
requires:
  - phase: 15-releases-transfer
    provides: "repo.rename/transfer + release assets from 15-02..04"
provides:
  - "Danger zone rename + transfer UI with type-confirm"
  - "repo-chrome Settings via can_admin"
  - "factory reset wipe of release-assets"
  - "API.md releases/rename/transfer docs"
affects: []
actuals:
  tokens: 6253
  tasks: 3
  commits: 1
plan_head_before: c44218570f7db7adbda506ceeac0c25ca3357944
tech-stack:
  added: []
  patterns: ["Danger zone IA shares soft-delete section; wipe_dir_contents shared for repos + assets"]
key-files:
  created: []
  modified:
    - apps/web/src/routes/$owner.$repo.settings.tsrx
    - apps/web/src/components/repo/repo-chrome.tsrx
    - crates/oxidean-api/src/auth/admin.rs
    - docs/API.md
key-decisions:
  - "Settings nav gated on repo.can_admin (not username equality)"
  - "Factory reset repos scope also clears OXIDEAN_RELEASE_ASSETS_DIR children"
requirements-completed: [GIT-14, GIT-15, GIT-16, GIT-17]
coverage:
  - id: D1
    description: "Danger zone rename/transfer + can_admin chrome"
    requirement: GIT-16
    verification:
      - kind: unit
        ref: "apps/web src/routes/$owner.$repo.settings.rename-transfer.integration.test.ts"
        status: pass
    human_judgment: false
  - id: D2
    description: "Factory reset wipes release-assets children"
    requirement: GIT-15
    verification:
      - kind: unit
        ref: "auth::admin::tests::factory_reset_wipe_release_assets_removes_children_keeps_root"
        status: pass
    human_judgment: false
  - id: D3
    description: "API/CONFIG docs for releases, rename/transfer, ENV"
    verification:
      - kind: other
        ref: "rg release.|repo.rename|OXIDEAN_RELEASE docs/API.md docs/CONFIGURATION.md"
        status: pass
    human_judgment: false
duration: 20min
completed: 2026-09-14
status: complete
---

# Phase 15 Plan 05: Settings, Factory Reset, Docs Summary

**Admin Danger zone rename/transfer UI, can_admin Settings chrome, factory-reset wipe of release assets, and API docs close Phase 15 surfaces (GIT-14..17).**

## Task Commits

| Task | Commit |
|------|--------|
| 1–3 UI + factory reset + docs | `7f3236a` |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Combined T1–T3 into one commit**
- Shared settings/chrome/docs/admin wipe surface; Vitest + nextest green together

## Self-Check: PASSED

- FOUND: settings rename/transfer UI, can_admin chrome, wipe_dir_contents + unit test, API.md release/rename/transfer
- Vitest 4/4; factory_reset|release_asset nextest 12/12; rpc-sync-check ok
