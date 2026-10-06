---
phase: 04-auth-sessions-email
plan: "06"
subsystem: auth
tags: [profile, avatar, multipart, admin-auth, rpc-gen, image, email-settings]

requires:
  - phase: 04-auth-sessions-email
    provides: "Local auth + sessions + WorkOS/OIDC (04-04/04-05)"
provides:
  - "user.get_profile / user.update_profile RPC (display name, username, bio)"
  - "POST /api/user/avatar multipart → var/uploads/avatars/{user_id}.webp"
  - "GET /uploads/avatars/{file} public serve with basename traversal guards"
  - "admin.auth.get_settings / update_settings with is_admin gate + ENV secret badges"
  - "api-client user.* and admin.auth.* procedures via rpc-gen"
affects:
  - 04-07-auth-ui
  - 04-08-admin-profile-ui

tech-stack:
  added: [image@0.25, axum/multipart]
  patterns:
    - "Avatar: ignore client filename; decode via image crate; re-encode webp; store under uploads_dir/avatars"
    - "Admin settings persist non-secrets in DB; secrets remain ENV-only with boolean configured badges"
    - "EmailSender rebuilt on admin.auth.update_settings and at API boot from DB email_provider"

key-files:
  created:
    - crates/oxidean-api/src/auth/profile.rs
    - crates/oxidean-api/src/auth/admin.rs
    - crates/oxidean-api/src/routes/avatar.rs
    - crates/oxidean-api/tests/profile_avatar.rs
    - crates/oxidean-api/tests/admin_auth_settings.rs
  modified:
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/app.rs
    - crates/oxidean-api/src/email/mod.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-api/src/main.rs
    - crates/oxidean-core/src/auth_types.rs
    - packages/api-client/src/index.ts
    - docker-compose.yml
    - .env.example

key-decisions:
  - "Avatar public URL path /uploads/avatars/{user_id}.webp stored in users.avatar_path"
  - "AppState email is Arc<RwLock<Arc<dyn EmailSender>>> so admin settings can hot-rebuild sender"
  - "AuthSettingsPublic returns workos_client_id display + ENV configured booleans (no secret values)"

patterns-established:
  - "Multipart uploads live as thin HTTP routes beside RPC; profile fields stay on user.* RPC"
  - "admin.forbidden → HTTP 403; auth.unauthenticated → 401"
  - "Compose binds ./var/uploads:/var/uploads; Traefik PathPrefix(/uploads) to api"

requirements-completed: [AUTH-08, AUTH-09, AUTH-10, AUTH-11]

duration: 10min
completed: 2026-09-10
---

# Phase 4 Plan 06: Profile CRUD, Avatar & Admin Auth Settings Summary

**Profile RPC + 2 MiB avatar multipart (jpeg/png/webp → 512px webp under `var/uploads/avatars`) and admin.auth settings with ENV-only secrets plus regenerated api-client**

## Performance

- **Duration:** 10 min
- **Started:** 2026-09-09T23:49:27Z
- **Completed:** 2026-09-10T00:00:09Z
- **Tasks:** 2
- **Files modified:** 17

## Accomplishments

- `user.get_profile` / `user.update_profile` with GitHub-like username rules and bio ≤160
- Avatar upload/serve with 2 MiB cap, content-type allowlist, path-traversal-safe basename serve
- `admin.auth.*` gated on `is_admin`; persists provider/email non-secrets; ENV configured badges only
- rpc-gen ships `user.getProfile` / `updateProfile` and `admin.auth.getSettings` / `updateSettings`

## Task Commits

Each task was committed atomically:

1. **Task 1: Profile RPC + avatar upload/serve** - `3c34d40` (feat)
2. **Task 2: admin.auth settings + rpc-gen** - `8409834` (feat)

