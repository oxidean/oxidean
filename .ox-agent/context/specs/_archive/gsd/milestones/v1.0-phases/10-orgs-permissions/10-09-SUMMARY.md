---
phase: 10-orgs-permissions
plan: "09"
subsystem: api
tags: [user-lookup, autocomplete, anti-enumeration, rpc, org-01]

requires:
  - phase: 10-orgs-permissions
    provides: org members add-by-username + require_verified gates (10-05/10-07)
provides:
  - "user.lookup RPC with prefix≥2, ≤10 hits, no emails (D-ORG-03 / T-10-03)"
  - "Dialect-safe username prefix DB helper"
  - "Generated api-client user.lookup + query options"
affects: [10-10 members UI autocomplete, 10-11 collaborators UI]

actuals:
  tokens: 8533
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Lean UserLookupRow SELECT (no email) at DB boundary"
    - "Per-session in-memory LookupLimiter on AppState/RpcCtx"

key-files:
  created:
    - crates/oxidean-api/src/user/lookup.rs
    - crates/oxidean-api/src/user/rate_limit.rs
    - crates/oxidean-api/src/user/mod.rs
    - crates/oxidean-api/tests/user_lookup.rs
    - .planning/phases/10-orgs-permissions/.tdd/10-09-t1-red-evidence.json
  modified:
    - crates/oxidean-db/src/users.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-core/src/auth_types.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - crates/oxidean-api/tests/org_create_members.rs

key-decisions:
  - "user/ module for lookup (profile stays under auth/); matches user.* RPC namespace"
  - "60 lookups / session / 60s sliding window — simple per-process limiter"
  - "Email-shaped and short prefixes return empty ok list (no oracle error codes)"

patterns-established:
  - "Autocomplete hits: username + display_name + avatar_url only"
  - "LIKE ESCAPE for dialect-safe prefix search"

requirements-completed: [ORG-01]

coverage:
  - id: D1
    description: "user.lookup returns ≤10 public profiles for prefix ≥2; never emails"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/user_lookup.rs#user_lookup_prefix_returns_public_fields_without_email"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/org_create_members.rs#org_lookup_shape_prefix_limit_no_email"
        status: pass
    human_judgment: false
  - id: D2
    description: "Short/empty/email-shaped prefixes return empty lists; case-insensitive match"
    requirement: ORG-01
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/user_lookup.rs#user_lookup_short_prefix_returns_empty"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/user_lookup.rs#user_lookup_email_shaped_prefix_returns_empty"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/user_lookup.rs#user_lookup_prefix_is_case_insensitive"
        status: pass
    human_judgment: false
  - id: D3
    description: "Generated api-client exposes user.lookup; rpc-sync-check clean"
    requirement: ORG-01
    verification:
      - kind: other
        ref: "make rpc-sync-check"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-09-14
status: complete
plan_head_before: 7e9d2db9e9aba585b2c2ed02401064a1443f2b55
commits: 3
---

# Phase 10 Plan 09: Username Lookup Summary

**Live `user.lookup` autocomplete RPC with prefix/limit/rate-limit anti-enumeration and no email leakage (ORG-01 / D-ORG-03).**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-14T01:27:51Z
- **Completed:** 2026-09-14T01:34:20Z
- **Tasks:** 2
- **Files modified:** 16

## Accomplishments

- Shipped `user.lookup` behind `require_verified` with per-session rate limiting
- Dialect-safe username prefix search (PG/MySQL/SQLite) returning lean public rows only
- Regenerated `@oxidean/api-client` with `user.lookup` + query options; sync-check green

## Task Commits

Each task was committed atomically (TDD RED → GREEN on Task 1):

1. **Task 1 RED:** `5c0455d` — failing user.lookup anti-enumeration tests
2. **Task 1 GREEN:** `c0d43db` — implement user.lookup + rpc-gen
3. **Task 2:** `217cd28` — expand coverage (case/empty + org_lookup shape)

| Task | RED commit | GREEN commit | RED evidence |
|------|------------|--------------|--------------|
| T1 | `5c0455d` | `c0d43db` | `.tdd/10-09-t1-red-evidence.json` → `RED_EVIDENCE_OK` |
| T2 | — | `217cd28` | coverage only (not tdd) |

## Files Created/Modified

- `crates/oxidean-api/src/user/lookup.rs` — RPC handler (prefix gates + DB map)
- `crates/oxidean-api/src/user/rate_limit.rs` — 60/session/60s sliding window
- `crates/oxidean-db/src/users.rs` — `list_by_username_prefix` + `UserLookupRow`
- `crates/oxidean-core/src/auth_types.rs` — `UserLookupRequest` / `Hit` / `Response`
- `crates/oxidean-api/tests/user_lookup.rs` — integration suite
- `packages/api-client/src/index.ts` — generated client

## Decisions Made

- New `user/` module for lookup (keeps `auth/profile` for get/update profile)
- Empty ok lists for short/email-shaped queries (no distinct error that aids enumeration)
- Rate limit keyed by `session_id` on shared `AppState` limiter

## Deviations from Plan

None - plan executed exactly as written (Task 2 expanded coverage in dedicated + org-colocated tests as allowed).

## TDD Gate Compliance

| Task | RED | GREEN | REFACTOR | Evidence |
|------|-----|-------|----------|----------|
| T1 | ✓ `5c0455d` + RED_EVIDENCE_OK | ✓ `c0d43db` | — | Pass |

## Threat Flags

None — mitigations match plan threat model (T-10-03 prefix/limit/rate-limit/no-email; T-10-SC no new packages).

## Known Stubs

None.

## Self-Check: PASSED
