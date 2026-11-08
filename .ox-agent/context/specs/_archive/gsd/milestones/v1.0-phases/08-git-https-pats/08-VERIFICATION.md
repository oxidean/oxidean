---
phase: 08-git-https-pats
verified: "2026-09-19T18:14:28Z"
status: passed
status_note: Automated fingerprint refresh — existing test/VALIDATION evidence accepted as proof (no conversational UAT).
score: 7/7 must-haves verified
covered_files:
  - .planning/REQUIREMENTS.md
  - .planning/ROADMAP.md
  - .planning/phases/08-git-https-pats/08-00-PLAN.md
  - .planning/phases/08-git-https-pats/08-00-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-01-PLAN.md
  - .planning/phases/08-git-https-pats/08-01-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-02-PLAN.md
  - .planning/phases/08-git-https-pats/08-02-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-03-PLAN.md
  - .planning/phases/08-git-https-pats/08-03-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-04-PLAN.md
  - .planning/phases/08-git-https-pats/08-04-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-05-PLAN.md
  - .planning/phases/08-git-https-pats/08-05-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-06-PLAN.md
  - .planning/phases/08-git-https-pats/08-06-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-07-PLAN.md
  - .planning/phases/08-git-https-pats/08-07-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-08-PLAN.md
  - .planning/phases/08-git-https-pats/08-08-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-09-PLAN.md
  - .planning/phases/08-git-https-pats/08-09-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-10-PLAN.md
  - .planning/phases/08-git-https-pats/08-10-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-11-PLAN.md
  - .planning/phases/08-git-https-pats/08-11-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-12-PLAN.md
  - .planning/phases/08-git-https-pats/08-12-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-13-PLAN.md
  - .planning/phases/08-git-https-pats/08-13-SUMMARY.md
  - .planning/phases/08-git-https-pats/08-CONTEXT.md
  - .planning/phases/08-git-https-pats/08-DISCUSSION-LOG.md
  - .planning/phases/08-git-https-pats/08-REVIEW-FIX.md
  - .planning/phases/08-git-https-pats/08-REVIEW.md
  - .planning/phases/08-git-https-pats/08-SECURITY.md
  - .planning/phases/08-git-https-pats/08-UAT.md
  - .planning/phases/08-git-https-pats/08-VALIDATION.md
  - Makefile
  - apps/web/src/components/chrome.tsrx
  - apps/web/src/components/repo/clone-box.pat.integration.test.ts
  - apps/web/src/components/repo/clone-box.tsrx
  - apps/web/src/components/repo/pat-how-to.tsrx
  - apps/web/src/components/repo/quick-setup.tsrx
  - apps/web/src/components/settings/pat-classic-form.tsrx
  - apps/web/src/components/settings/pat-fg-form.tsrx
  - apps/web/src/components/settings/pat-list.tsrx
  - apps/web/src/components/settings/pat-reveal.tsrx
  - apps/web/src/components/settings/pat-revoke-dialog.tsrx
  - apps/web/src/components/settings/settings-nav.tsrx
  - apps/web/src/routes/settings/profile.tsrx
  - apps/web/src/routes/settings/tokens.integration.test.ts
  - apps/web/src/routes/settings/tokens.new.fine-grained.tsrx
  - apps/web/src/routes/settings/tokens.new.tsrx
  - apps/web/src/routes/settings/tokens.tsrx
  - crates/oxidean-api/src/app.rs
  - crates/oxidean-api/src/auth/session.rs
  - crates/oxidean-api/src/git/http_backend.rs
  - crates/oxidean-api/src/git/mod.rs
  - crates/oxidean-api/src/pat/mod.rs
  - crates/oxidean-api/src/pat/rate_limit.rs
  - crates/oxidean-api/src/repo/acl.rs
  - crates/oxidean-api/src/routes/git_smart_http.rs
  - crates/oxidean-api/src/routes/mod.rs
  - crates/oxidean-api/src/rpc.rs
  - crates/oxidean-api/tests/git_smart_http.rs
  - crates/oxidean-api/tests/pat_rpc.rs
  - crates/oxidean-core/src/auth_types.rs
  - crates/oxidean-core/src/lib.rs
  - crates/oxidean-core/src/pat_types.rs
  - crates/oxidean-db/migrations/mysql/0008_pats.sql
  - crates/oxidean-db/migrations/postgres/0008_pats.sql
  - crates/oxidean-db/migrations/sqlite/0008_pats.sql
  - crates/oxidean-db/src/lib.rs
  - crates/oxidean-db/src/pats.rs
  - crates/oxidean-db/tests/dialect_pats.rs
  - docker-compose.yml
  - docs/API.md
  - docs/ARCHITECTURE.md
  - docs/CONFIGURATION.md
  - packages/api-client/src/index.ts
  - scripts/smoke-git-https.sh
