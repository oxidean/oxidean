---
phase: 07-git-repos-browse
plan: "17"
subsystem: infra
tags: [git, docker, compose, boot-gate, D-33]

requires:
  - phase: 07-git-repos-browse
    provides: "D-33 fail_boot_git locked (07-01); proceed_locked (07-02); version stubs (07-00); create tracer (07-12)"
provides:
  - "assert_git_version((2,5,0)) green + API boot exit(1) on missing/old git"
  - "Dockerfile installs git; Compose ./var/repos:/var/repos; .env.example OXIDEAN_REPOS_DIR"
affects: [07-10, 07-11, compose-ops]

actuals:
  tokens: 1440
  tasks: 1
  commits: 3

plan_head_before: c6f7cd3a35691cc9313e22cffa06ae6e1f599cc6

tech-stack:
  added: []
  patterns:
    - "Boot fail-closed git version gate before bind/serve"
    - "Compose repos bind mirrors uploads (CWD / → /var/repos)"

key-files:
  created: []
  modified:
    - crates/oxidean-git/src/version.rs
    - crates/oxidean-api/src/main.rs
    - crates/oxidean-api/Dockerfile
    - docker-compose.yml
    - .env.example

key-decisions:
  - "Fail boot with eprintln + exit(1) when git missing or < 2.5.0 (D-33)"
  - "Install distro git in API image; bind ./var/repos without overriding default OXIDEAN_REPOS_DIR"

patterns-established:
  - "parse_git_version strips Apple/Windows suffixes; tuple compare vs floor"
  - "Repos volume path aligned with AppState default var/repos under CWD /"

requirements-completed: [GIT-09, GIT-08]

coverage:
  - id: D1
    description: "API boot exits 1 when git missing or older than 2.5.0"
    requirement: GIT-09
    verification:
      - kind: unit
        ref: "cargo nextest run -p oxidean-git -E 'test(git_version_gate)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Dockerfile installs git; Compose binds ./var/repos; OXIDEAN_REPOS_DIR documented"
    requirement: GIT-08
    verification:
      - kind: other
        ref: "rg -n 'git' crates/oxidean-api/Dockerfile; rg -n 'var/repos|OXIDEAN_REPOS_DIR' docker-compose.yml .env.example"
        status: pass
    human_judgment: false

duration: 1min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 17: Fail-boot git gate + Compose repos volume Summary

**API refuses to start without git ≥2.5; Compose/Dockerfile ship the operator repos volume contract.**

## Performance

- **Duration:** 1 min
- **Started:** 2026-09-12T17:12:43Z
- **Completed:** 2026-09-12T17:13:40Z
- **Tasks:** 1
- **Files modified:** 5

## Accomplishments

- Implemented `parse_git_version` / `assert_git_version` and wired fail-boot in `oxidean-api` main (D-33 / GIT-09)
- Installed `git` in the API Dockerfile; bound `./var/repos:/var/repos` in Compose (D-30 / D-38)
- Documented `OXIDEAN_REPOS_DIR` (default `var/repos`) in `.env.example` (D-31)

## Task Commits

Each task was committed atomically:

1. **Task 1: Fail-boot git gate + Dockerfile/Compose repos volume** - `3cd14a4` (feat)

**Plan metadata:** `4f49b61` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-git/src/version.rs` — green `git_version_gate` helpers
- `crates/oxidean-api/src/main.rs` — boot gate → `exit(1)`
- `crates/oxidean-api/Dockerfile` — apt `git`
- `docker-compose.yml` — `./var/repos:/var/repos`
- `.env.example` — `OXIDEAN_REPOS_DIR` docs

## Decisions Made

- Soft-continue on missing/old git is prohibited; match CORS/DB fail style
- Compose uses relative default path via CWD `/` (same as uploads), no forced env override

## Deviations from Plan

None - plan executed exactly as written.

## Known Stubs

- `crates/oxidean-git/src/version.rs` — `git_archive_formats_zip_and_tar_gz` still Wave 0 `assert!(false)` (owned by archive plan, not 07-17)

## Threat Flags

None — surface matches plan threat model (T-07-08 mitigated; T-07-SC accepted).

## Verification

```
cargo nextest run -p oxidean-git -E 'test(git_version_gate)'  # 2 passed
rg assert_git_version / exit(1) / Dockerfile git / var/repos / OXIDEAN_REPOS_DIR  # green
cargo check -p oxidean-api --bin oxidean-api  # ok
```

## Self-Check: PASSED

- FOUND: crates/oxidean-git/src/version.rs
- FOUND: crates/oxidean-api/src/main.rs
- FOUND: crates/oxidean-api/Dockerfile
- FOUND: docker-compose.yml
- FOUND: .env.example
- FOUND: commit 3cd14a4
