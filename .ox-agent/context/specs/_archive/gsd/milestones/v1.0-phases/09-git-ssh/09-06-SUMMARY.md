---
phase: 09-git-ssh
plan: "06"
subsystem: api
tags: [rpc-gen, api-client, sshKey, docs]
requires:
  - phase: 09-03
    provides: sshKey.add/list/revoke RPC handlers
provides:
  - Generated @oxidean/api-client sshKey.* + Query helpers
  - docs/API.md SSH key + Git-over-SSH sections
affects: [09-07-ui, 09-08-clonebox]
actuals:
  tokens: 3331
  tasks: 1
  commits: 1
plan_head_before: "6e7f75da4b8f96370689325f7908cc493450af2c"
tech-stack:
  added: []
  patterns:
    - "rpc_gen.rs template is SoT for TS client; never hand-edit packages/api-client"
key-files:
  created: []
  modified:
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - docs/API.md
key-decisions:
  - "sshKey client mirrors pat: list query + add/revoke mutations"
requirements-completed: [GIT-04]
coverage:
  - id: D1
    description: make rpc-gen emits sshKey.add/list/revoke; rpc-sync-check passes
    requirement: GIT-04
    verification:
      - kind: other
        ref: make rpc-sync-check
        status: pass
    human_judgment: false
duration: 12min
completed: 2026-09-14
status: complete
---

# Phase 09 Plan 06: sshKey api-client + API.md Summary

**`@oxidean/api-client` now exposes `sshKey.add` / `list` / `revoke` with Query helpers, and API.md documents keys as full account identity for Git-over-SSH.**

## Performance

- **Duration:** ~12 min
- **Tasks:** 1
- **Files:** 3

## Accomplishments

- Extended `rpc_gen.rs` template with SSH key types, client methods, and TanStack helpers.
- Regenerated `packages/api-client/src/index.ts`; `make rpc-sync-check` green.
- Documented `sshKey.*` RPCs, error codes, and scp-style Git-over-SSH in `docs/API.md`.

## Task Commits

1. **Task 1: rpc-gen + API.md** - `6b5278d` (feat)

## Decisions Made

- Document SSH keys as full-account identity (no PAT scopes) and force user `git`.

## Deviations from Plan

None - plan has no separate `types.ts`; client lives in generated `index.ts` only.

## Self-Check: PASSED

- FOUND: packages/api-client sshKey
- FOUND: 6b5278d
- rpc-sync-check: ok
