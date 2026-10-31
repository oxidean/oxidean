# Phase 6: Self-Host Admin Bootstrap - Research

**Researched:** 2026-09-11
**Domain:** Empty-instance admin bootstrap (ENV seed + `/setup` wizard), `allow_signup` gate, forced ENV credential confirm, SSR home/session gating
**Confidence:** HIGH (codebase seams); MEDIUM (TanStack Start SSR cookie-forward pattern — official docs cited; Context7 unavailable this session)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
### A — Deployment scope (one product)
- **D-01:** Empty-instance bootstrap applies to **any empty DB** — same path for cloud and self-host (no mode gate)
- **D-02:** Reframe AUTH-06/07 (and related docs) as **empty-instance**, not “On self-host” — update REQUIREMENTS when touched
- **D-03:** **No deployment modes** — do not add `OXIDEAN_DEPLOYMENT_MODE` or equivalent; cloud ≡ self-host
- **D-04:** **Hard rule:** no cloud/self-host conditionals in bootstrap (or related Phase 6 paths); tests assert one path — **Reversibility:** costly — product identity depends on single release train

### B — Post-bootstrap signup (`allow_signup`)
- **D-05:** After bootstrap, **`allow_signup`** governs local signup (revises Phase 5 D-08 / AUTH-05 “always open” for this product rule)
- **D-06:** When `allow_signup` is off: **`/signup` returns 404**; logged-out UI **omits all Sign-up references** (no AuthShell soft page; no invite codes) — **Reversibility:** costly — routing + chrome contract
- **D-07:** Default when ENV unset: **`allow_signup = false`** (`OXIDEAN_ALLOW_SIGNUP`)
- **D-08:** **ENV seed** applies `OXIDEAN_ALLOW_SIGNUP` at seed time; **wizard** exposes the same control (**Switch**) — **Reversibility:** costly — persisted instance setting + ENV contract

### C — Empty-instance lock
- **D-09:** **Hard SSR/server gate** to `/setup` before paint while `needs_setup`; signup/SSO blocked until bootstrap completes
- **D-10:** Carve-outs: **`/status` and readiness** stay public; all other app UI → `/setup`
- **D-11:** **Strict RPC** while `needs_setup`: only `auth.bootstrap_status` / `auth.bootstrap_setup` (+ health) succeed
- **D-12:** After successful wizard setup (session issued): land on **signed-in `/`**

### D — ENV seed edge cases
- **D-13:** If **either or both** `OXIDEAN_ADMIN_*` unset → treat as unset; wizard creates sys-admin
- **D-14:** If both set but **seed fails** → **fail boot** (surface error; do not serve the app) — **Reversibility:** one-way — operators rely on fail-closed boot
- **D-15:** ENV-seeded username: **`system-administrator`** (not `admin`/`admin1`)
- **D-16:** ENV seed **creates** the account; first visit gates **`/setup/credentials`** — must change **default** values (username `system-administrator`); **ENV email/password may be kept** (Keep current password Switch)
- **D-17:** Forced credential change applies to **ENV-seeded admins only** — wizard-created credentials are already chosen

### E — Signed-in home SSR (folded flicker fix)
- **D-18:** **SSR session** chooses marketing vs `SignedInHome` at `/`; first HTML matches final UI
- **D-19:** Direct **`/dashboard` → 404** (not a public route; remove soft-redirect pattern)
- **D-20:** `/` SSR priority: **`needs_setup` first** → `/setup`; else session → SignedInHome vs marketing
- **D-21:** Keep `oxidean_signed_in` **presence hint** as progressive enhancement; **drop and rely on SSR** if it causes trouble — agent discretion
- **D-22:** After login and after ENV forced credential change: honor safe **`returnTo`**; else `/`

### Claude's Discretion
- Presence hint retention vs removal if flicker/race issues persist (D-21)
- Exact wizard copy/layout for `allow_signup` control and forced-change screen (match AuthShell patterns)
- How `allow_signup` is persisted (settings table vs dedicated column) — researcher/planner choose existing admin.auth settings patterns

