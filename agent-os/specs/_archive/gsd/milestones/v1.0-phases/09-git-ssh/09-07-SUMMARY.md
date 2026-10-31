---
phase: 09-git-ssh
plan: "07"
subsystem: ui
tags: [octane, ssh-keys, settings, AlertDialog]
requires:
  - phase: 09-06
    provides: sshKey api-client + Query helpers
provides:
  - /settings/ssh-keys list/add/revoke UI
  - SettingsNav + chrome Account SSH keys link
affects: [09-08-clonebox]
actuals:
  tokens: 6034
  tasks: 2
  commits: 1
plan_head_before: "4671fb7392246b0102bd64c2564911405796d857"
tech-stack:
  added: []
  patterns:
    - "SSH keys settings mirrors PAT: Query list + AlertDialog revoke; no one-time secret"
key-files:
  created:
    - apps/web/src/routes/settings/ssh-keys.tsrx
    - apps/web/src/components/settings/ssh-key-list.tsrx
    - apps/web/src/components/settings/ssh-key-add-form.tsrx
    - apps/web/src/components/settings/ssh-key-revoke-dialog.tsrx
  modified:
    - apps/web/src/components/settings/settings-nav.tsrx
    - apps/web/src/components/chrome.tsrx
    - apps/web/src/routeTree.gen.ts
key-decisions:
  - "Single feat commit for list+nav+revoke (tests green together)"
requirements-completed: [GIT-04]
coverage:
  - id: D1
    description: /settings/ssh-keys list, verify wall, nav, revoke dialog
    requirement: GIT-04
    verification:
      - kind: automated_ui
        ref: bunx vitest run src/routes/settings/ssh-keys.integration.test.ts
        status: pass
    human_judgment: false
duration: 25min
completed: 2026-09-14
status: complete
---

# Phase 09 Plan 07: SSH Keys Settings UI Summary

**Users can manage SSH public keys at `/settings/ssh-keys` with verify/cap gates, SettingsNav + Account links, and a Keep/Revoke confirm dialog.**

## Accomplishments

- Built Octane route + list/add/revoke components wired to `sshKey.*` Query/mutations.
- Disabled Add when unverified or at 25 keys; revoke uses AlertDialog copy from UI-SPEC.
- Extended SettingsNav and chrome Account menu with SSH keys.

## Task Commits

1. **Task 1+2: page + revoke** - `9f9f25c` (feat)

## Deviations from Plan

Combined Task 1+2 into one commit after green vitest + build.

## Self-Check: PASSED

- FOUND: apps/web/src/routes/settings/ssh-keys.tsrx
- FOUND: 9f9f25c
