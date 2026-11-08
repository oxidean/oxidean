# Phase 4: Auth Sessions & Email - Research

**Researched:** 2026-09-10
**Domain:** Rust-native auth (local + WorkOS + OIDC), cookie sessions, profile/avatar, email adapters
**Confidence:** HIGH

## Summary

Phase 4 adds the first real identity surface on top of the existing Axum RPC + multi-dialect `oxidean-db` stack. Auth must live in **`oxidean-api` / Rust domain code**, not Better Auth. Sessions are **opaque HttpOnly cookies** backed by a **first-class `sessions` table** (so logout-this-device and logout-all-devices are trivial SQL). Email is a small **`EmailSender` trait** with log-sink / SMTP (`lettre`) / Resend (`reqwest`) adapters; local signup sends a welcome message to prove the path. WorkOS and generic OIDC are first-class provider adapters that, after callback, **mint the same Oxidean session** as local password login.

The codebase already has CORS `allow_credentials(true)`, API client `credentials: "include"`, dotted `system.*` RPC + `rpc-gen`, per-dialect sqlx migrations with parity tests, Vite `/api` proxy, Traefik same-host Compose, and an assets-only SW that bypasses `/api/*`. Phase 4 should extend those patterns — not introduce a second auth stack or a dialect-specific session middleware that bypasses `oxidean-db`.

**Primary recommendation:** Build `auth.*` / `user.*` / `admin.auth.*` RPC + thin HTTP callback/upload routes; store users/sessions/identities/settings in `oxidean-db` migrations on all three dialects; use Argon2id, lettre, WorkOS official crate, and `openidconnect` — **do not** adopt `tower-sessions-sqlx-store` (breaks uniform DB boundary and weakens logout-all).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### A — Identity (local)
- **D-01:** Signup requires **email + username/handle** (GitHub-shaped)
- **D-02:** Login accepts **either email or username** plus password
- **D-03:** Username rules: **GitHub-like** — 1–39 chars, alphanumeric + hyphen, no leading/trailing hyphen, unique, reserved list (`admin`, `api`, `settings`, …)

#### B — Auth architecture & providers
- **D-04:** **Rust-native auth core** in the API (not Better Auth in the web layer)
- **D-05:** Uniform **provider adapter** interface; modes: **`local` | `workos` | `oidc`** (generic OIDC)
- **D-06:** Provider naming: use **`local`** (not “in-house”)
- **D-07:** **WorkOS** integrated via the **official WorkOS Rust SDK** (`workos` crate) — AuthKit/SSO start + callback on the API; after callback, mint the **same Oxidean session** as local
- **D-08:** **Generic OIDC** supported alongside local and WorkOS (Auth0/Keycloak/Okta-class IdPs)
- **D-09:** Provider configuration: **ENV/image bootstrap defaults** + **system-admin dashboard** can **override and persist** instance auth settings
- **D-10:** Rejected: Better Auth + `@octanejs/better-auth` as the session owner (client bindings alone don’t solve Rust forge identity; WorkOS AuthKit is not a Better Auth plugin)

#### C — Sessions & cookies
- **D-11:** **HttpOnly secure cookie** + **server-side session store** (CORS credentials already prepared in Phase 1)
- **D-12:** **Shorter default session** (e.g. ~24h / idle-oriented) with **Remember me** extending lifetime (e.g. ~30d) — exact numbers planner/researcher may refine
- **D-13:** Header/account **Log out** = **this device/session only**; profile/settings also offers **Log out all devices** (revoke all sessions)

#### D — Auth UI
- **D-14:** Dedicated routes: **`/login`** and **`/signup`**
- **D-15:** Post-auth redirect: **`returnTo` previous page if it wasn’t the homepage; otherwise `/dashboard`** (thin signed-in shell acceptable until a richer home exists)
- **D-16:** Mode-exclusive UI on those routes: **`local`** → Oxidean custom email/password (+ username on signup) forms; **`workos`** → WorkOS AuthKit/SSO flow driven from Rust (redirect/PKCE + callback; Oxidean chrome around CTA); **`oidc`** → standard OIDC redirect/callback with Oxidean chrome
- **D-17:** Enable header **Sign in / Sign up** (and landing Get started as appropriate) to these routes; signed-in chrome shows account menu (profile, log out)

#### E — Profile
- **D-18:** Profile fields: **display name**, **username**, **bio**, **avatar file upload** (volume-backed storage — planner chooses path layout; no external object store required in Phase 4)

#### F — Email
- **D-19:** Implement **log-sink**, **SMTP**, and **Resend** adapters (AUTH-09/10/11)
- **D-20:** Phase 4 sends a **welcome email on local signup** to exercise adapters
- **D-21:** **Email verification** and **password-reset** messages belong to **Phase 5** (do not implement those flows here)

### Claude's Discretion
- Exact session TTLs within D-12 spirit; cookie names; CSRF strategy for cookie sessions
- Reserved username list contents beyond the examples
- Avatar size/format limits and image processing
- Minimal `/dashboard` content for Phase 4
- How OIDC/WorkOS claim → local user linking is modeled (`auth_identities` shape)
- Whether admin auth settings live under `/admin/...` and how the first admin is available in Phase 4 before Phase 6 wizard (dev seed / env admin acceptable)
- Exact WorkOS AuthKit vs SSO API surface within the Rust SDK for the first vertical slice

