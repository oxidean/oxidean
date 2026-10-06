---
phase: 08-git-https-pats
reviewed: 2026-09-13T20:05:04Z
depth: deep
files_reviewed: 42
files_reviewed_list:
  - crates/oxidean-core/src/pat_types.rs
  - crates/oxidean-core/src/auth_types.rs
  - crates/oxidean-db/src/pats.rs
  - crates/oxidean-db/migrations/postgres/0008_pats.sql
  - crates/oxidean-db/migrations/mysql/0008_pats.sql
  - crates/oxidean-db/migrations/sqlite/0008_pats.sql
  - crates/oxidean-api/src/pat/mod.rs
  - crates/oxidean-api/src/pat/rate_limit.rs
  - crates/oxidean-api/src/routes/git_smart_http.rs
  - crates/oxidean-api/src/git/http_backend.rs
  - crates/oxidean-api/src/git/mod.rs
  - crates/oxidean-api/src/repo/acl.rs
  - crates/oxidean-api/src/app.rs
  - crates/oxidean-api/src/rpc.rs
  - crates/oxidean-api/src/auth/session.rs
  - crates/oxidean-api/src/bin/rpc_gen.rs
  - crates/oxidean-api/tests/git_smart_http.rs
  - crates/oxidean-api/tests/pat_rpc.rs
  - packages/api-client/src/index.ts
  - apps/web/src/routes/settings/tokens.tsrx
  - apps/web/src/routes/settings/tokens.new.tsrx
  - apps/web/src/routes/settings/tokens.new.fine-grained.tsrx
  - apps/web/src/components/settings/pat-classic-form.tsrx
  - apps/web/src/components/settings/pat-fg-form.tsrx
  - apps/web/src/components/settings/pat-list.tsrx
  - apps/web/src/components/settings/pat-reveal.tsrx
  - apps/web/src/components/settings/pat-revoke-dialog.tsrx
  - apps/web/src/components/settings/settings-nav.tsrx
  - apps/web/src/components/repo/pat-how-to.tsrx
  - apps/web/src/components/repo/clone-box.tsrx
  - apps/web/src/components/repo/quick-setup.tsrx
  - apps/web/src/components/chrome.tsrx
  - apps/web/src/routes/settings/profile.tsrx
  - docs/API.md
  - docs/ARCHITECTURE.md
  - docs/CONFIGURATION.md
  - docker-compose.yml
  - scripts/smoke-git-https.sh
  - Makefile
findings:
  critical: 0
  warning: 4
  info: 3
  total: 7
status: issues_found
---

# Phase 08: Code Review Report

**Reviewed:** 2026-09-13T20:05:04Z
**Depth:** deep
**Files Reviewed:** 42
**Status:** issues_found

## Summary

Phase 08’s PAT mint/list/revoke path and Smart HTTP gate were reviewed adversarially with cross-module tracing (RPC session → PAT hash-at-rest → Basic auth → ACL/scope → `git-http-backend`).

**Critical product decisions — validated as implemented:**

| Decision | Verdict |
|----------|---------|
| Prefixes `oxidean_pat_` / `oxidean_fg_` (not `ona_*` / `gh*`) | Pass — `pat_types`, mint, api-client, docs, UI |
| PATs authenticate HTTPS git only, not RPC Bearer | Pass — RPC resolves session cookie only; docs state Bearer ignored |
| Private unauth git → 401 + `WWW-Authenticate` | Pass — `unauthorized_basic` / tests |
| Hash-at-rest; one-time reveal; cookies ignored for Smart HTTP | Pass — SHA-256 store; `CreatePatResponse.token` only; Cookie header unused for auth |

No Critical/BLOCKER defects found. Four Warnings degrade robustness of create integrity and failed-auth rate limiting / expiry UX.

## Warnings

### WR-01: Fine-grained PAT create is not transactional

**File:** `crates/oxidean-db/src/pats.rs:187-266`
**Issue:** `create` inserts `personal_access_tokens` then calls `insert_repo_links` with no transaction. If a link insert fails (FK error, unique violation, mid-loop failure), the PAT row remains while the RPC returns an error and **never returns plaintext**. The user sees a create failure but an orphan (or partially linked) token appears in `pat.list` with no recoverable secret.
**Fix:** Wrap insert + repo links in a dialect transaction and roll back on any link failure; or delete the PAT row if links fail before returning `Err`.

```rust
// Pseudocode — Postgres example
let mut tx = p.begin().await?;
sqlx::query("INSERT INTO personal_access_tokens ...").execute(&mut *tx).await?;
for repo_id in repository_ids {
    sqlx::query("INSERT INTO personal_access_token_repos ...").execute(&mut *tx).await?;
}
tx.commit().await?;
```

