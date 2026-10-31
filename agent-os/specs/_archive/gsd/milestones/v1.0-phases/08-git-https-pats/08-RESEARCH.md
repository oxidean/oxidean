# Phase 8: Git HTTPS & PATs - Research

**Researched:** 2026-09-13
**Domain:** Git Smart HTTP + personal access tokens (Axum, system `git-http-backend`, Octane settings UI)
**Confidence:** HIGH (codebase + official git-scm docs); MEDIUM on forge scope catalogs / rate-limit numbers (discretion recommendations)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
### A — PAT capabilities
- **D-01:** PATs authenticate **git over HTTPS only** in Phase 8 — not RPC/API Bearer; web stays on opaque session cookies — **Reversibility:** costly — credential class split
- **D-02:** Dual remotes like GitHub/other forges: **HTTPS + PAT now**; **SSH** remains Phase 9
- **D-03:** Support **both classic scopes and fine-grained scopes** in Phase 8 (pulls PAT portion of PLAT-V2-02 forward; OAuth apps stay later) — **Reversibility:** costly — token model + UI
- **D-04:** Fine-grained model targets **GitHub-/forge parity** (repository selection + account permissions as needed for real forge use); exact scope catalog is Claude discretion via research
- **D-05:** **Two separate create flows** — Classic PAT and Fine-grained PAT (not a single wizard)
- **D-06:** Fine-grained tokens support **selected repos** and **all current + future repositories** (GitHub-style)
- **D-07:** **Optional expiry** — user may set an expiration date or choose no expiration
- **D-08:** **Prefixed opaque** token strings with **Oxidean-only** prefixes (distinct classic vs fine-grained variants); **no** `github` / `gh*` in the prefix — **Reversibility:** one-way — published token format / secret-scanning hooks
- **D-09:** Token list shows **last-used timestamp** and **last-used IP**

### B — HTTPS credential contract
- **D-10:** Credential form is **username + PAT as password** (forge-compatible); researcher locks exact username aliases (`git` / account username / etc.) — **Reversibility:** costly — client docs + auth parser
- **D-11:** **Hard reject** account passwords for git HTTPS, with a **hint** pointing users to create a PAT / docs (GIT-02)
- **D-12:** **PAT-only** for Smart HTTP — **session cookies never** authenticate git operations
- **D-13:** Clone box / empty-repo guidance includes a **full how-to panel** (username, password=PAT, create-token CTA, examples) — extends Phase 7 D-22

### C — Token management UX
- **D-14:** Manage tokens at **`/settings/tokens`** with settings/chrome nav — **Reversibility:** costly — settings IA
- **D-15:** After create: **one-time plaintext reveal** with copy; never shown again (must revoke + recreate)
- **D-16:** **Required note/name** on create (e.g. “laptop”, “CI”)
- **D-17:** **Confirm dialog** before revoke

### D — Smart HTTP surface
- **D-18:** Clone URL shape **`https://{host}/{owner}/{repo}.git`** on the same public origin — **Reversibility:** one-way — public git URL scheme
- **D-19:** Clone URL host comes from **`OXIDEAN_PUBLIC_ORIGIN`** (operator config), not the request Host header
- **D-20:** **Anonymous clone/fetch** of **public** repos; **push always requires a PAT**
- **D-21:** Unauthenticated access to **private** / no-access over git → **401 + WWW-Authenticate** (not the web UI’s 404 anti-enumeration) — **Reversibility:** costly — git vs web error contracts differ by design
- **D-22:** Smart HTTP is served **only** on `/{owner}/{repo}.git`; bare `/{owner}/{repo}` remains the web UI
- **D-23:** Authenticated but **insufficient PAT scope** → **403** (not 401)

### E — Verify gate & abuse limits
- **D-24:** **Email verified required** to **create PATs** and to **HTTPS push** (aligns with Phase 5 `require_verified` / `repo.create`)
- **D-25:** Unverified create/push surfaces **`auth.email_unverified` / verify wall** + link to `/verify` (same as other privileged actions)
- **D-26:** **Rate-limit** failed Smart HTTP Basic/PAT auth **per IP and per user**; when limited return **429 + Retry-After**; exact thresholds left to research/planning