### Deferred Ideas (OUT OF SCOPE)
- Email verification gate and password reset (Phase 5)
- Self-host admin bootstrap wizard / env-first admin creation polish (Phase 6) — Phase 4 may use a thinner admin seed path
- Consumer social OAuth as primary signup (out of v1 per PROJECT)
- Better Auth / Octane better-auth client as auth owner (rejected)
- Directory Sync / SCIM / advanced WorkOS enterprise features beyond AuthKit/SSO sign-in — backlog unless required for the first WorkOS slice
- SAML without OIDC bridge — prefer OIDC + WorkOS for Phase 4; raw SAML later if needed
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| AUTH-01 | User can sign up with email and password | Local provider: Argon2id hash, unique email+username, `auth.signup` RPC, welcome email |
| AUTH-02 | User can log in with email/password and stay logged in across refresh | `auth.login` + opaque session cookie + server `sessions` row; `auth.me` for chrome |
| AUTH-03 | User can log out from the web UI | `auth.logout` deletes current session + clears cookie; header account menu |
| AUTH-08 | User can view/edit own profile (display name, avatar, bio) | `user.get_profile` / `user.update_profile` + multipart avatar route + volume path |
| AUTH-09 | Unconfigured email → log/dev sink | Default `EmailSender = LogSink` when no provider env/settings |
| AUTH-10 | Operator can configure SMTP | `lettre` AsyncSmtpTransport + settings/ENV for host/port/creds/from |
| AUTH-11 | Operator can configure Resend | `reqwest` POST `https://api.resend.com/emails` with Bearer API key + User-Agent |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

`CLAUDE.md` is configured in `.planning/config.json` (`claude_md_path: "./CLAUDE.md"`) but **the file is not present** in the workspace as of research. Follow PROJECT.md / prior CONTEXT locks and existing crate conventions instead. [VERIFIED: workspace listing]

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Password hashing / credential verify | API / Backend | Database | Secrets never leave Rust; Argon2id in API; hash stored in DB |
| Session create / revoke / resolve | API / Backend | Database | Opaque cookie SID; rows in `sessions` for multi-device revoke |
| Auth provider adapter (local/workos/oidc) | API / Backend | — | Uniform facade; external IdP only for start/callback |
| WorkOS / OIDC redirects & callbacks | API / Backend | Browser | Browser follows redirects; API owns PKCE/state + code exchange |
| Cookie issuance (Set-Cookie) | API / Backend | Frontend Server (proxy) | API sets cookie; Vite/Traefik must preserve Set-Cookie on same site |
| CORS + credentials | API / Backend | Browser | Already prepared; browser must `credentials: "include"` |
| Auth UI `/login` `/signup` `/dashboard` | Browser / Client | Frontend Server (SSR) | Octane routes; mode-exclusive forms/CTAs |
| Profile edit + avatar upload | Browser + API | Database / Storage | JSON RPC for fields; multipart HTTP for file; volume for bytes |
| Email send | API / Backend | External (SMTP/Resend) | Trait in API; log sink is no-op network |
| Instance auth settings (admin) | API / Backend | Browser | Persisted settings + ENV bootstrap; admin UI reads/writes via RPC |
| Migration of users/sessions | Database / Storage | API | Per-dialect sqlx migrations via `oxidean-db` only |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `axum` | 0.8.x (repo: `0.8`) | HTTP/WS RPC host + auth callback/upload routes | Already in `oxidean-api` [VERIFIED: crates/oxidean-api/Cargo.toml] |
| `sqlx` | 0.8 | Multi-dialect persistence | Already in `oxidean-db` [VERIFIED: crates/oxidean-db/Cargo.toml] |
| `argon2` | **0.6.0** | Argon2id password hashing (PHC strings) | RustCrypto standard; `PasswordHasher`/`PasswordVerifier` [VERIFIED: crates.io 2026-08-27] |
| `password-hash` | **0.6.1** | PHC parse/verify traits (pulled with argon2) | Required companion API [VERIFIED: crates.io] |
| `workos` | **3.4.0** | Official WorkOS AuthKit/SSO + `authenticate_with_code` | Locked D-07; MSRV 1.88 [VERIFIED: crates.io + workos.com/docs/sdks/rust] |
| `openidconnect` | **4.0.1** | Generic OIDC auth-code + PKCE | De-facto Rust OIDC client [VERIFIED: crates.io 2025-07-06] |
| `lettre` | **0.11.23** | Async SMTP (`AsyncSmtpTransport` + `Tokio1Executor`) | Standard Rust mailer [VERIFIED: crates.io + docs.rs] |
| `reqwest` | **0.13.x** | Resend HTTP + OIDC HTTP (if not using openidconnect’s bundled client) | Align TLS with WorkOS (`rustls`) [VERIFIED: crates.io] |
| `cookie` / `tower-cookies` or `axum-extra` (cookie) | cookie **0.18.2** / tower-cookies **0.11.0** / axum-extra **0.12.6** | Parse/set HttpOnly cookies on RPC responses | Don’t hand-roll Set-Cookie formatting [VERIFIED: crates.io] |
| `uuid` | **1.26.0** | User/session IDs | Standard [VERIFIED: crates.io] |
| `rand` | **0.9/0.10** (pin what argon2/`OsRng` expects) | Session token entropy | CSPRNG for opaque SIDs [VERIFIED: crates.io] |
| `chrono` | **0.4.45** (already sqlx feature) | Expiry timestamps | Matches sqlx chrono feature [VERIFIED: crates/oxidean-db/Cargo.toml] |
| `image` | **0.25.10** | Optional avatar decode/resize | Safe bounds on uploads [VERIFIED: crates.io] |
| `sha2` | latest 0.10.x | Hash session tokens at rest | Store only hash of cookie value [ASSUMED: pin at plan time via `cargo search`] |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `thiserror` | **2.0.20** | Typed auth/email errors | Domain error mapping to `AppError` codes |
| `serde` / `serde_json` | workspace | RPC types | Existing |
| `tracing` | existing | Log-sink emails + auth audit | Existing |
| `tokio` | workspace | Async runtime | Existing |
| `multipart` via `axum` feature `multipart` | axum 0.8 | Avatar upload | Enable `axum` feature `multipart` [ASSUMED: axum 0.8 feature name] |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Custom `sessions` table + opaque cookie | `tower-sessions` + `tower-sessions-sqlx-store` | Official middleware, but dialect-specific pools + MessagePack blob fight `oxidean-db` D-08 and make “logout all” awkward; **reject for Phase 4** |
| Cookie-only encrypted sessions (`tower-sessions-cookie-store`) | — | Violates D-11 server-side store; cannot revoke all devices |
| Better Auth / Node session owner | — | Explicitly rejected D-10 |
| `lettre` FileTransport as “log sink” | tracing LogSink | FileTransport writes `.eml`; log sink is enough for AUTH-09 and simpler in Compose |
| Official Resend Rust SDK | raw `reqwest` | No first-party Resend Rust SDK in common use; HTTP API is stable and small [CITED: resend.com/docs/api-reference] |
| WorkOS sealed session cookies | Oxidean sessions | D-07: after WorkOS callback, mint **Oxidean** session — ignore WorkOS cookie helpers for app session |