### Deferred Ideas (OUT OF SCOPE)
- **Invite codes / invite issuance** when `allow_signup` is off — later phase; Phase 6 is hard-block only
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| AUTH-06 | Empty instance: when both `OXIDEAN_ADMIN_EMAIL` and `OXIDEAN_ADMIN_PASSWORD` are set, first boot creates that admin | Extend `maybe_seed_admin` (username `system-administrator`, `must_change_credentials`, apply `OXIDEAN_ALLOW_SIGNUP`); fail-closed boot already in `main.rs` |
| AUTH-07 | Empty instance: when those env vars are absent/incomplete, show one-time `/setup` wizard | Existing `needs_setup` / `bootstrap_setup` + SSR gate + `allow_signup` Switch + strict RPC allowlist |
| AUTH-05 (interaction) | Open signup superseded by `allow_signup` product rule | Persist flag on `instance_auth_settings`; enforce in `auth.signup` + `/signup` 404 + chrome omit links |
</phase_requirements>

## Summary

Phase 6 is mostly **extension of existing empty-instance bootstrap**, not a greenfield auth rewrite. `auth.bootstrap_*`, `/setup`, ENV `maybe_seed_admin`, SSO `reject_if_setup_required`, and fail-closed seed on boot already exist. Gaps vs locked decisions: ENV username still `admin`/`admin1`; no `must_change_credentials` or `/setup/credentials`; no `allow_signup` persistence/enforcement; client-only `redirectIfNeedsSetup` (flicker); soft `/dashboard` redirect; RPC not strictly allowlisted while `needs_setup`; signup always open after bootstrap.

**Primary recommendation:** Persist `allow_signup` on `instance_auth_settings`; add `users.must_change_credentials`; harden seed/wizard/RPC; implement SSR gates via `createServerFn` + Cookie forward to existing `auth.*` RPCs (do not adopt TanStack Start `useSession` — Oxidean already owns `oxidean_session`).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| ENV admin seed + fail-closed boot | API / Backend | Database | `maybe_seed_admin` at API boot; process exit on error |
| Wizard create sys-admin | API / Backend | Browser | `auth.bootstrap_setup` owns create+session; `/setup` is form UX |
| `needs_setup` / strict RPC lock | API / Backend | Frontend Server (SSR) | Security boundary is RPC/HTTP; SSR redirects for UX |
| Forced credential change | API / Backend | Browser | Flag on user row; RPC updates credentials; `/setup/credentials` UI |
| `allow_signup` persistence + enforce | API / Backend + Database | Browser | Settings singleton + `auth.signup` reject; UI 404/omit |
| SSR `/` marketing vs SignedInHome | Frontend Server (SSR) | Browser | Cookie-forward server fn → first HTML matches |
| `/dashboard` / closed `/signup` 404 | Frontend Server (SSR) | — | `notFound()` from router; no soft pages |
| Public `allow_signup` for chrome | API / Backend | Browser | Extend public config RPC so header can omit Sign up |

## Standard Stack