covered_digest: "v1:sha256:00d950089d1d12bff72589ec260752b96e47d7e2b689ab22be707e77443c95c6"
behavior_unverified: 0
overrides_applied: 0
decision_coverage: "{'honored': 26, 'total': 26, 'not_honored': []}"
re_verification: "{'previous_status': 'passed', 'previous_score': '7/7', 'gaps_closed': ['covered_digest refresh — prior VERIFICATION status:passed but digest stale (stored ≠ recomputed); UAT already complete 4/4', '08-UAT.md complete 4/4 — tokens UI, FG+Smart HTTP, how-to wrap backstop, judgment prohibitions (not reopened)', 'Truth 7 long clone URL wrap — prior backstop closed via UAT #3 + vitest wrap-class assertion', 'Post-review fixes WR-01..04, IN-01..03 still present (transactional FG create, XFF rightmost, expires_at, repos_required, token_prefix fingerprint, cookie+private 401)'], 'gaps_remaining': [], 'regressions': []}"
behavior_unverified_items: "[]"
human_verification: "[]"
---

# Phase 8: Git HTTPS & PATs Verification Report

**Phase Goal:** Users can authenticate git over HTTPS with personal access tokens (never account passwords) and manage those tokens in the UI

**Verified:** 2026-09-13T21:49:18Z  
**Status:** passed  
**Re-verification:** Yes — digest refresh after UAT complete (prior status passed but `covered_digest` stale; UAT not reopened)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | User can create, list, and revoke personal access tokens for HTTPS git (session RPC; PATs not RPC Bearer per D-01 / GIT-11 “where applicable”) | ✓ VERIFIED | `pat.createClassic` / `createFineGrained` / `list` / `revoke` in `crates/oxidean-api/src/pat/mod.rs` + `rpc.rs`; `pat_create_classic_returns_one_time_token` PASS this run |
| 2 | User can clone, fetch, and push over HTTPS using a PAT; account password is rejected for git auth | ✓ VERIFIED | Smart HTTP Basic → SHA-256 → `find_pat_by_token_hash`; `looks_like_pat` rejects non-prefix secrets; `git_smart_basic_account_password_rejected_401` PASS this run |
| 3 | Token prefixes are `oxidean_pat_` / `oxidean_fg_` (D-08 locked) | ✓ VERIFIED | `CLASSIC_PAT_PREFIX` / `FINE_GRAINED_PAT_PREFIX` in `pat_types.rs`; unit asserts reject `ona_*` / github prefixes |
| 4 | HTTPS Smart HTTP URL is `/{owner}/{repo}.git` (D-18) | ✓ VERIFIED | Axum `/{owner}/{repo_git}/…`; Traefik `PathRegexp(^/[^/]+/[^/]+\.git)` priority 110 in `docker-compose.yml` |
| 5 | Private unauth git → 401 + `WWW-Authenticate`; PATs HTTPS-git-only (not RPC Bearer) (D-21 / D-01) | ✓ VERIFIED | `WWW-Authenticate` + `looks_like_pat` / `find_pat_by_token_hash` in `git_smart_http.rs`; prior named tests + UAT #2/#4; docs session-only RPC |
| 6 | Users manage tokens in UI (list/create classic+FG/revoke + CloneBox how-to) | ✓ VERIFIED | Routes `/settings/tokens`, `/new`, `/new/fine-grained`; vitest tokens + clone-box **17/17 PASS** this run |
| 7 | Long clone URLs wrap or overflow-x-auto in how-to code blocks | ✓ VERIFIED | `pat-how-to.tsrx` `overflow-x-auto whitespace-pre-wrap break-all`; vitest asserts those classes; **08-UAT.md test 3 pass** |

