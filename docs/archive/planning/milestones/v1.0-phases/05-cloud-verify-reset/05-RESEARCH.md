# Phase 5: Cloud Verify & Reset - Research

**Researched:** 2026-09-10
**Domain:** Email verification + password reset (OTP + magic link), require-verified gate, auth.me email_verified, input-otp UI
**Confidence:** HIGH

## Summary

Phase 5 extends the Phase 4 Rust-native auth stack with **email verification** and **password reset** that share one channel model (magic link + 8-digit OTP in one email, 30-minute TTL, resend replaces prior issuance, soft rate limits). Verification does **not** block login; a reusable **`require_verified`** helper denies privileged RPCs with stable code **`auth.email_unverified`**. Phase 5 ships that helper plus a **dev/test-only privileged RPC** so CI can prove the gate before Phase 7’s real `repo.create`. `UserPublic` / `auth.me` gain **`email_verified: bool`** for the chrome banner and disabled CTAs. Open signup (AUTH-05) is retained as-is — no invite system.

Existing seams to extend: `EmailSender` + log/SMTP/Resend; `users.email_verified_at` (already migrated, never wired); session SHA-256-at-rest pattern; `logout_all` / session mint for reset success; WorkOS `User.email_verified` and OIDC `email_verified` claims for IdP-trust; `OXIDEAN_PUBLIC_ORIGIN` for link URLs; Octane auth routes + `AuthShell`.

**Primary recommendation:** Add dialect-parity `auth_email_tokens` (hash-at-rest magic + OTP), verify/reset RPC + templates on existing `EmailSender`, `require_verified` + `auth.dev.privileged_ping`, expose `email_verified` on `UserPublic`, and ship `/verify` + `/reset-password` with local `input-otp` wrapper — do not reopen Phase 6/7 or Better Auth.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### A — Gate policy
- **D-01:** No cloud/self-host detection for verify — **same policy everywhere**
- **D-02:** Verify gate **always** applies (including log-sink / Mailpit)
- **D-03:** Local password accounts **must** verify; WorkOS/OIDC **IdP-trust** → mark verified on SSO success when the IdP asserts verified email
- **D-04:** Env/wizard seeded admin is **auto-verified**
- **D-05:** Email change **clears** verified and sends a **new** verify email
- **D-06:** Unverified users **may log in**; only privileged actions are gated
- **D-07:** Privileged API denial uses stable code **`auth.email_unverified`** (+ short message) — **Reversibility:** costly — clients and e2e will key off this code
- **D-08:** AUTH-05 — signup stays **open / no invite** (Phase 4 behavior retained; no invite system in Phase 5)

#### B — Privileged-action gate
- **D-09:** Phase 5 ships a **require-verified** helper and marks future **`repo.create`** as the first product consumer
- **D-10:** Ship a **dev/test-only privileged RPC** so CI can prove the gate without Phase 7 repos
- **D-11:** **Persistent verify banner** on signed-in chrome with **resend**
- **D-12:** Future privileged CTAs (e.g. create repo): **visible but disabled** until verified
- **D-13:** **`auth.me` exposes `email_verified` boolean** for banner and disabled CTAs — **Reversibility:** costly — public RPC contract

