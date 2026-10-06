# Phase 4: Auth Sessions & Email - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in `04-CONTEXT.md` — this log preserves alternatives considered.

**Date:** 2026-09-09
**Phase:** 4-auth-sessions-email
**Areas discussed:** Account identity, Sessions & cookies, Auth UI, Profile depth, Email ops, Auth stack (Better Auth vs Rust-native vs WorkOS)

---

## Account identity

| Option | Description | Selected |
|--------|-------------|----------|
| Email + username required | GitHub-shaped signup | ✓ |
| Email-only now | Username later | |
| Email required; username optional | Set handle later | |
| You decide | Default to email+username | |

**User's choice:** Email + username/handle required  
**Notes:** Also requested uniform adapter with WorkOS support; later expanded to local + WorkOS + generic OIDC.

### Login identifier

| Option | Description | Selected |
|--------|-------------|----------|
| Email only | | |
| Username only | | |
| Either email or username | | ✓ |
| You decide | | |

**User's choice:** Either email or username

### Username rules

| Option | Description | Selected |
|--------|-------------|----------|
| GitHub-like | 1–39, alnum+hyphen, reserved list | ✓ |
| Strict lowercase | | |
| Loose | | |
| You decide | | |

**User's choice:** GitHub-like

---

## Auth stack / providers

| Option | Description | Selected |
|--------|-------------|----------|
| Adapter seam only; WorkOS later | | |
| Ship WorkOS behind adapter in Phase 4 | Initially as `local`\|`workos` | ✓ (evolved) |
| Design only for WorkOS | | |
| Better Auth + Octane adapter | Considered; rejected as session owner | |
| Rust-native Better Auth–like + enterprise SSO | | ✓ |
| WorkOS via official Rust SDK | | ✓ |
| Also generic OIDC | | ✓ |
| ENV + system-admin dashboard config | | ✓ |

**User's choice:** Rust-native auth; modes `local` | `workos` | `oidc`; WorkOS via Rust SDK; ENV defaults + admin dashboard overrides  
**Notes:** User asked whether Better Auth would be easier; advised that it helps `local` UI but not WorkOS AuthKit and splits identity from Rust forge. User chose Rust-native including enterprise SSO from the get-go. Confirmed WorkOS runnable from Rust (official SDK). Renamed in-house → **`local`**.

---

## Sessions & cookies

| Option | Description | Selected |
|--------|-------------|----------|
| HttpOnly cookie + server session store | | ✓ |
| Bearer + refresh cookie | | |
| JWT in cookie | | |
| You decide | | |

**User's choice:** HttpOnly secure cookie + server-side store

### Lifetime

| Option | Description | Selected |
|--------|-------------|----------|
| Fixed ~30d | | |
| Shorter default + Remember me | | ✓ |
| Sliding 14d no Remember me | | |
| You decide | | |

**User's choice:** Shorter default + Remember me extends

### Logout

| Option | Description | Selected |
|--------|-------------|----------|
| This device only | | |
| Everywhere only | | |
| This device + Log out all on profile | | ✓ |
| You decide | | |

**User's choice:** Log out here + Log out all devices on profile/settings

---

## Auth UI

| Option | Description | Selected |
|--------|-------------|----------|
| `/login` + `/signup` | | ✓ |
| Modal + deep links | | |
| Single `/auth` tabs | | |
| You decide | | |

**User's choice:** Dedicated `/login` and `/signup`

### Post-login

| Option | Description | Selected |
|--------|-------------|----------|
| returnTo or `/` | | |
| `/dashboard` placeholder | | |
| `/settings/profile` | | |
| Custom | returnTo if not homepage, else `/dashboard` | ✓ |

**User's choice:** `returnTo` previous page if not homepage; else `/dashboard`

### Provider UI

| Option | Description | Selected |
|--------|-------------|----------|
| Mode-exclusive forms vs WorkOS CTA | | |
| Local + optional WorkOS button | | |
| Thin redirect shells for WorkOS | | |
| Custom forms for local; AuthKit in-app for WorkOS | Desired; refined to Rust redirect/PKCE + Oxidean chrome | ✓ |

**User's choice:** Local = custom forms; WorkOS/OIDC = provider flow from Rust with Oxidean chrome on shared routes

---

## Profile depth

| Option | Description | Selected |
|--------|-------------|----------|
| Core + initials avatar | | |
| + avatar URL | | |
| + file upload | | ✓ |
| You decide | | |

**User's choice:** File upload (volume-backed) plus display name / username / bio

---

## Email ops

| Option | Description | Selected |
|--------|-------------|----------|
| Adapters only; no mail yet | | |
| Welcome on local signup + adapters | | ✓ |
| Welcome + SSO notify | | |
| You decide | Recommended #2; user agreed | ✓ |

**User's choice:** Welcome email on local signup; verify/reset deferred to Phase 5  
**Notes:** ENV bootstrap + dashboard override confirmed as the config model.

---

## Claude's Discretion

Session TTL numbers, CSRF, reserved-name list, avatar limits, `/dashboard` content, identity-link schema, Phase 4 admin seed path before Phase 6 wizard, WorkOS AuthKit vs SSO API slice.

## Deferred Ideas

- Phase 5: email verify gate, password reset
- Phase 6: self-host admin bootstrap wizard
- Better Auth as owner (rejected)
- Raw SAML without OIDC/WorkOS
- Advanced WorkOS Directory Sync/SCIM beyond sign-in
