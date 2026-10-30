# Phase 6: Self-Host Admin Bootstrap - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-11
**Phase:** 6-Self-Host Admin Bootstrap
**Areas discussed:** Deployment scope, Empty-instance lock, ENV seed edge cases, Signed-in home SSR

---

## Deployment scope

| Option | Description | Selected |
|--------|-------------|----------|
| Any empty DB | Same bootstrap for cloud and self-host | ✓ |
| Self-host only | Need deployment-mode signal | |
| You decide | | |

**User's choice:** Any empty DB
**Notes:** Later clarified cloud and self-host must be the exact same; no deployment modes.

| Option | Description | Selected |
|--------|-------------|----------|
| Rephrase as empty-instance | Update AUTH-06/07 wording | ✓ |
| Keep “self-host” in docs only | Code mode-agnostic | |
| You decide | | |

**User's choice:** Rephrase as empty-instance

| Option | Description | Selected |
|--------|-------------|----------|
| Avoid deployment mode env | | (via freeform) |
| Add unused mode env | | |
| You decide | | ✓ |

**User's choice:** You decide + “there should not be different deployment modes”
**Notes:** Locked: no OXIDEAN_DEPLOYMENT_MODE; hard rule against cloud/self-host conditionals.

### Post-bootstrap signup (extended)

| Option | Description | Selected |
|--------|-------------|----------|
| Same everywhere open | Leave AUTH-05 always-open | |
| Leave AUTH-05 alone | Phase 6 only AUTH-06/07 | |
| You decide / invite-only default | | ✓ (refined) |

**User's choice:** Bootstrap first; configure `allow_signup` (ENV default); after bootstrap rule applies. Off = hard block (no invites). Default false. ENV seed + wizard both apply `OXIDEAN_ALLOW_SIGNUP`.
**Notes:** Invite codes deferred. Conflicts with Phase 5 D-08 — Phase 6 supersedes for product rule.

---

## Empty-instance lock

| Option | Description | Selected |
|--------|-------------|----------|
| Hard SSR/server gate | `/setup` before paint | ✓ |
| Keep client redirects | Accept flash | |
| You decide | | |

**User's choice:** Hard SSR/server gate

| Option | Description | Selected |
|--------|-------------|----------|
| Everything → `/setup` | | |
| Carve out health/status | | ✓ |
| You decide | | |

**User's choice:** Carve out `/status` + readiness

| Option | Description | Selected |
|--------|-------------|----------|
| Strict RPC | Only bootstrap + health | ✓ |
| Narrow | Block signup/login/SSO only | |
| You decide | | |

**User's choice:** Strict RPC

| Option | Description | Selected |
|--------|-------------|----------|
| Signed-in home `/` | | ✓ |
| Admin auth settings | | |
| You decide | | |

**User's choice:** Land on `/`

---

## ENV seed edge cases

| Option | Description | Selected |
|--------|-------------|----------|
| Fail boot on partial ENV | | |
| Warn + treat as unset / wizard | | ✓ |
| You decide | | |

**User's choice:** If either or both unset → wizard includes sys-admin setup

| Option | Description | Selected |
|--------|-------------|----------|
| Fail boot on seed failure | | ✓ |
| Warn + fall back to wizard | | |
| You decide | | |

**User's choice:** Fail boot

| Option | Description | Selected |
|--------|-------------|----------|
| Keep admin/admin1 | | |
| Derive from email | | |
| You decide | | ✓ |

**User's choice:** Default username `system-administrator`; force email/password change — refined to ENV creates + first-login forced change (ENV-seeded only)

---

## Signed-in home SSR

| Option | Description | Selected |
|--------|-------------|----------|
| Confirm todo (SSR `/`, `/dashboard` 404) | | ✓ |
| Keep `/dashboard` as real URL | | |
| You decide | | |

**User's choice:** Confirm todo

| Option | Description | Selected |
|--------|-------------|----------|
| needs_setup first | | ✓ |
| Valid session first | | |
| You decide | | |

**User's choice:** needs_setup first

| Option | Description | Selected |
|--------|-------------|----------|
| Remove presence hint | | |
| Keep as progressive enhancement | | (via freeform) |
| You decide | | ✓ |

**User's choice:** Keep hint; fine removing if it causes trouble → rely on SSR

| Option | Description | Selected |
|--------|-------------|----------|
| Always `/` after login | | |
| Honor safe returnTo else `/` | | ✓ |
| You decide | | |

**User's choice:** Honor `returnTo` when safe; else `/`

---

## the agent's Discretion

- Presence hint keep vs drop if problematic
- Wizard/`allow_signup` UI details and forced-change screen within AuthShell
- Persistence mechanism for `allow_signup` (settings patterns)

## Deferred Ideas

- Invite codes / invite issuance when `allow_signup` is off

## Folded Todos

- Fix signed-in home flicker on load (SSR session gate)