### Claude's Discretion
- Exact classic and fine-grained **scope catalogs** (must achieve GitHub-/forge-comparable git HTTPS capabilities)
- Exact username alias set for Basic auth (within D-10)
- Exact rate-limit **N / window** values (within D-26)
- Smart HTTP implementation approach (`git-http-backend` vs custom Axum handlers) — research/planning
- Traefik routing details to serve `/{owner}/{repo}.git` alongside web routes

### Deferred Ideas (OUT OF SCOPE)
- SSH clone/push and SSH key management — Phase 9
- PAT / token use as RPC Bearer — out of Phase 8 by D-01
- OAuth apps / third-party integrations — remaining PLAT-V2-02
- Org-owned / fine-grained org tokens — Phase 10+
- Credential helper packaging beyond docs/how-to panel
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| GIT-02 | User can clone, fetch, and push over HTTPS using a personal access token (not account password) | Smart HTTP via Axum + `git-http-backend`; Basic auth PAT-only; Traefik `.git` route; anonymous public fetch; push always PAT; password never accepted |
| GIT-11 | User can create, list, and revoke personal access tokens used for HTTPS git (and RPC/API where applicable) | **Phase 8 interprets as HTTPS git only** per D-01 (ROADMAP “RPC/API” wording superseded). RPC `pat.*` CRUD + `/settings/tokens` classic/FG flows; hash-at-rest; one-time reveal |
</phase_requirements>

## Summary

Phase 8 adds two complementary surfaces on the existing Phase 7 bare-repo layout (`{OXIDEAN_REPOS_DIR}/{owner}/{name}.git`) and clone URL helper (`httpsCloneUrl` → `{origin}/{owner}/{repo}.git`): (1) **PAT lifecycle** (classic + fine-grained) stored like sessions (SHA-256 at rest, plaintext once), managed via typed RPC and `/settings/tokens`; (2) **Git Smart HTTP** on `/{owner}/{repo}.git/...` authenticated with **username + PAT as password**, never session cookies and never account passwords.

The codebase has **no** Smart HTTP, PAT tables, or Traefik `.git` rules today. Web private ACL returns `repo.not_found` (404-style anti-enumeration); git must diverge per D-21 to **401 + `WWW-Authenticate: Basic`**. Official git docs prescribe CGI `git-http-backend` behind HTTP Basic and **forbid relying on cookies** for git HTTP auth — aligning with D-12.

**Primary recommendation:** Implement Axum Smart HTTP routes that authenticate/authorize then **CGI-spawn `/usr/lib/git-core/git-http-backend`** (already on PATH via system git ≥2.5), with Traefik PathRegexp routing `.git` to the API; ship dual PAT create UIs with the locked scope catalog below; store PAT hashes in `oxidean-db` migration `0008_*`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| PAT create / list / revoke | API / Backend | Browser / Client | Secrets minted & hashed in API; UI is form + one-time reveal |
| PAT persistence | Database / Storage | — | Dialect SQL only in `oxidean-db` |
| Smart HTTP wire protocol | API / Backend | — | Pack streaming + CGI; not web SSR |
| Basic / PAT auth for git | API / Backend | — | Cookie sessions must not apply (D-12) |
| ACL (public anon read / owner private / push) | API / Backend | — | Extend Phase 7 owner-only stub with git status codes |
| Scope check (classic / FG) | API / Backend | — | 403 on insufficient scope (D-23) |
| Email verified gate | API / Backend | Browser / Client | `require_verified` for create+push; verify wall UI |
| Failed-auth rate limit | API / Backend | — | Per IP + per user; 429 + Retry-After |
| Clone URL display / how-to | Browser / Client | Frontend Server (SSR) | `OXIDEAN_PUBLIC_ORIGIN` via existing helpers |
| Traefik `.git` vs web | CDN / Static (edge router) | API / Backend | PathRegexp to API before web catch-all |
| Settings tokens page | Browser / Client | — | Octane `.tsrx` at `/settings/tokens` |

## Discretion Recommendations (planner MUST treat as locked defaults)

### Classic scope catalog (Phase 8)
| Scope id | Label | Grants |
|----------|-------|--------|
| `repo` | Full repository access | HTTPS **fetch + push** for **all** repositories the token owner can access (Phase 8: owner-only private ACL) |

No other classic scopes in Phase 8 (no `gist`, `user`, `admin:*`, `workflow`). Create UI: single required checkbox `repo` (or “Full control of repositories”). [CITED: docs.github.com managing-your-personal-access-tokens — classic `repo` for command-line repo access]