### Core
| Library / module | Version | Purpose | Why Standard |
|------------------|---------|---------|--------------|
| Existing `oxidean-api` auth (`bootstrap`, `seed`, `local`, `admin`) | workspace | Seed, wizard, signup gates | Already AUTH-06/07 skeleton [VERIFIED: crates/oxidean-api/src/auth/bootstrap.rs:1-167] |
| `instance_auth_settings` singleton | migration `0002_auth` | Persist `allow_signup` | Same pattern as `provider_mode` [VERIFIED: crates/oxidean-db/src/auth_settings.rs:7-16] |
| `@octanejs/tanstack-start` | `0.1.44` | `createServerFn` SSR | Project Start adapter exports it [VERIFIED: apps/web/package.json:22] |
| `@octanejs/tanstack-router` | `0.1.53` | `beforeLoad`, `redirect`, `notFound` | Re-exports router-core [VERIFIED: apps/web/package.json:21] |
| `@octanejs/base-ui` Switch | `0.1.52` | `allow_signup` + keep-password toggles | Matches hand-authored Checkbox pattern [VERIFIED: apps/web/package.json:18] |
| argon2 / existing password helpers | `argon2 0.6` | Hash on seed / forced change | Already used by seed/signup |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `sqlx` multi-dialect migrations | `0.8` | `0006_*` columns | Add `allow_signup` + `must_change_credentials` on all three dialects |
| Vitest | `5.0.0` | Web unit/integration | Route gate + Switch form tests |
| cargo-nextest | `0.9.143` | API integration | Extend `auth_bootstrap.rs` / new credentials tests |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `instance_auth_settings.allow_signup` | New `instance_flags` table | Extra table for one bool — reject |
| Cookie-forward `createServerFn` | TanStack Start `useSession` | Would duplicate Oxidean session store — reject (D-11 Phase 4) |
| Client-only presence hint | Keep as sole gate | Causes flicker — demote to PE only |

**Installation:** No new npm/cargo packages required. Add Switch UI wrapper (hand-author like Checkbox). Run `make rpc-gen` after DTO changes.

**Version verification:** `@octanejs/tanstack-start@0.1.44`, `@octanejs/base-ui@0.1.52`, `vitest@5.0.0` read from installed package.json this session.

## Package Legitimacy Audit

> No external packages to install this phase.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| — | — | — | — | — | — | N/A — reuse existing workspace deps |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```text
                    ┌─────────────────────────────┐
  Operator ENV      │ OXIDEAN_ADMIN_EMAIL/PASSWORD│
  OXIDEAN_ALLOW_*  │ (+ optional ALLOW_SIGNUP)    │
                    └──────────────┬──────────────┘
                                   │ boot
                                   ▼
                    ┌──────────────────────────────┐
                    │ API main: migrate → seed     │
                    │ seed fail → exit(1)          │
                    └──────────────┬───────────────┘
                                   │ serve
          ┌────────────────────────┼────────────────────────┐
          ▼                        ▼                        ▼
   GET /health              POST /api/rpc              /api/auth/* SSO
   (always)                 dispatch allowlist         reject_if_setup
                                   │
              needs_setup? ────────┤
                 │ yes             │ no
                 ▼                 ▼
        only bootstrap_*     normal auth + allow_signup
        (+ system.health)    enforce on auth.signup
                 │
                 ▼
        ┌────────────────────────────────────────────┐
        │ Web SSR (createServerFn + Cookie forward)  │
        │ priority: needs_setup → /setup             │
        │           must_change → /setup/credentials │
        │           session → SignedInHome at `/`    │
        │           else marketing                   │
        │ /dashboard → notFound; closed /signup 404  │
        └────────────────────────────────────────────┘
```

### Recommended Project Structure
```
crates/oxidean-db/migrations/{sqlite,postgres,mysql}/0006_bootstrap_flags.sql
crates/oxidean-db/src/{auth_settings,users}.rs          # columns + getters/setters
crates/oxidean-core/src/auth_types.rs                   # DTOs: allow_signup, must_change, BootstrapSetupRequest
crates/oxidean-api/src/auth/{seed,bootstrap,local,admin}.rs
crates/oxidean-api/src/rpc.rs                           # needs_setup allowlist
crates/oxidean-api/tests/auth_bootstrap.rs (+ credentials tests)
apps/web/src/lib/ssr-auth.ts                             # createServerFn cookie-forward helpers
apps/web/src/routes/{index,setup,setup.credentials,signup,dashboard,login}.tsrx
apps/web/src/components/ui/switch.tsrx                   # new Base UI wrapper
apps/web/src/components/chrome.tsrx                      # gate Sign up on allow_signup
docs/{CONFIGURATION,ARCHITECTURE,API}.md + .env.example
```

