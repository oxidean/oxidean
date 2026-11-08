# Phase 9: Git SSH - Pattern Map

**Mapped:** 2026-09-14
**Files analyzed:** 16 (implied from 09-CONTEXT; no RESEARCH.md yet)
**Analogs found:** 16 / 16

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/oxidean-api/src/ssh/pack.rs` (+ `server.rs` / `auth.rs` / `host_keys.rs`) | service | file-I/O + request-response | `crates/oxidean-api/src/git/http_backend.rs` | role-match — RESEARCH layout `src/ssh/*` |
| `crates/oxidean-api/src/ssh/mod.rs` | controller | request-response | `crates/oxidean-api/src/routes/git_smart_http.rs` | role-match — in-process russh (not Axum `.git` route) |
| ACL reuse in SSH authz | utility | request-response | `crates/oxidean-api/src/repo/acl.rs` | exact |
| `crates/oxidean-db/migrations/{sqlite,postgres,mysql}/0009_ssh_keys.sql` | migration | CRUD | `…/0008_pats.sql` | exact |
| `crates/oxidean-db/src/ssh_keys.rs` (+ `Database` facade) | model | CRUD | `crates/oxidean-db/src/pats.rs` | exact |
| `crates/oxidean-db/tests/dialect_ssh_keys.rs` | test | CRUD | `crates/oxidean-db/tests/dialect_pats.rs` | exact |
| `crates/oxidean-core/src/ssh_key_types.rs` | model | transform | `crates/oxidean-core/src/pat_types.rs` | role-match |
| `crates/oxidean-api/src/ssh_keys/mod.rs` (RPC) | controller | request-response | `crates/oxidean-api/src/pat/mod.rs` | exact |
| `crates/oxidean-api/src/ssh/rate_limit.rs` | middleware | request-response | `crates/oxidean-api/src/pat/rate_limit.rs` | exact |
| `crates/oxidean-api/src/rpc.rs` + `rpc_gen` + `make rpc-gen` | config | request-response | `pat.*` dispatch in `rpc.rs` / `bin/rpc_gen.rs` | exact |
| `crates/oxidean-api/tests/ssh_key_rpc.rs` | test | request-response | `crates/oxidean-api/tests/pat_rpc.rs` | role-match |
| `apps/web/src/routes/settings/ssh-keys*.tsrx` | route | CRUD | `apps/web/src/routes/settings/tokens.tsrx` | exact |
| `apps/web/src/components/settings/settings-nav.tsrx` | component | request-response | same file (extend active union) | exact |
| `apps/web/src/components/settings/ssh-key-*.tsrx` | component | CRUD | `pat-list.tsrx` / `pat-revoke-dialog.tsrx` | exact |
| `apps/web/src/lib/public-origin.ts` (`sshCloneUrl`) | utility | transform | `httpsCloneUrl` in same file | exact |
| `apps/web/src/components/repo/clone-box.tsrx` (+ SSH how-to) | component | request-response | `clone-box.tsrx` + `pat-how-to.tsrx` | exact |
| `docker-compose.yml` (TCP publish SSH) | config | streaming | `docker-compose.yml` api Traefik labels | partial |
| `scripts/smoke-git-ssh.sh` + `Makefile` | config | request-response | `scripts/smoke-git-https.sh` | exact |

## Pattern Assignments

### `crates/oxidean-api/src/ssh/mod.rs` (controller, request-response)

**Analog:** `crates/oxidean-api/src/routes/git_smart_http.rs`

**Imports / ACL helpers** (lines 20–24):
```rust
use crate::app::AppState;
use crate::git::bare_repo_path;
use crate::git::http_backend::{self, CgiRequest};
use crate::repo::{can_read_as_owner, is_private_visibility};
```

**Auth matrix to mirror for SSH (D-SSH-04)** — map HTTP statuses → git stderr errors (lines 408–434):
```rust
// receive-pack always requires auth.
if receive && authed.is_none() { /* deny */ }
// Private + unauth → deny (HTTP: 401 Basic; SSH: clear git error).
if is_private && authed.is_none() { /* deny */ }
if let Some(ref auth) = authed {
    if is_private && !can_read_as_owner(Some(caller_id), &resolved.owner_id) {
        /* deny — anti-leak; SSH: git error not HTTP 401/404 */
    }
    if receive && !can_read_as_owner(Some(caller_id), &resolved.owner_id) {
        /* push owner-only until Phase 10 */
    }
    // Unverified may fetch; push denied (email_unverified).
    if receive && auth.owner.email_verified_at.is_none() {
        return email_unverified_push();
    }
    touch_last_used(/* key id */, /* peer addr */).await;
}
```

**Failed-auth rate limit + last-used** (lines 348–358, 384–388):
```rust
async fn touch_last_used(state: &AppState, pat_id: &str, headers: &HeaderMap) {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let ip = client_ip(headers);
    if let Err(e) = state.db.touch_pat_last_used(pat_id, &now, ip.as_deref()).await {
        tracing::warn!(error = %e, pat_id, "touch_pat_last_used failed");
    }
}
// IP gate before credential work:
if let Err(retry) = limiter_lock(state).check_ip(&ip) {
    return too_many_requests(retry);
}
```

**Reserved `git` principal** — already reserved + HTTPS username alias (lines 30–32; core `is_reserved_username`):
```rust
const USERNAME_ALIASES: &[&str] = &["git", "token", "oauth2"];
```
SSH forces login user `git` only (D-SSH-03); identity comes from key fingerprint, not username.

**Do not copy for SSH:** PAT scope checks (`pat_allows_operation`) — keys map to the full account (D-SSH-04 / D-SSH-05).

---

### `crates/oxidean-api/src/ssh/pack.rs` (service, file-I/O)

**Analog:** `crates/oxidean-api/src/git/http_backend.rs` + path helper `git/mod.rs`

**Spawn pattern** (lines 51–74) — adapt `git-http-backend` CGI → `git-upload-pack` / `git-receive-pack` with bare path:
```rust
let mut cmd = Command::new(&backend);
cmd.stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .env_clear()
    .env("PATH", /* … */)
    .env("GIT_PROJECT_ROOT", req.repos_dir)
    // SSH: Command::new("git").args(["upload-pack"|"receive-pack", "--", bare_path])