**Installation (Cargo — add to workspace/`oxidean-api` + `oxidean-db` as appropriate):**

```bash
cargo add -p oxidean-api argon2 password-hash uuid rand chrono thiserror cookie tower-cookies sha2 image reqwest --features reqwest/rustls-tls
cargo add -p oxidean-api workos
cargo add -p oxidean-api openidconnect
cargo add -p oxidean-api lettre --features tokio1,tokio1-rustls,smtp-transport,builder,hostname
# Enable axum multipart in oxidean-api Cargo.toml: axum = { version = "0.8", features = ["ws", "multipart"] }
```

**Version verification:** crates.io queried 2026-09-10 for `workos@3.4.0`, `argon2@0.6.0`, `lettre@0.11.23`, `openidconnect@4.0.1`, `axum@0.8.9`, `tower-sessions-sqlx-store@0.15.0` (rejected). [VERIFIED: crates.io]

**Discretion locks (researcher recommendations — treat as plan defaults):**

| Item | Recommendation |
|------|----------------|
| Default session TTL | **24h** idle (`Expiry::OnInactivity`-style: refresh `expires_at` on authenticated RPC) |
| Remember-me TTL | **30d** from login (absolute max); still refresh last_seen |
| Cookie name | `oxidean_session` |
| Cookie flags | `HttpOnly; Path=/; SameSite=Lax; Secure` when `OXIDEAN_ENV` ∉ `{development,dev}` (Secure off only for local HTTP) |
| CSRF | **SameSite=Lax + same-site Traefik/Vite** is sufficient for Phase 4; no double-submit token yet |
| Session token storage | 32+ byte random → cookie plaintext; **SHA-256 hash** in DB |
| Reserved usernames | `admin`, `api`, `settings`, `login`, `signup`, `logout`, `status`, `dashboard`, `explore`, `orgs`, `org`, `help`, `support`, `www`, `root`, `system`, `null`, `undefined`, `oxidean`, `assets`, `static`, `uploads`, `health`, `rpc`, `auth`, `account`, `profile`, `admin`, `robots`, `favicon` |
| Avatar limits | ≤ **2 MiB**; `image/jpeg`, `image/png`, `image/webp`; resize longest edge to **512px**; store `var/uploads/avatars/{user_id}.{ext}` |
| `/dashboard` | Thin signed-in shell: greeting + links to Profile + Status; no forge widgets |
| `auth_identities` | See schema below |
| Admin UI path | `/admin/auth` |
| First admin in Phase 4 | If `OXIDEAN_ADMIN_EMAIL` + `OXIDEAN_ADMIN_PASSWORD` set at boot and no users exist, create `is_admin=true` user (thin preview of AUTH-06); document that full wizard is Phase 6 |
| WorkOS first slice | **AuthKit**: authorization URL with `provider=authkit` + PKCE; callback → `user_management().authenticate_with_code`; map WorkOS user id → `auth_identities` |

## Architecture Patterns

### System Architecture Diagram

```text
Browser (Octane web)
  │  credentials:include
  │  /login|/signup|/dashboard|/settings/profile|/admin/auth
  ▼
Vite proxy OR Traefik (same site)
  ├─ POST /api/rpc  ──► oxidean-api rpc_http
  │                       │ read Cookie → resolve session
  │                       │ dispatch auth.* / user.* / admin.auth.* / system.*
  │                       │ maybe Set-Cookie on login/logout
  │                       ▼
  │                    AuthFacade
  │                       ├─ LocalProvider (email|username + password)
  │                       ├─ WorkOsProvider (start URL + code exchange)
  │                       └─ OidcProvider (discover + PKCE + code exchange)
  │                       ▼
  │                    SessionService ──► sessions table (oxidean-db)
  │                    UserService    ──► users + auth_identities
  │                    EmailService   ──► LogSink | Smtp | Resend
  │
  ├─ GET  /api/auth/workos/start|/callback
  ├─ GET  /api/auth/oidc/start|/callback
  ├─ POST /api/user/avatar          (multipart)
  └─ GET  /uploads/avatars/*        (static volume; authz optional public URL with unguessable path OR auth-gated)

External: WorkOS API | OIDC IdP | SMTP host | api.resend.com
```

