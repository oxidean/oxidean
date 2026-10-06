---
phase: "06"
slug: "self-host-admin-bootstrap"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
block_on: high
created: "2026-09-12"
---

# Phase 06 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Operator ENV → boot seed | Both `OXIDEAN_ADMIN_*` create first sys-admin | Email/password secrets; seeded `system-administrator` |
| Unauthenticated client → bootstrap wizard | First-admin create when ENV seed absent | Wizard credentials + `allow_signup` |
| Authenticated ENV admin → confirm credentials | Forced leave of default username | New username/password; session |
| Anonymous → `auth.signup` / chrome / `/signup` | Closed signup must not admit users | Public `allow_signup` policy |
| Sys-admin → `admin.auth.update_settings` | Only admins flip signup policy | `allow_signup` boolean |
| Browser Cookie → SSR → API | Session forwarded for gates | Cookie header (never logged) |
| SSR/root gate → route tree | UX redirect only; API allowlist remains authority | `needs_setup` / `must_change_credentials` |
| Seed failure → process lifecycle | Fail-closed prevents empty/open serve | Process exit before bind |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-06-00 | Elevation of privilege | Wave 0 stubs only | low | accept | Stubs assert secure outcomes; no production code in 06-00 | closed |
| T-06-01 | Elevation of privilege | `allow_signup` default | high | mitigate | Column DEFAULT false; provider_config/signup fail closed on unset/error | closed |
| T-06-02 | Elevation of privilege | `must_change_credentials` | high | mitigate | Column DEFAULT false; only ENV seed sets true | closed |
| T-06-03 | Elevation of privilege | ENV default credentials | high | mitigate | Seed sets `must_change_credentials=true`; confirm rejects `system-administrator` (case-insensitive) | closed |
| T-06-04 | Elevation of privilege | Partial ENV | high | mitigate | Both non-empty `OXIDEAN_ADMIN_*` required; else no seed | closed |
| T-06-05 | Denial of service / Tampering | Seed failure with both ENV set | high | mitigate | `main` exits 1 on seed Err; no wizard fallback | closed |
| T-06-06 | Elevation of privilege | RPC during `needs_setup` | high | mitigate | Central allowlist in `rpc::dispatch` (`bootstrap_status` / `bootstrap_setup` / `system.health`) | closed |
| T-06-07 | Elevation of privilege | Race two wizards | high | mitigate | Re-check `count_users` before create; second gets `setup_unavailable` | closed |
| T-06-08 | Spoofing | SSO during setup | high | mitigate | `reject_if_setup_required` on WorkOS/OIDC starts | closed |
| T-06-09 | Elevation of privilege | `auth.signup` when closed | high | mitigate | Server-side `allow_signup` check → `auth.signup_closed`; default false | closed |
| T-06-10 | Elevation of privilege | Admin settings | medium | mitigate | `require_admin` before get/update; `allow_signup` only on that path | closed |
| T-06-11 | Information disclosure | SSR Cookie forward | high | mitigate | Forward Cookie only to configured API origin; never log cookies; `credentials: include` | closed |
| T-06-12 | Spoofing | Open redirect after gates | medium | mitigate | `safeReturnTo` only — no raw `returnTo` | closed |
| T-06-13 | Elevation of privilege | Closed signup UI bypass | medium | mitigate | Chrome omit until `allow_signup===true`; SSR `/signup` `notFound()`; API remains authority | closed |
| T-06-14 | Spoofing | `returnTo` after confirm | medium | mitigate | `safeReturnTo` on credentials/login success | closed |
| T-06-15 | Elevation of privilege | App UI while needs_setup / must_change | high | mitigate | Shared root `beforeLoad` + `resolveAppAccessRedirect` before paint | closed |
| T-06-15† | Information disclosure | `.env.example` / docs | medium | mitigate | Comment placeholders only; fail-closed boot noted; never real credentials | closed |
| T-06-16 | Information disclosure | `/dashboard` soft page | medium | mitigate | `notFound()` — no soft-redirect content | closed |
| T-06-SC | Tampering | cargo/npm installs | high | mitigate | No new packages this phase; Switch hand-authored from existing `@octanejs/base-ui` | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `workflow.security_block_on` (`high`) count toward `threats_open`*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

† Plan `06-07-PLAN.md` reused threat ID `T-06-15` for the docs/ENV placeholder threat (also used in `06-05-PLAN.md` for the SSR access gate). Both mitigations verified; listed separately for audit clarity.

### Evidence (ASVS L1)

| Threat ID | Evidence |
|-----------|----------|
| T-06-00 | Accepted Risks Log (wave-0 stubs only) |
| T-06-01 | `crates/oxidean-db/migrations/*/0006_bootstrap_flags.sql` DEFAULT false/0; `local.rs` provider_config/signup fail-closed |
| T-06-02 | `0006_bootstrap_flags.sql` `must_change_credentials` DEFAULT false; `seed.rs:52` sets true only on ENV seed |
| T-06-03 | `seed.rs:52` + `bootstrap.rs:232-236` reject default username |
| T-06-04 | `seed.rs:18-25` early `Ok(())` unless both non-empty |
| T-06-05 | `main.rs:66-68` `process::exit(1)` on seed Err |
| T-06-06 | `rpc.rs:55-69` D-11 allowlist |
| T-06-07 | `bootstrap.rs:130-135` re-check `count_users` |
| T-06-08 | `auth_callbacks.rs:60-68` + callsites on WorkOS/OIDC start |
| T-06-09 | `local.rs:141-151` `auth.signup_closed` |
| T-06-10 | `admin.rs:136-146` `require_admin` before settings |
| T-06-11 | `ssr-auth.ts:5-28` configured origin + Cookie forward; comment forbids logging |
| T-06-12 | `return-to.ts:5-22` `safeReturnTo`; used from login/signup/credentials |
| T-06-13 | `chrome.tsrx:120-126` fail-closed; `signup.tsrx` `notFound()` when false |
| T-06-14 | `setup.credentials.tsrx` / `login.tsrx` `safeReturnTo` |
| T-06-15 | `__root.tsrx:34-63` + `ssr-auth.ts:71-90` `resolveAppAccessRedirect` |
| T-06-15† | `.env.example:32-39` commented placeholders; `docs/CONFIGURATION.md` fail-closed note |
| T-06-16 | `dashboard.tsrx:8-9` `throw notFound()` |
| T-06-SC | Summaries claim no new packages; `switch.tsrx` imports existing `@octanejs/base-ui/switch` |

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-06-00 | T-06-00 | Wave 0 (06-00) is RED-only stubs asserting secure outcomes; no production code change in that plan. Later plans implement mitigations. | phase-06 secure audit | 2026-09-12 |

*Accepted risks do not resurface in future audit runs.*

---

## Unregistered Flags

None — SUMMARY.md `## Threat Flags` entries all map to plan threat IDs (T-06-01…T-06-16 / T-06-SC).

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open (blocking) | Open (non-blocking) | Run By |
|------------|---------------|--------|-----------------|---------------------|--------|
| 2026-09-12 | 19 | 19 | 0 | 0 | gsd-security-auditor (secure-phase state B) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-09-12

**Verdict:** SECURED — ASVS L1; `block_on: high`; all declared mitigations present in code; no blocking open threats.