### Pattern 1: ENV bool parse (match existing)
**What:** Parse `OXIDEAN_ALLOW_SIGNUP` like `OXIDEAN_AUTO_MIGRATE`.
**When to use:** Seed-time + docs.
**Example:**
```rust
// Source: crates/oxidean-api/src/main.rs:49-51 (verbatim pattern)
let auto_migrate = std::env::var("OXIDEAN_AUTO_MIGRATE")
    .map(|v| v == "true" || v == "1")
    .unwrap_or(true);
// Phase 6: same parse, unwrap_or(false) for OXIDEAN_ALLOW_SIGNUP
```

### Pattern 2: SSR session via Cookie forward (not Start useSession)
**What:** Server fn reads incoming Cookie and calls Oxidean RPC.
**When to use:** `/` `beforeLoad`/`loader`, setup gates, signup 404 decision.
**Example:**
```typescript
// Source: TanStack Start auth guide + @octanejs/tanstack-start exports
// [CITED: https://tanstack.com/start/latest/docs/framework/react/guide/authentication]
import { createServerFn } from "@octanejs/tanstack-start";
import { getRequestHeader } from "@octanejs/tanstack-start/server";
import { createClient } from "@oxidean/api-client";

export const fetchBootstrapStatus = createServerFn({ method: "GET" }).handler(
  async () => {
    const cookie = getRequestHeader("cookie") ?? "";
    const client = createClient({
      baseUrl: process.env.OXIDEAN_API_ORIGIN ?? "http://127.0.0.1:8080",
      credentials: "include",
      fetch: (input, init) =>
        fetch(input, {
          ...init,
          headers: { ...init?.headers, cookie },
        }),
    });
    return client.auth.bootstrapStatus();
  },
);
```
> Note: `createClient` today does not accept custom `fetch` headers beyond options — planner must either extend `CreateClientOptions` with `headers`/`fetch` (already has `fetch?: typeof fetch`) or wrap RPC call manually. `fetch` override is already on the type [VERIFIED: packages/api-client/src/index.ts:123-127].

### Pattern 3: Persist `allow_signup` on auth settings
**What:** Add column to singleton `instance_auth_settings`; expose on `AuthSettingsPublic` + public `ProviderConfigPublic` (or bootstrap status).
**When to use:** Seed, wizard, admin.auth UI Switch.
**Recommendation:** Column `allow_signup` default false; public read via extended `auth.provider_config` so chrome does not need admin RPC.

### Anti-Patterns to Avoid
- **Deployment-mode forks:** Violates D-01–D-04.
- **Soft AuthShell for closed signup:** UI-SPEC requires 404 + omit links (supersedes older soft-block wording).
- **TanStack Start encrypted cookie sessions for forge auth:** Duplicates Rust session store.
- **SSR `apiClient` without Cookie forward:** Current `baseUrl` SSR path cannot see HttpOnly session [VERIFIED: apps/web/src/lib/api-client.ts:3-7].
- **Leaving `/dashboard` soft-redirect:** Violates D-19 / UI-SPEC.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Password hashing | Custom crypto | Existing `hash_password_str` / `set_password_hash` | Already ASVS-aligned argon2 |
| Session mint | New cookie scheme | `issue_session` | Same as wizard/login |
| SSR auth store | Start `useSession` | Forward `oxidean_session` Cookie to API | Single source of truth |
| Switch UI | Raw checkbox/CSS | `@octanejs/base-ui/switch` wrapper | Design-system continuity |
| 404 routing | Custom error page hack | `notFound()` from `@octanejs/tanstack-router` | router-core export |

**Key insight:** Phase 6 is gate/policy + SSR wiring on mature auth modules — deepen those modules rather than adding parallel systems.

## Common Pitfalls