### Recommended Project Structure

```text
crates/oxidean-core/src/
  auth_types.rs          # UserPublic, SessionInfo, ProviderMode, AuthSettings DTOs (serde)
crates/oxidean-db/src/
  users.rs               # CRUD via DbPool match (only dialect branch here)
  sessions.rs
  auth_identities.rs
  auth_settings.rs
  migrations/{postgres,mysql,sqlite}/0002_auth.sql
crates/oxidean-api/src/
  auth/
    mod.rs               # AuthFacade
    local.rs
    workos.rs
    oidc.rs
    session.rs           # cookie + session service
    password.rs          # argon2 helpers
  email/
    mod.rs               # EmailSender trait
    log_sink.rs
    smtp.rs
    resend.rs
  routes/
    auth_callbacks.rs    # WorkOS/OIDC start+callback
    avatar.rs
  rpc.rs                 # extend dispatch + session-aware context
  app.rs                 # mount routes; CookieManagerLayer; AppState expansions
apps/web/src/routes/
  login.tsx
  signup.tsx
  dashboard.tsx
  settings/profile.tsx   # or /profile
  admin/auth.tsx
apps/web/src/components/
  chrome.tsx             # wire Sign in/up + account menu
packages/api-client/     # regenerated via rpc-gen
var/uploads/avatars/     # gitignored volume (Compose bind mount)
```

### Pattern 1: Session-aware RPC dispatch

**What:** Extend `rpc_http` to parse the session cookie, resolve `Option<AuthUser>`, pass into `dispatch`, and attach `Set-Cookie` when handlers request it.

**When to use:** All authenticated procedures (`auth.me`, `user.*`, `admin.*`, logout).

**Example:**

```rust
// Pattern for planners — adapt to existing RpcResponse / AppState
pub struct RpcCtx {
    pub db: Database,
    pub email: EmailHandle,
    pub auth_settings: AuthSettings,
    pub session: Option<ResolvedSession>,
    pub set_cookie: Option<CookieChange>, // set | clear
}

pub async fn dispatch(ctx: &mut RpcCtx, req: RpcRequest) -> RpcResponse {
    match req.procedure.as_str() {
        "auth.signup" => auth::signup(ctx, req.input).await,
        "auth.login" => auth::login(ctx, req.input).await,
        "auth.logout" => auth::logout(ctx).await,
        "auth.logout_all" => auth::logout_all(ctx).await,
        "auth.me" => auth::me(ctx).await,
        "auth.provider_config" => auth::public_provider_config(ctx).await, // mode + UI hints only
        "user.update_profile" => user::update_profile(ctx, req.input).await,
        "admin.auth.get_settings" => admin::get_settings(ctx).await,
        "admin.auth.update_settings" => admin::update_settings(ctx, req.input).await,
        // existing system.* ...
        other => RpcResponse::err(AppError::new("rpc.unknown_procedure", format!("unknown procedure: {other}"))),
    }
}
```

[ASSUMED: exact procedure names — planner may rename within `auth.*` / `user.*` / `admin.auth.*` namespaces]

### Pattern 2: Provider adapter trait

**What:** One facade; mode from merged ENV + DB settings.

```rust
#[async_trait]
trait AuthProvider: Send + Sync {
    fn mode(&self) -> ProviderMode; // local | workos | oidc
    async fn signup_local(&self, ...) -> Result<User, AuthError>; // local only
    async fn login_local(&self, ...) -> Result<User, AuthError>;  // local only
    fn start_external(&self, return_to: &str) -> Result<StartAuth, AuthError>; // workos|oidc
    async fn finish_external(&self, callback: CallbackParams) -> Result<ExternalIdentity, AuthError>;
}
```

After any successful identity resolution: `SessionService::create(user_id, remember_me)` → Set-Cookie.

### Pattern 3: Email trait + welcome on local signup only

```rust
#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send(&self, msg: OutboundEmail) -> Result<(), EmailError>;
}

pub struct OutboundEmail {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
}
```

- No provider configured → `LogSink` (`tracing::info!(target: "oxidean.mail", ...)`).
- SMTP configured → `lettre` async SMTP.
- Resend configured → HTTP JSON to Resend.
- **Only** `local` signup triggers welcome email in Phase 4 (D-20). WorkOS/OIDC users get no welcome here.

### Pattern 4: Multi-dialect migrations

**What:** Add `0002_auth.sql` (or next free number) to **all three** `migrations/{postgres,mysql,sqlite}/` with parity enforced by existing `migration_parity` test. [VERIFIED: crates/oxidean-db/src/migrate.rs]

Suggested tables (logical):

