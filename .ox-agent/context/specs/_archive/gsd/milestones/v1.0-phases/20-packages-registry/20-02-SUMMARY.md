---
phase: 20-packages-registry
plan: "02"
subsystem: database
tags: [packages, migration, traefik, vite, packages-dir]

requires:
  - phase: 20-packages-registry
    provides: Wave 0 dialect_packages stub
provides:
  - Tri-dialect 0015_packages schema
  - OXIDEAN_PACKAGES_DIR Compose volume + docs
  - Traefik/Vite /v2|/npm|/generic routing; reserved slugs
affects: [20-03, 20-04, 20-05, 20-06, 20-12]

actuals:
  tokens: 5148
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns: [sqlx migrate 0015_packages, Traefik api-packages PathPrefix]

key-files:
  created:
    - crates/oxidean-db/migrations/postgres/0015_packages.sql
    - crates/oxidean-db/migrations/mysql/0015_packages.sql
    - crates/oxidean-db/migrations/sqlite/0015_packages.sql
  modified:
    - crates/oxidean-db/tests/dialect_packages.rs
    - docker-compose.yml
    - .env.example
    - docs/CONFIGURATION.md
    - crates/oxidean-core/src/auth_types.rs
    - apps/web/vite.config.ts

key-decisions:
  - "Resolved migration id to 0015_packages (after 0011_issues)"
  - "Unlinked packages default visibility private; optional repository_id link"

patterns-established:
  - "Registry edge: Traefik api-packages priority 110 mirrors api-git"

requirements-completed: [PKG-04, PKG-01, PKG-02, PKG-03]

coverage:
  - id: D1
    description: Tri-dialect packages migration applies
    requirement: PKG-04
    verification:
      - kind: unit
        ref: cargo test -p oxidean-db --test dialect_packages
        status: pass
    human_judgment: false
  - id: D2
    description: PACKAGES_DIR + Traefik/Vite routing
    requirement: PKG-01
    verification:
      - kind: other
        ref: rg PathPrefix api-packages docker-compose.yml
        status: pass
    human_judgment: false

plan_head_before: 7d0a9413b02fd4ef37cc2419831465f8bde58b0a
duration: 12min
completed: 2026-09-14
status: complete
---

# Phase 20 Plan 02: Schema + Edge Routing Summary

**Tri-dialect `0015_packages` schema, separate `OXIDEAN_PACKAGES_DIR` volume, and Traefik/Vite PathPrefix routing for `/v2|/npm|/generic` with reserved usernames.**

## Performance

- **Duration:** ~12 min
- **Tasks:** 3
- **Files modified:** 9

## Accomplishments
- packages / versions / blobs / blob_refs / quota_overrides tables on postgres/mysql/sqlite
- Compose `./var/packages` + env defaults (2 GiB blob / 10 GiB owner)
- Reserved `v2`/`npm`/`generic`; Traefik `api-packages` + Vite proxies

## Task Commits

1. **Task 1: Tri-dialect packages migration** - `479e76f` (feat)
2. **Task 2: OXIDEAN_PACKAGES_DIR + Compose volume** - `cdbd746` (feat)
3. **Task 3: Reserved usernames + Vite + Traefik** - `095e056` (feat)

## Decisions Made
- Migration number **0012** (after issues 0011)
- Linked packages use `repository_id`; unlinked default `visibility=private`

## Deviations from Plan
None — plan executed as written.

## Self-Check: PASSED
- FOUND: 0015_packages.sql ×3 dialects
- FOUND: dialect_packages green
- FOUND: 479e76f, cdbd746, 095e056
