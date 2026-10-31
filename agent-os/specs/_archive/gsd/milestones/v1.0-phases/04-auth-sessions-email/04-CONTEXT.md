# Phase 4: Auth Sessions & Email - Context

**Gathered:** 2026-09-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can create accounts, stay signed in, manage a basic profile (including avatar upload), and operators can send mail via log sink, SMTP, or Resend. Auth is **Rust-native** in `oxidean-api`, with a uniform provider adapter supporting **`local`**, **`workos`** (official WorkOS Rust SDK), and **generic OIDC** from day one. Provider mode is configurable via **image/ENV defaults** and a **system-admin dashboard** inside Oxidean.

**Requirements:** AUTH-01, AUTH-02, AUTH-03, AUTH-08, AUTH-09, AUTH-10, AUTH-11

**Also in scope (discussion lock):** enterprise SSO paths (WorkOS + generic OIDC), welcome email on local signup, thin `/dashboard` post-login home, instance auth settings admin UI.

**Success criteria (from ROADMAP):**
1. User can sign up with email and password
2. User can log in and remain logged in across browser refresh, and can log out from the web UI
3. User can view and edit their own profile (display name, avatar, bio)
4. With no email provider configured, outbound mail appears in a log/dev sink; with SMTP or Resend configured, mail is sent through that provider

**Out of scope (later phases):**
- Email verification gate + open cloud signup nuances (Phase 5 — AUTH-04, AUTH-05)
- Password reset via email (Phase 5 — AUTH-12)
- Self-host first-admin env/wizard bootstrap polish (Phase 6 — AUTH-06, AUTH-07) — Phase 4 may ship a minimal admin settings surface assuming an admin principal exists or is seedable for dev
- OAuth social login as a product path (PROJECT out of scope for v1) — WorkOS/OIDC are enterprise SSO, not “Sign in with Google” consumer OAuth as the primary cloud path
- Better Auth / `@octanejs/better-auth` (explicitly rejected in favor of Rust-native)

**UI hint:** yes — auth pages, profile, admin auth settings.

</domain>

<decisions>
## Implementation Decisions

### A — Identity (local)
- **D-01:** Signup requires **email + username/handle** (GitHub-shaped)
- **D-02:** Login accepts **either email or username** plus password
- **D-03:** Username rules: **GitHub-like** — 1–39 chars, alphanumeric + hyphen, no leading/trailing hyphen, unique, reserved list (`admin`, `api`, `settings`, …)

### B — Auth architecture & providers
- **D-04:** **Rust-native auth core** in the API (not Better Auth in the web layer)
- **D-05:** Uniform **provider adapter** interface; modes: **`local` | `workos` | `oidc`** (generic OIDC)
- **D-06:** Provider naming: use **`local`** (not “in-house”)
- **D-07:** **WorkOS** integrated via the **official WorkOS Rust SDK** (`workos` crate) — AuthKit/SSO start + callback on the API; after callback, mint the **same Oxidean session** as local
- **D-08:** **Generic OIDC** supported alongside local and WorkOS (Auth0/Keycloak/Okta-class IdPs)
- **D-09:** Provider configuration: **ENV/image bootstrap defaults** + **system-admin dashboard** can **override and persist** instance auth settings
- **D-10:** Rejected: Better Auth + `@octanejs/better-auth` as the session owner (client bindings alone don’t solve Rust forge identity; WorkOS AuthKit is not a Better Auth plugin)

### C — Sessions & cookies
- **D-11:** **HttpOnly secure cookie** + **server-side session store** (CORS credentials already prepared in Phase 1)
- **D-12:** **Shorter default session** (e.g. ~24h / idle-oriented) with **Remember me** extending lifetime (e.g. ~30d) — exact numbers planner/researcher may refine
- **D-13:** Header/account **Log out** = **this device/session only**; profile/settings also offers **Log out all devices** (revoke all sessions)

### D — Auth UI
- **D-14:** Dedicated routes: **`/login`** and **`/signup`**
- **D-15:** Post-auth redirect: **`returnTo` previous page if it wasn’t the homepage; otherwise `/dashboard`** (thin signed-in shell acceptable until a richer home exists)
- **D-16:** Mode-exclusive UI on those routes: **`local`** → Oxidean custom email/password (+ username on signup) forms; **`workos`** → WorkOS AuthKit/SSO flow driven from Rust (redirect/PKCE + callback; Oxidean chrome around CTA); **`oidc`** → standard OIDC redirect/callback with Oxidean chrome
- **D-17:** Enable header **Sign in / Sign up** (and landing Get started as appropriate) to these routes; signed-in chrome shows account menu (profile, log out)