### Pitfall 1: ENV set ⇒ `needs_setup=false` before seed runs
**What goes wrong:** Wizard hidden while users still empty if seed skipped.
**Why:** `admin_env_configured()` short-circuits `needs_setup` [VERIFIED: crates/oxidean-api/src/auth/bootstrap.rs:55-61].
**How to avoid:** Always run seed before serve (already in `main`); tests that set ENV must call `maybe_seed_admin` or assert fail-boot.
**Warning signs:** Empty DB + ENV + login works for nobody.

### Pitfall 2: Strict RPC not centralized
**What goes wrong:** New procedures bypass setup lock.
**Why:** Today only `auth.signup` checks `needs_setup` [VERIFIED: crates/oxidean-api/src/auth/local.rs:132-138]; dispatch has no allowlist.
**How to avoid:** Early allowlist in `rpc::dispatch` when `needs_setup` (bootstrap_* + `system.health` only); keep SSO `reject_if_setup_required`.
**Warning signs:** `auth.login` / `auth.me` succeed on empty unseeded instance.

### Pitfall 3: SSR flicker remains
**What goes wrong:** Marketing still flashes for signed-in users.
**Why:** Client `useEffect` + presence hint after mount [VERIFIED: apps/web/src/routes/index.tsrx:64-106]; SSR client has no Cookie.
**How to avoid:** `beforeLoad`/`loader` with cookie-forwarded `auth.me` + `needs_setup`; render tree from loader data.
**Warning signs:** Hydration mismatch or marketing HTML in View Source while cookie present.

### Pitfall 4: Forced-change without email update API
**What goes wrong:** Username/password changeable, email stuck.
**Why:** DB has `set_password_hash` and profile username update; **no** `UPDATE users SET email` helper found.
**How to avoid:** Add dialect-safe `update_user_email` (or combined credentials update) in `oxidean-db`.
**Warning signs:** Forced-change RPC only updates username.

### Pitfall 5: Reserved username / default clash
**What goes wrong:** `system-administrator` accepted forever or blocked incorrectly.
**Why:** Reserved list has `"system"` and `"admin"` but not `"system-administrator"` [VERIFIED: crates/oxidean-core/src/auth_types.rs:151-184].
**How to avoid:** Seed uses fixed default; forced-change rejects case-insensitive equality to `system-administrator`; add name to reserved list for normal signup after bootstrap (wizard may keep reserved bypass for `admin`).
**Warning signs:** Users can still sign up as `system-administrator`.

### Pitfall 6: Partial ENV treated as seed path
**What goes wrong:** One of email/password set → confusing state.
**Why:** D-13 requires either/both unset ⇒ wizard; current `admin_env_configured` already requires both non-empty [VERIFIED: crates/oxidean-api/src/auth/bootstrap.rs:45-52].
**How to avoid:** Keep both-required; document; add test for email-only / password-only ⇒ `needs_setup=true`.

## Code Examples

### Existing seed (must change)
```rust
// Source: crates/oxidean-api/src/auth/seed.rs:27-30 [VERIFIED]
let username = match db.find_user_by_username("admin").await? {
    None => "admin".to_string(),
    Some(_) => "admin1".to_string(),
};
```
Replace with fixed `"system-administrator"`, set `must_change_credentials=true`, write `allow_signup` from ENV.

### Fail-closed boot (already present — keep)
```rust
// Source: crates/oxidean-api/src/main.rs:66-69 [VERIFIED]
if let Err(e) = seed::maybe_seed_admin(&db).await {
    eprintln!("admin seed failed: {e}");
    std::process::exit(1);
}
```

### Bootstrap status shape (extend carefully)
```rust
// Source: crates/oxidean-core/src/auth_types.rs:79-84 [VERIFIED]
pub struct BootstrapStatus {
    /// True when `users` is empty and `OXIDEAN_ADMIN_*` ENV seed is not configured.
    pub needs_setup: bool,
}
```
Recommend keeping this lean; put `allow_signup` on `ProviderConfigPublic` instead:
```rust
// Source: crates/oxidean-core/src/auth_types.rs:110-114 [VERIFIED]
pub struct ProviderConfigPublic {
    pub mode: ProviderMode,
}
```

