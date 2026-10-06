---
phase: 08-git-https-pats
plan: "06"
subsystem: api
tags: [pat, smart-http, acl, rate-limit, git-02, last-used, email-unverified]

requires:
  - phase: 08-git-https-pats
    provides: 08-04 Smart HTTP tracer + classic PAT; 08-05 FG create
provides:
  - "Full GIT-02 Smart HTTP ACL/status matrix (private 401, scope 403, push)"
  - "Failed-auth rate limit 20/IP + 10/user per 15m → 429 + Retry-After"
  - "Unverified push deny (auth.email_unverified); fetch allowed"
  - "touch_pat_last_used on successful PAT auth"
affects:
  - 08-07 Traefik / Compose e2e
  - 08-09 CloneBox how-to
  - 08-11 settings tokens UI last-used display

actuals:
  tokens: 8423
  tasks: 2
  commits: 4

plan_head_before: 702e70d4894a53cfd99b0108267f19c17bec2f07

tech-stack:
  added: []
  patterns:
    - "Shared acl can_read_as_owner / is_private_visibility; git maps 401 not web not_found"
    - "In-memory FailedAuthLimiter on AppState (no new crates)"
    - "X-Forwarded-For first hop for last_used_ip / rate-limit key"

key-files:
  created:
    - crates/oxidean-api/src/pat/rate_limit.rs
    - .planning/phases/08-git-https-pats/08-06-SUMMARY.md
  modified:
    - crates/oxidean-api/src/routes/git_smart_http.rs
    - crates/oxidean-api/src/repo/acl.rs
    - crates/oxidean-api/src/repo/mod.rs
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/src/pat/mod.rs
    - crates/oxidean-api/tests/git_smart_http.rs

key-decisions:
  - "Prefixes remain oxidean_pat_ / oxidean_fg_ via pat_types (not ona_*)"
  - "Rate-limit IP test uses username alias git so user bucket (10) does not trip before IP (20)"
  - "Unverified receive-pack → 403 JSON auth.email_unverified; upload-pack still allowed"

patterns-established:
  - "PAT scope matrix: classic repo; FG selected|all+owner + contents read|write"
  - "Failed auth: check IP before Basic; record on failure; clear user bucket on success"

requirements-completed: [GIT-02]

coverage:
  - id: D1
    description: "Private anon upload-pack → 401 + WWW-Authenticate (D-21)"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/git_smart_http.rs#git_smart_private_anon_401_www_authenticate"
        status: pass
    human_judgment: false
  - id: D2
    description: "FG contents:read cannot receive-pack → 403 (D-23)"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/git_smart_http.rs#git_smart_insufficient_scope_403"
        status: pass
    human_judgment: false
  - id: D3
    description: "Classic PAT fetch/push refs OK; last_used_at/ip touched (D-09)"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/git_smart_http.rs#git_smart_pat_push_fetch_happy_path"
        status: pass
    human_judgment: false
  - id: D4
    description: "20 failed Basic from same IP → 429 + Retry-After (D-26)"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/git_smart_http.rs#git_smart_failed_auth_rate_limit_429_retry_after"
        status: pass
      - kind: unit
        ref: "crates/oxidean-api/src/pat/rate_limit.rs#ip_limit_trips_at_20"
        status: pass
    human_judgment: false
  - id: D5
    description: "Unverified may fetch with PAT; receive-pack denied with auth.email_unverified"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/git_smart_http.rs#git_smart_unverified_push_denied"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 06: Smart HTTP GIT-02 Matrix Summary

**Full Smart HTTP ACL/auth matrix: private 401, scope 403, classic push refs, last-used touch, failed-auth 429, unverified push deny.**

## Performance

- **Duration:** 6 min
- **Started:** 2026-09-13T18:44:30Z
- **Completed:** 2026-09-13T18:51:00Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- Expanded Smart HTTP beyond public-anon tracer to private 401, FG scope 403, and classic receive-pack advertisement with verified PAT
- Successful PAT auth updates `last_used_at` / `last_used_ip` (X-Forwarded-For)
- In-memory failed-auth limiter (20/IP, 10/user, 15m) returns 429 + Retry-After; unverified push blocked with `auth.email_unverified`

## Task Commits

Each task was committed atomically:

1. **Task 1 RED:** `975b6e1` — test(08-06): ACL/scope/last_used failing tests
2. **Task 1 GREEN:** `997852d` — feat(08-06): ACL, scope 403, last_used touch
3. **Task 2 RED:** `b1c1d2e` — test(08-06): rate-limit + unverified-push failing tests
4. **Task 2 GREEN:** `001ce79` — feat(08-06): rate limits + unverified push deny

**Plan metadata:** `56b99b7` (docs: complete plan); `f97e0cf` (docs: ROADMAP detailed checklist)

_Note: TDD tasks may have multiple commits (test → feat → refactor)_

- `crates/oxidean-api/src/pat/rate_limit.rs` — sliding-window IP/user counters
- `crates/oxidean-api/src/routes/git_smart_http.rs` — full ACL/scope/rate-limit/verify matrix
- `crates/oxidean-api/src/repo/acl.rs` — shared `can_read_as_owner` / `is_private_visibility`
- `crates/oxidean-api/src/app.rs` — `git_auth_limiter` on `AppState`
- `crates/oxidean-api/tests/git_smart_http.rs` — all eight `git_smart_*` cases green

## Decisions Made

- Token prefixes stay `oxidean_pat_` / `oxidean_fg_` (pat_types); never `ona_*`
- IP rate-limit integration test authenticates as username alias `git` so the 10/user bucket does not fire before 20/IP
- Unverified push returns 403 JSON with `auth.email_unverified`; fetch still allowed (Open Q2)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Rate-limit test username tripped user bucket early**
- **Found during:** Task 2 GREEN
- **Issue:** Using account username `rluser` recorded against the 10/user limit, so the 11th failure returned 429 before the IP (20) threshold
- **Fix:** Use Basic username alias `git` in the IP-limit integration test so only the IP bucket fills
- **Files modified:** `crates/oxidean-api/tests/git_smart_http.rs`
- **Commit:** `001ce79`

## Threat Flags

None — mitigations T-08-02 / T-08-05 / T-08-08 covered by plan threat model (receive-pack gates, private 401, failed-auth rate limit).

## Known Stubs

None — all prior `#[ignore]` expansion stubs for private/scope/429/unverified are implemented and green.

## Self-Check: PASSED
