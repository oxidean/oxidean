---
phase: 4
slug: auth-sessions-email
status: verified
threats_open: 0
asvs_level: 1
created: 2026-09-10
verified: 2026-09-10
register_authored_at_plan_time: true
---

# Phase 4 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Browser → `/api/rpc` `auth.*` / `user.*` / `admin.auth.*` | Untrusted credentials, cookies, profile edits | Passwords (ephemeral), session cookie, PII |
| Browser ↔ cookie `oxidean_session` | Opaque session id on credentialed requests | Raw token (browser) ↔ SHA-256 hash (DB) |
| API → `oxidean-db` → SQL engines | Auth PII + password PHC + session token hashes | Users, sessions, identities, settings |
| Browser multipart → `/api/user/avatar` | Untrusted image bytes | ≤2 MiB JPEG/PNG/WebP → `{user_id}.webp` |
| Public `GET /uploads/avatars/*` | Public read of avatar bytes | Re-encoded WebP only |
| API → SMTP / Resend | Outbound mail + ENV credentials | Message content; secrets never logged |
| Browser → WorkOS / OIDC IdP → `/api/auth/*/callback` | Auth codes, state, PKCE | External IdP tokens → Oxidean session mint |
| Admin OIDC issuer URL → discovery HTTP | SSRF risk if issuer attacker-controlled | HTTPS allowlist only |
| ENV `OXIDEAN_ADMIN_*` → boot seed | Privilege creation when DB empty | Admin email/password once |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-04-01 | Information disclosure | `password_hash` / `token_hash` columns | high | mitigate | Hash-only TEXT columns; Argon2id PHC + SHA-256 session; never store raw cookie token | closed |
| T-04-02 | Tampering | username/email uniqueness | medium | mitigate | UNIQUE on email/username all dialects; email lowercased before insert | closed |
| T-04-03 | Elevation of privilege | `users.is_admin` | high | mitigate | DEFAULT false; only empty-DB `OXIDEAN_ADMIN_*` seed / Phase 6; no public insert | closed |
| T-04-04 | Information disclosure | email logs / errors | high | mitigate | `oxidean.mail` logs to/subject/body; SMTP/Resend errors redacted | closed |
| T-04-05 | Tampering | email header injection | medium | mitigate | lettre typed `Mailbox`; Resend structured JSON fields | closed |
| T-04-06 | Spoofing | From address | low | accept | Operator configures From via ENV; no SPF/DKIM in Phase 4 | closed |
| T-04-07 | Elevation of privilege | session cookie | high | mitigate | HttpOnly; SameSite=Lax; SHA-256 in DB; CSPRNG token | closed |
| T-04-08 | Elevation of privilege | session fixation | high | mitigate | `issue_session` / SSO `sessions.create` mint new id on login/signup | closed |
| T-04-09 | Information disclosure | password handling | high | mitigate | Argon2id PHC; never log password/hash | closed |
| T-04-10 | Tampering | cookie Secure flag | medium | mitigate | Secure unless `OXIDEAN_ENV` ∈ {development,dev}; Path=/; no Domain | closed |
| T-04-11 | Spoofing | auth.login | medium | mitigate | Generic `auth.invalid_credentials` (no user enumeration) | closed |
| T-04-12 | Elevation of privilege | auth.* when mode≠local | high | mitigate | Signup/login rejected unless `ProviderMode::Local` | closed |
| T-04-13 | Elevation of privilege | OXIDEAN_ADMIN_* seed | high | mitigate | Both env set AND `count_users()==0` only | closed |
| T-04-14 | Information disclosure | signup duplicate | medium | mitigate | Stable `auth.taken`; combined UI message | closed |
| T-04-15 | Spoofing | OIDC/WorkOS callback | high | mitigate | Server pending store validates state (+ OIDC nonce); PKCE | closed |
| T-04-16 | Spoofing / SSRF | OIDC issuer discovery | high | mitigate | HTTPS only; reject localhost/loopback/link-local/10/8/metadata; residual if admin compromised accepted at L1 | closed |
| T-04-17 | Elevation of privilege | WorkOS sealed session | high | mitigate | Always mint Oxidean `sessions` row — never WorkOS sealed cookie as app session | closed |
| T-04-18 | Information disclosure | IdP errors | medium | mitigate | Log server-side; redirect `/login?error=sso` | closed |
| T-04-19 | Tampering | avatar upload path | high | mitigate | Ignore client filename; `{user_id}.webp` only; reject `..`; 2 MiB; re-encode | closed |
| T-04-20 | Denial of service | image decode | medium | mitigate | 2 MiB + allowlisted content-types jpeg/png/webp | closed |
| T-04-21 | Elevation of privilege | admin.auth.* | high | mitigate | Session + `is_admin`; else `admin.forbidden` | closed |
| T-04-22 | Information disclosure | auth settings RPC | high | mitigate | Boolean “configured via ENV” only — no secret values in JSON | closed |
| T-04-23 | Spoofing | returnTo redirect | medium | mitigate | `safeReturnTo`: relative `/` only, reject `//` and schemes | closed |
| T-04-24 | Injection XSS | error banners | medium | mitigate | Text nodes only; no `dangerouslySetInnerHTML` on auth pages | closed |
| T-04-25 | Elevation of privilege | /admin/auth UI | high | mitigate | UI forbidden copy; API `admin.forbidden` is the boundary | closed |
| T-04-26 | Information disclosure | admin form | medium | mitigate | ENV badges only; no password-style secret inputs | closed |

*Status: open · closed · open — below high threshold (non-blocking)*  
*Block on: **high** (`workflow.security_block_on`). `threats_open` counts open threats at or above that severity.*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-04-01 | T-04-06 | SPF/DKIM/DMARC for From-domain authenticity is an operator/mail-infra concern; Phase 4 ships adapters only | plan disposition (accept) | 2026-09-10 |
| AR-04-02 | T-04-16 (residual) | If a compromised admin sets a malicious HTTPS issuer still passing SSRF filters, discovery could be abused; mitigated by admin-only settings + https/private-host denylist at ASVS L1 | plan disposition (residual accept) | 2026-09-10 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-09-10 | 26 | 26 | 0 | gsd-secure-phase (ASVS L1 grep-depth; register from PLAN `<threat_model>`) |

### Evidence notes (L1)

- Schema/hash: `migrations/*/0002_auth.sql`, `auth/password.rs`, `auth/session.rs`
- Email: `email/{log_sink,smtp,resend,mod}.rs` + redact helpers
- Local auth: `auth/local.rs` (`issue_session`, mode gate, `auth.taken` / `invalid_credentials`)
- Admin seed: `main.rs` empty-table guard
- SSO: `auth/oidc.rs` issuer validation + PKCE/nonce; `routes/auth_callbacks.rs` `mint_session_and_redirect` (T-04-17 comment)
- Avatar: `routes/avatar.rs` path/type/size limits + traversal tests
- Admin RPC/UI: `auth/admin.rs` public DTO; `routes/admin/auth.tsx` ENV badges / no password inputs
- Redirect/XSS: `lib/return-to.ts`; no `dangerouslySetInnerHTML` on login/signup

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed

**Approval:** verified 2026-09-10 — no blocking (high+) open threats
