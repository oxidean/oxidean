---
phase: "05"
slug: "cloud-verify-reset"
status: verified
threats_open: 0
asvs_level: 1
created: "2026-09-11"
---

# Phase 5 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Browser ↔ API | Session cookie + RPC JSON | Credentials, OTPs/tokens (in transit), profile |
| API ↔ DB | SQL via dialect pool | Hashed tokens/OTPs, `email_verified_at`, password hashes |
| API ↔ Email sender | Outbound verify/reset mail | Magic URL + OTP plaintext only in message body (never DB/logs) |
| API ↔ IdP (WorkOS/OIDC) | SSO start/callback | OAuth codes, IdP `email_verified` claim |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-05-01 | Elevation of privilege | `require_verified` / privileged RPCs | high | mitigate | Deny null `email_verified_at` with `auth.email_unverified` + HTTP 403 | closed |
| T-05-02 | Spoofing | `auth.verify` consume | high | mitigate | Session `user_id` must match token row | closed |
| T-05-03 | Information disclosure | `auth_email_tokens` | high | mitigate | Hash-only `token_hash` / `otp_hash` columns | closed |
| T-05-04 | Elevation of privilege | `auth.dev.privileged_ping` | high | mitigate | Env allowlist only; else `rpc.unknown_procedure` | closed |
| T-05-05 | Denial of service | verify issue/resend | high | mitigate | ≥60s between issues; ≤5/hour; `auth.rate_limited` | closed |
| T-05-06 | Elevation of privilege | OTP attempts | high | mitigate | ~10 fails invalidate; single-use; 30m TTL | closed |
| T-05-07 | Spoofing | magic-link URL | high | mitigate | Links from `OXIDEAN_PUBLIC_ORIGIN` only | closed |
| T-05-08 | Information disclosure | email/log sink | high | mitigate | No plaintext OTP/token in logs; verify mail fail ≠ signup fail | closed |
| T-05-09 | Information disclosure | `auth.request_password_reset` | high | mitigate | Identical success anti-enumeration; mail only local-password | closed |
| T-05-10 | Elevation of privilege | `auth.reset_password` | high | mitigate | Hashed tokens, TTL, rate limits, min password 8 | closed |
| T-05-11 | Elevation of privilege | post-reset sessions | medium | mitigate | `revoke_all` + mint fresh session cookie | closed |
| T-05-12 | Information disclosure | SSO-only reset | medium | mitigate | No mail on request; stable `auth.sso_only` on redeem | closed |
| T-05-13 | Spoofing | IdP `email_verified` claim | high | mitigate | Verified only when provider asserts true | closed |
| T-05-14 | Elevation of privilege | `clear_email_verification` | medium | mitigate | Internal helper only; no public RPC | closed |
| T-05-15 | Information disclosure | magic token in URL | medium | mitigate | No echo into fields; Referrer-Policy on `/verify` | closed |
| T-05-16 | Spoofing | returnTo after verify | medium | mitigate | `safeReturnTo` / sanitize rules | closed |
| T-05-17 | Information disclosure | reset request UI | high | mitigate | Identical anti-enumeration success panel; never not-found | closed |
| T-05-18 | Information disclosure | reset magic URL | medium | mitigate | Referrer-Policy on `/reset-password` | closed |
| T-05-19 | Elevation of privilege | New repository CTA | medium | mitigate | Button always disabled in Phase 5; hints by verify state | closed |
| T-05-20 | Injection XSS | AuthErrorBanner | medium | mitigate | Message as text node only | closed |
| T-05-SC | Tampering | package install | high | mitigate | OTP via `@octanejs/base-ui/otp-field` (no raw `input-otp` dep); plan checkpoints for installs | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `workflow.security_block_on` (high) count toward `threats_open`*
*Disposition: mitigate · accept · transfer*

---

## Accepted Risks Log

No accepted risks.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-09-11 | 21 | 21 | 0 | gsd-secure-phase (ASVS L1) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-09-11