### Wizard reserved bypass (preserve for bootstrap names)
```rust
// Source: crates/oxidean-api/src/auth/bootstrap.rs:100-106 [VERIFIED]
match validate_username(&username) {
    Ok(()) => {}
    Err(e) if e.contains("reserved") && is_reserved_username(&username) => {}
    Err(e) => return Err(map_username_err(e)),
}
```

### Session cookie names (SSR / gates)
```rust
// Source: crates/oxidean-api/src/auth/session.rs:15-20 [VERIFIED]
pub const SESSION_COOKIE_NAME: &str = "oxidean_session";
pub const SESSION_PRESENCE_COOKIE_NAME: &str = "oxidean_signed_in";
```

## State of the Art

| Old Approach (today) | Phase 6 Approach | Impact |
|----------------------|------------------|--------|
| Client `redirectIfNeedsSetup` | SSR/server `beforeLoad` | No setup/marketing flicker |
| Soft `/dashboard` → `/` | `notFound()` | Matches product URL surface |
| Open signup after bootstrap | `allow_signup` default false | AUTH-05 product rule update |
| Seed username `admin`/`admin1` | `system-administrator` + forced confirm | Safer defaults |
| Presence hint as primary gate | PE only (D-21 discretion) | SSR owns first paint |

**Deprecated/outdated:**
- Client-only home gating for signed-in users
- REQUIREMENTS/ROADMAP “On self-host” wording for AUTH-06/07 (reframe empty-instance per D-02)

## Researcher Recommendations (Discretion)

1. **`allow_signup` persistence:** Column on `instance_auth_settings` (not a new table). Wire through `AuthSettingsRow` / `admin.auth.*` / seed / `bootstrap_setup`.
2. **Public exposure:** Add `allow_signup: bool` to `ProviderConfigPublic` (chrome + signup route). Fail closed (omit Sign up) until resolved — matches UI-SPEC.
3. **`must_change_credentials`:** Boolean on `users`, default false; set true only in ENV seed; clear in new `auth.confirm_admin_credentials` (or `auth.complete_forced_credentials`) RPC; include on `UserPublic` so SSR/client can gate.
4. **Presence hint (D-21):** Keep initially; remove if SSR loader path still races. Do not use as security boundary.
5. **ENV parse:** `OXIDEAN_ALLOW_SIGNUP` → true only for `"true"` or `"1"`; default false (mirror AUTO_MIGRATE parse, opposite default).
6. **UI Switch:** Hand-author `apps/web/src/components/ui/switch.tsrx` from `@octanejs/base-ui/switch` like Checkbox (Phase 4 pattern); optional `bunx shadcn@latest add switch` only if it emits compatible Octane `.tsrx`.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Extending `CreateClientOptions.fetch` is sufficient for Cookie forward without new client fields | Pattern 2 | May need explicit `headers` helper in api-client |
| A2 | `auth.provider_config` is the right public surface for `allow_signup` (vs new RPC) | Recommendations | Chrome may need a dedicated lightweight endpoint later |
| A3 | Forced-change RPC name `auth.confirm_admin_credentials` | Recommendations | Naming only — behavior locked by UI-SPEC |
| A4 | Adding `system-administrator` to reserved list is desired | Pitfall 5 | If not reserved, post-bootstrap collision risk |

**If this table is empty:** — not empty; confirm A2 with planner if preferred to hang `allow_signup` on `BootstrapStatus` instead.

## Open Questions (RESOLVED)