```sql
-- users
id UUID/TEXT PK
email CITEXT/VARCHAR UNIQUE NOT NULL
username VARCHAR(39) UNIQUE NOT NULL
password_hash TEXT NULL          -- NULL for SSO-only users
display_name TEXT NOT NULL
bio TEXT NOT NULL DEFAULT ''
avatar_path TEXT NULL
is_admin BOOLEAN NOT NULL DEFAULT FALSE
email_verified_at TIMESTAMPTZ NULL  -- column OK now; gate in Phase 5
created_at / updated_at

-- sessions
id UUID/TEXT PK
user_id FK NOT NULL
token_hash CHAR(64) UNIQUE NOT NULL  -- sha256 hex
expires_at TIMESTAMPTZ NOT NULL
remember_me BOOLEAN NOT NULL DEFAULT FALSE
created_at / last_seen_at
user_agent / ip optional

-- auth_identities
id PK
user_id FK NOT NULL
provider TEXT NOT NULL           -- 'workos' | 'oidc' | future
provider_subject TEXT NOT NULL   -- IdP sub
provider_email TEXT NULL
UNIQUE(provider, provider_subject)

-- instance_auth_settings (singleton row id=1)
provider_mode TEXT NOT NULL      -- local|workos|oidc
workos_client_id / workos_api_key_ref  -- prefer ENV for secrets; DB may store non-secret overrides
oidc_issuer / oidc_client_id / oidc_client_secret_ref
email_provider TEXT              -- log|smtp|resend
smtp_* / resend_* non-secret fields
from_address TEXT
updated_at
```

Dialect notes: MySQL lacks CITEXT — use `VARCHAR` + unique + app-normalized lowercase email. SQLite same. Postgres may use `CITEXT` or lowercase `TEXT` + unique index on `lower(email)`. Prefer **lowercase normalize in Rust** for portability. [ASSUMED: choose TEXT + app normalize for all three]

### Anti-Patterns to Avoid

- **Better Auth / Node as session owner** — rejected; forge identity is Rust.
- **JWT-as-only-session in localStorage** — conflicts with D-11; XSS-exposable.
- **Cookie-backed session blob without DB** — cannot logout-all (D-13).
- **Dialect branching in `oxidean-api`** — violates Phase 2 D-08; keep in `oxidean-db`.
- **Putting avatar bytes in JSON RPC** — use multipart route.
- **Implementing verify/reset email flows** — Phase 5 only.
- **Caching `/api/*` in SW** — already forbidden; keep bypass when adding `/api/auth/*` and `/api/user/avatar`.
- **Using WorkOS sealed cookies as Oxidean session** — mint local sessions after callback.
- **`tower-sessions-sqlx-store.migrate()` beside sqlx migrator** — dual migration ownership; avoid.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Password hashing | Custom scrypt/bcrypt wrappers | `argon2` + `password-hash` PHC | Timing-safe verify, param evolution |
| SMTP protocol | Raw TCP SMTP | `lettre` AsyncSmtpTransport | TLS, auth, pooling, MIME |
| Resend wire format | Ad-hoc without User-Agent | `reqwest` + documented headers | Missing UA → 403 [CITED: resend.com/docs/api-reference/introduction] |
| OIDC discovery/PKCE/ID token verify | Custom JWT parse | `openidconnect` 4.x | Nonce, `at_hash`, JWKS |
| WorkOS AuthKit HTTP | Unofficial `workos-rust` | Official `workos` 3.4 | Locked D-07; typed AuthKit helpers |
| Cookie header encoding | String concat | `cookie` / `tower-cookies` | Expires, SameSite, Secure edge cases |
| Image codecs | Manual JPEG parsing | `image` crate | Malformed upload safety |

**Key insight:** Hand-roll the **domain** (users/sessions/provider facade over `oxidean-db`); never hand-roll **crypto, mail protocols, or OIDC**.

## Common Pitfalls

### Pitfall 1: Cross-origin cookies between Vite `:3000` and API `:8080`

**What goes wrong:** Login appears to work in curl but browser never stores cookie / `auth.me` is anonymous after refresh.  
**Why:** Cookie set for API origin without proxy, or `Secure` on HTTP, or missing `credentials: "include"`.  
**How to avoid:** Keep Vite proxy for `/api` (already present); set cookie `Path=/` without Domain; `Secure=false` only in development; client already defaults credentials include. [VERIFIED: apps/web/vite.config.ts, packages/api-client]  
**Warning signs:** Set-Cookie in Network tab but Cookie not sent on next `/api/rpc`.

### Pitfall 2: Traefik / Compose CORS allowlist omits credentials origin

**What goes wrong:** Browser blocks credentialed RPC in Compose.  
**Why:** `OXIDEAN_CORS_ORIGINS` must list the public origin (`http://localhost`) and CORS already uses `allow_credentials(true)`. [VERIFIED: cors.rs, .env.example]  
**How to avoid:** Do not switch to `AllowOrigin::any()` with credentials (illegal); keep allowlist.

### Pitfall 3: Logout-all without indexed `user_id` on sessions

**What goes wrong:** Revoke-all scans MessagePack blobs or is impossible.  
**Why:** Cookie-session stores and opaque tower-sessions rows without user FK.  
**How to avoid:** First-class `sessions.user_id` FK + `DELETE FROM sessions WHERE user_id = ?`.

### Pitfall 4: Storing raw session token in DB

**What goes wrong:** DB leak = instant account takeover for all active sessions.  
**How to avoid:** Store SHA-256(token) only; constant-time compare on hash.

### Pitfall 5: WorkOS MSRV / edition surprise

**What goes wrong:** CI on older stable Rust fails compiling `workos` 3.4.  
**Why:** Official docs require Rust **1.88+** (edition 2024 crate). [CITED: workos.com/docs/sdks/rust]  
**How to avoid:** Pin toolchain ≥ 1.88 in CI/`rust-toolchain.toml` (local env already `1.100.0-nightly`). [VERIFIED: rustc --version]

