# Phase 4: User Setup Required

**Generated:** 2026-09-09
**Updated:** 2026-09-09 (plan 04-05 WorkOS/OIDC)
**Phase:** 04-auth-sessions-email
**Status:** Incomplete

Complete these items for live SMTP/Resend delivery, optional first-admin bootstrap, and live WorkOS/OIDC E2E. Claude automated adapters, auth RPC, SSO routes, and tests; CI uses the log sink, wiremock, and unit mocks. These items require operator credentials.

**Local development without cloud secrets:** use [`docs/dev-auth.md`](../../../docs/dev-auth.md) (`make up-dev-auth`) — Mailpit SMTP, OIDC mock, and Resend/WorkOS HTTP stubs with throwaway ENV from `docs/dev-auth.env.example`.

## Environment Variables

| Status | Variable | Source | Add to |
|--------|----------|--------|--------|
| [ ] | `OXIDEAN_SMTP_URL` | Operator SMTP URL e.g. `smtp://user:pass@host:587` (lettre `from_url`) | `.env` / Compose |
| [ ] | `OXIDEAN_MAIL_FROM` | From address e.g. `Oxidean <noreply@example.com>` (default: `Oxidean <noreply@localhost>`) | `.env` / Compose |
| [ ] | `OXIDEAN_RESEND_API_KEY` | Resend Dashboard → API Keys | `.env` / Compose |
| [ ] | `OXIDEAN_ADMIN_EMAIL` | Optional first-admin email when `users` is empty (before Phase 6 wizard) | `.env` / Compose |
| [ ] | `OXIDEAN_ADMIN_PASSWORD` | Optional first-admin password (paired with `OXIDEAN_ADMIN_EMAIL`) | `.env` / Compose |
| [ ] | `WORKOS_API_KEY` | WorkOS Dashboard → API Keys | `.env` / Compose |
| [ ] | `WORKOS_CLIENT_ID` | WorkOS Dashboard → AuthKit | `.env` / Compose |
| [ ] | `OXIDEAN_OIDC_ISSUER` | IdP issuer URL (`https://…` only; no localhost/link-local) | `.env` / Compose |
| [ ] | `OXIDEAN_OIDC_CLIENT_ID` | OIDC client id | `.env` / Compose |
| [ ] | `OXIDEAN_OIDC_CLIENT_SECRET` | OIDC client secret | `.env` / Compose |
| [ ] | `OXIDEAN_PUBLIC_ORIGIN` | Browser-facing origin for OAuth redirect_uri (e.g. `http://localhost:3000`) | `.env` / Compose |

## Account Setup

- [ ] **Create Resend account** (optional — only if using Resend)
  - URL: https://resend.com
  - Skip if: Using SMTP only, or local log-sink development

- [ ] **Have SMTP credentials** (optional — only if using SMTP)
  - Skip if: Using Resend only, or local log-sink development

- [ ] **Optional admin seed** (dev / self-host bootstrap)
  - Set both `OXIDEAN_ADMIN_EMAIL` and `OXIDEAN_ADMIN_PASSWORD` only when you want a single `is_admin` user created on first boot with an empty users table
  - Username becomes `admin` (or `admin1` if taken); Phase 6 owns the interactive wizard

- [ ] **WorkOS AuthKit** (optional — when `provider_mode=workos`)
  - URL: https://dashboard.workos.com
  - Register redirect URI: `{OXIDEAN_PUBLIC_ORIGIN}/api/auth/workos/callback`
  - Skip if: Using local or OIDC mode only

- [ ] **Generic OIDC IdP** (optional — when `provider_mode=oidc`)
  - Register redirect URI: `{OXIDEAN_PUBLIC_ORIGIN}/api/auth/oidc/callback`
  - Skip if: Using local or WorkOS mode only

## Dashboard Configuration

- [ ] **Verify Resend From domain** (if using Resend)
  - Location: Resend Dashboard → Domains
  - Set to: Domain matching `OXIDEAN_MAIL_FROM`
  - Notes: Unverified From addresses are rejected by Resend

- [ ] **Set instance auth provider mode** (admin UI in plan 04-06 / ENV bootstrap)
  - Modes: `local` | `workos` | `oidc` (exclusive)

## Verification

After completing setup, verify with:

```bash
# Default (no provider): LogSink — no network
unset OXIDEAN_SMTP_URL OXIDEAN_RESEND_API_KEY
cargo test -p oxidean-api --lib email::

# Auth signup/session (local mode + welcome email path)
cargo test -p oxidean-api --test auth_signup --test auth_session

# WorkOS / OIDC unit tests (no live keys required)
cargo test -p oxidean-api --lib auth::workos
cargo test -p oxidean-api --lib auth::oidc

# With Resend key set at runtime, signup welcome hits Resend.
# With OXIDEAN_SMTP_URL set, SmtpSender is selected instead.
```

Expected results:
- Unconfigured env → LogSink only (`oxidean.mail` tracing target)
- `OXIDEAN_RESEND_API_KEY` set → Resend preferred over SMTP
- `OXIDEAN_SMTP_URL` set (no Resend key) → lettre SMTP path
- Both `OXIDEAN_ADMIN_*` set + empty users → one admin user logged once at boot
- Missing WorkOS/OIDC keys → `/api/auth/*/start` returns `auth.not_configured` (503)
- Invalid OIDC issuer (`http://`, link-local) rejected by `validate_issuer_url`

---

**Once all items complete:** Mark status as "Complete" at top of file.