### Fine-grained catalog (Phase 8)
**Repository access (D-06):**
- `selected` — explicit list of repo IDs owned by the user
- `all` — all current **and future** repos the user owns

**Repository permissions:**
| Permission | Levels | Git effect |
|------------|--------|------------|
| `contents` | `read` \| `write` | `read` → upload-pack (clone/fetch); `write` → receive-pack (push) **and** read |
| `metadata` | `read` (always) | Auto-granted whenever any repo permission is set (GitHub parity); no standalone control in Phase 8 UI |

No FG account/org permissions in Phase 8 (orgs = Phase 10). [CITED: docs.github.com FG Contents read/write for git; Forgejo `read:repository` / `write:repository`]

### Username aliases (D-10)
Accept Basic username if **any** of:
1. Token owner’s account `username` (case-insensitive)
2. Literal `git`
3. Literal `token`
4. Literal `oauth2`

**Identity always from PAT** (password field). Empty username → 401. Username that does not match the alias set → 401 with same generic failure (do not leak whether PAT was valid). [CITED: docs.github.com — username required but not used to authenticate; Forgejo commonly accepts `token`]

### Rate limits (D-26)
| Dimension | N | Window | On exceed |
|-----------|---|--------|-----------|
| Client IP | **20** failed Basic/PAT attempts | **15 minutes** sliding | `429` + `Retry-After` (seconds until window end) |
| User id (when username/PAT maps to a user) | **10** failed attempts | **15 minutes** sliding | same |

Successful auth clears that user bucket only (not IP). Aligns in spirit with existing email redeem `MAX_REDEEM_ATTEMPTS = 10`. [ASSUMED] — GitHub does not publish identical git-HTTPS failed-auth numbers; values chosen for forge abuse resistance without locking out shared NATs too aggressively.

### Smart HTTP approach
**Use system `git-http-backend` CGI spawned from Axum** after auth/ACL — do **not** hand-roll pkt-line / pack protocol in Rust. Set `GIT_HTTP_EXPORT_ALL`, `GIT_PROJECT_ROOT=<repos_dir>`, `PATH_INFO=/{owner}/{repo}.git/...`, forward `Git-Protocol` → `GIT_PROTOCOL`, set `REMOTE_USER` when authenticated so receive-pack is enabled. **Smart-only** paths (`info/refs?service=…`, `git-upload-pack`, `git-receive-pack`); do not AliasMatch dumb `/objects/` (ACL bypass risk). [CITED: git-scm.com/docs/git-http-backend; git-scm.com/docs/http-protocol]

### Traefik
Add API router rule (priority **110** > existing API 100 / web 1):

```
Host(`localhost`) && PathRegexp(`^/[^/]+/[^/]+\.git`)
```

Merge into API service (port 8080) so `/{owner}/{repo}.git/info/refs` never hits the web SPA. [VERIFIED: docker-compose.yml:69-102]

### Token string prefixes (D-08)
| Kind | Prefix |
|------|--------|
| Classic | `ona_pat_` |
| Fine-grained | `ona_fg_` |

Follow with CSPRNG hex (32+ bytes). Never `ghp_`, `github_pat_`, `gho_`, etc.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| system `git` + `git-http-backend` | ≥2.5.0 (boot gate already) | Smart HTTP CGI | Official git server; already required by Phase 7 [VERIFIED: crates/oxidean-api/src/main.rs git version gate; `/usr/lib/git-core/git-http-backend` present] |
| Axum | 0.8 (in-tree) | HTTP routes + Basic auth gate | Existing API router [VERIFIED: crates/oxidean-api/Cargo.toml axum 0.8] |
| `sha2` | 0.11 (in-tree) | PAT token_hash at rest | Same pattern as sessions [VERIFIED: session.rs SHA-256; Cargo.toml sha2] |
| `argon2` | 0.6 (in-tree) | **Not** for git auth | Passwords rejected without git using password verify as success path [VERIFIED: Cargo.toml] |
| Octane `.tsrx` + TanStack Query | in-tree `@octanejs/*` | `/settings/tokens`, clone how-to | Project UI standard |
| `@oxidean/api-client` | generated | `pat.*` RPC | `make rpc-gen` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio process | workspace | Spawn CGI, stream stdio | Smart HTTP handler |
| Existing AlertDialog (Base UI) | in-tree | Revoke confirm (D-17) | Same pattern as branch delete |
| In-memory / DB counters | — | Failed-auth rate limit | Prefer simple store keyed by IP/user; no new rate-limit crate required for Phase 8 |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `git-http-backend` CGI | Pure Axum pkt-line + `git upload-pack --stateless-rpc` | More code, protocol v2 edge cases; only if CGI streaming proves intractable |
| `git-http-backend` | gitoxide HTTP server | Out of Phase 7 D-32 (CLI adapter); defer |
| SHA-256 PAT hash | Argon2id for PATs | Argon2 slows every git request; sessions already use SHA-256 for opaque tokens |
| New `tower-governor` crate | Custom counter | Avoid new dep unless counters become complex |