### Pitfall 6: Resend 403 with valid API key

**What goes wrong:** Resend adapter “broken” in tests.  
**Why:** Missing `User-Agent` header → error 1010. [CITED: resend.com/docs/api-reference/introduction]  
**How to avoid:** Set `User-Agent: oxidean-api/0.1` (or package version) on every Resend request.

### Pitfall 7: rpc-gen drift

**What goes wrong:** UI calls procedures that TS client lacks.  
**Why:** `rpc_gen.rs` is a hand-maintained string template today — not specta reflection. [VERIFIED: crates/oxidean-api/src/bin/rpc_gen.rs]  
**How to avoid:** Every new procedure updates `rpc.rs` **and** `rpc_gen.rs` (+ CI sync check from Phase 1).

### Pitfall 8: Avatar path traversal / unbounded decode

**What goes wrong:** Upload writes outside volume or OOM on huge images.  
**How to avoid:** Cap body size (2 MiB), decode with `image`, rewrite to controlled path `{user_id}.webp`, never trust client filename.

### Pitfall 9: Mode-exclusive UI still posting local signup in `workos` mode

**What goes wrong:** Confusing errors / half-created users.  
**How to avoid:** `auth.provider_config` drives UI; server rejects `auth.signup`/`auth.login` when mode ≠ `local`.

### Pitfall 10: SW caches new auth routes

**What goes wrong:** Stale 401/200 for `/api/auth/*`.  
**How to avoid:** Existing `pathname.startsWith("/api/")` bypass covers new routes; do not narrow it. [VERIFIED: apps/web/public/sw.js]

## Code Examples

### Argon2id hash + verify

```rust
// Source: https://docs.rs/argon2 (Password Hashing example) [CITED: docs.rs/argon2]
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

fn hash_password(password: &[u8]) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default().hash_password(password, &salt)?.to_string())
}

fn verify_password(password: &[u8], password_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(password_hash) else { return false };
    Argon2::default().verify_password(password, &parsed).is_ok()
}
```

### Lettre async SMTP

```rust
// Source: https://docs.rs/lettre/latest/lettre/transport/smtp/struct.AsyncSmtpTransport.html [CITED]
use lettre::{message::header::ContentType, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

async fn send_smtp(smtp_url: &str, to: &str, subject: &str, body: &str) -> Result<(), Box<dyn std::error::Error>> {
    let email = Message::builder()
        .from("Oxidean <noreply@example.com>".parse()?)
        .to(to.parse()?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())?;
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::from_url(smtp_url)?.build();
    mailer.send(email).await?;
    Ok(())
}
```

### Resend HTTP

```rust
// Source: https://resend.com/docs/api-reference/emails [CITED]
async fn send_resend(api_key: &str, from: &str, to: &str, subject: &str, html: &str) -> reqwest::Result<()> {
    let client = reqwest::Client::new();
    let res = client
        .post("https://api.resend.com/emails")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("User-Agent", "oxidean-api/0.1")
        .json(&serde_json::json!({
            "from": from,
            "to": [to],
            "subject": subject,
            "html": html
        }))
        .send()
        .await?;
    res.error_for_status()?;
    Ok(())
}
```

### WorkOS authenticate with code (post-callback)

```rust
// Source: https://workos.com/docs/reference/authkit/authentication/code + docs.rs AuthenticateWithCode [CITED]
use workos::Client;
use workos::user_management::AuthenticateWithCodeParams;

async fn exchange(code: &str) -> Result<(), workos::Error> {
    let client = Client::builder()
        .api_key(std::env::var("WORKOS_API_KEY").unwrap())
        .client_id(std::env::var("WORKOS_CLIENT_ID").unwrap())
        .build();
    let _result = client
        .user_management()
        .authenticate_with_code(AuthenticateWithCodeParams {
            code: code.into(),
            ..Default::default()
        })
        .await?;
    Ok(())
}
```

### OIDC auth-code + PKCE (sketch)

```rust
// Source: openidconnect-rs async Authorization Code + PKCE example [CITED: context7 /ramosbugs/openidconnect-rs]
// Discover CoreProviderMetadata → CoreClient → PkceCodeChallenge::new_random_sha256()
// → authorize_url(...) → exchange_code(code).set_pkce_verifier(verifier).request_async(&http_client)
// → id_token.claims(&verifier, &nonce) → subject + email claims → link auth_identities
```

### Username validation (GitHub-like)

```rust
fn validate_username(raw: &str) -> Result<(), AppError> {
    let u = raw.trim();
    if u.len() < 1 || u.len() > 39 {
        return Err(AppError::new("auth.invalid_username", "username must be 1–39 characters"));
    }
    if u.starts_with('-') || u.ends_with('-') {
        return Err(AppError::new("auth.invalid_username", "username cannot start or end with a hyphen"));
    }
    if !u.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::new("auth.invalid_username", "username must be alphanumeric or hyphen"));
    }
    if RESERVED.contains(&u.to_ascii_lowercase().as_str()) {
        return Err(AppError::new("auth.reserved_username", "username is reserved"));
    }
    Ok(())
}
```

