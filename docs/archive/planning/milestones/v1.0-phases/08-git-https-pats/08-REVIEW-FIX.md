---
phase: 08-git-https-pats
fixed_at: 2026-09-13T20:30:00Z
review_path: .planning/phases/08-git-https-pats/08-REVIEW.md
iteration: 2
findings_in_scope: 7
fixed: 3
already_fixed: 4
skipped: 0
status: all_fixed
---

# Phase 08: Code Review Fix Report

**Fixed at:** 2026-09-13T20:30:00Z
**Source review:** `.planning/phases/08-git-https-pats/08-REVIEW.md`
**Iteration:** 2 (`--fix --all`)

**Summary:**
- Findings in scope: 7 (Critical + Warning + Info)
- Fixed this pass: 3 (IN-01, IN-02, IN-03)
- Already fixed (prior pass): 4 (WR-01..WR-04)
- Skipped: 0

**Verification environment:** Main checkout (`workflow.use_worktrees=false`). Gates ran in the main working tree, not an isolated worktree.

**Targeted tests:** `cargo nextest run -p oxidean-api --locked --test pat_rpc --test git_smart_http` — 18 passed.

## Already Fixed (prior iteration)

Verified still present in tree; not re-applied.

### WR-01: Fine-grained PAT create is not transactional

**Files:** `crates/oxidean-db/src/pats.rs`
**Commit:** `3f416f7`
**Status:** `already_fixed`
**Verified:** `create` still uses per-dialect `begin` → insert PAT + repo links → `commit`.

### WR-02: Duplicate `repository_ids` can orphan a fine-grained PAT

**Files:** `crates/oxidean-api/src/pat/mod.rs`
**Commit:** `69fde82`
**Status:** `already_fixed`
**Verified:** Order-preserving dedupe after ownership checks still present.

### WR-03: Failed-auth IP identity uses first `X-Forwarded-For` hop (spoofable)

**Files:** `crates/oxidean-api/src/routes/git_smart_http.rs`, `docs/CONFIGURATION.md`, `docs/API.md`
**Commit:** `57daaba`
**Status:** `already_fixed`
**Verified:** `client_ip` still takes rightmost non-empty XFF hop.

### WR-04: No server-side `expires_at` validation; UI mislabels unparseable expiry

**Files:** `crates/oxidean-api/src/pat/mod.rs`, `apps/web/src/components/settings/pat-list.tsrx`
**Commit:** `ce30698`
**Status:** `already_fixed`
**Verified:** `validate_expires_at` still gates create; UI still maps NaN to `"Invalid expiration"`.

## Fixed Issues (this pass)

### IN-01: UI handles `pat.repos_required` but API never emits it

**Files modified:** `crates/oxidean-api/src/pat/mod.rs`, `crates/oxidean-api/tests/pat_rpc.rs`, `docs/API.md`
**Commit:** `a4aa1ce`
**Applied fix:** Empty fine-grained `selected` now returns `pat.repos_required` (matches existing UI branch). Foreign/invalid repo ids still use `pat.invalid_scope`. Updated empty-selected RPC test and API docs / error-code table.

### IN-02: List `token_prefix` is brand prefix only (no secret fingerprint)

**Files modified:** `crates/oxidean-api/src/pat/mod.rs`, `crates/oxidean-api/tests/pat_rpc.rs`
**Commit:** `557dd17`
**Applied fix:** Classic and fine-grained create store `token_prefix` as brand prefix + first 8 hex chars of the secret (not the hash), e.g. `oxidean_pat_a1b2c3d4`. Updated create/list assertions to require prefix length and plaintext starts-with fingerprint.

### IN-03: Session-cookie ignore test does not cover private + cookie → 401

**Files modified:** `crates/oxidean-api/tests/git_smart_http.rs`
**Commit:** `57c158b`
**Applied fix:** Extended `git_smart_session_cookie_ignored_as_anon` to create a private repo and assert cookie-only `info/refs` → 401 + `WWW-Authenticate` Basic realm.

## Skipped Issues

None — all in-scope findings were fixed or already fixed.

---

_Fixed: 2026-09-13T20:30:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 2_
