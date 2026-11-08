# Phase 5: Cloud Verify & Reset - Context

**Gathered:** 2026-09-10
**Status:** Ready for planning

<domain>
## Phase Boundary

Open signup stays invite-free; email verification gates **privileged** actions (not login); password reset works when mail can be sent. Same verify/reset policy everywhere — **no** cloud vs self-host detection in this phase.

**Requirements:** AUTH-04, AUTH-05, AUTH-12

**Success criteria (from ROADMAP):**
1. Signup requires no invite (AUTH-05) — remains open as shipped in Phase 4; no invite gate introduced here
2. Unverified users cannot perform privileged actions (at minimum: create repository) until email is verified — Phase 5 ships the gate helper + CI-proving RPC; `repo.create` consumes it in Phase 7
3. When an email provider is configured, user can reset password via email (link + code)

**Out of scope (later phases):**
- Self-host first-admin env/wizard (Phase 6 — AUTH-06, AUTH-07)
- Real `repo.create` and git browse (Phase 7) — only mark as first privileged consumer + disable CTAs when present
- Git HTTPS / PATs (Phase 8+)
- Consumer social OAuth / Better Auth (rejected)

**UI hint:** yes — `/verify`, `/reset-password` (and forgot entry), verify banner, OTP inputs.

</domain>

<decisions>
## Implementation Decisions

### A — Gate policy
- **D-01:** No cloud/self-host detection for verify — **same policy everywhere**
- **D-02:** Verify gate **always** applies (including log-sink / Mailpit)
- **D-03:** Local password accounts **must** verify; WorkOS/OIDC **IdP-trust** → mark verified on SSO success when the IdP asserts verified email
- **D-04:** Env/wizard seeded admin is **auto-verified**
- **D-05:** Email change **clears** verified and sends a **new** verify email
- **D-06:** Unverified users **may log in**; only privileged actions are gated
- **D-07:** Privileged API denial uses stable code **`auth.email_unverified`** (+ short message) — **Reversibility:** costly — clients and e2e will key off this code
- **D-08:** AUTH-05 — signup stays **open / no invite** (Phase 4 behavior retained; no invite system in Phase 5)

### B — Privileged-action gate
- **D-09:** Phase 5 ships a **require-verified** helper and marks future **`repo.create`** as the first product consumer
- **D-10:** Ship a **dev/test-only privileged RPC** so CI can prove the gate without Phase 7 repos
- **D-11:** **Persistent verify banner** on signed-in chrome with **resend**
- **D-12:** Future privileged CTAs (e.g. create repo): **visible but disabled** until verified
- **D-13:** **`auth.me` exposes `email_verified` boolean** for banner and disabled CTAs — **Reversibility:** costly — public RPC contract

### C — Verification experience
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

### D — Password reset
- **D-24:** Same channel model as verify: magic link + 8-digit code in one email; **`/reset-password`** accepts either; **`input-otp`** for code (Octane React-compat)
- **D-25:** Reuse verify TTL/rate limits: **30 min** shared; resend replaces; **~1/min** and **~5/hour**
- **D-26:** **Local-password accounts only**; SSO-only users get no reset (point to IdP)
- **D-27:** Reset flow is **logged-out**; success sets password, **revokes other sessions**, **signs in** on this device
- **D-28:** Forgot-password request **always** returns the same success copy (**anti-enumeration**) — never reveal whether the email matched

### Claude's Discretion
- Exact magic-link token length/encoding and storage (hash-at-rest vs opaque id)
- Exact OTP auto-submit vs explicit submit button after `input-otp` completes
- Exact anti-enumeration and email template copy wording
- Password strength rules on reset (prefer matching Phase 4 signup rules)
- Token table / column naming and cleanup job cadence
- Whether “email provider configured” for AUTH-12 includes log-sink (likely yes for local/dev parity with D-02) vs requiring SMTP/Resend only in production docs

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product & requirements
- `.planning/PROJECT.md` — dual-mode cloud + self-host; email/password first; log/SMTP/Resend
- `.planning/REQUIREMENTS.md` — AUTH-04, AUTH-05, AUTH-12 (this phase); AUTH-06/07 → Phase 6; GIT-01 privileged create → Phase 7
- `.planning/ROADMAP.md` — Phase 5 goal and success criteria; Phase 6/7 follow-ons

### Prior phase decisions
- `.planning/phases/04-auth-sessions-email/04-CONTEXT.md` — Rust-native auth; sessions; email adapters; **D-21** deferred verify/reset to Phase 5; `returnTo` / `/dashboard`; logout_all
- `.planning/phases/04-auth-sessions-email/04-UI-SPEC.md` — auth page chrome; no verify/reset UI yet (to be extended)
- `.planning/phases/03-brand-shell-theme/03-CONTEXT.md` — ShadCN/Base UI chrome; SW must not cache `/api/*`

### External
- https://www.npmjs.com/package/input-otp — OTP input for `/verify` and `/reset-password` (via Octane React-compat as needed)

No phase-local SPEC.md — decisions above are the implementation lock.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/oxidean-api/src/email/` — `EmailSender` + log/SMTP/Resend adapters; extend with verify/reset templates (welcome already exercises send path)
- `crates/oxidean-db/src/users.rs` — `email_verified_at` column already exists; wire gating + `auth.me`
- `packages/api-client` + `auth.me` / signup / login / `logout_all` — extend procedures; expose `email_verified`
- `apps/web` routes: `login.tsx`, `signup.tsx`, `dashboard.tsx`, signed-in chrome — add `/verify`, `/reset-password`, banner, forgot-password entry
- ShadCN/Base UI form primitives (`Input`, `Label`, `Button`) — compose with `input-otp`

### Established Patterns
- Dotted RPC namespaces (`auth.*`, `user.*`, `admin.auth.*`) — add verify/reset procedures alongside
- Stable `auth.*` error codes (e.g. `auth.unauthenticated`) — add `auth.email_unverified`
- Cookie sessions + `logout_all` — reuse revoke-other-sessions on successful password reset
- Provider modes `local` | `workos` | `oidc` — IdP-trust verify only when claims support it

### Integration Points
- `UserPublic` today has no `email_verified` — add boolean per D-13
- Privileged gate helper with no `repo.create` yet — test RPC + disabled CTA placeholders until Phase 7
- Signup already open — AUTH-05 is confirm/retain, not a new invite product
- Email change path (if any profile email edit exists or is added) must clear verified per D-05

</code_context>

<specifics>
## Specific Ideas

- Use **`input-otp`** (npm) for code entry on verify and reset, run through **Octane React-compat** within the Octane/TanStack Start app as needed
- Prefer parity between verify and reset channel/TTL/rate-limit behavior unless a decision says otherwise (reset reuses verify policy)

</specifics>

<deferred>
## Deferred Ideas

- Real **`repo.create`** privileged enforcement + enabled CTA — Phase 7 (Phase 5 only helper + test RPC + disabled CTA pattern)
- Self-host **admin bootstrap** polish — Phase 6 (AUTH-06/07); seeded admin auto-verify still applies when that admin exists
- None other — discussion stayed within phase scope

</deferred>

---

*Phase: 5-Cloud Verify & Reset*
*Context gathered: 2026-09-10*