[ASSUMED: GitHub allows single-char usernames and hyphen rules as stated in D-03]

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Better Auth in Node for SaaS apps | Rust-native forge auth | Oxidean Phase 4 lock | Sessions owned by API |
| Cookie-only sessions | Opaque cookie + server table | ASVS / forge multi-device | Enables logout-all |
| Sync SMTP libs | `lettre` async tokio | lettre 0.11 | Fits Axum/tokio |
| Unofficial WorkOS crates | Official `workos` 3.x | 2025–2026 | AuthKit helpers from Rust |

**Deprecated/outdated:**
- Unofficial `workos-rust` 0.2.x — do not use (D-07).
- Storing passwords with MD5/SHA alone — use Argon2id PHC.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Exact RPC names `auth.*` / `user.*` / `admin.auth.*` | Architecture | Rename cost only |
| A2 | Prefer app-normalized lowercase email over Postgres CITEXT for dialect parity | Schema | Minor migration tweak |
| A3 | Axum 0.8 enables uploads via `multipart` feature | Stack | Use `axum-extra` multipart if feature name differs |
| A4 | SameSite=Lax without CSRF token is enough for Phase 4 same-site deploy | Sessions | May need synchronizer token if future cross-site UI |
| A5 | Early `OXIDEAN_ADMIN_*` seed is acceptable before Phase 6 wizard | Admin | Document clearly to avoid double-implementing wizard |
| A6 | `sha2` for token hashing is appropriate (vs HMAC with server key) | Sessions | If threat model requires keyed MAC, switch to HMAC-SHA256 |
| A7 | WorkOS AuthKit (`provider=authkit`) is the first vertical slice vs connection-scoped SSO only | WorkOS | Planner follows discretion; SSO-by-connection still supported later |

## Open Questions (RESOLVED)

1. **Should SSO-only users (WorkOS/OIDC) get a username at first login?** — **RESOLVED**
   - **Decision (plans 04-05 / 04-07):** Derive username from email local-part + collision suffix on first SSO login; if invalid/reserved, use a temp unique placeholder (e.g. `u{shortid}`) and set `profile_incomplete=true` so UI forces `/settings/profile` completion before treating the profile as done.

2. **Public vs auth-gated avatar URLs?** — **RESOLVED**
   - **Decision (plan 04-06):** Public hashed avatar URLs — serve `/uploads/avatars/{user_id}-{hash}.webp` (or equivalent) as publicly readable; no auth gate on avatar bytes for Phase 4.

3. **Where do SMTP/Resend secrets live — ENV only vs DB?** — **RESOLVED**
   - **Decision (plan 04-06):** ENV-only secrets for Phase 4 (`OXIDEAN_SMTP_URL`, `OXIDEAN_RESEND_API_KEY`, `WORKOS_API_KEY`, OIDC client secret); admin UI persists provider mode + non-secret fields only and surfaces “configured via ENV” for secrets.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust / cargo | API build | ✓ | rustc 1.100.0-nightly (≥1.88 for workos) | Pin stable ≥1.88 in CI |
| Docker | Compose smoke | ✓ | 28.4.0 | — |
| Bun | Web UI | ✓ | 1.4.0 | — |
| Node | tooling | ✓ | v24.5.0 | — |
| PostgreSQL (Compose) | default dialect | ✓ via compose | 16-alpine image | sqlite/mysql overlays |
| WorkOS account / API keys | live WorkOS E2E | ✗ (optional) | — | Unit-test provider with mock; manual E2E when keys present |
| Resend API key | live Resend E2E | ✗ (optional) | — | Log-sink + HTTP mock |
| SMTP server | live SMTP E2E | ✗ (optional) | — | Log-sink; Mailpit optional later |

**Missing dependencies with no fallback:** None for core Phase 4 (log-sink + local auth work offline).

**Missing dependencies with fallback:** WorkOS/Resend/SMTP live keys — mock/log-sink for automated tests.

## Validation Architecture

> `workflow.nyquist_validation` is **true** in `.planning/config.json`. [VERIFIED]

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust: `cargo test` (workspace); TS: Vitest `^5` in `@oxidean/api-client` |
| Config file | crates’ `[[test]]` / `packages/api-client/vitest.config.ts` |
| Quick run command | `cargo test -p oxidean-api --lib && cargo test -p oxidean-db --lib` |
| Full suite command | `make test` (= `cargo test --workspace`) + `bun run --filter @oxidean/api-client test` + dialect probe `make db-matrix` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AUTH-01 | Signup creates user + password hash + welcome via LogSink | integration | `cargo test -p oxidean-api --test auth_signup` | ❌ Wave 0 |
| AUTH-02 | Login sets cookie; subsequent RPC with cookie → `auth.me` | integration (http) | `cargo test -p oxidean-api --test auth_session` | ❌ Wave 0 |
| AUTH-03 | Logout clears cookie + deletes session row | integration | same `auth_session` | ❌ Wave 0 |
| AUTH-03+ | Logout-all deletes all sessions for user | unit/integration | `cargo test -p oxidean-db sessions::logout_all` | ❌ Wave 0 |
| AUTH-08 | Profile update + avatar multipart round-trip | integration | `cargo test -p oxidean-api --test profile_avatar` | ❌ Wave 0 |
| AUTH-09 | Default email path logs, no network | unit | `cargo test -p oxidean-api email::log_sink` | ❌ Wave 0 |
| AUTH-10 | SMTP adapter builds message / mock transport | unit | `cargo test -p oxidean-api email::smtp` | ❌ Wave 0 |
| AUTH-11 | Resend adapter JSON + User-Agent (wiremock) | unit | `cargo test -p oxidean-api email::resend` | ❌ Wave 0 |
| Multi-DB | migrations apply; signup works on pg/mysql/sqlite | integration | `cargo test -p oxidean-db --test dialect_auth` | ❌ Wave 0 |
| Providers | local rejected when mode=workos; oidc/workos start URL shape | unit | `cargo test -p oxidean-api auth::providers` | ❌ Wave 0 |
| UI smoke | `/login` `/signup` render (optional) | manual / later e2e | browser UAT | Phase verify-work |

