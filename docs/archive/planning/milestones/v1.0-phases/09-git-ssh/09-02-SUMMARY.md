---
phase: 09-git-ssh
plan: "02"
subsystem: database
tags: [ssh, migrations, fingerprint, sqlx, oxidean-db]
requires:
  - phase: 09-00
    provides: dialect_ssh_keys Wave 0 stubs + migration parity expectation
provides:
  - Tri-dialect 0009_ssh_keys (ssh_public_keys, UNIQUE fingerprint)
  - oxidean-core ssh_key_types (SshKeyListItem, AddSshKeyRequest)
  - Database create/list/find_by_fingerprint/touch_last_used/revoke (hard-delete)
affects: [09-03-tracer, 09-04-rpc, 09-07-ui]
actuals:
  tokens: 6162
  tasks: 2
  commits: 1
plan_head_before: "17fabec90beb0d653bef928d60f89d4455ae1f69"
tech-stack:
  added: []
  patterns:
    - "SSH keys mirror PAT dialect CRUD without soft-revoke column"
    - "D-SSH-02 proceed: scp-style clone URL + single OXIDEAN_SSH_PORT"
key-files:
  created:
    - crates/oxidean-db/migrations/postgres/0009_ssh_keys.sql
    - crates/oxidean-db/migrations/mysql/0009_ssh_keys.sql
    - crates/oxidean-db/migrations/sqlite/0009_ssh_keys.sql
    - crates/oxidean-db/src/ssh_keys.rs
    - crates/oxidean-core/src/ssh_key_types.rs
  modified:
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-db/tests/dialect_ssh_keys.rs
    - crates/oxidean-core/src/lib.rs
key-decisions:
  - "D-SSH-02 proceed: scp-style git@host:owner/repo.git + single OXIDEAN_SSH_PORT for listen/advertise"
  - "Hard-delete on revoke (no revoked_at) per RESEARCH A3"
requirements-completed: [GIT-04]
coverage:
  - id: D1
    description: Tri-dialect 0009_ssh_keys with UNIQUE fingerprint
    requirement: GIT-04
    verification:
      - kind: unit
        ref: cargo test -p oxidean-db --lib migration_parity
        status: pass
      - kind: unit
        ref: cargo test -p oxidean-db --test dialect_ssh_keys
        status: pass
    human_judgment: false
  - id: D2
    description: Database facade SSH key CRUD + core DTOs without secret reveal field
    requirement: GIT-04
    verification:
      - kind: unit
        ref: cargo test -p oxidean-core --lib ssh_key
        status: pass
      - kind: unit
        ref: dialect_ssh_keys_migrate_0009_schema_presence
        status: pass
    human_judgment: false
duration: 3min
completed: 2026-09-13
status: complete
---

# Phase 09 Plan 02: SSH Key Schema + DB CRUD Summary

**Tri-dialect `0009_ssh_keys` + fingerprint CRUD landed after D-SSH-02 `proceed` (scp-style URL, single `OXIDEAN_SSH_PORT`).**

## Performance

- **Duration:** 3 min
- **Tasks:** 2 (T0 checkpoint + T1 schema)
- **Files:** 8

## Accomplishments

- Human confirmed D-SSH-02 `proceed` (scp-style primary clone URL; single listen/advertise port env).
- Shipped postgres/mysql/sqlite `0009_ssh_keys` with `ssh_public_keys` and UNIQUE `fingerprint`.
- Added `ssh_key_types` (`SshKeyListItem`, `AddSshKeyRequest`) — no one-time secret field.
- Wired `ssh_keys` dialect helpers + Database facade; greened `dialect_ssh_keys` round-trip (create → find → list DESC → touch → hard revoke + UNIQUE reject).

## Task Commits

| Task | Commit | Description |
|------|--------|-------------|
| 0 | — | D-SSH-02 decision: `proceed` (no code) |
| 1 | `ff7fe9e` | Migration 0009 + types + DB CRUD |

## Deviations from Plan

None - plan executed exactly as written after checkpoint resolution.

## Self-Check: PASSED

- FOUND: crates/oxidean-db/migrations/postgres/0009_ssh_keys.sql
- FOUND: crates/oxidean-db/src/ssh_keys.rs
- FOUND: crates/oxidean-core/src/ssh_key_types.rs
- FOUND: ff7fe9e
