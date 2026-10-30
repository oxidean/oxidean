# Phase 5: Cloud Verify & Reset - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-10
**Phase:** 5-Cloud Verify & Reset
**Areas discussed:** gate-policy, privileged-gate, verification-ux, password-reset

---

## Gate policy

| Option | Description | Selected |
|--------|-------------|----------|
| Same policy everywhere | No cloud/self-host detection | ✓ |
| Cloud-only verify | Self-host skips gate | |

**User's choice:** Same policy everywhere; gate always on (incl. log-sink); local must verify; SSO IdP-trust when asserted; env/wizard admin auto-verified; email change clears verified; unverified may log in; API code `auth.email_unverified`
**Notes:** Areas selected as “all” four gray areas at discuss start.

---

## Privileged-action gate

| Option | Description | Selected |
|--------|-------------|----------|
| Helper + future repo.create + test RPC | Phase 5 gate without real repos | ✓ |
| Full repo.create in Phase 5 | Scope creep into Phase 7 | |

**User's choice:** require-verified helper; mark `repo.create` first consumer; dev/test privileged RPC; persistent banner + resend; privileged CTAs visible but disabled; `auth.me` includes `email_verified`
**Notes:** —

---

## Verification experience — channels & route

| Option | Description | Selected |
|--------|-------------|----------|
| Magic link AND code | Both work | ✓ |
| Link only | | |
| Code only | | |

| Option | Description | Selected |
|--------|-------------|----------|
| `/verify` token or form | Success → dashboard/returnTo | ✓ |
| Separate routes | | |

| Option | Description | Selected |
|--------|-------------|----------|
| `input-otp` via Octane React-compat | User specified npm package | ✓ |

**User's choice:** Link + code; single `/verify`; `input-otp` through React-compat
**Notes:** User: “make sure to use https://www.npmjs.com/package/input-otp … react compat within octane”

---

## Verification experience — TTL & shape

| Option | Description | Selected |
|--------|-------------|----------|
| ~24h shared | | |
| Shorter 15–60 min shared | Same lifespan for link and code | ✓ (then 30 min) |
| 7d link + short code | | |

| Option | Description | Selected |
|--------|-------------|----------|
| 30 min + resend replace + ~1/min ~5/hour | | ✓ |
| 15 min same rules | | |
| 60 min same rules | | |

| Option | Description | Selected |
|--------|-------------|----------|
| 6-digit same-family secret | | |
| 8-digit + longer separate link token | | ✓ |

| Option | Description | Selected |
|--------|-------------|----------|
| Must be signed in as target | Logged-out link → sign-in → complete | ✓ |
| Link works logged out | | |

| Option | Description | Selected |
|--------|-------------|----------|
| One email with link + code | | ✓ |

**User's choice:** 30 min shared TTL; resend replaces; soft rate limits; 8-digit OTP + separate long link secret; signed-in consume; one email both channels
**Notes:** User insisted magic link and code share the same lifespan; link token is a longer separate secret (not the OTP itself).

---

## Password reset

| Option | Description | Selected |
|--------|-------------|----------|
| Same as verify (link + code, `/reset-password`, input-otp) | | ✓ |
| Link only (AUTH-12 wording) | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Reuse verify 30 min / rate limits | | ✓ |

| Option | Description | Selected |
|--------|-------------|----------|
| Local-password only; logged-out; revoke others + sign in | | ✓ |
| Success → login page only | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Anti-enumeration always-same success copy | | ✓ |
| Reveal unknown email | | |

**User's choice:** Parity with verify channels/TTL; local-password only; logged-out reset; revoke other sessions + sign in; anti-enumeration
**Notes:** —

---

## Claude's Discretion

- Token encoding/storage details, OTP auto-submit vs button, exact copy, password strength on reset, token schema/cleanup, whether log-sink counts as “email provider configured” for reset docs

## Deferred Ideas

- Real `repo.create` enforcement — Phase 7
- Self-host admin bootstrap — Phase 6