**Installation:** None required for Phase 8 if CGI path is used — **no new npm/crates packages**.

**Version verification:** `git version 2.55.0` on research host; `git-http-backend` at `/usr/lib/git-core/git-http-backend`. Axum/sha2 versions from `crates/oxidean-api/Cargo.toml` (read this session).

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| *(none — no new installs)* | — | — | — | — | — | N/A |

**Packages removed due to [SLOP] verdict:** none  
**Packages flagged as suspicious [SUS]:** none  

Phase 8 should **not** add `tower-governor`, CGI helper crates, or UI kits. If planner later adds a crate, re-run `gsd_run query package-legitimacy check` before install.

## Architecture Patterns

### System Architecture Diagram

```text
git client
   │  HTTPS Basic (username + PAT)  OR  anonymous GET (public fetch)
   ▼
Traefik :80
   │  PathRegexp ^/[^/]+/[^/]+\.git  → API :8080 (prio 110)
   │  PathPrefix /api|/uploads|/health → API (prio 100)
   │  Host catch-all → Web :3000 (prio 1)   # /{owner}/{repo} UI only
   ▼
Axum Smart HTTP (`/{owner}/{repo}.git/*`)
   │  ignore Cookie: oxidean_session
   ├─ rate-limit failed auth (IP + user)
   ├─ parse Authorization: Basic
   │     ├─ password not PAT-shaped → 401 + hint (create PAT)
   │     ├─ PAT hash miss / expired / revoked → 401 + WWW-Authenticate
   │     └─ PAT ok → resolve user; update last_used_*
   ├─ ACL
   │     ├─ public + upload-pack + anon → allow
   │     ├─ private + no auth → 401 + WWW-Authenticate (D-21)
   │     ├─ private + auth but not owner → 401 (no existence leak via 404)
   │     ├─ receive-pack always needs PAT + verified email
   │     └─ scope insufficient → 403 (D-23)
   └─ spawn git-http-backend CGI
         GIT_PROJECT_ROOT=OXIDEAN_REPOS_DIR
         PATH_INFO=/{owner}/{repo}.git/...
         GIT_HTTP_EXPORT_ALL=1
         REMOTE_USER=<username> when authenticated
         stream request/response bodies

Browser (session cookie)
   │  /api/rpc  pat.createClassic | createFineGrained | list | revoke
   │  require_verified → auth.email_unverified
   ▼
/settings/tokens  + CloneBox how-to panel
```

### Recommended Project Structure
```
crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0008_pats.sql
crates/oxidean-db/src/pats.rs
crates/oxidean-core/src/pat_types.rs          # DTOs + scope enums
crates/oxidean-api/src/pat/mod.rs             # RPC handlers
crates/oxidean-api/src/routes/git_smart_http.rs
crates/oxidean-api/src/git/http_backend.rs    # CGI spawn helper
crates/oxidean-api/tests/git_smart_http_*.rs
crates/oxidean-api/tests/pat_rpc.rs
apps/web/src/routes/settings/tokens.tsrx
apps/web/src/components/settings/pat-*.tsrx
apps/web/src/components/repo/clone-box.tsrx    # extend how-to (D-13)
docker-compose.yml                             # Traefik PathRegexp
docs/API.md / docs/CONFIGURATION.md            # document Smart HTTP + PAT
```

### Pattern 1: PAT hash storage (mirror sessions)
**What:** CSPRNG opaque token; store only `SHA-256` hex; show plaintext once.
**When to use:** All PAT creates.
**Example:** Session pattern already uses cookie CSPRNG + SHA-256 hex at rest — reuse hashing helper style from `auth/session.rs` (`SESSION_COOKIE_NAME = "oxidean_session"`). [VERIFIED: crates/oxidean-api/src/auth/session.rs:15-16]

### Pattern 2: Axum + CGI Smart HTTP
**What:** Authenticate in Axum; exec `git-http-backend` with CGI env; pipe body.
**When to use:** All `/{owner}/{repo}.git` smart endpoints.
**Anti-pattern:** Serving dumb object URLs without the same ACL.

### Pattern 3: Dual create flows (D-05)
**What:** Separate routes/components: Classic vs Fine-grained; shared list + revoke.
**When to use:** `/settings/tokens` and empty-repo CTA.

### Anti-Patterns to Avoid
- **Reusing `resolve_repo_for_read` HTTP semantics for git** — web returns `repo.not_found`; git private unauth must be **401** (D-21). Share ACL *decision* logic; split *response* mapping. [VERIFIED: crates/oxidean-api/src/repo/acl.rs:8-11,53-58]
- **Cookie auth on Smart HTTP** — protocol SHOULD NOT require cookies; locked by D-12. [CITED: git-scm.com/docs/http-protocol Authentication / Session State]
- **Calling `verify_password` to “detect” account passwords as a success path** — never authenticate git with Argon2 password hash; reject non-PAT secrets with hint (D-11).
- **Enabling dumb HTTP `/objects/` static maps** — bypasses pack ACL.
- **Hand-editing `packages/api-client`** — change Rust + `make rpc-gen`.
- **Dialect SQL in `oxidean-api`** — PAT tables only in `oxidean-db`.
- **Mixing `return (` JSX with Rivet in `.tsrx`** — Octane skill.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Smart HTTP pack negotiation | Custom pkt-line server | `git-http-backend` / `git-{upload,receive}-pack` | Protocol v1/v2, sideband, edge cases |
| Session auth for git | Cookie parser on `.git` | Basic + PAT only | Spec + D-12 |
| PAT KDF | New Argon2 per request | SHA-256 hex like sessions | Latency on every fetch |
| Confirm UI | Custom modal | Existing `AlertDialog` | Branch delete / repo settings already |
| Public origin | Request Host for clone URL | `OXIDEAN_PUBLIC_ORIGIN` / `httpsCloneUrl` | D-19; already implemented |

**Key insight:** Auth/ACL/rate-limit are the product; git wire protocol is a solved CGI.

## Common Pitfalls

### Pitfall 1: Web catch-all steals `.git`
**What goes wrong:** Traefik web priority 1 serves `/{owner}/{repo}.git` as SPA → broken clones.
**Why:** Current compose has no `.git` rule. [VERIFIED: docker-compose.yml:69-102]
**How to avoid:** PathRegexp on API with priority 110; e2e `git ls-remote` through Traefik.
**Warning signs:** HTML 200 from clone URL.

### Pitfall 2: `http.receivepack` / anonymous push
**What goes wrong:** Anonymous push succeeds or push gets 403 without auth challenge.
**Why:** `git-http-backend` disables receive-pack for anonymous unless config/`REMOTE_USER` set. [CITED: git-http-backend SERVICES]
**How to avoid:** Require PAT before CGI for receive-pack / `service=git-receive-pack`; set `REMOTE_USER` after auth; never set `http.receivepack=true` for anon.

### Pitfall 3: Missing `GIT_HTTP_EXPORT_ALL`
**What goes wrong:** 403 export denied — bare repos lack `git-daemon-export-ok` (Phase 7 `init_bare` does not create it). [VERIFIED: crates/oxidean-git/src/cli.rs:329-353]
**How to avoid:** Always set `GIT_HTTP_EXPORT_ALL` in CGI env; ACL stays in Axum.

### Pitfall 4: Scope 401 vs 403 confusion
**What goes wrong:** Clients re-prompt for credentials forever on missing scope.
**Why:** 401 triggers credential retry; insufficient scope must be **403** (D-23).

### Pitfall 5: Email unverified push without verify wall
**What goes wrong:** Unverified users push or create PATs.
**How to avoid:** `require_verified` on `pat.create*`; Smart HTTP push checks `email_verified_at` and returns structured denial + docs link (map to `auth.email_unverified` semantics). [VERIFIED: gate.rs:20-42 — codes `auth.unauthenticated`, `auth.email_unverified`]

### Pitfall 6: last-used IP spoofing
**What goes wrong:** Trusting `X-Forwarded-For` without Traefik trust.
**How to avoid:** Use same trusted proxy / peer addr pattern as rest of API; document Compose Traefik as sole front.

### Pitfall 7: PAT plaintext in logs / RPC
**What goes wrong:** Token echoed in tracing or list responses.
**How to avoid:** Create response field `token` only once; list returns prefix + metadata only.

## Code Examples

### Clone URL (already shipped)
```typescript
// Source: apps/web/src/lib/public-origin.ts:24-31 [VERIFIED]
export function httpsCloneUrl(
  origin: string,
  owner: string,
  repo: string,
): string {
  const base = (origin || resolvePublicOriginClient()).replace(/\/$/, "");
  return `${base}/${owner}/${repo}.git`;
}
```

### Bare path (already shipped)
```rust
// Source: crates/oxidean-api/src/git/mod.rs:10-28 [VERIFIED]
/// Bare repo path: `{repos_dir}/{owner}/{name}.git` (D-30).
pub fn bare_repo_path(repos_dir: &Path, owner: &str, name: &str) -> Result<PathBuf, AppError> {
    // ... validation ...
    Ok(repos_dir.join(owner).join(format!("{name}.git")))
}
```

### require_verified (reuse for PAT create)
```rust
// Source: crates/oxidean-api/src/auth/gate.rs:20-42 [VERIFIED]
/// Unauthenticated → `auth.unauthenticated`; unverified → `auth.email_unverified`.
pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> { /* ... */ }
```

### Smart HTTP entry points (protocol)
```http
GET  /{owner}/{repo}.git/info/refs?service=git-upload-pack
GET  /{owner}/{repo}.git/info/refs?service=git-receive-pack
POST /{owner}/{repo}.git/git-upload-pack
POST /{owner}/{repo}.git/git-receive-pack
```
[CITED: git-scm.com/docs/http-protocol]

### Suggested RPC surface (planner)
| Procedure | Auth | Notes |
|-----------|------|-------|
| `pat.createClassic` | `require_verified` | note, scopes[`repo`], optional expires_at → one-time token |
| `pat.createFineGrained` | `require_verified` | note, repo_access, contents, optional expires_at |
| `pat.list` | session | no secrets; include last_used_at/ip, prefix, kind |
| `pat.revoke` | session | id; confirm in UI only |

Error codes: reuse `auth.email_unverified`, `auth.unauthenticated`; add `pat.not_found`, `pat.invalid_scope`, `pat.note_required`.

### Suggested DB sketch (all dialects)
Tables: `personal_access_tokens` (id, user_id, kind `classic|fine_grained`, name, token_prefix, token_hash CHAR(64) UNIQUE, scopes_json / contents_perm, repo_access `all|selected`, expires_at NULL, revoked_at NULL, last_used_at NULL, last_used_ip NULL, created_at); `personal_access_token_repos` (token_id, repository_id) for FG selected.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Account password over HTTPS git | PAT / SSH only | GitHub 2021+; forges followed | D-11 hard reject |
| Classic-only broad scopes | Classic + fine-grained | GitHub FG PATs | D-03 dual model |
| Dumb HTTP object serving | Smart HTTP | git ≥1.6.6 default clients | Prefer smart-only |
| Cookie sessions for git | Basic / SSH | http-protocol guidance | D-12 |

**Deprecated/outdated:**
- Password auth for git HTTPS — reject with PAT hint
- RPC Bearer PATs in Phase 8 — deferred by D-01 despite GIT-11 parenthetical

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Failed-auth limits 20/IP and 10/user per 15m are appropriate | Discretion / D-26 | Too strict → shared NAT pain; too loose → brute force — tune in ops |
| A2 | Accepting `git`/`token`/`oauth2` aliases will not conflict with usernames | Username aliases | If a user registers those usernames, alias and account collide — recommend reserved-username block or prefer exact username match first |
| A3 | CGI streaming from Axum is reliable enough under Compose without a dedicated CGI crate | Smart HTTP approach | May need fallback to `--stateless-rpc` direct spawn |
| A4 | Phase 8 FG “all future repos” means all repos **owned by** the user (not collab) | FG catalog | Phase 10 collab will need ACL expansion |
| A5 | Private + authenticated non-owner → 401 (not 404) for git is acceptable | Pitfalls / D-21 | Slightly different from “no enumeration”; still no 200 |

## Open Questions (RESOLVED)

1. **Reserved usernames for aliases?** — **RESOLVED**
   - What we know: Aliases `git` / `token` / `oauth2` recommended.
   - What's unclear: Whether signup already blocks them.
   - Recommendation: Planner adds reserved-name check if missing; prefer documenting aliases in how-to panel.
   - **Resolution:** Extend `RESERVED_USERNAMES` with `git`, `token`, `oauth2` (08-02 ASSUME / 08-03 schema); document aliases in CloneBox/QuickSetup how-to (08-12).

2. **Unverified user fetch of private own repo?** — **RESOLVED**
   - What we know: D-24 requires verified for create + **push**.
   - What's unclear: Whether unverified owner may **fetch** private via PAT.
   - Recommendation: Allow fetch with valid PAT; block push + create only (matches “privileged write” spirit).
   - **Resolution:** Allow upload-pack (fetch) with valid PAT for unverified owners; block `pat.create*` and receive-pack (push) via `require_verified` / `auth.email_unverified` (08-04 ASSUME Open Q2; 08-06).

3. **Rate-limit storage process-local vs DB?** — **RESOLVED**
   - What we know: No global HTTP limiter exists; email limits are DB-backed.
   - Recommendation: In-memory per API process for Phase 8 (Compose single API replica); document multi-replica follow-up.
   - **Resolution:** In-memory `pat/rate_limit.rs` per API process (Compose single replica); multi-replica deferred (08-06 ASSUME / D-26).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` + `git-http-backend` | Smart HTTP | ✓ | 2.55.0; backend at `/usr/lib/git-core/git-http-backend` | — |