1. **Should `auth.login` be allowed while `must_change_credentials` is set?** — **RESOLVED**
   - What we know: UI-SPEC gates all app UI to `/setup/credentials` after session exists.
   - Resolution (assumed by 06-02 / 06-05 / 06-06): **Login may mint a session** while `must_change_credentials` is set; SSR/UI hard-gates `/setup/credentials` (D-16/D-17). Optional extra RPC allowlisting for flagged users is out of scope unless a later plan adds it.
   - Rationale: Matches UI-SPEC first-login flow; avoids blocking session issue needed to call `auth.confirm_admin_credentials`.

2. **Compose/cloud default for `OXIDEAN_ALLOW_SIGNUP`** — **RESOLVED**
   - What we know: Default false (D-07); cloud previously “open signup” (AUTH-05).
   - Resolution (assumed by 06-04 / 06-07): **Code default remains false** (one product path). Cloud Compose/Railway (and docs) set `OXIDEAN_ALLOW_SIGNUP=true` when open signup is desired — documentation-only; no Compose file change required unless ADMIN env is already listed.
   - Rationale: D-03/D-04 forbid deployment-mode forks; ENV at deploy covers cloud AUTH-05 without branching code.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust / cargo | API changes | ✓ | rustc 1.100.0-nightly | — |
| cargo-nextest | API tests | ✓ | 0.9.143 | `cargo test --workspace` |
| bun | Web Switch / vitest | ✓ | 1.4.0 | — |
| Node | engines for Start | ✓ | v24.5.0 | — |
| Vitest | Web tests | ✓ | 5.0.0 | — |
| Context7 CLI | Docs lookup | ✗ | — | WebFetch official docs (used) |
| PostgreSQL/MySQL | Dialect migrations | optional | — | SQLite tempdir tests cover logic |