```

**Bare path layout** (`git/mod.rs` lines 13–31):
```rust
/// Bare repo path: `{repos_dir}/{owner}/{name}.git` (D-30).
pub fn bare_repo_path(repos_dir: &Path, owner: &str, name: &str) -> Result<PathBuf, AppError> {
    // reject empty, `/`, `\`, `..`
    Ok(repos_dir.join(owner).join(format!("{name}.git")))
}
```

Shared volume: Compose already binds `./var/repos:/var/repos` + `OXIDEAN_REPOS_DIR=/var/repos` on `api` — SSH service must share the same mount.

---

### `crates/oxidean-api/src/repo/acl.rs` (utility, request-response)

**Analog:** same file (reuse; do not fork ACL for SSH)

**Owner-only private read** (lines 21–24, 66–70):
```rust
pub fn can_read_as_owner(caller_user_id: Option<&str>, owner_id: &str) -> bool {
    caller_user_id == Some(owner_id)
}
// Private + caller ≠ owner → deny (web: repo.not_found; git HTTP: 401; SSH: git error)
```

Phase 10 will deepen this module; SSH must call the same helpers so collaborator ACL lands once.

---

### `crates/oxidean-db/migrations/*/0009_ssh_keys.sql` (migration, CRUD)

**Analog:** `crates/oxidean-db/migrations/postgres/0008_pats.sql` (tri-dialect siblings required)

**Schema shape to copy** (lines 1–19) — adapt columns for public key + fingerprint uniqueness:
```sql
CREATE TABLE IF NOT EXISTS personal_access_tokens (
  id             TEXT        PRIMARY KEY,
  user_id        TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name           TEXT        NOT NULL,
  -- SSH: public_key TEXT NOT NULL, fingerprint TEXT NOT NULL UNIQUE
  last_used_at   TIMESTAMPTZ NULL,
  last_used_ip   TEXT        NULL,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_pats_user_id ON personal_access_tokens(user_id);
```

Ship identical logical migration under `sqlite/`, `postgres/`, `mysql/` (next number after `0008_pats`).

---

### `crates/oxidean-db/src/ssh_keys.rs` (model, CRUD)

**Analog:** `crates/oxidean-db/src/pats.rs`

**Dialect match + row type** (lines 7–25, 108–147):
```rust
pub struct PatRow { /* id, user_id, name, … last_used_at, last_used_ip, created_at */ }