#### C — Verification experience
- **D-14:** **Magic link and one-time code** both accepted
- **D-15:** SSO: IdP-trust when verified email asserted; otherwise same link+code flows; **reuse** one verify implementation for local and SSO edge cases
- **D-16:** Single **`/verify`** page — token query **or** code form; success → **`/dashboard`** or **`returnTo`**
- **D-17:** Code entry via **[`input-otp`](https://www.npmjs.com/package/input-otp)**; integrate through **Octane React-compat** as needed (ShadCN-style slots OK)
- **D-18:** Magic link and OTP share the **same 30-minute** TTL
- **D-19:** Resend **invalidates/replaces** prior issuance; soft rate limit **~1/min** and **~5/hour**
- **D-20:** **8-digit** numeric OTP, single-use; magic-link token is a **longer separate secret** (same TTL; replaced together on resend)
- **D-21:** Consuming verify requires a **signed-in session as the target user**; logged-out magic link lands on `/verify` → sign-in → complete
- **D-22:** **One email** contains both the magic link and the 8-digit code
- **D-23:** Auto-send verify email on **local signup**; resend from banner and `/verify`

#### D — Password reset
- **D-24:** Same channel model as verify: magic link + 8-digit code in one email; **`/reset-password`** accepts either; **`input-otp`** for code (Octane React-compat)
- **D-25:** Reuse verify TTL/rate limits: **30 min** shared; resend replaces; **~1/min** and **~5/hour**
- **D-26:** **Local-password accounts only**; SSO-only users get no reset (point to IdP)
- **D-27:** Reset flow is **logged-out**; success sets password, **revokes other sessions**, **signs in** on this device
- **D-28:** Forgot-password request **always** returns the same success copy (**anti-enumeration**) — never reveal whether the email matched

### the agent's Discretion
- Exact magic-link token length/encoding and storage (hash-at-rest vs opaque id)
- Exact OTP auto-submit vs explicit submit button after `input-otp` completes
- Exact anti-enumeration and email template copy wording
- Password strength rules on reset (prefer matching Phase 4 signup rules)
- Token table / column naming and cleanup job cadence
- Whether “email provider configured” for AUTH-12 includes log-sink (likely yes for local/dev parity with D-02) vs requiring SMTP/Resend only in production docs

### Deferred Ideas (OUT OF SCOPE)
- Real **`repo.create`** privileged enforcement + enabled CTA — Phase 7 (Phase 5 only helper + test RPC + disabled CTA pattern)
- Self-host **admin bootstrap** polish — Phase 6 (AUTH-06/07); seeded admin auto-verify still applies when that admin exists
- None other — discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| AUTH-04 | On Oxidean Cloud, user must verify email before privileged actions (at minimum: create repository) | Same-everywhere verify gate (D-01/D-02); `require_verified` + `auth.email_unverified`; test RPC until Phase 7 `repo.create`; IdP-trust + local verify flows |
| AUTH-05 | On Oxidean Cloud, signup is open (no invite required) | Confirm/retain Phase 4 open `auth.signup`; no invite schema/UI |
| AUTH-12 | User can reset password via email link when an email provider is configured | Reset request/redeem RPCs; magic+OTP email via existing `EmailSender` (log-sink counts); anti-enumeration; local-password only |
</phase_requirements>

## Project Constraints (from AGENTS.md / CLAUDE.md)

`CLAUDE.md` is configured in `.planning/config.json` (`claude_md_path: "./CLAUDE.md"`) but **the file is not present**. `AGENTS.md` is also absent. Follow PROJECT.md / CONTEXT locks, Phase 4 patterns, and `.agents/skills` (rust-best-practices, tdd, frontend-design) instead. [VERIFIED: workspace listing this session]

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Issue / store / consume verify+reset secrets | API / Backend | Database | Secrets never leave Rust; hashes in DB |
| `require_verified` gate + test privileged RPC | API / Backend | — | Authorization at RPC boundary |
| `email_verified` on `auth.me` / `UserPublic` | API / Backend | Browser | Contract for banner/CTAs |
| IdP-trust mark verified (WorkOS/OIDC) | API / Backend | — | Claims only trustworthy after SSO callback |
| Auto-verify seeded admin | API / Backend | — | Boot seed path in `main` |
| Outbound verify/reset mail | API / Backend | External (SMTP/Resend) or LogSink | Reuse `EmailSender` |
| Magic-link absolute URLs | API / Backend | Config | `OXIDEAN_PUBLIC_ORIGIN` — never trust Host alone |
| `/verify`, `/reset-password`, banner, forgot link | Browser / Client | Frontend Server | Octane routes + chrome |
| OTP UI (`input-otp`) | Browser / Client | — | npm widget; API owns validation |
| Dialect migrations for tokens + verified helpers | Database / Storage | API | Parity across sqlite/postgres/mysql |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Existing `oxidean-api` auth + `EmailSender` | in-repo | Verify/reset send + consume | Phase 4 already owns adapters [VERIFIED: crates/oxidean-api/src/email/mod.rs:36-39] |
| Existing `sha2` / `rand` | sha2 **0.11.0**, rand **0.10.2** | Hash tokens / CSPRNG OTP+magic | Same as sessions [VERIFIED: crates/oxidean-api/Cargo.toml] |
| Existing `argon2` | 0.6.x | Reset password hashing | Match signup `MIN_PASSWORD_LEN = 8` [VERIFIED: crates/oxidean-api/src/auth/password.rs:13] |
| `input-otp` | **1.5.0** | 8-slot OTP UI | Locked D-17; React 16.8–19 peer; zero deps [VERIFIED: npm view 1.5.0; npmjs.com/package/input-otp] |
| Existing Octane / ShadCN Base UI | in-repo | AuthShell, Button, Input, chrome | UI-SPEC extends Phase 4 — no new shadcn CLI blocks |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| Existing sqlx migrations (3 dialects) | in-repo | `0003_*` token table + helpers | Must ship postgres/mysql/sqlite together |
| Existing `vitest` / `cargo test` | in-repo | Unit/integration for gate + tokens | Wave 0 tests below |
| `OXIDEAN_PUBLIC_ORIGIN` | env | Absolute magic-link base | Already documented for SSO [VERIFIED: docs/CONFIGURATION.md:20] |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|----------|----------|
| Hash-at-rest tokens (recommended) | Opaque DB id in URL only | Weaker if DB leak + id enumerable; reject |
| Separate verify vs reset tables | One `auth_email_tokens` with `purpose` | One table simpler; **use purpose column** |
| Argon2 for 8-digit OTP | SHA-256 / HMAC | OTP keyspace small — rate limits matter more than KDF cost [CITED: OWASP MFA cheat sheet] |
| Better Auth / Node mail flows | — | Rejected Phase 4 |
| shadcn registry `input-otp` block | Direct npm `input-otp` | UI-SPEC forbids third-party registry OTP |

**Installation:**

```bash
# apps/web
bun add input-otp@1.5.0
# Rust: no new crates required (sha2/rand/argon2 already present)
```

**Version verification:** `npm view input-otp version` → `1.5.0` (2026-09-10). Created 2024-02-19; ~21M weekly downloads; MIT; repo `guilhermerodz/input-otp`. [VERIFIED: npm registry]

**Discretion locks (plan defaults):**

| Item | Recommendation |
|------|----------------|
| Magic-link token | **32-byte** CSPRNG → **lowercase hex** (64 chars), same as session cookie entropy |
| Magic-link storage | **SHA-256 hex** of raw token in DB (mirror `SessionService`) |
| OTP | **8 numeric digits**, CSPRNG rejection sampling; store **SHA-256 hex** of UTF-8 digits (hygiene); **never log** plaintext |
| OTP + magic on resend | Single row replace: new magic hash + new OTP hash + new `expires_at`; delete/replace prior for `(user_id, purpose)` |
| Table | `auth_email_tokens` — see schema below |
| TTL | **30 minutes** absolute from issuance (D-18/D-25) |
| Rate limit | Soft: **≥60s** between issues per `(user_id, purpose)` **and** **≤5** issues per rolling **hour**; return stable `auth.rate_limited` |
| Redeem attempts | Cap **10** failed OTP/token attempts per issuance then invalidate row |
| Cleanup | Lazy delete on lookup + optional delete-expired in redeem/issue paths; no separate cron required in Phase 5 |
| AUTH-12 “provider configured” | **Log-sink counts** (parity D-02 / UI-SPEC); production docs can still recommend SMTP/Resend |
| Reset password rules | **`MIN_PASSWORD_LEN = 8`**; confirm field client-side (UI-SPEC) |
| OTP UX | **`onComplete` auto-submit** + keep primary button (UI-SPEC lock) |
| Anti-enumeration copy | Exact UI-SPEC strings |
| Test privileged RPC | `auth.dev.privileged_ping` — registered only when `OXIDEAN_ENV` ∈ `{development,dev,test}` **or** `cfg(test)`; else `rpc.unknown_procedure` |
| HTTP status for `auth.email_unverified` | **403 Forbidden** (extend `rpc_status` like `admin.forbidden`) |
| Magic URLs | `{OXIDEAN_PUBLIC_ORIGIN}/verify?token=…` and `…/reset-password?token=…` — strip trailing slash; fallback derivation same as SSO if unset |
| Reserved usernames | Add `verify`, `reset-password` to reserved list |
| Admin seed | Set `email_verified_at = now` on `maybe_seed_admin` create (D-04) |
| Email change (D-05) | No email-edit API yet — ship `clear_email_verification(user_id)` helper used when/if email update is added; document for planner |
| Post-reset session | Honor **D-27** auto sign-in (intentional override of OWASP “don’t auto-login after reset”) |

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `input-otp` | npm | first publish 2024-02-19; latest 1.5.0 @ 2026-08-18 | ~21M/wk | github.com/guilhermerodz/input-otp | seam **SUS** (`too-new` on latest publish) | **Keep — CONTEXT/UI-SPEC lock**; treat as Approved with human confirm of version pin. No postinstall. |

**Packages removed due to [SLOP] verdict:** none  
**Packages flagged as suspicious [SUS]:** `input-otp` — planner should add a short `checkpoint:human-verify` to confirm installing `input-otp@1.5.0` (seam false-positive: package is mature + high download; locked by D-17).

## Architecture Patterns

### System Architecture Diagram

```text
Browser (Octane)
  │ credentials:include
  ├─ /verify (?token=) /reset-password /login(forgot) /dashboard CTA
  ├─ chrome VerifyBanner ← auth.me.email_verified
  ▼
Vite/Traefik → POST /api/rpc
  │
  ├─ auth.request_verify / auth.verify / auth.resend_verify
  ├─ auth.request_password_reset / auth.reset_password
  ├─ auth.dev.privileged_ping  (env-gated)
  ├─ auth.me → UserPublic{ email_verified }
  │
  ▼
VerifyResetService
  ├─ issue(purpose) → CSPRNG magic+OTP → hash → auth_email_tokens
  ├─ rate_limit + replace prior issuance
  ├─ EmailSender.send(verify|reset template)  // LogSink|SMTP|Resend
  └─ consume(token|otp) → set users.email_verified_at | set password + revoke_all + mint session
  │
  ├─ require_verified(user) → Ok | Err auth.email_unverified
  │
SSO callback (WorkOS/OIDC)
  └─ if IdP asserts email_verified → set email_verified_at
```

### Recommended Project Structure

```
crates/oxidean-db/
  migrations/{postgres,mysql,sqlite}/0003_email_tokens.sql
  src/email_tokens.rs          # CRUD for auth_email_tokens
  src/users.rs                 # set/clear email_verified_at
crates/oxidean-api/src/auth/
  verify_reset.rs              # issue/consume/rate-limit
  gate.rs                      # require_verified
  local.rs                     # signup auto-send verify; me email_verified
  external.rs                  # ExternalIdentity.email_verified + apply on link
  workos.rs / oidc.rs          # pass email_verified claim
crates/oxidean-core/src/auth_types.rs  # UserPublic.email_verified
apps/web/src/
  components/ui/input-otp.tsx  # thin wrapper around input-otp
  components/verify-banner.tsx
  routes/verify.tsx
  routes/reset-password.tsx
  routes/login.tsx             # forgot link (local only)
  routes/__root.tsx / chrome   # banner host
  routes/dashboard.tsx         # disabled New repository CTA
packages/api-client            # regenerated via rpc-gen
```

### Pattern 1: Hash-at-rest dual-channel issuance
**What:** One DB row holds both magic-token hash and OTP hash; email contains both plaintext secrets once.  
**When to use:** Every verify and reset issue/resend.  
**Example:**

```rust
// Mirror SessionService: raw hex in email/URL; SHA-256 hex in DB.
let magic_raw = random_hex_32();
let otp_raw = random_otp_8(); // "01234567".."99999999" via CSPRNG
db.upsert_email_token(user_id, purpose, &sha256_hex(magic_raw.as_bytes()), &sha256_hex(otp_raw.as_bytes()), expires_at).await?;
```

### Pattern 2: `require_verified` helper
**What:** Central gate used by privileged RPCs (test ping now; `repo.create` later).  
**When to use:** Any mutating privileged action.  
**Example:**

```rust
pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> {
    let session = ctx.session.as_ref().ok_or_else(|| {
        AppError::new("auth.unauthenticated", "not authenticated")
    })?;
    let user = ctx.db.find_user_by_id(&session.user_id).await?...;
    if user.email_verified_at.is_none() {
        return Err(AppError::new(
            "auth.email_unverified",
            "verify your email to continue",
        ));
    }
    Ok(user)
}
```

`email_verified_at` field already exists on `UserRow`:

```17:17:crates/oxidean-db/src/users.rs
    pub email_verified_at: Option<String>,
```

[VERIFIED: crates/oxidean-db/src/users.rs:17]

### Pattern 3: Verify consume requires matching session (D-21)
**What:** Redeem verify only if `ctx.session.user_id == token.user_id`. Anonymous with `?token=` → UI prompts login with `returnTo=/verify?token=…`.  
**When to use:** `auth.verify` only (reset is logged-out per D-27).

### Pattern 4: Anti-enumeration reset request (D-28)
**What:** Always return identical success payload after normalize email; only send mail when local-password user exists; avoid early-return timing skew (do comparable work / async send).  
**When to use:** `auth.request_password_reset`.

### Pattern 5: IdP-trust (D-03 / D-15)
**What:** Extend `ExternalIdentity` with `email_verified: bool`; on WorkOS use `user.email_verified`; on OIDC use `claims.email_verified() == Some(true)`; if true, `set_email_verified_at` after link/create. If false/absent, leave unverified and reuse local verify email flows.  
**WorkOS User field (verbatim):**

```28:30:/home/jesse/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/workos-3.4.0/src/models/user.rs
    pub email: String,
    /// Whether the user's email has been verified.
    pub email_verified: bool,
```

[VERIFIED: workos-3.4.0 User model]  
OIDC: `set_email_verified -> email_verified[Option<bool>]` on ID token claims [VERIFIED: openidconnect-4.0.1 id_token/mod.rs:317].

### Anti-Patterns to Avoid
- **Blocking login until verified:** Violates D-06.
- **Cloud-only gate:** Violates D-01/D-02.
- **Storing plaintext OTP/token in DB or logs:** OWASP secret handling.
- **Building magic URLs from request `Host`:** Host-header injection [CITED: OWASP Forgot Password Cheat Sheet].
- **Revealing “email not found” on reset:** Violates D-28.
- **Implementing real `repo.create`:** Phase 7 deferred.
- **Installing shadcn third-party OTP registry block:** UI-SPEC forbids.
- **Separate verify implementation for SSO vs local:** Violates D-15 reuse.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| OTP slot focus/paste/SMS autofill | Six `<input>`s + key handlers | `input-otp` `OTPInput` | Autofill/`one-time-code` needs one real field [CITED: input-otp.rodz.dev] |
| Cookie/session crypto for reset login | Custom cookie format | Existing `SessionService` | Already SHA-256 + flags |
| Password hash on reset | New KDF | `hash_password_str` | Same Argon2id + min length |
| Mail transport | New HTTP client | `EmailSender` trait | Log/SMTP/Resend already hot-swappable |
| Dialect SQL branching in API | Raw sqlx in API | `oxidean-db` helpers | Phase 2 boundary |

**Key insight:** Phase 5 is mostly **policy + token table + thin RPC/UI** on top of Phase 4 primitives — resist new frameworks.

## Common Pitfalls

### Pitfall 1: Verify consume without session match
**What goes wrong:** Attacker with stolen magic link verifies while logged in as someone else, or verifies without proving mailbox ownership on that device.  
**Why:** Ignoring D-21.  
**How to avoid:** Reject with wrong-user / unauthenticated; UI forces login with token preserved in `returnTo`.  
**Warning signs:** Integration test missing “wrong session user” case.

### Pitfall 2: OTP brute-force / email flood
**What goes wrong:** 8-digit space + unlimited attempts or resends.  
**Why:** Small OTP keyspace [CITED: OWASP MFA].  
**How to avoid:** Soft issue limits (1/min, 5/hour), attempt cap, single-use, replace-on-resend.  
**Warning signs:** Missing `auth.rate_limited` tests.

### Pitfall 3: Reset enumeration via timing or SSO errors
**What goes wrong:** Different latency or messages for unknown email vs SSO-only.  
**Why:** Branchy early returns.  
**How to avoid:** Identical success for request; SSO-only failure only on **redeem** with stable copy (UI-SPEC).  
**Warning signs:** Asserting different response bodies in request tests.

### Pitfall 4: Dialect migration drift
**What goes wrong:** Tokens work on SQLite CI only.  
**Why:** Forgetting mysql/postgres `0003` parity (Phase 2 PLAT-08).  
**How to avoid:** Three migration files + extend `dialect_auth` (or new dialect test) for token CRUD + verified flag.  
**Warning signs:** Migration numbering mismatch across dialects.

### Pitfall 5: `UserPublic` without regenerating client
**What goes wrong:** Web banner never sees `email_verified`.  
**Why:** Hand-edit only one of core/rpc_gen/api-client.  
**How to avoid:** Change `oxidean-core` DTO → `rpc-gen` → web.  
**Warning signs:** TS type missing field after gen.

### Pitfall 6: Auto-login after reset vs OWASP guidance
**What goes wrong:** Planner “fixes” D-27 to force re-login.  
**Why:** OWASP Forgot Password prefers manual re-login.  
**How to avoid:** Honor **D-27** (sign in + revoke others); optional notify email later.  
**Warning signs:** Success path without `Set-Cookie`.

### Pitfall 7: Admin seed remains unverified
**What goes wrong:** Seeded admin blocked from privileged actions.  
**Why:** `create_user` never sets `email_verified_at` today.  
**How to avoid:** After insert in `maybe_seed_admin`, set verified (D-04).  
**Warning signs:** Fresh Compose admin fails `privileged_ping`.

## Code Examples

### input-otp auto-submit (8 digits)

```tsx
// Source: https://input-otp.rodz.dev/docs/forms — adapt maxLength={8}
import { OTPInput } from "input-otp";

<OTPInput
  maxLength={8}
  inputMode="numeric"
  autoComplete="one-time-code"
  pattern="^[0-9]+$"
  onComplete={(code) => { void submitVerify(code); }}
  disabled={pending}
  render={({ slots }) => (
    <div className="flex gap-1">{/* Slot UI — Octane/Tailwind */}</div>
  )}
/>
```

[CITED: input-otp.rodz.dev/docs/forms]

### `UserPublic` extension (D-13)

Current fields (no `email_verified` yet):

```25:36:crates/oxidean-core/src/auth_types.rs
pub struct UserPublic {
    pub id: String,
    pub email: String,
    pub username: String,
    pub display_name: String,
    pub bio: String,
    /// Public URL path (e.g. `/uploads/avatars/{id}.webp`), not a filesystem path.
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    /// True when username needs completion (e.g. after SSO with placeholder handle).
    pub profile_incomplete: bool,
}
```

[VERIFIED: crates/oxidean-core/src/auth_types.rs:25-36]

Add: `pub email_verified: bool` derived as `row.email_verified_at.is_some()` in `user_to_public`.

### Suggested `auth_email_tokens` schema (logical)

```sql
-- Ship as 0003_email_tokens.sql on postgres / mysql / sqlite (type dialects differ)
CREATE TABLE auth_email_tokens (
  id            TEXT PRIMARY KEY,
  user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  purpose       TEXT NOT NULL,           -- 'verify' | 'reset'
  token_hash    CHAR(64) NOT NULL,       -- magic link
  otp_hash      CHAR(64) NOT NULL,       -- 8-digit
  expires_at    /* dialect timestamp */ NOT NULL,
  attempt_count INTEGER NOT NULL DEFAULT 0,
  created_at    /* dialect timestamp */ NOT NULL,
  UNIQUE (user_id, purpose)
);
CREATE INDEX idx_auth_email_tokens_token_hash ON auth_email_tokens(token_hash);
```

### RPC surface (planner checklist)

| Procedure | Auth | Notes |
|-----------|------|-------|
| `auth.request_verify` / `auth.resend_verify` | Signed-in | Issue+send; rate limit |
| `auth.verify` | Signed-in as target | `{ token? , code? }` |
| `auth.request_password_reset` | Anonymous | Anti-enumeration always-ok |
| `auth.reset_password` | Anonymous | `{ token? , code? , password }` → set hash, revoke_all, mint session cookie |
| `auth.dev.privileged_ping` | Signed-in + env gate | Calls `require_verified` |
| `auth.me` | Signed-in | Include `email_verified` |

Signup: after create, auto-issue verify email (D-23); keep existing welcome send (two messages OK).

### HTTP status mapping

Today:

```156:169:crates/oxidean-api/src/app.rs
fn rpc_status(resp: &RpcResponse) -> StatusCode {
    match resp {
        RpcResponse::Ok { .. } => StatusCode::OK,
        RpcResponse::Err { error, .. } if error.code == "rpc.unknown_procedure" => {
            StatusCode::NOT_FOUND
        }
        RpcResponse::Err { error, .. } if error.code == "auth.unauthenticated" => {
            StatusCode::UNAUTHORIZED
        }
        RpcResponse::Err { error, .. } if error.code == "admin.forbidden" => {
            StatusCode::FORBIDDEN
        }
        RpcResponse::Err { .. } => StatusCode::BAD_REQUEST,
    }
}
```

[VERIFIED: crates/oxidean-api/src/app.rs:156-169]

Add branch: `auth.email_unverified` → `FORBIDDEN`.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Activate account before email proof | Gate privileged actions only; login allowed | Product D-06 | Forge usable for browse later; create gated |
| Reset → force re-login | Reset → revoke others + mint session (D-27) | Locked | Slightly vs OWASP default; simpler UX |
| Separate SMS OTP vs email link | Dual channel one email | D-14/D-22 | One template, one issuance row |
| Cloud-only verify | Same policy everywhere | D-01 | Simpler ops; self-host also gated |

**Deprecated/outdated:**
- Plaintext token columns in app DB
- Relying on Host header for email link origin

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Soft rate limits implemented via DB timestamps/counts (not Redis) suffice for single-process Compose v1 | Discretion | Multi-instance may need shared limiter later |
| A2 | Combining SHA-256 OTP hash + attempt caps is adequate without Argon2/HMAC pepper for Phase 5 | Token storage | DB dump + online guessing still mitigated by TTL/rate limits; pepper optional later |
| A3 | WorkOS stub fixtures may need `email_verified: true` for IdP-trust e2e | IdP-trust | Stub SSO users stay unverified until verify email |

## Open Questions (RESOLVED)

1. **Notify email after successful password reset?**
   - What we know: OWASP recommends notification; D-27 silent on it; UI-SPEC does not require.
   - What's unclear: Product desire for “your password was changed” mail.
   - Recommendation: Optional stretch — not required for AUTH-12; skip unless planner has spare capacity.
   - RESOLVED: Skip notify-after-reset mail in Phase 5. AUTH-12 is satisfied by request/redeem only; D-27 and UI-SPEC do not require a “password was changed” message. Reset plan does not send post-reset notification.

2. **Exact env allowlist for `auth.dev.privileged_ping` in Compose `OXIDEAN_ENV=compose`?**
   - What we know: D-10 wants CI proof; Compose often uses `compose`.
   - What's unclear: Whether ping should exist in Compose e2e.
   - Recommendation: Allow `{development,dev,test,compose}`; never `production`.
   - RESOLVED: Register `auth.dev.privileged_ping` when `OXIDEAN_ENV` ∈ `{development,dev,test,compose}` or `cfg(test)`; never in `production`. Locked in the tracer plan (gate + privileged_ping).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust/cargo | API + migrations | ✓ | rustc 1.100 nightly / cargo 1.100 | — |
| Bun/Node | web + input-otp | ✓ | bun 1.4.0 / node v24.5 | — |
| Docker | Mailpit e2e | ✓ | 28.4.0 | LogSink unit tests without Mailpit |
| PostgreSQL/SQLite/MySQL | Dialect parity | via Compose/CI | existing Phase 2 matrix | — |
| `input-otp` npm | OTP UI | installable | 1.5.0 | — |

**Missing dependencies with no fallback:** none  
**Missing dependencies with fallback:** live SMTP/Resend — LogSink + Mailpit stubs cover AUTH-12 locally

Step 2.6: External tools present; phase is primarily code + existing mail stack.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust `cargo test` / nextest + Vitest 5 (apps/web) |
| Config file | workspace Cargo / apps/web vitest projects |
| Quick run command | `cargo test -p oxidean-api --test auth_verify_reset` (new) |
| Full suite command | `make test` / existing CI matrix + `bun run test` in apps/web |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AUTH-04 | Unverified → `auth.email_unverified` on privileged ping; verified → ok | integration | `cargo test -p oxidean-api --test auth_verify_gate` | ❌ Wave 0 |
| AUTH-04 | Local signup leaves `email_verified_at` null; consume OTP/token sets it | integration | `cargo test -p oxidean-api --test auth_verify_reset` | ❌ Wave 0 |
| AUTH-04 | IdP `email_verified=true` sets verified | unit/integration | workos/oidc mapping + DB assert | ❌ Wave 0 |
| AUTH-04 | Admin seed auto-verified | integration | seed path test | ❌ Wave 0 |
| AUTH-05 | Signup without invite fields still succeeds | integration | extend `auth_signup.rs` assert | ✅ extend existing |
| AUTH-12 | Reset request always same ok; mail only for local-password | integration | recording EmailSender | ❌ Wave 0 |
| AUTH-12 | Redeem sets password, revokes other sessions, sets cookie | integration | multi-session like `auth_session.rs` | ❌ Wave 0 |
| AUTH-12 | SSO-only account no send / redeem error class | integration | password_hash null user | ❌ Wave 0 |
| PLAT-08 | Token migrate + CRUD on 3 dialects | integration | extend `dialect_auth` or new test | ❌ Wave 0 |
| UI | OTP wrapper + verify/reset pages | component | vitest unit/integration | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** targeted `cargo test -p oxidean-api --test …` / `bun run test:unit`
- **Per wave merge:** workspace Rust tests + web unit
- **Phase gate:** Full suite green + human UAT per UI-SPEC copy/banner

### Wave 0 Gaps
- [ ] `crates/oxidean-api/tests/auth_verify_reset.rs` — issue/consume/rate-limit/anti-enumeration
- [ ] `crates/oxidean-api/tests/auth_verify_gate.rs` — `require_verified` + env-gated ping
- [ ] Extend `crates/oxidean-db/tests/dialect_auth.rs` (or sibling) for `0003` tokens + `set_email_verified`
- [ ] Web: InputOtp + verify/reset route smoke tests
- [ ] rpc-gen / api-client regeneration in plan after DTO change

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | Verify/reset tokens; Argon2id reset; session mint |
| V3 Session Management | yes | Revoke-all on reset; cookie flags unchanged |
| V4 Access Control | yes | `require_verified` + `auth.email_unverified` |
| V5 Input Validation | yes | Email normalize; OTP digit pattern; password min 8; sanitize returnTo |
| V6 Cryptography | yes | CSPRNG tokens; SHA-256 at rest; Argon2id passwords — no hand-rolled crypto |

### Known Threat Patterns for verify/reset

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Account enumeration via reset | Information Disclosure | Identical success (D-28); timing hygiene |
| Token theft via Host injection | Spoofing | `OXIDEAN_PUBLIC_ORIGIN` only |
| OTP online brute force | Elevation | Rate limit + attempt cap + TTL + single-use |
| Privilege use before verify | Elevation | `require_verified` on privileged RPC |
| Session fixation after reset | Elevation | Fresh session id (existing SessionService) + revoke others |
| Referrer leakage of magic token | Information Disclosure | `Referrer-Policy: no-referrer` on verify/reset pages [CITED: OWASP Forgot Password] |
| Log leakage of codes | Information Disclosure | Never log token/OTP/URL with secret |

## Sources

### Primary (HIGH confidence)
- In-repo Phase 4 auth/email/session/migrations (Read this session)
- `npm view input-otp` + https://www.npmjs.com/package/input-otp + https://input-otp.rodz.dev/docs/forms
- WorkOS 3.4.0 `User.email_verified` / openidconnect 4.0.1 `email_verified` claim getters (cargo registry Read)
- docs/CONFIGURATION.md `OXIDEAN_PUBLIC_ORIGIN`
- 05-CONTEXT.md / 05-UI-SPEC.md locked decisions

### Secondary (MEDIUM confidence)
- https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html — anti-enumeration, hash storage, rate limits, Host header
- https://cheatsheetseries.owasp.org/cheatsheets/Multifactor_Authentication_Cheat_Sheet.html — OTP TTL/single-use/hash hygiene
- https://cheatsheetseries.owasp.org/cheatsheets/Email_Validation_and_Verification_Cheat_Sheet.html — verify/reset token properties

### Tertiary (LOW confidence)
- ASVS GitHub discussion on OTP hashing tradeoffs (clarifies why rate limits dominate for short OTPs)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — reuses Phase 4 crates; `input-otp@1.5.0` registry-verified
- Architecture: HIGH — CONTEXT locks + existing code seams mapped
- Pitfalls: HIGH — OWASP + in-repo session/email patterns

**Research date:** 2026-09-10  
**Valid until:** ~30 days (auth patterns stable; re-check `input-otp` minor if peer React changes)
