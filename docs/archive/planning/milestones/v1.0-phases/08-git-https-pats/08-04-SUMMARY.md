---
phase: 08-git-https-pats
plan: "04"
subsystem: api
tags: [pat, smart-http, git-http-backend, basic-auth, oxidean_pat, git-02, git-11]

requires:
  - phase: 08-git-https-pats
    provides: 08-03 PAT schema + pat_types with oxidean_pat_/oxidean_fg_
provides:
  - "pat.createClassic / list / revoke RPC (classic mint with oxidean_pat_)"
  - "Smart HTTP CGI via git-http-backend on /{owner}/{repo}.git"
  - "PAT Basic auth; password reject + cookie ignore"
affects:
  - 08-05 fine-grained PAT create
  - 08-06 private/push/rate-limit expansion
  - 08-08 rpc-gen for api-client

actuals:
  tokens: 13400
  tasks: 2
  commits: 3

plan_head_before: e0a77baaf30833a9a491b24b0e93b86e033a811f

tech-stack:
  added: []
  patterns:
    - "PAT mint reuses session sha256_hex/bytes_to_hex; plaintext once"
    - "Axum captures {repo_git} segment (*.git); strip suffix before bare_repo_path"
    - "git-http-backend CGI with GIT_HTTP_EXPORT_ALL + REMOTE_USER"

key-files:
  created:
    - crates/oxidean-api/src/pat/mod.rs
    - crates/oxidean-api/src/git/http_backend.rs
    - crates/oxidean-api/src/routes/git_smart_http.rs
    - .planning/phases/08-git-https-pats/08-04-SUMMARY.md
  modified:
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/lib.rs
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/src/auth/session.rs
    - crates/oxidean-api/src/git/mod.rs
    - crates/oxidean-api/src/routes/mod.rs
    - crates/oxidean-api/tests/pat_rpc.rs
    - crates/oxidean-api/tests/git_smart_http.rs

key-decisions:
  - "D-08 mint uses oxidean_pat_ (CLASSIC_PAT_PREFIX), not plan-text ona_pat_"
  - "Axum 0.8 forbids {repo}.git suffix syntax — use {repo_git} full segment"
  - "Expansion git_smart cases #[ignore] until 08-06 (private/scope/429/push)"

patterns-established:
  - "pat.* RPC beside repo.* with require_verified on create only"
  - "Smart HTTP ignores Cookie; identity from Basic PAT password hash lookup"

requirements-completed: [GIT-02, GIT-11]

coverage:
  - id: D1
    description: "Verified createClassic returns one-time oxidean_pat_ token; list omits secret; revoke hides"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_create_classic_returns_one_time_token"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_list_omits_secret_token"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_revoke_removes_from_list"
        status: pass
    human_judgment: false
  - id: D2
    description: "Unverified createClassic → auth.email_unverified; empty note → pat.note_required"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_create_unverified_email_unverified"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/pat_rpc.rs#pat_create_empty_note_required"
        status: pass
    human_judgment: false
  - id: D3
    description: "Public anon + PAT Basic upload-pack info/refs; password reject; cookie ignore"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "crates/oxidean-api/tests/git_smart_http.rs#git_smart_public_anon_upload_pack_info_refs_ok"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/git_smart_http.rs#git_smart_pat_push_fetch_happy_path"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/git_smart_http.rs#git_smart_basic_account_password_rejected_401"
        status: pass
      - kind: integration
        ref: "crates/oxidean-api/tests/git_smart_http.rs#git_smart_session_cookie_ignored_as_anon"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 04: Classic PAT + Smart HTTP Tracer Summary

**Classic `oxidean_pat_` mint/list/revoke RPC plus production Smart HTTP upload-pack via `git-http-backend` with PAT Basic auth (password rejected, cookies ignored)**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-09-13T18:28:19Z
- **Completed:** 2026-09-13T18:36:38Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments

- `pat.createClassic` / `list` / `revoke` with verified gate, note required, SHA-256 at rest, one-time plaintext
- Smart HTTP routes on `/{owner}/{repo}.git/{info/refs,git-upload-pack,git-receive-pack}`
- CGI spawn with `GIT_HTTP_EXPORT_ALL` + `REMOTE_USER`; public anon fetch; PAT Basic OK; account password → 401 + PAT hint

## Task Commits

1. **Task 1: pat.createClassic / list / revoke RPC** - `dbe0168` (feat)
2. **Task 2: Smart HTTP upload-pack tracer** - `eee81d8` (feat)
3. **Docs note: WWW-Authenticate** - `d795b86` (docs)

**Plan metadata:** (final docs commit after this SUMMARY)

## Files Created/Modified

- `crates/oxidean-api/src/pat/mod.rs` — classic PAT RPC handlers
- `crates/oxidean-api/src/git/http_backend.rs` — git-http-backend CGI helper
- `crates/oxidean-api/src/routes/git_smart_http.rs` — Basic/PAT auth + CGI gate
- `crates/oxidean-api/src/rpc.rs` / `app.rs` / `lib.rs` / `routes/mod.rs` / `git/mod.rs` — wiring
- `crates/oxidean-api/src/auth/session.rs` — pub `sha256_hex` / `bytes_to_hex`
- `crates/oxidean-api/tests/pat_rpc.rs` / `git_smart_http.rs` — Wave 0 → green tracer

## Decisions Made

- Mint prefix locked to `oxidean_pat_` (D-08 from 08-02/08-03), ignoring stale `ona_pat_` in plan prose
- Axum 0.8 path: capture `{repo_git}` including `.git` suffix (cannot use `{repo}.git` template)
- Expansion Smart HTTP tests `#[ignore]` for 08-06 (private 401, scope 403, 429, unverified push)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Axum rejects `{repo}.git` route syntax**
- **Found during:** Task 2
- **Issue:** Axum 0.8 panics: "Only one parameter is allowed per path segment"
- **Fix:** Routes use `/{owner}/{repo_git}/…` and strip `.git` in-handler
- **Files modified:** `app.rs`, `git_smart_http.rs`
- **Committed in:** `eee81d8`

**2. [Rule 1 - Bug] CGI Content-Type lost under default octet-stream**
- **Found during:** Task 2 (public/PAT info/refs assertions)
- **Issue:** `body.into_response()` set `application/octet-stream` before CGI headers appended
- **Fix:** Build `Response` with CGI `HeaderMap` only (insert Content-Type from backend)
- **Files modified:** `http_backend.rs`
- **Committed in:** `eee81d8`

**3. [Rule 2 - Critical] Plan prose `ona_pat_` → `oxidean_pat_`**
- **Found during:** Task 1 (critical_deviation lock)
- **Issue:** Plan still named `ona_pat_` prefix
- **Fix:** Mint/match `CLASSIC_PAT_PREFIX` (`oxidean_pat_`) only
- **Files modified:** `pat/mod.rs`, tests
- **Committed in:** `dbe0168`

**Total deviations:** 3 auto-fixed (Rules 1–3)
**Impact on plan:** Required for Axum/CGI correctness and locked D-08 branding; no scope creep

## Issues Encountered

None beyond the Axum route and CGI header fixes above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 08-05 can implement `pat.createFineGrained` (currently stub `pat.not_implemented`)
- 08-06 can un-ignore private/push/rate-limit Smart HTTP cases
- 08-08 should run `make rpc-gen` for `pat.*` client types (not hand-edited here)

## Known Stubs

| File | Stub | Reason |
|------|------|--------|
| `crates/oxidean-api/src/pat/mod.rs` | `create_fine_grained` → `pat.not_implemented` | Deferred to 08-05 |
| `crates/oxidean-api/tests/git_smart_http.rs` | 4 tests `#[ignore]` | Expansion in 08-06 |

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/pat/mod.rs`
- FOUND: `crates/oxidean-api/src/git/http_backend.rs`
- FOUND: `crates/oxidean-api/src/routes/git_smart_http.rs`
- FOUND: `dbe0168`, `eee81d8`, `d795b86`

---
*Phase: 08-git-https-pats*
*Completed: 2026-09-13*