**Missing dependencies with no fallback:** none for implementation.
**Missing dependencies with fallback:** Context7 → WebFetch/official docs.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo-nextest 0.9.143 (API) + Vitest 5.0.0 (web) |
| Config file | workspace Cargo; `apps/web/vitest.config.ts` |
| Quick run command | `cargo nextest run -p oxidean-api -E 'test(bootstrap)'` |
| Full suite command | `make test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AUTH-06 | Both ENV set + empty users → seed `system-administrator`, verified, `must_change_credentials` | integration | `cargo nextest run -p oxidean-api -E 'test(seeded_admin)'` | ⚠️ extend `auth_signup.rs` / seed tests |
| AUTH-06 | Seed failure → process exit (library returns Err; main exits) | unit/integration | assert `maybe_seed_admin` Err path | ❌ Wave 0 (partial: main already exits) |
| AUTH-06 | Partial ENV → no seed, `needs_setup=true` | integration | new test in `auth_bootstrap.rs` | ❌ Wave 0 |
| AUTH-07 | Empty + no ENV → `needs_setup` + wizard creates sys-admin | integration | existing `auth_bootstrap.rs` | ✅ extend for `allow_signup` |
| AUTH-07 | Second setup → `auth.setup_unavailable` | integration | existing | ✅ |
| D-11 | While `needs_setup`, non-allowlisted RPC fails | integration | new | ❌ Wave 0 |
| allow_signup | Default false blocks `auth.signup` after bootstrap | integration | new | ❌ Wave 0 |
| allow_signup | `/signup` route `notFound` when false | integration (web) | vitest integration | ❌ Wave 0 |
| D-16/17 | Forced credentials rejects default username; keep password OK | integration | new API test | ❌ Wave 0 |
| D-18/20 | SSR/home loader chooses tree (mock server fn) | unit/integration | vitest | ❌ Wave 0 |
| D-19 | `/dashboard` → notFound | unit/integration | vitest | ❌ Wave 0 |
| D-04 path | No cloud/self-host branches in bootstrap | grep/CI assert | `rg` in plan verify | ❌ Wave 0 checklist |

### Sampling Rate
- **Per task commit:** `cargo nextest run -p oxidean-api -E 'test(bootstrap) | test(seeded_admin) | test(signup)'` and/or `bun run --filter @oxidean/web test:unit`
- **Per wave merge:** `cargo nextest run -p oxidean-api` + `bun run --filter @oxidean/web test:unit test:integration`
- **Phase gate:** `make test` green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] Extend `crates/oxidean-api/tests/auth_bootstrap.rs` — partial ENV, allow_signup on setup, strict RPC allowlist
- [ ] Extend seed tests — username `system-administrator`, `must_change_credentials`, `OXIDEAN_ALLOW_SIGNUP`
- [ ] New API tests — forced credential change RPC; signup blocked when `allow_signup=false`
- [ ] Web: `/setup` Switch + `/setup/credentials` form integration tests
- [ ] Web: signup/chrome omit + dashboard notFound tests
- [ ] Web: `apps/web/src/routes/index.integration.test.ts` — D-18/D-20 home SSR tree gate (needs_setup vs SignedInHome vs marketing)
- [ ] Migration `0006` dialect triple + `dialect_auth` coverage for new columns

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | Existing local auth + bootstrap; forced change for ENV defaults |
| V3 Session Management | yes | Reuse `issue_session` / HttpOnly `oxidean_session`; SSR forwards Cookie only |
| V4 Access Control | yes | Strict RPC allowlist while `needs_setup`; `allow_signup` on signup; sys-admin settings |
| V5 Input Validation | yes | Existing username/email/password validators; reject default username on confirm |
| V6 Cryptography | yes | argon2 via existing helpers — never hand-roll |

### Known Threat Patterns for empty-instance bootstrap

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Unauthenticated signup on empty DB | Elevation | `needs_setup` blocks signup; only wizard/ENV creates first admin |
| Race two wizards | Elevation | Re-check `count_users` before create (already) |
| ENV default credentials left forever | Elevation | `must_change_credentials` + `/setup/credentials` gate |
| Closed-signup bypass via RPC | Elevation | Enforce `allow_signup` in `auth.signup` (UI 404 is UX only) |
| Session fixation / SSO during setup | Spoofing | Keep `reject_if_setup_required` on SSO starts |
| Open redirect after confirm | Spoofing | Reuse `safeReturnTo` [VERIFIED: apps/web/src/lib/return-to.ts:5-22] |

## Project Constraints (from .cursor/rules/)

No `.cursor/rules/` directory present. Follow project skills: deep-module seams (`codebase-design`), test at public RPC/UI seams (`tdd`), idiomatic Rust Results without unwrap in prod (`rust-best-practices`). UI must follow approved `06-UI-SPEC.md`.

## Sources

### Primary (HIGH confidence)
- In-repo: `bootstrap.rs`, `seed.rs`, `local.rs`, `main.rs`, `auth_settings.rs`, `auth_types.rs`, `session.rs`, `index.tsrx`, `api-client.ts`, `06-CONTEXT.md`, `06-UI-SPEC.md`
- `@octanejs/tanstack-start@0.1.44` package exports (`createServerFn`, `/server` → `getRequestHeader`)
- `@tanstack/start-server-core@1.169.17` `request-response.d.ts` (`getRequestHeader`, `getCookie`)

### Secondary (MEDIUM confidence)
- [CITED: https://tanstack.com/start/latest/docs/framework/react/guide/authentication] — `beforeLoad` + server fn auth UX; data auth still on API
- [CITED: https://ui.shadcn.com/docs/components/base/switch] — Switch install/usage for Base UI path

### Tertiary (LOW confidence)
- WebSearch orbit-starter cookie-forward example (illustrative only; pattern confirmed against Start server APIs)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — reuse existing crates/UI; versions verified installed
- Architecture: HIGH — locked decisions map cleanly onto existing modules; SSR approach verified against Start APIs
- Pitfalls: HIGH — derived from current code paths and UI-SPEC edge cases

**Research date:** 2026-09-11
**Valid until:** 2026-10-11 (stable auth domain; recheck if `@octanejs/tanstack-start` major bump)