**Plan metadata:** `f9701e5` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-api/src/auth/profile.rs` — profile get/update RPC handlers
- `crates/oxidean-api/src/auth/admin.rs` — admin.auth get/update + ENV badges
- `crates/oxidean-api/src/routes/avatar.rs` — multipart upload + static serve
- `crates/oxidean-api/tests/profile_avatar.rs` — AUTH-08 round-trip, oversized, traversal
- `crates/oxidean-api/tests/admin_auth_settings.rs` — forbidden + admin update
- `crates/oxidean-api/src/rpc.rs` / `app.rs` — dispatch + routes + email slot
- `crates/oxidean-api/src/email/mod.rs` — `build_email_sender_for_settings`
- `packages/api-client/src/index.ts` — regenerated client
- `docker-compose.yml` — uploads volume + Traefik `/uploads`

## Decisions Made

- Store public URL path in `avatar_path` (not filesystem path) so UI can use `avatar_url` directly
- Hot-swap email sender via `RwLock` on AppState when admin changes `email_provider`
- Extended `AuthSettingsPublic` with display `workos_client_id` and secret-presence booleans (replacing prior `workos_client_id_configured`-only shape)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Traefik `/uploads` + Compose volume bind**
- **Found during:** Task 1
- **Issue:** Plan noted Compose volume; Traefik only routed `/api` and `/health`, so public avatar GETs would 404 at the edge
- **Fix:** Added `PathPrefix(/uploads)` to api router rule and `./var/uploads:/var/uploads` bind (api CWD is `/`)
- **Files modified:** `docker-compose.yml`
- **Verification:** compose file contains both mount and rule
- **Committed in:** `3c34d40` (Task 1)

**2. [Rule 1 - Bug] Hardcoded test PNG bytes were corrupt**
- **Found during:** Task 1 verification
- **Issue:** First tiny PNG fixture failed decode → upload 400
- **Fix:** Replaced with valid zlib-compressed 1×1 RGB PNG bytes
- **Files modified:** `tests/profile_avatar.rs`
- **Verification:** `cargo test -p oxidean-api --test profile_avatar` green
- **Committed in:** `3c34d40` (Task 1)

**3. [Rule 2 - Missing Critical] Boot rebuild of EmailSender from DB settings**
- **Found during:** Task 2
- **Issue:** Plan prefers rebuild on settings update + boot; `router()` only used ENV
- **Fix:** `main` loads `get_auth_settings` after migrate and builds sender via `build_email_sender_for_settings`
- **Files modified:** `main.rs`, `email/mod.rs`
- **Verification:** `cargo check -p oxidean-api --bins`
- **Committed in:** `8409834` (Task 2)

---

**Total deviations:** 3 auto-fixed (1 bug, 2 missing critical)
**Impact on plan:** Required for correct edge routing, tests, and D-09 email provider selection; no scope creep.

## Issues Encountered

None

## User Setup Required

None - no new external services. Existing WorkOS/OIDC/Resend/SMTP ENV vars remain as documented in [04-USER-SETUP.md](./04-USER-SETUP.md) and `.env.example`.

## Next Phase Readiness

- API ready for `/settings/profile` and `/admin/auth` UI (plans 04-07 / 04-08)
- AUTH-08 complete at API layer; AUTH-09/10/11 still satisfied via settings-driven email provider + ENV secrets

## Verification

- `cargo test -p oxidean-api --test profile_avatar` — 3 passed
- `cargo test -p oxidean-api --test admin_auth_settings` — 2 passed (non-admin → `admin.forbidden`)
- `cargo test -p oxidean-api --lib` — 31 passed
- `bun run --filter @oxidean/api-client test` — 3 passed

## Self-Check: PASSED

- `crates/oxidean-api/src/auth/profile.rs` — FOUND
- `crates/oxidean-api/src/auth/admin.rs` — FOUND
- `crates/oxidean-api/src/routes/avatar.rs` — FOUND
- `crates/oxidean-api/tests/profile_avatar.rs` — FOUND
- `crates/oxidean-api/tests/admin_auth_settings.rs` — FOUND
- Commits `3c34d40`, `8409834` — FOUND
- `cargo test -p oxidean-api --test profile_avatar` — 3 passed
- `cargo test -p oxidean-api --test admin_auth_settings` — 2 passed


---
*Phase: 04-auth-sessions-email*
*Completed: 2026-09-10*
