---
phase: 19-actions-runners
plan: "10"
subsystem: api,web
tags: [actions, secrets, runners, admin, policy]

requires:
  - phase: 19-actions-runners
    provides: runner protocol + runs UI (19-05, 19-09)
provides:
  - AES-GCM repo Actions secrets + enable RPC
  - Admin registration tokens + runner list UI
  - Repo Settings Actions panel
  - ACT-07 registered-runners-only policy tests + docs
affects: [19-11]

actuals:
  tokens: 62000
  tasks: 3
  commits: 3

tech-stack:
  added: [aes-gcm, getrandom]
  patterns: [secrets write-only RPC, FetchTask secrets injection]

key-files:
  created:
    - crates/oxidean-api/src/actions/secrets.rs
    - crates/oxidean-api/tests/actions_secrets.rs
    - apps/web/src/routes/admin/runners.tsrx
    - apps/web/src/components/repo/actions-settings-panel.tsrx
    - apps/web/src/routes/$owner.$repo.settings.actions.tsrx
  modified:
    - crates/oxidean-api/src/actions/rpc.rs
    - crates/oxidean-api/src/actions/runner_proto.rs
    - packages/api-client/src/index.ts
    - docs/CONFIGURATION.md

key-decisions:
  - "Secrets stored as hex(nonce||ciphertext) with OXIDEAN_ACTIONS_SECRETS_KEY fallback chain"
  - "List secrets returns names only; values injected only via FetchTask to assigned runner"
  - "No managed CI minutes — copy in Admin + Settings states registered-runners-only (ACT-07)"

patterns-established:
  - "repo.actions.secrets.* + getEnabled/setEnabled session RPC"
  - "admin.actions.createRegistrationToken + listRunners"

requirements-completed: [ACT-06, ACT-07]

coverage:
  - id: T1
    description: actions_secrets + rpc-sync-check
    requirement: ACT-06
    verification:
      - kind: test
        ref: cargo nextest actions_secrets
        status: pass
    human_judgment: false
  - id: T2
    description: Admin runners + Settings Actions UI lint/format
    requirement: ACT-06
    verification:
      - kind: lint
        ref: make web-lint web-format-check
        status: pass
    human_judgment: false
  - id: T3
    description: dispatch policy + CONFIGURATION ACT-07 note
    requirement: ACT-07
    verification:
      - kind: test
        ref: cargo nextest actions_dispatch_policy
        status: pass
    human_judgment: false

plan_head_before: 8fc33de
duration: 90min
completed: 2026-09-16
status: complete
---

# Phase 19 Plan 10: Secrets, Admin Runners, Settings Actions

**Repo Actions secrets (encrypt at rest, write-only read path), enable toggle RPC, Admin registration-token UX, Settings Actions panel, and explicit no-managed-minutes policy.**

## Deviations from Plan

- Commit messages on branch use `encrypt/store` and `assert registered-runners-only` wording (same scope).

## Self-Check: PASSED