**Score:** 7/7 truths verified (0 present, behavior-unverified)

**Locked decisions honored in code (not SUMMARY claims):**

- Prefixes: `oxidean_pat_` / `oxidean_fg_` (not `ona_*`)
- Clone path: `/{owner}/{repo}.git`
- Private unauth: 401 + WWW-Authenticate; PATs not RPC Bearer

### Advisory (New Scope, Unevidenced)

None — re-verification digest refresh after UAT; no new unevidenced Step 7 blockers.

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/oxidean-db/migrations/*/0008_pats.sql` | PAT tables + `token_hash` | ✓ VERIFIED | Tri-dialect; hash UNIQUE; no plaintext column |
| `crates/oxidean-db/src/pats.rs` | create/find/list/revoke | ✓ VERIFIED | Exists + wired via `Database` |
| `crates/oxidean-core/src/pat_types.rs` | prefixes + DTOs | ✓ VERIFIED | Classic/FG kinds; locked prefixes |
| `crates/oxidean-api/src/pat/mod.rs` | PAT RPC | ✓ VERIFIED | createClassic/FG, list, revoke |
| `crates/oxidean-api/src/routes/git_smart_http.rs` | Smart HTTP auth | ✓ VERIFIED | Basic PAT, password reject, WWW-Authenticate |
| `crates/oxidean-api/src/git/http_backend.rs` | CGI helper | ✓ VERIFIED | Exists + used by Smart HTTP |
| `apps/web/src/routes/settings/tokens*.tsrx` | Token UI | ✓ VERIFIED | List + classic + FG create |
| `apps/web/src/components/repo/pat-how-to.tsrx` | HTTPS how-to | ✓ VERIFIED | Wrap classes present; wired CloneBox/QuickSetup |
| `packages/api-client/src/index.ts` | Generated client | ✓ VERIFIED | Present (prior `rpc-sync-check` green) |
| `docker-compose.yml` + `scripts/smoke-git-https.sh` | Traefik + smoke | ✓ VERIFIED | PathRegexp + smoke target present |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `git_smart_http.rs` | DB `find_pat_by_token_hash` | Basic password → SHA-256 | ✓ WIRED | |
| `pat/mod.rs` | `oxidean_db` pats | create / list / revoke | ✓ WIRED | |
| `tokens.tsrx` / `pat-list.tsrx` | api-client | `pat.*` Query/RPC | ✓ WIRED | |
| `tokens.new*.tsrx` | api-client | `createClassic` / `createFineGrained` | ✓ WIRED | |
| `clone-box.tsrx` | `pat-how-to.tsrx` | `<PatHowTo />` | ✓ WIRED | Also QuickSetup |
| `app.rs` | Smart HTTP handlers | `/{owner}/{repo_git}/…` | ✓ WIRED | |
| `docker-compose.yml` | API Smart HTTP | Traefik PathRegexp | ✓ WIRED | |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| Pat list UI | list query | `pat.list` RPC → DB | Yes (Query) | ✓ FLOWING |
| Classic create | one-time plaintext | `create_classic` mint | Yes | ✓ FLOWING |
| Smart HTTP auth | `pat` row | `find_pat_by_token_hash` | Yes | ✓ FLOWING |
| How-to clone example | `httpsUrl` prop | Repo clone URL from parent | Yes (caller-supplied) | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Password rejected | `cargo test -p oxidean-api --test git_smart_http git_smart_basic_account_password_rejected_401 -- --exact` | ok | ✓ PASS |
| Create classic | `cargo test -p oxidean-api --test pat_rpc pat_create_classic_returns_one_time_token -- --exact` | ok | ✓ PASS |
| Tokens + how-to UI | `vitest run tokens.integration.test.ts clone-box.pat.integration.test.ts` | 17 passed | ✓ PASS |

### Probe Execution

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| `scripts/smoke-git-https.sh` | (not re-run live) | Documented skip without Docker; UAT #2 accepted nextest + smoke script presence | ⚠️ SKIP (env; covered by UAT automated map) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| **GIT-02** | 00–08, 12–13 | Clone/fetch/push HTTPS with PAT; not account password | ✓ SATISFIED | Smart HTTP + password-reject tests; UI how-to; UAT #2/#3 |
| **GIT-11** | 00–06, 08–11, 13 | Create/list/revoke PATs for HTTPS git (RPC/API where applicable) | ✓ SATISFIED | PAT RPC + UI; D-01 HTTPS-git-only; UAT #1/#4 |

Orphaned requirements mapped to Phase 8: none (only GIT-02, GIT-11). Both marked Complete in REQUIREMENTS.md.

### Decision Coverage

All trackable CONTEXT.md decisions are honored by shipped artifacts (26/26). Message: *All trackable CONTEXT.md decisions are honored by shipped artifacts.*

### Test Quality Audit

| Test File | Linked Req | Active | Skipped | Circular | Assertion Level | Verdict |
|-----------|-----------|--------|---------|----------|-----------------|---------|
| `crates/oxidean-api/tests/pat_rpc.rs` | GIT-11 | 10 | 0 | No | Behavioral / value | PASS |
| `crates/oxidean-api/tests/git_smart_http.rs` | GIT-02 | 8 | 0 | No | Behavioral / status | PASS |
| `crates/oxidean-db/tests/dialect_pats.rs` | GIT-11 | 2 | 0 | No | Value (schema) | PASS |
| `apps/web/.../tokens.integration.test.ts` | GIT-11 | (suite) | 0 | No | Behavioral (mocked RPC) | PASS |
| `apps/web/.../clone-box.pat.integration.test.ts` | GIT-02 | (suite) | 0 | No | Value (copy/CTA/wrap) | PASS |

**Disabled tests on requirements:** 0  
**Circular patterns detected:** 0  
**Insufficient assertions:** 0 blockers

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | No TBD/FIXME/XXX debt markers in key PAT/Smart HTTP/UI sources | — | — |

### Human Verification Required

N/A — `08-UAT.md` status `complete`, **4/4 passed** (not reopened). Prior human_verification items remain closed; digest-only re-verification found no new gaps. SECURITY.md `threats_open: 0`.

### Gaps Summary

None. Roadmap success criteria, GIT-02 / GIT-11, and all 7 must-have truths remain verified. `covered_digest` refreshed so `verification.status` is no longer stale. Phase goal achieved.

### Inversion / disconfirmation notes

1. **GIT-11 “RPC/API where applicable”** — Intentional Phase 8 narrowing (D-01): PATs are HTTPS-git-only; token CRUD uses session RPC. Not a gap.  
2. **Web vitest mocks** — Do not alone prove RPC; nextest `pat_rpc` / `git_smart_http` close that gap.  
3. **Compose smoke** — Live Docker path still optional; UAT accepted nextest + script presence.

---

_Verified: 2026-09-13T21:49:18Z_  
_Verifier: Claude (gsd-verifier)_

## Automated re-verification (2026-09-19T18:14:28Z)

- Mode: fingerprint refresh (`covered_files` + `covered_digest`)
- Policy: existing phase VERIFICATION must-haves + SUMMARY/test evidence treated as sufficient; conversational UAT not re-run
- Covered inputs: 81 files

