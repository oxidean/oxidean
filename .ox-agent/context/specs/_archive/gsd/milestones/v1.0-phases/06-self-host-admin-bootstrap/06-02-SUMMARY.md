---
phase: 06-self-host-admin-bootstrap
plan: "02"
subsystem: auth
tags: [auth-06, env-seed, system-administrator, must_change_credentials, confirm_admin_credentials, fail_closed]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: 0006 bootstrap flags + UserPublic.must_change_credentials + allow_signup DTOs
provides:
  - "ENV seed username system-administrator + must_change_credentials + OXIDEAN_ALLOW_SIGNUP"
  - "auth.confirm_admin_credentials RPC (reject default username; keep_password path)"
  - "Fail-closed boot retained (D-14 fail_closed human decision)"
affects:
  - 06-03 wizard allow_signup + RPC allowlist
  - 06-05 SSR must_change gate
  - 06-06 /setup/credentials UI

actuals:
  tokens: 6627
  tasks: 3
  commits: 6

plan_head_before: e7be77e2ca3732e811c9bc627dfc1bbd04b5a4f7

tech-stack:
  added: []
  patterns:
    - "OXIDEAN_ALLOW_SIGNUP parsed like AUTO_MIGRATE (true/1) with default false"
    - "confirm_admin_credentials: session + must_change gate; case-insensitive system-administrator reject"

key-files:
  created:
    - crates/oxidean-api/src/auth/bootstrap.rs
  modified:
    - crates/oxidean-api/src/auth/seed.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-core/src/auth_types.rs
    - crates/oxidean-api/tests/auth_signup.rs
    - crates/oxidean-api/tests/auth_forced_credentials.rs
    - crates/oxidean-api/tests/auth_bootstrap.rs

key-decisions:
  - "D-14 fail_closed: keep exit(1) when both ADMIN ENV set and maybe_seed_admin returns Err (do not serve wizard fallback)"
  - "Landed previously untracked bootstrap.rs as tracked module with confirm_admin_credentials"

patterns-established:
  - "ENV seed writes allow_signup via get+update_auth_settings preserving other settings fields"
  - "Forced confirm clears must_change only after username leaves system-administrator"

requirements-completed: [AUTH-06]

coverage:
  - id: D1
    description: "ENV seed creates system-administrator with must_change_credentials and OXIDEAN_ALLOW_SIGNUP"
    requirement: AUTH-06
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(seeded_admin)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "auth.confirm_admin_credentials rejects default username and clears must_change on success"
    requirement: AUTH-06
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(confirm_admin) | test(forced_credentials)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Partial ENV and seed idempotency (D-13 / second-run no-op)"
    requirement: AUTH-06
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(partial_env) | test(seed_partial) | test(seed_second_run) | test(seeded_admin)'"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 02: ENV Seed + Confirm Credentials Summary

**AUTH-06 API tracer: ENV seeds `system-administrator` with forced-credential flag, fail-closed boot kept, and `auth.confirm_admin_credentials` clears the flag.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-11T20:37:35Z
- **Completed:** 2026-09-11T20:41:47Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- Human confirmed D-14 **fail_closed** — `main.rs` still `exit(1)` on seed Err; no wizard fallback
- `maybe_seed_admin` uses fixed `system-administrator`, sets `must_change_credentials`, applies `OXIDEAN_ALLOW_SIGNUP`
- Tracked `bootstrap.rs` + `auth.confirm_admin_credentials` (reject default username; keep-password path)
- Partial ENV / empty-string / second-seed idempotency covered under nextest

## Task Commits

1. **Task 1: Confirm fail-closed boot (D-14)** — decision only (`fail_closed`); no code commit
2. **Task 2: E2E ENV seed → must_change → confirm credentials** — `93c33ea` (feat)
3. **Task 3: Partial ENV + seed idempotency expansion** — `ad568bb` (test)

**Plan metadata:** `2f5b546` (docs: complete plan); `f8ada7e` (SUMMARY metadata/count sync)

## Decisions Made

- **fail_closed (D-14):** Keep process exit on seed failure when both ADMIN ENV vars are set
- Untracked WIP `bootstrap.rs` landed as intentional tracked module (wizard handlers + new confirm RPC)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Forced-credentials login omitted `remember_me`**
- **Found during:** Task 2
- **Issue:** Login JSON missing `remember_me` failed deserialization so confirm tests never obtained a session cookie
- **Fix:** Include `remember_me:false` in test login; require cookie presence
- **Files modified:** `crates/oxidean-api/tests/auth_forced_credentials.rs`
- **Commit:** `93c33ea`

**2. [Rule 3 - Blocking] Plan verify filter includes 06-03 Wave 0 RED**
- **Found during:** Task 3
- **Issue:** `test(bootstrap)` also matches allowlist + wizard `allow_signup` stubs owned by 06-03
- **Fix:** Verified Task 3 acceptance via scoped filters (`partial_env` / `seed_*` / `seeded_admin`); deferred remaining bootstrap RED to `deferred-items.md`
- **Files modified:** `deferred-items.md`
- **Commit:** docs commit with SUMMARY

## TDD Notes (Task 3)

- Wave 0 (06-00) already held RED stubs for partial ENV; Task 2 made seed behavior green
- Task 3 added empty-string ENV + dedicated idempotency tests against the Task 2 implementation (no further seed.rs changes required)

## Auth Gates

None.

## Known Stubs

None in 06-02 deliverables. Wizard `allow_signup` / strict RPC stubs remain intentional RED for 06-03 (see deferred-items.md).

## Threat Flags

None beyond plan threat model (confirm RPC is the planned AUTH-06 surface).

## Self-Check: PASSED
