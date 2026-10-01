# Administration: users, sessions & invites

This guide covers what sys-admins can do from **Admin → Users** (`/admin/users`) and how invites work at every scope. Everything here is also available over RPC — see [API.md](../API.md).

## Roles at a glance

| Role | Scope |
|------|-------|
| `sys-admin` | Whole instance: users, instance invites, auth settings, factory reset |
| Org Owner / Admin | Org members and org invites |
| Repo Admin | Repository collaborators and repo invites |

Org and repo admins manage invites from **org Settings → Members** and **repo Settings → Collaborators** — the same invite controls apply at all three scopes.

## User administration (sys-admin)

`/admin/users` lists every account with search and pagination. Each row's action menu covers role changes, ban/unban, session revocation, and delete, plus a **details** view.

### Changing roles

- `user` ↔ `sys-admin` only.
- You cannot demote your own sys-admin role.
- Demoting the **last** sys-admin is refused — promote someone else first.

### Banning

Banning is **soft and reversible**: it sets `banned_at` and revokes all sessions immediately. While banned, an account is rejected at every authentication boundary — web sessions, PAT/Smart HTTP, LFS, SSH keys, and the packages registry — and its public profile reads as "not found."

- Banned accounts cannot sign in, but their data (repos, comments) is untouched.
- Unbanning clears the flag; previously issued PATs work again.
- You cannot ban yourself, and you must demote another sys-admin before banning them.

### Deleting

Deleting is **hard and irreversible**:

- Type the username to confirm — this is deliberate friction.
- Removes the account's personal repositories and any organizations where it is the **sole owner**.
- If a sole-owned org still has other members, deletion is refused unless you opt in again (`delete_orgs`) — other members' data is never removed silently.
- DB rows are deleted before repo files are wiped from disk, so a failed wipe cannot leave ghost repos in the UI.
- You cannot delete yourself or the last sys-admin.

### Inspecting sessions

The details view lists each of a user's sessions: created / last-seen / expiry timestamps, remember-me, **IP address**, and **user-agent**.

- IP and user-agent are *last-known* values recorded when the session was created and refreshed on use.
- The IP comes from the rightmost `X-Forwarded-For` hop — accurate only when the API sits behind a proxy that sanitizes forwarded headers (the same caveat as the auth rate limiter).
- **Revoke sessions** force-logs the user out on every device. Session tokens themselves are never exposed — only SHA-256 hashes exist at rest.

### Inspecting activity

The details view also shows a merged activity feed, newest first, from two sources:

- **Audit** — durable `audit_events`: sign-ins, signups, logouts, admin actions (role changes, bans, deletes, session revocations), and invite lifecycle events, each with timestamp, event type, target, detail, IP, and user-agent.
- **Repository** — `repository_activity`: pushes, merges, and branch events authored by the user, with repo, ref, and commit counts.

Filter by source (audit / repository) or by event type. Audit rows survive user deletion (the actor link is nulled but the username snapshot remains), so historical accountability is preserved. Factory reset wipes the audit log along with everything else.

## Invites

Every scope supports two invite styles. Both produce a URL under `/invites/{token}`; tokens are 32-byte secrets stored as SHA-256 — plaintext appears only in the outbound email or the one-time `invite_url` response.

### Bulk email invites

Paste a list of addresses — commas, spaces, semicolons, and newlines all work.

- One **email-bound** invite per address: single use, 7-day expiry.
- Results are **per-recipient**: each address reports success or a specific error (already a member, already registered, rate-limited, …) without aborting the batch.
- The bound email is enforced on accept — a link mailed to `a@x.com` cannot be redeemed for `b@y.com`.
- Re-inviting the same address within 60 seconds is rejected; after that, the new invite **revokes** the pending one (the old link dies).
- The invite URL is shown once in the result — copy it if mail delivery isn't configured.

### Shareable link invites

**Create link** produces one unbound URL anyone can use — no email required upfront.

- Optional **expiration** and optional **max seats** (blank = unlimited).
- Anonymous visitors enter an email to accept; if that email already has an account, they're asked to sign in instead.
- Stays valid until revoked, expired, or out of seats — share links deliberately.
- Counts as one invite against the hourly issuance budget (see below).

### Rate limits

Invite issuance is capped per scope: **20 issued per hour** (counted cumulatively — a bulk batch stops when the remaining budget is exhausted, and the rest fail per-recipient), **50 recipients per request**, and the 60-second per-email reissue interval above. These are code constants, not env vars.

### Accepting

`/invites/{token}` renders a preview first — invite kind (instance / organization / repository), bound email if any, expiry, seats remaining — and refuses cleanly when the invite is revoked, expired, or exhausted.

- **Bound email invite:** the accepting account's email must match — either the signed-in user or the new account being created.
- **Link invite:** signed-in users accept directly; anonymous users supply an email (registered emails are sent to sign in).
- Accepting any valid invite can create a verified account **even when `allow_signup` is closed** — invites are the enrollment path on closed instances.

## Auditing the instance

Admins leave their own trail: every `admin.*` mutation, invite create/revoke/accept, and auth event lands in `audit_events` and is visible in the acting user's activity feed — there is no way to act silently. The feed is per-user; there is no global audit listing UI (query `audit_events` directly, or use `admin.users.getActivity` per user).
