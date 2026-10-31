---
phase: 08-git-https-pats
plan: "00"
subsystem: testing
tags: [wave0, nyquist, pat, git-smart-http, git-02, git-11, nextest, dialect]

requires:
  - phase: 07-git-repos-browse
    provides: auth/repo integration harness patterns + bare-repo layout for later Smart HTTP greens
provides:
  - "Wave 0 RED pat_* RPC stubs (createClassic/list/revoke, email_unverified, note_required)"
  - "Wave 0 RED git_smart_* Smart HTTP auth/ACL status stubs"
  - "Wave 0 dialect_pats stub for 0008_pats + personal_access_tokens"
affects:
  - 08-03 schema migration 0008_pats
  - 08-04 pat RPC + Smart HTTP greens
  - 08-06 rate-limit / unverified push greens

actuals:
  tokens: 1477
  tasks: 1
  commits: 1

plan_head_before: 51b1bedad7b2e477d483740ce3d341737e7d3b0e

tech-stack:
  added: []
  patterns:
    - "Wave 0 intentional RED stubs with assert!(false) until PAT/Smart HTTP land"
    - "pat_ / git_smart / dialect_pats nextest filters mirror VALIDATION.md Wave 0 checklist"

key-files:
  created:
    - crates/oxidean-api/tests/pat_rpc.rs
    - crates/oxidean-api/tests/git_smart_http.rs
    - crates/oxidean-db/tests/dialect_pats.rs
  modified: []

key-decisions:
  - "Wave 0 is RED-only — no pat handlers, Smart HTTP routes, or 0008 migrations"
  - "dialect_pats test fn names include dialect_pats so nextest test(dialect_pats) discovers them"
  - "Threat stubs encode T-08-01 (list omits secret) and T-08-02 (password reject, cookie ignore, private 401)"

patterns-established:
  - "Nyquist Wave 0 for Phase 8: failing nextest paths exist before PAT + Smart HTTP implementation waves"

requirements-completed: []  # Wave 0 scaffolds only; greens land in later 08-xx plans

coverage:
  - id: D1
    description: "API Wave 0 stubs for pat.createClassic/list/revoke, email_unverified, pat.note_required"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(pat_)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "API Wave 0 stubs for Smart HTTP 401/403/429, password reject, cookie ignore, PAT happy path"
    requirement: GIT-02
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-api -E 'test(git_smart)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "dialect_pats expects tri-dialect 0008_pats + personal_access_tokens / personal_access_token_repos"
    requirement: GIT-11
    verification:
      - kind: integration
        ref: "cargo nextest list -p oxidean-db -E 'test(dialect_pats)'"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-09-13
status: complete
---

# Phase 08 Plan 00: Wave 0 PAT + Smart HTTP Nyquist Stubs Summary

**Failing nextest stubs for pat_* RPC, git_smart_* HTTPS auth/ACL codes, and dialect_pats 0008 schema before Phase 8 implementation waves**

## Performance

- **Duration:** 2 min
- **Started:** 2026-09-13T17:56:36Z
- **Completed:** 2026-09-13T17:58:24Z
- **Tasks:** 1
- **Files modified:** 3

## Accomplishments

- PAT RPC Wave 0 stubs cover verified `createClassic` one-time token, list omits secret, revoke, `auth.email_unverified`, and `pat.note_required` (GIT-11 / D-15 / D-16 / D-24 / T-08-01)
- Smart HTTP Wave 0 stubs cover public anon fetch, private 401 + WWW-Authenticate, password reject, cookie ignore, scope 403, rate-limit 429, unverified push, PAT happy path (GIT-02 / T-08-02)
- `dialect_pats` expects tri-dialect `0008_pats` with `personal_access_tokens` + `personal_access_token_repos`

## Task Commits

Each task was committed atomically:

1. **Task 1: PAT RPC + Smart HTTP + dialect Wave 0 stubs** - `80275ab` (test)

**Plan metadata:** `fe1a95a` (docs: complete plan)

_Note: Wave 0 is RED-only by design — GREEN belongs to later 08-xx plans._

## Files Created/Modified

- `crates/oxidean-api/tests/pat_rpc.rs` — GIT-11 create/list/revoke + verified/note stubs
- `crates/oxidean-api/tests/git_smart_http.rs` — GIT-02 Smart HTTP auth/ACL status stubs
- `crates/oxidean-db/tests/dialect_pats.rs` — 0008_pats migration parity stub

## Decisions Made

- Kept Wave 0 RED-only (`assert!(false)`); no production PAT RPC, Smart HTTP routes, or migrations
- Named dialect tests `dialect_pats_*` so `test(dialect_pats)` nextest filter discovers them (binary name alone is insufficient)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] nextest `test(dialect_pats)` did not match migrate_* names**
- **Found during:** Task 1 verification
- **Issue:** Plan verify uses `-E 'test(dialect_pats)'`, which matches test function names, not the binary id; `migrate_0008_pats_*` produced an empty list
- **Fix:** Renamed to `dialect_pats_migrate_0008_schema_presence` and `dialect_pats_tri_dialect_files`
- **Files modified:** `crates/oxidean-db/tests/dialect_pats.rs`
- **Commit:** `80275ab`

### Auto-fixed Issues (close-out)

**2. [Rule 1 - Bug] Reverted premature GIT-02/GIT-11 mark-complete**
- **Found during:** SUMMARY close-out
- **Issue:** `requirements.ready-ids` reports GIT-02/GIT-11 blocked for Wave 0; mark-complete had unchecked them incorrectly
- **Fix:** `requirements.revert-phase GIT-02 GIT-11`; SUMMARY `requirements-completed: []`
- **Files modified:** `.planning/REQUIREMENTS.md`, `08-00-SUMMARY.md`

## Auth Gates

None.

## Known Stubs

Wave 0 intentional RED stubs (expected until later plans):

| File | Stub | Reason |
|------|------|--------|
| `crates/oxidean-api/tests/pat_rpc.rs` | all 5 `assert!(false)` tests | RED until 08-04 pat RPC |
| `crates/oxidean-api/tests/git_smart_http.rs` | all 8 `assert!(false)` tests | RED until 08-04 / 08-06 Smart HTTP |
| `crates/oxidean-db/tests/dialect_pats.rs` | schema + tri-dialect asserts | RED until 08-03 `0008_pats` migration |

## Threat Flags

None beyond plan threat model (T-08-01 / T-08-02 encoded in stub messages; no new production surface).

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/tests/pat_rpc.rs`
- FOUND: `crates/oxidean-api/tests/git_smart_http.rs`
- FOUND: `crates/oxidean-db/tests/dialect_pats.rs`
- FOUND: commit `80275ab`