| `cargo` / nextest | Rust tests | ✓ | cargo 1.100.0-nightly | `cargo test` |
| `bun` | Vitest / web | ✓ | 1.4.0 | — |
| Docker / Traefik Compose | Routing e2e | ✗ (docker CLI missing on research host) | — | Unit/integration against Axum router; Traefik rule still required in compose for `make up` |
| PostgreSQL/MySQL/SQLite | PAT migrations | ✓ (project already) | tri-dialect | — |

**Missing dependencies with no fallback:**
- None for code implementation; Compose smoke needs Docker on the operator machine (normal for this repo).

**Missing dependencies with fallback:**
- Docker absent here → research validated Traefik labels from `docker-compose.yml` text; executor must verify with `make up` / `git ls-remote` through Traefik.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust: cargo-nextest / `cargo test`; Web: Vitest via Bun |
| Config file | workspace Cargo; `apps/web` Vitest (existing) |
| Quick run command | `cargo nextest run -p oxidean-api -E 'test(pat_)|test(git_smart)'` |
| Full suite command | `make test` (+ `make test-e2e-stack` for Traefik/git client) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GIT-11 | create classic/FG returns one-time token; list omits secret; revoke hides token | integration | `cargo nextest run -p oxidean-api -E 'test(pat_)'` | ❌ Wave 0 |
| GIT-11 | create without verified email → `auth.email_unverified` | integration | same | ❌ Wave 0 |
| GIT-11 | migration parity 0008 across dialects | unit | `cargo nextest run -p oxidean-db -E 'test(migration_parity)'` | ✅ (extend) |
| GIT-02 | public anon `info/refs?service=git-upload-pack` 200 | integration | `cargo nextest run -p oxidean-api -E 'test(git_smart)'` | ❌ Wave 0 |
| GIT-02 | private anon → 401 + `WWW-Authenticate` | integration | same | ❌ Wave 0 |
| GIT-02 | push with PAT (contents write / classic repo) succeeds | integration | same (+ temp bare + `git push` subprocess) | ❌ Wave 0 |
| GIT-02 | account password as Basic password → 401 + PAT hint | integration | same | ❌ Wave 0 |
| GIT-02 | session cookie alone → treated as anon (no auth) | integration | same | ❌ Wave 0 |
| GIT-02 | insufficient scope → 403 | integration | same | ❌ Wave 0 |
| GIT-02 | failed auth over limit → 429 + Retry-After | integration | same | ❌ Wave 0 |
| GIT-02 | unverified push denied | integration | same | ❌ Wave 0 |
| GIT-11 | `/settings/tokens` create/list/revoke UI | component/e2e | Vitest + optional stack e2e | ❌ Wave 0 |
| GIT-02 | CloneBox how-to panel CTA | component | Vitest | ❌ Wave 0 |
| GIT-02 | Traefik `.git` → API | smoke/e2e | `make up` + `git ls-remote http://localhost/...` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** targeted nextest filter above
- **Per wave merge:** `make test` + `make rpc-sync-check`
- **Phase gate:** Full suite green + Smart HTTP e2e through Traefik before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/oxidean-api/tests/pat_rpc.rs` — covers GIT-11
- [ ] `crates/oxidean-api/tests/git_smart_http.rs` — covers GIT-02 auth/ACL/status codes
- [ ] `crates/oxidean-db/migrations/*/0008_pats.sql` + parity
- [ ] `apps/web` Vitest for tokens page / clone how-to
- [ ] Compose Traefik PathRegexp + smoke script for `git ls-remote` / `git push`
- [ ] Optional: harness helper to spawn `git` client against `router_with_state` (hyper listener) without Docker

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | PAT Basic auth; reject passwords; no cookie for git |
| V3 Session Management | yes (negative) | Session cookies ignored on Smart HTTP (D-12) |
| V4 Access Control | yes | Owner-only private; scope checks; public anon read |
| V5 Input Validation | yes | Owner/repo path validation (`bare_repo_path`); scope enums |
| V6 Cryptography | yes | CSPRNG tokens; SHA-256 at rest; never log plaintext |

### Known Threat Patterns for Git HTTPS + PATs

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Password stuffing / PAT brute force | Elevation | Per-IP + per-user failed-auth rate limit; 429 |
| Session cookie used as git credential | Spoofing | Ignore cookies on `.git` routes |
| Account password accepted for git | Elevation | PAT-prefix / hash-only auth; hint on failure |
| Scope bypass (read token pushes) | Elevation | Enforce contents/classic scope → 403 |
| Private repo enumeration via git 404 vs 401 | Information disclosure | D-21 401 for unauth private; do not 200 |
| Dumb HTTP object scrape | Information disclosure | Smart-only endpoints; no `/objects/` alias |
| PAT leakage in list/logs | Information disclosure | One-time reveal; hash at rest; prefix only in list |
| CGI path traversal | Tampering | Validate owner/name; `PATH_INFO` under repos_dir only |
| Unverified mass token mint | Elevation | `require_verified` on create |

## Project Constraints (from .cursor/rules/)

| Rule | Directive |
|------|-----------|
| oxidean-core | One product; Octane `.tsrx` not React; `make rpc-gen` for RPC; dialect SQL only in `oxidean-db`; no secrets in commits; prefer `make test` / `rpc-sync-check` / e2e; extend existing patterns |
| octane-ui | `.tsrx` + `@if`/`@else` (no `@else if`); `onInput` for text; TanStack Query via session helpers; no `react`→Octane alias |
| rpc-codegen | Rust authoritative; regenerate client; no hand-patch as lasting fix; `make rpc-sync-check` |
| rust-crates | core = pure types; db = SQL; api = HTTP; `Result` no unwrap outside tests; preserve `require_verified`; nextest CI |

## Sources

### Primary (HIGH confidence)
- [VERIFIED: codebase] `08-CONTEXT.md`, Phase 7 ACL/git layout, `session.rs`, `gate.rs`, `public-origin.ts`, `docker-compose.yml`, migrations `0002`/`0007`
- [CITED: https://git-scm.com/docs/http-protocol] Smart HTTP, Basic auth, no cookies
- [CITED: https://git-scm.com/docs/git-http-backend] CGI env, EXPORT_ALL, receive-pack auth behavior

### Secondary (MEDIUM confidence)
- [CITED: https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens] Classic `repo`; FG Contents; username+PAT
- [CITED: https://forgejo.org/docs/v15.0/user/authentication/token-scope/] `read:repository` / `write:repository`; repo selectors
- [CITED: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api] Retry-After patterns (not identical to git failed-auth)

### Tertiary (LOW confidence)
- Web search digests on Axum CGI wrapping — treated as corroboration only; recommendation grounded in official `git-http-backend` docs

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — in-tree crates + official git CGI; no new packages
- Architecture: HIGH — matches CONTEXT locks + existing Traefik/API split
- Pitfalls: HIGH — derived from compose labels, init_bare, http-backend docs
- Discretion numbers (rate limits, aliases): MEDIUM — recommended defaults, not upstream mandates

**Research date:** 2026-09-13  
**Valid until:** 2026-10-13 (git HTTP protocol stable; re-check if Traefik major or Axum 0.9 migrates)