### E — Profile
- **D-18:** Profile fields: **display name**, **username**, **bio**, **avatar file upload** (volume-backed storage — planner chooses path layout; no external object store required in Phase 4)

### F — Email
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

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product & requirements
- `.planning/PROJECT.md` — email/password first; OAuth social out of v1; email providers log/SMTP/Resend; dual-mode cloud + self-host
- `.planning/REQUIREMENTS.md` — AUTH-01, AUTH-02, AUTH-03, AUTH-08, AUTH-09, AUTH-10, AUTH-11 (Phase 4); note AUTH-04/05/12 → Phase 5; AUTH-06/07 → Phase 6
- `.planning/ROADMAP.md` — Phase 4 goal and success criteria; Phase 5/6 auth follow-ons

### Prior phase decisions
- `.planning/phases/01-monorepo-scaffold/01-CONTEXT.md` — CORS + credentials for future session cookies; RPC namespaces; header auth placeholders
- `.planning/phases/02-multi-db-storage/02-CONTEXT.md` — multi-dialect DB via `oxidean-db` (users/sessions must migrate on all dialects)
- `.planning/phases/03-brand-shell-theme/03-CONTEXT.md` — disabled Sign in/Sign up until Auth; ShadCN/Base UI chrome rules; assets-only SW must not cache `/api/*`

### External (implementers should consult)
- WorkOS Rust SDK: https://github.com/workos/workos-rust and https://workos.com/docs/sdks/rust — AuthKit/SSO/PKCE/JWKS/webhooks from Rust
- WorkOS AuthKit docs (server redirect model) — UI widgets are JS-oriented; Rust path is start URL + callback
- OIDC (Authorization Code + PKCE) — generic enterprise IdP path

No phase-local SPEC.md yet — decisions above are the implementation lock.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `apps/web` ShadCN/Base UI `Button` / `Input` / chrome — wire real Sign in/Sign up; build `/login`, `/signup`, profile, admin settings with same kit
- API client package (generated) — prefer `credentials: "include"`; extend with auth/session procedures + TanStack Query helpers
- `crates/oxidean-api` CORS — credentials-ready for session cookies
- `crates/oxidean-db` + migrations — add users/sessions/identities/auth settings across dialects

### Established Patterns
- Dotted RPC namespaces (`system.*`) — prefer `auth.*` / `user.*` style procedures
- No users/sessions tables yet (instance probe tables only from earlier phases)
- Service worker must bypass `/api/` — keep auth responses uncached

### Integration Points
- Header account actions + landing CTAs currently disabled placeholders (Phase 3)
- Post-login `/dashboard` route does not exist yet
- Email: no provider env keys or crates yet — add alongside auth
- Admin dashboard for auth provider config — new surface

</code_context>

<specifics>
## Specific Ideas

- Provider mode labels: **`local`**, **`workos`**, **`oidc`**
- “Better Auth–like” DX on the **Rust** side: one facade, swappable adapters, shared session after any provider
- WorkOS from Rust is intentional (official SDK); AuthKit embed widgets are not the primary integration model
- Welcome email proves mail adapters without implementing verify/reset yet
- Avatar upload is in-phase (volume-backed), not deferred

</specifics>

<deferred>
## Deferred Ideas

- Email verification gate and password reset (Phase 5)
- Self-host admin bootstrap wizard / env-first admin creation polish (Phase 6) — Phase 4 may use a thinner admin seed path
- Consumer social OAuth as primary signup (out of v1 per PROJECT)
- Better Auth / Octane better-auth client as auth owner (rejected)
- Directory Sync / SCIM / advanced WorkOS enterprise features beyond AuthKit/SSO sign-in — backlog unless required for the first WorkOS slice
- SAML without OIDC bridge — prefer OIDC + WorkOS for Phase 4; raw SAML later if needed

</deferred>

---

*Phase: 4-auth-sessions-email*
*Context gathered: 2026-09-09*