### Sampling Rate

- **Per task commit:** `cargo test -p oxidean-api --lib` + targeted `--test` for touched area (<30s goal)
- **Per wave merge:** `make test` + `make db-matrix` (with DATABASE_URL) + api-client vitest if client changed
- **Phase gate:** Full suite green + Compose smoke signup/login on default Postgres + human UAT of success criteria

### Wave 0 Gaps

- [ ] `crates/oxidean-api/tests/auth_signup.rs` — AUTH-01 + welcome LogSink
- [ ] `crates/oxidean-api/tests/auth_session.rs` — AUTH-02/03 cookie jar via `tower`/`oneshot`
- [ ] `crates/oxidean-api/tests/profile_avatar.rs` — AUTH-08
- [ ] `crates/oxidean-api/src/email/*` unit tests — AUTH-09/10/11 (smtp/resend with mock)
- [ ] `crates/oxidean-db/migrations/*/0002_auth.sql` + extend `migration_parity`
- [ ] `crates/oxidean-db/tests/dialect_auth.rs` — signup/session on each dialect (or extend `dialect_probe`)
- [ ] Shared test helpers: cookie-aware RPC client in api tests
- [ ] Optional: `wiremock` or `httpmock` dev-dep for Resend/WorkOS HTTP

*(Existing `rpc_http` / `rpc_db_probe` tests are the template for new http oneshot tests.)*

## Security Domain

> `security_enforcement` enabled; ASVS level **1**. [VERIFIED: .planning/config.json]

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | Argon2id; lockout not required at L1 beyond basic rate-limit later; username enumeration: return generic login errors |
| V3 Session Management | yes | Opaque HttpOnly cookie; server revoke; idle + absolute TTLs; regenerate session id on login |
| V4 Access Control | yes | `auth.me` / profile / admin gated on session + `is_admin` |
| V5 Input Validation | yes | Username/email/password/bio length checks; multipart size/type allowlist |
| V6 Cryptography | yes | Argon2id; SHA-256 token hash; TLS for SMTP/Resend/WorkOS/OIDC |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Password DB leak | Information Disclosure | Argon2id PHC; never log passwords |
| Session fixation | Elevation | New session id on privilege login; delete old anonymous session |
| XSS steals session | Elevation | HttpOnly cookie; CSP later; no token in JS |
| CSRF on state-changing RPC | Elevation | SameSite=Lax + same-site deploy; credentials only to allowlisted origins |
| Avatar path traversal | Tampering | Ignore filename; server-chosen path |
| SSRF via OIDC issuer URL | Spoofing | Admin-only settings; allowlist https issuers; no link-local fetch if feasible [ASSUMED] |
| Email header injection | Tampering | lettre typed addresses; validate To/From |
| Admin settings open | Elevation | Require `is_admin`; seed admin carefully |

## Sources

### Primary (HIGH confidence)

- Workspace: `crates/oxidean-api` (rpc, cors, app), `oxidean-db` (pool, migrate), `packages/api-client`, `apps/web` (chrome, vite proxy, sw.js), `docker-compose.yml`, `.env.example`
- CONTEXT: `.planning/phases/04-auth-sessions-email/04-CONTEXT.md`
- crates.io versions queried 2026-09-10: workos, argon2, lettre, openidconnect, axum, tower-sessions-sqlx-store
- https://workos.com/docs/sdks/rust — WorkOS Rust SDK install, MSRV 1.88, AuthKit helpers
- https://docs.rs/workos/latest/workos/ — Client / user_management
- https://docs.rs/lettre/latest/lettre/ — AsyncSmtpTransport
- https://docs.rs/argon2 — password hashing examples
- https://docs.rs/tower-sessions/latest/tower_sessions/ — session model (evaluated, custom store preferred)
- https://resend.com/docs/api-reference/introduction — Bearer auth + User-Agent requirement
- https://resend.com/docs/api-reference/emails — send payload
- Context7 CLI: `/websites/workos`, `/websites/rs_lettre`, `/websites/rs_argon2`, `/ramosbugs/openidconnect-rs`

### Secondary (MEDIUM confidence)

- WebSearch synthesis on WorkOS get-authorization-url / AuthKit provider parameter
- crates.io README for `tower-sessions-sqlx-store` (rejected integration path)

### Tertiary (LOW confidence)

- Exact GitHub reserved-username full list parity — use curated list above
- SSRF hardening depth for OIDC discovery in Phase 4

## Metadata

**Confidence breakdown:**
- Standard stack: **HIGH** — versions verified on crates.io + official docs
- Architecture: **HIGH** — maps cleanly onto existing RPC/db/cors/client patterns; session approach deliberately avoids known multi-DB footgun
- Pitfalls: **HIGH** — cookie proxy, Resend UA, workos MSRV, rpc-gen drift verified in-repo or docs

**Research date:** 2026-09-10  
**Valid until:** 2026-10-10 (30 days; re-check `workos` MSRV/API if planning slips)

---

*Phase: 4-auth-sessions-email*  
*Researcher: gsd-phase-researcher*