async fn load_repo_ids(pool: &DbPool, token_id: &str) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => { /* $1 */ }
        DbPool::MySql(p) => { /* ? */ }
        DbPool::Sqlite(p) => { /* ?1 */ }
    }
}
```

**CRUD surface to mirror:** `create`, `list_for_user`, `find_by_fingerprint` (auth path), `revoke` / delete, `touch_last_used`. All SQL dialect branching stays in `oxidean-db` only.

---

### `crates/oxidean-db/tests/dialect_ssh_keys.rs` (test, CRUD)

**Analog:** `crates/oxidean-db/tests/dialect_pats.rs`

**Schema presence + round-trip** (lines 8–41, 125–158):
```rust
#[tokio::test]
async fn dialect_pats_migrate_0008_schema_presence() {
    // assert migration file contents
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    // create → find → list → touch → revoke
}

#[test]
fn dialect_pats_tri_dialect_files() {
    for dialect in ["sqlite", "postgres", "mysql"] {
        let path = format!("{root}/{dialect}/0008_pats.sql");
        // assert non-empty + required columns
    }
}
```

---

### `crates/oxidean-api/src/ssh_keys/mod.rs` (controller, request-response)

**Analog:** `crates/oxidean-api/src/pat/mod.rs`

**require_verified on create; session-only list/revoke** (lines 106–124, 291–335):
```rust
use crate::auth::gate::require_verified;

pub async fn create_classic(ctx: &RpcCtx, input: serde_json::Value) -> Result<…, AppError> {
    let user = require_verified(ctx).await?;
    // note required → pat.note_required
    // …
}

pub async fn list(ctx: &RpcCtx) -> Result<Vec<PatListItem>, AppError> {
    let user_id = require_session_user_id(ctx)?;
    // …
}

