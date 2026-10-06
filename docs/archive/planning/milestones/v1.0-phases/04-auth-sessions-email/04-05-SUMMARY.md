---
phase: 04-auth-sessions-email
plan: "05"
subsystem: auth
tags: [workos, oidc, pkce, sso, sessions, authkit, openidconnect]

requires:
  - phase: 04-auth-sessions-email
    provides: "Local auth RPC + SessionService + auth_identities (04)"
provides:
  - "WorkOS AuthKit start/callback minting Oxidean session cookie"
  - "Generic OIDC auth-code+PKCE start/callback minting same session"
  - "Pending OAuth state store (TTL 10m) + SSRF-safe issuer validation"
  - "External identity link via auth_identities; username/profile_incomplete rules"
affects:
  - 04-06-profile-admin
  - 04-07-auth-ui

tech-stack:
  added: [workos@3.4, openidconnect@4.0, url]
  patterns:
    - "External IdP finish → link_or_create_user → SessionService.create → Set-Cookie redirect"
    - "Never use WorkOS sealed cookies as app session (T-04-17)"
    - "In-memory PendingAuthStore keyed by OAuth state"
    - "OIDC issuer must be https; reject localhost/link-local/10/8/metadata (T-04-16)"

key-files:
  created:
    - crates/oxidean-api/src/auth/workos.rs
    - crates/oxidean-api/src/auth/oidc.rs
    - crates/oxidean-api/src/auth/external.rs
    - crates/oxidean-api/src/auth/pending.rs
    - crates/oxidean-api/src/routes/auth_callbacks.rs
    - crates/oxidean-api/src/routes/mod.rs
  modified:
    - crates/oxidean-api/Cargo.toml
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/src/auth/mod.rs
    - crates/oxidean-api/src/auth/local.rs
    - crates/oxidean-api/src/lib.rs

key-decisions:
  - "WorkOS uses client.authkit().pkce_authorization_url + user_management().authenticate_with_code with PKCE verifier"
  - "OIDC uses openidconnect 4 DiscoveredClient type alias (EndpointSet/MaybeSet) after CoreProviderMetadata::discover_async"
  - "profile_incomplete derived from placeholder username u{8hex}; no DB column"
  - "No rust-toolchain.toml bump — local rustc 1.100.0-nightly already ≥ workos MSRV 1.88"

patterns-established:
  - "SSO callbacks always mint Oxidean sessions via SessionService (remember_me=false)"
  - "IdP errors logged server-side; browser redirected to /login?error=sso"
  - "sanitize_return_to allows only same-origin relative paths"

requirements-completed: [AUTH-02]

duration: 7min
completed: 2026-09-09
---

# Phase 4 Plan 05: WorkOS & OIDC Adapters Summary

**WorkOS AuthKit and generic OIDC PKCE adapters with HTTP start/callback routes that mint the same Oxidean `oxidean_session` cookie as local login**

## Performance

- **Duration:** 7 min
- **Started:** 2026-09-09T23:38:53Z
- **Completed:** 2026-09-09T23:46:15Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments

- WorkOS AuthKit: `provider=authkit` + PKCE start URL; `authenticate_with_code` finish; Oxidean session on callback
- Generic OIDC: discover + PKCE + nonce; ID token verify; SSRF issuer checks; same session mint path
- Shared `auth_identities` linking, username derivation / `u{shortid}` placeholders, safe `return_to`
- Local signup/login remain rejected when mode is workos/oidc (existing `require_local`)

## Task Commits

Each task was committed atomically:

1. **Task 1: WorkOS AuthKit adapter + routes** - `20e507c` (feat)
2. **Task 2: Generic OIDC adapter + routes** - `2bf2bcb` (feat)

**Plan metadata:** `d96c83f` (docs: complete WorkOS/OIDC adapters plan)

## Files Created/Modified

- `crates/oxidean-api/src/auth/workos.rs` — AuthKit start/finish + unit tests
- `crates/oxidean-api/src/auth/oidc.rs` — OIDC PKCE + SSRF validation + unit tests
- `crates/oxidean-api/src/auth/external.rs` — ExternalIdentity, link_or_create_user, sanitize_return_to
- `crates/oxidean-api/src/auth/pending.rs` — in-memory state/PKCE store (TTL 10m)
- `crates/oxidean-api/src/routes/auth_callbacks.rs` — `/api/auth/{workos,oidc}/{start,callback}`
- `crates/oxidean-api/src/app.rs` — mount callback routes; PendingAuthStore on AppState
- `crates/oxidean-api/src/auth/local.rs` — profile_incomplete from placeholder username

## Decisions Made

- Used official `workos` 3.4 AuthKit helper + `authenticate_with_code` (not sealed session cookies)
- openidconnect 4 requires typed `CoreClient<EndpointSet, …, EndpointMaybeSet, …>` after discovery
- Placeholder usernames (`u` + 8 hex) signal `profile_incomplete` without a schema column
- Toolchain: no `rust-toolchain.toml` — environment already satisfies workos MSRV 1.88; document for CI

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] PendingAuth gained `redirect_uri` for OIDC token exchange**
- **Found during:** Task 2
- **Issue:** OIDC code exchange requires the same redirect_uri used at authorize time; plan’s pending map fields omitted it
- **Fix:** Added `redirect_uri` to `PendingAuth` (WorkOS stores it too for consistency)
- **Files modified:** `pending.rs`, `workos.rs`, `oidc.rs`
- **Verification:** `cargo test -p oxidean-api --lib auth::oidc` / `auth::workos`
- **Committed in:** `2bf2bcb` (Task 2)

**2. [Rule 3 - Blocking] AuthKitHelper::new is crate-private — use Client::authkit()**
- **Found during:** Task 1
- **Issue:** Research sketched `AuthKitHelper::new`; SDK only exposes `client.authkit()`
- **Fix:** Call `c.authkit().pkce_authorization_url(...)`
- **Files modified:** `workos.rs`
- **Verification:** start URL unit test passes with `provider=authkit`
- **Committed in:** `20e507c` (Task 1)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Required for correct SDK usage and OIDC token exchange; no scope creep.

## Issues Encountered

None

## User Setup Required

**External services require manual configuration for live E2E.** See [04-USER-SETUP.md](./04-USER-SETUP.md) for:
- `WORKOS_API_KEY` / `WORKOS_CLIENT_ID`
- `OXIDEAN_OIDC_ISSUER` / `OXIDEAN_OIDC_CLIENT_ID` / `OXIDEAN_OIDC_CLIENT_SECRET`
- Optional `OXIDEAN_PUBLIC_ORIGIN` for correct callback redirect URIs behind proxy

CI uses unit tests without live IdP keys.

## Next Phase Readiness

- SSO adapters ready for profile/admin (04-06) and mode-exclusive auth UI (04-07)
- `auth.provider_config` already returns mode for Continue with WorkOS / SSO CTAs

## Self-Check: PASSED

- `crates/oxidean-api/src/auth/workos.rs` — FOUND
- `crates/oxidean-api/src/auth/oidc.rs` — FOUND
- `crates/oxidean-api/src/auth/external.rs` — FOUND
- `crates/oxidean-api/src/auth/pending.rs` — FOUND
- `crates/oxidean-api/src/routes/auth_callbacks.rs` — FOUND
- Commits `20e507c`, `2bf2bcb` — FOUND
- `cargo test -p oxidean-api --lib auth::` — 22 passed

---
*Phase: 04-auth-sessions-email*
*Completed: 2026-09-09*