### WR-02: Duplicate `repository_ids` can orphan a fine-grained PAT

**File:** `crates/oxidean-api/src/pat/mod.rs:178-217` (feeds `crates/oxidean-db/src/pats.rs:149-185`)
**Issue:** `create_fine_grained` does not dedupe `repository_ids`. A client that sends the same owned repo id twice will insert the PAT, then fail the second `personal_access_token_repos` insert on the composite primary key — same orphan outcome as WR-01 (error to client, token in list, no plaintext).
**Fix:** Deduplicate after ownership checks (preserve order):

```rust
let mut owned = Vec::new();
for repo_id in &req.repository_ids {
    // ... ownership checks ...
    if !owned.iter().any(|id| id == &row.id) {
        owned.push(row.id);
    }
}
```

### WR-03: Failed-auth IP identity uses first `X-Forwarded-For` hop (spoofable)

**File:** `crates/oxidean-api/src/routes/git_smart_http.rs:128-136`
**Issue:** `client_ip` takes the **first** XFF hop. Traefik typically **appends** the connecting client, so a caller can send `X-Forwarded-For: <fresh-ip>` and bypass the per-IP sliding window (D-26). Alias usernames (`git` / `token` / `oauth2`) already skip the per-user bucket, so IP limiting is the main brake for `git:<guess>` / fake `oxidean_pat_*` sprays. Compose does not document trusted forwarded-header settings.
**Fix:** Prefer the rightmost hop added by the trusted proxy (or `X-Real-Ip` from Traefik), and document that the API must not be exposed without a proxy that overwrites/sanitizes forwarded headers:

```rust
fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').map(str::trim).filter(|p| !p.is_empty()).next_back())
        .map(|s| s.to_string())
}
```

### WR-04: No server-side `expires_at` validation; UI mislabels unparseable expiry

**Files:** `crates/oxidean-api/src/pat/mod.rs:137`, `crates/oxidean-api/src/pat/mod.rs:236`; `apps/web/src/components/settings/pat-list.tsrx:19-27`
**Issue:** Create handlers pass `expires_at` through without requiring RFC3339 or a future instant. On SQLite, arbitrary strings can be stored. Smart HTTP treats parse failure as **expired** (`pat_expired` → true), but the list UI’s `formatExpiry` maps `Date.parse` NaN to **"No expiration"** — so a bad expiry can look valid in Settings while git auth always fails.
**Fix:** Validate on create (reject past / invalid with `rpc.bad_input` or `pat.invalid_expiry`). In the UI, treat NaN as `"Invalid expiration"` (and show Expired), not `"No expiration"`.

```rust
if let Some(raw) = req.expires_at.as_deref() {
    let dt = DateTime::parse_from_rfc3339(raw)
        .map_err(|_| AppError::new("rpc.bad_input", "expires_at must be RFC3339"))?;
    if dt.with_timezone(&Utc) <= Utc::now() {
        return Err(AppError::new("rpc.bad_input", "expires_at must be in the future"));
    }
}
```

## Info

### IN-01: UI handles `pat.repos_required` but API never emits it

**File:** `apps/web/src/components/settings/pat-fg-form.tsrx:117-118`
**Issue:** Empty selected repos return `pat.invalid_scope` from the server; the `pat.repos_required` branch is dead.
**Fix:** Map `pat.invalid_scope` to the repo error when `repoAccess === "selected"`, or align the API error code with the UI.

### IN-02: List `token_prefix` is brand prefix only (no secret fingerprint)

**File:** `crates/oxidean-api/src/pat/mod.rs:132` / `231` (stores `CLASSIC_PAT_PREFIX` / `FINE_GRAINED_PAT_PREFIX`)
**Issue:** Every classic token displays as `oxidean_pat_…` with no distinguishing suffix (unlike typical forge “first/last chars” fingerprints). Not a secret leak; weakens list distinguishability (D-09 is last-used, so acceptable).
**Fix (optional):** Store e.g. first 8 hex chars of the secret (not the hash) in `token_prefix` for display only.

### IN-03: Session-cookie ignore test does not cover private + cookie → 401

**File:** `crates/oxidean-api/tests/git_smart_http.rs:257-301`
**Issue:** D-12 is covered for public+cookie and cookie+password, but not private repo with session cookie and no Basic (strongest “cookie must not elevate” case). Implementation still ignores cookies; test gap only.
**Fix:** Add assertion: private `info/refs` with only `oxidean_session` → 401 + `WWW-Authenticate`.

---

_Reviewed: 2026-09-13T20:05:04Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