pub async fn revoke(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let user_id = require_session_user_id(ctx)?;
    // ownership check → not_found if missing; soft-revoke
    Ok(serde_json::json!({ "ok": true }))
}
```

SSH differences vs PAT: no one-time secret reveal on create; store full public key + fingerprint; enforce max ~25 keys; accept ed25519 + rsa-sha2 only (D-SSH-05).

**RPC wire-up** (`rpc.rs` lines 281–296):
```rust
"pat.createClassic" => match pat::create_classic(ctx, req.input).await { … },
"pat.list" => match pat::list(ctx).await { … },
"pat.revoke" => match pat::revoke(ctx, req.input).await { … },
```
Add `sshKey.add` / `sshKey.list` / `sshKey.revoke` the same way, then `make rpc-gen`.

---

### `crates/oxidean-api/src/auth/gate.rs` (middleware, request-response)

**Analog:** same file — apply to SSH key **add** only

```rust
/// Unauthenticated → auth.unauthenticated; unverified → auth.email_unverified.
pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> {
    let Some(session) = &ctx.session else {
        return Err(AppError::new("auth.unauthenticated", "not authenticated"));
    };
    let user = ctx.db.find_user_by_id(&session.user_id).await…?;
    if user.email_verified_at.is_none() {
        return Err(AppError::new(
            "auth.email_unverified",
            "verify your email to continue",
        ));
    }
    Ok(user)
}
```

UI: disable “Add key” when `!user.email_verified` (same as tokens Generate — `tokens.tsrx` lines 116–178).

---

### `crates/oxidean-api/src/ssh/rate_limit.rs` (middleware, request-response)

**Analog:** `crates/oxidean-api/src/pat/rate_limit.rs` + `AppState.git_auth_limiter`

**Sliding window** (lines 9–87):
```rust
const WINDOW: Duration = Duration::from_secs(15 * 60);
const IP_LIMIT: usize = 20;
// SSH (D-SSH-07): also bucket by key fingerprint (failed pubkey attempts).
pub fn check_ip(&mut self, ip: &str) -> Result<(), Duration> { … }
pub fn record_ip(&mut self, ip: &str) { … }
pub fn clear_user(&mut self, user_id: &str) { … } // success clears identity bucket
```

Wire a second limiter (or extend keys) on `AppState` like `git_auth_limiter` in `app.rs`.

---

### `crates/oxidean-core/src/ssh_key_types.rs` (model, transform)

**Analog:** `crates/oxidean-core/src/pat_types.rs`

**DTO conventions** (lines 1–20):
```rust
//! List/response items never carry plaintext secrets — only CreatePatResponse.token
//! holds the one-time reveal value.
use serde::{Deserialize, Serialize};
```
SSH list items expose title, fingerprint, algorithm, `created_at`, `last_used_at` — public key may be returned (not secret); no create-time secret reveal.

---

### `apps/web/src/routes/settings/ssh-keys.tsrx` (route, CRUD)

**Analog:** `apps/web/src/routes/settings/tokens.tsrx`

**Loader + AuthShell + verified gate** (lines 30–50, 116–180):
```typescript
export const Route = createFileRoute("/settings/tokens")({
  loader: async (): Promise<TokensLoaderData> => {
    const res = await fetchSessionMe();
    // unauthenticated | error | ready
  },
});
// redirect unauthenticated → /login?returnTo=…
const verified = phase.kind === "ready" ? phase.user.email_verified === true : false;
// Generate disabled when !verified; copy for Add SSH key
```

**SettingsNav sibling** (`settings-nav.tsrx` lines 1–31) — extend:
```typescript
type SettingsNavProps = {
  active: "profile" | "tokens"; // add | "ssh-keys"
};
```

---

### `apps/web/src/components/settings/ssh-key-list` + revoke dialog (component, CRUD)

**Analogs:** `pat-list.tsrx`, `pat-revoke-dialog.tsrx`

**Query list + AlertDialog confirm** (revoke dialog lines 25–87):
```typescript
/** D-17 revoke confirm — Keep / Revoke (no type-to-confirm). */
export function PatRevokeDialog({ item, open, onOpenChange, onRevoked }) @{
  const res = await apiClient.pat.revoke({ id: item.id });
  // Keep token / Revoke token buttons
}
```
SSH: confirm revoke is enough (D-SSH-05 — public keys, no secret reveal).

**List pattern** (`pat-list.tsrx` lines 63–68):
```typescript
const list = useQuery({
  ...patListQueryOptions(apiClient),
  enabled,
});
```
After `make rpc-gen`, add `sshKeyListQueryOptions` beside PAT helpers in generated client.

---

### `apps/web/src/lib/public-origin.ts` + CloneBox (utility + component)

**Analogs:** `public-origin.ts`, `clone-box.tsrx`, `pat-how-to.tsrx`

**HTTPS URL builder** (lines 24–32) — add sibling `sshCloneUrl`:
```typescript
export function httpsCloneUrl(origin: string, owner: string, repo: string): string {
  const base = (origin || resolvePublicOriginClient()).replace(/\/$/, "");
  return `${base}/${owner}/${repo}.git`;
}
// SSH (D-SSH-02): `git@{OXIDEAN_SSH_HOST}:{owner}/{repo}.git`
// Prefer scp-style; document Port/Host alias when advertised port ≠ 22 — do not make ssh:// primary.
```

**Replace SSH placeholder** (`clone-box.tsrx` lines 125–132):
```typescript
<PatHowTo httpsUrl={httpsUrl as string} compact />
{/* replace: */}
<p className="…">SSH cloning arrives in a later phase.</p>
```
Copy HTTPS copy-input + clipboard pattern for SSH URL; add compact “add a key” CTA modeled on `PatHowTo` (`pat-how-to.tsrx` lines 14–48) linking to `/settings/ssh-keys`.

---

### `docker-compose.yml` + `scripts/smoke-git-ssh.sh` (config)

**Analogs:** `docker-compose.yml`, `scripts/smoke-git-https.sh`, `Makefile`

**Compose today is HTTP-only Traefik** (lines 6–14, 67–78) — SSH must **not** use Traefik HTTP routers; publish TCP on the SSH service (e.g. `2222:2222`), share `./var/repos` volume, set `OXIDEAN_SSH_HOST` / `OXIDEAN_SSH_PORT`.

**Smoke script skeleton** (`smoke-git-https.sh` lines 23–60, 94–140):
```bash
set -euo pipefail
# skip gracefully if docker missing (exit 0)
# wait for health
# git ls-remote / optional push
# Makefile: smoke-git-ssh: @./scripts/smoke-git-ssh.sh
```
SSH smoke: `GIT_SSH_COMMAND` or `~/.ssh` test key → `git@{host}:{owner}/{repo}.git` on port 2222; do not assert Traefik HTML routing (that check is HTTPS-only).

---

## Shared Patterns

### Authentication / gates
**Source:** `crates/oxidean-api/src/auth/gate.rs`, `pat/mod.rs`, Smart HTTP ACL
**Apply to:** SSH key create RPC; SSH push path; settings Add button
- Session `require_verified` for key registration
- SSH transport: fingerprint → user; force username `git`
- Push: verified email + owner-only (same as Smart HTTP receive-pack)
- Fetch private: authenticated owner only until Phase 10

### Error handling
**Source:** `git_smart_http.rs`, `repo/acl.rs`
**Apply to:** SSH pack sessions
- Prefer clear **git protocol errors** on deny (not HTTP 401/404 codes)
- Soft-fail last-used updates with `tracing::warn`
- RPC: `AppError` codes (`auth.*`, `rpc.bad_input`, domain `sshKey.*` / `pat.*`-shaped)

### Rate limiting
**Source:** `crates/oxidean-api/src/pat/rate_limit.rs`
**Apply to:** failed SSH pubkey auth (per IP + per fingerprint)

### Validation / RPC codegen
**Source:** `pat/mod.rs`, `rpc.rs`, `bin/rpc_gen.rs`
**Apply to:** all new SSH RPCs — change Rust → `make rpc-gen`; never hand-edit `@oxidean/api-client` as source of truth

### DB dialect isolation
**Source:** `oxidean-db` `pats.rs` + tri-dialect `0008_pats`
**Apply to:** SSH key tables/CRUD — no dialect branching in API

### Ops / smoke
**Source:** `scripts/smoke-git-https.sh`, `Makefile` `smoke-git-https`
**Apply to:** `smoke-git-ssh` Compose-first; API-only `make dev` may omit SSH listener

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| Russh/host-key persistence & rotation | service | streaming | No SSH server in-repo yet — use RESEARCH/discretion for crate + host-key files |
| Traefik TCP router for SSH | config | streaming | Stack uses HTTP entrypoints only; prefer direct host port publish (D-SSH-07) |

## Metadata

**Analog search scope:** `crates/oxidean-api/{routes,git,pat,repo,auth}`, `crates/oxidean-db/{migrations,src,tests}`, `crates/oxidean-core`, `apps/web/{routes/settings,components/{repo,settings},lib}`, `docker-compose.yml`, `scripts/`, `Makefile`
**Files scanned:** ~35 tracked analogs (git ls-files verified)
**Upstream:** `09-CONTEXT.md` only (no `09-RESEARCH.md` at map time)
**Pattern extraction date:** 2026-09-14
