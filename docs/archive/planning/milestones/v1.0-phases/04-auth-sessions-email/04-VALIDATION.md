---
phase: 4
slug: auth-sessions-email
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-09-10
updated: 2026-09-10
validated: 2026-09-10
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust: `cargo test` (workspace); TS: Vitest `^5` in `@oxidean/api-client` |
| **Config file** | crates’ `[[test]]` / `packages/api-client/vitest.config.ts` |
| **Quick run command** | `cargo test -p oxidean-api --lib && cargo test -p oxidean-db --lib` |
| **Full suite command** | `make test` + `bun run --filter @oxidean/api-client test` + `make db-matrix` |
| **Estimated runtime** | ~30s quick / ~3–8m full + dialect matrix |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p oxidean-api --lib` + targeted `--test` for the touched area
- **After every plan wave:** Run `make test` + `make db-matrix` (when DB touched) + api-client vitest if client changed
- **Before `/gsd-verify-work`:** Full suite must be green + Compose smoke signup/login on default Postgres
- **Max feedback latency:** 30 seconds (quick path)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 04-01-T1 | 01 | 1 | AUTH-01/08 | T-04-02 | Username validation / reserved list | unit | `cargo test -p oxidean-core --lib` | ✅ | ✅ green |
| 04-01-T2 | 01 | 1 | AUTH-01/02 | T-04-01 | Hash-only columns in schema | migration | `cargo test -p oxidean-db --lib migration_parity` | ✅ | ✅ green |
| 04-01-T3 | 01 | 1 | AUTH-02/03 | T-04-01 | sessions.user_id + delete_all | integration | `cargo test -p oxidean-db --test dialect_auth` | ✅ | ✅ green |
| 04-02-T1 | 02 | 1 | AUTH-09 | T-04-04 | LogSink only; no network | unit | `cargo test -p oxidean-api --lib email::` | ✅ | ✅ green |
| 04-02-T2 | 02 | 1 | AUTH-10/11 | T-04-05 | Typed addresses; Resend User-Agent | unit | `cargo test -p oxidean-api --lib email::` | ✅ | ✅ green |
| 04-03-T1 | 03 | 2 | AUTH-02 | T-04-09 | Argon2id PHC; no password logs | unit | `cargo test -p oxidean-api --lib auth::password` | ✅ | ✅ green |
| 04-03-T2 | 03 | 2 | AUTH-02 | T-04-07/08/10 | Opaque HttpOnly cookie; SHA-256 token | unit | `cargo test -p oxidean-api --lib auth::session` | ✅ | ✅ green |
| 04-04-T1 | 04 | 3 | AUTH-01/02/03 | T-04-11/12 | Generic login errors; mode gate | unit/lib | `cargo test -p oxidean-api --lib` | ✅ | ✅ green |
| 04-04-T2 | 04 | 3 | AUTH-01/02/03 | T-04-13/14 | Cookie jar signup/session/logout | integration | `cargo test -p oxidean-api --test auth_signup --test auth_session` | ✅ | ✅ green |
| 04-05-T1 | 05 | 4 | AUTH-02 | T-04-15/17 | WorkOS → Oxidean session mint | unit | `cargo test -p oxidean-api --lib auth::workos` | ✅ | ✅ green |
| 04-05-T2 | 05 | 4 | AUTH-02 | T-04-15/16 | OIDC PKCE + https issuer | unit | `cargo test -p oxidean-api --lib auth::oidc` | ✅ | ✅ green |
| 04-06-T1 | 06 | 5 | AUTH-08 | T-04-19/20 | Server-chosen path; size/type allowlist | integration | `cargo test -p oxidean-api --test profile_avatar` | ✅ | ✅ green |
| 04-06-T2 | 06 | 5 | AUTH-09/10/11 | T-04-21/22 | Admin gate; no secrets in RPC | integration | `cargo test -p oxidean-api --test admin_auth_settings` | ✅ | ✅ green |
| 04-07-T1 | 07 | 6 | AUTH-01/02 | T-04-23/24 | Safe returnTo; text-only errors | build | `cd apps/web && bun run build` | ✅ | ✅ green |
| 04-07-T2 | 07 | 6 | AUTH-03 | T-04-23 | Header logout this device | build | `cd apps/web && bun run build` | ✅ | ✅ green |
| 04-08-T1 | 08 | 7 | AUTH-08 | T-04-19 | Profile + avatar UI | build | `cd apps/web && bun run build` | ✅ | ✅ green |
| 04-08-T2 | 08 | 7 | AUTH-09/10/11 | T-04-25/26 | ENV badges only | build | `cd apps/web && bun run build` | ✅ | ✅ green |
| 04-08-T3 | 08 | 7 | ALL | — | Human UAT success criteria | manual | checkpoint | n/a | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Wave 0 stubs were created inside execution plans (not a separate 04-00 plan):

- [x] `crates/oxidean-db/tests/dialect_auth.rs` — 04-01-T3
- [x] `crates/oxidean-api` email unit tests — 04-02-T1/T2
- [x] `crates/oxidean-api/tests/auth_signup.rs` — 04-04-T2
- [x] `crates/oxidean-api/tests/auth_session.rs` — 04-04-T2
- [x] `crates/oxidean-api/tests/profile_avatar.rs` — 04-06-T1
- [x] `crates/oxidean-api/tests/admin_auth_settings.rs` — 04-06-T2
- [x] Optional: `wiremock` for Resend — 04-02-T2

*Existing `rpc_http` / `rpc_db_probe` tests are the template for new http oneshot tests.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `/login` and `/signup` render with Oxidean chrome | AUTH-01/02 UI | Browser chrome / theme | Open routes in light and dark; confirm brand shell |
| Post-login redirect to `/dashboard` or `returnTo` | AUTH-02 | Navigation UX | Sign in from landing vs deep link |
| Live WorkOS / Resend / SMTP E2E | AUTH-10/11 + providers | Requires operator secrets | Only when keys configured; default CI uses mocks/log-sink |
| Phase success criteria UAT | ALL | 04-08-T3 checkpoint | Follow checkpoint how-to-verify — human **approved** 2026-09-10 (no defects) |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (checkpoint is manual by design)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covered by early plan tasks
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` — automated AUTH-01…03, 08…11 covered green (2026-09-10 audit)

**Approval:** validated 2026-09-10 (automated); human UAT for 04-08-T3 **approved** 2026-09-10

---

## Validation Audit 2026-09-10

| Metric | Count |
|--------|-------|
| Gaps found | 0 MISSING tests (map was stale) |
| Resolved | 17 automated rows → ✅ green; Wave 0 checkboxes closed |
| Escalated | 0 |
| Manual-only retained | 4 (UI chrome, returnTo, live secrets E2E, human UAT) |

**Commands re-run green:** `oxidean-core --lib`, `oxidean-db --lib` + `dialect_auth`, `oxidean-api --lib email::|auth::password|auth::session|auth::workos|auth::oidc`, `--test auth_signup|auth_session|profile_avatar|admin_auth_settings`, `apps/web bun run build`.
