<!-- generated-by: gsd-doc-writer -->
# API

Oxidean exposes a versioned JSON RPC over HTTP and WebSocket, plus a small set of browser-oriented auth and avatar routes. The Rust Axum router lives in `crates/oxidean-api`; shared envelopes and DTOs live in `crates/oxidean-core`. Clients should prefer the generated TypeScript package `@oxidean/api-client`.

<!-- VERIFY: production / public base URL for the API -->

Local defaults: API bind `127.0.0.1:8080` (`API_BIND`), or same-origin via Traefik on `:80` (`/api`, `/uploads`, `/health`).

## Authentication

Session auth uses an **opaque HttpOnly cookie** named `oxidean_session` (not JWTs or API keys).

| Detail | Value |
| --- | --- |
| Cookie name | `oxidean_session` |
| Attributes | `HttpOnly`, `Path=/`, `SameSite=Lax`; `Secure` unless `OXIDEAN_ENV` is `development` or `dev` |
| Idle TTL | 24 hours (sliding on resolve for non-remember sessions) |
| Remember-me TTL | 30 days absolute (`auth.login` with `remember_me: true`) |
| Storage | CSPRNG token in cookie; only SHA-256(token) stored in the DB |

**How to send credentials**

- Browser / generated client: `credentials: "include"` so the cookie is sent same-origin (Vite proxy or Traefik).
- Manual HTTP: include `Cookie: oxidean_session=<token>`.
- Signup, login, and WorkOS/OIDC callbacks attach `Set-Cookie`. Logout / logout-all clear the cookie (`Max-Age=0`).

**Personal access tokens (PATs) are not RPC Bearer credentials (D-01).** Typed `/api/rpc` and `/api/rpc/ws` use the session cookie only. PATs authenticate **Git Smart HTTP** over HTTPS via HTTP Basic (password = token). Do not send `Authorization: Bearer <pat>` to RPC — it is ignored for session resolution.

**Provider modes** (instance setting via `admin.auth.*`): `local` | `workos` | `oidc`. Local signup/login RPC only works when mode is `local`. SSO browser flows require matching mode and ENV secrets (see [CONFIGURATION.md](CONFIGURATION.md)).

**RPC version gate** — every `/api/rpc` and `/api/rpc/ws` request must send:

```http
Oxidean-RPC-Version: 1
```

Missing or mismatched value → error `rpc.version_mismatch` (HTTP 400).

## Endpoints overview

### HTTP routes

| Method | Path | Description | Auth required |
| --- | --- | --- | --- |
| `GET` | `/health` | Liveness: `{"ok":true}` | No |
| `POST` | `/api/rpc` | JSON RPC dispatch | Cookie when procedure needs session |
| `GET` | `/api/rpc/ws` | WebSocket upgrade; same procedures as HTTP | Cookie when procedure needs session |
| `GET` | `/api/auth/workos/start` | Start WorkOS AuthKit (optional `?return_to=`) | No (redirect) |
| `GET` | `/api/auth/workos/callback` | WorkOS code exchange; sets session cookie | No (redirect) |
| `GET` | `/api/auth/oidc/start` | Start OIDC + PKCE (optional `?return_to=`) | No (redirect) |
| `GET` | `/api/auth/oidc/callback` | OIDC code exchange; sets session cookie | No (redirect) |
| `POST` | `/api/user/avatar` | Multipart avatar upload (field `avatar`) | Yes (`oxidean_session`) |
| `DELETE` | `/api/user/avatar` | Remove profile picture | Yes (`oxidean_session`) |
| `GET` | `/uploads/avatars/{file}` | Public WebP avatar bytes (`{user_id}.webp`) | No |
| `GET` | `/{owner}/{repo}.git/info/refs` | Git Smart HTTP discovery (`?service=git-upload-pack` \| `git-receive-pack`) | PAT Basic when required (not session) |
| `POST` | `/{owner}/{repo}.git/git-upload-pack` | Git fetch / clone body | PAT Basic when required (not session) |
| `POST` | `/{owner}/{repo}.git/git-receive-pack` | Git push body | PAT Basic (verified email; write scope) |
| `POST` | `/{owner}/{repo}.git/info/lfs/objects/batch` | Git LFS Batch API | PAT Basic (Read download / Write upload) |
| `PUT` | `/{owner}/{repo}.git/info/lfs/objects/{oid}` | LFS basic transfer upload (streaming) | PAT Basic (Write) |
| `GET` | `/{owner}/{repo}.git/info/lfs/objects/{oid}` | LFS basic transfer download (Range supported) | PAT Basic (Read) |
| `POST` | `/{owner}/{repo}.git/info/lfs/objects/{oid}/verify` | Optional LFS verify | PAT Basic (Write) |
| `POST` | `/api/repos/{owner}/{repo}/releases/{release_id}/assets` | Multipart release asset upload (field `asset`) | Session cookie (Write+) |
| `GET` | `/api/releases/assets/{asset_id}` | Download release asset by opaque id | Session when private (Read+) |
| `GET` | `/v2/` | OCI Distribution Spec API base (Docker/OCI clients) | PAT Bearer / Basic (not session cookie) |
| `*` | `/v2/{owner}/{image}/…` | OCI blobs, manifests, tags | PAT with `package:read` / `package:write` ∩ ACL |
| `*` | `/npm/{owner}/…` | npm registry (publish, packument, tarball, dist-tags) | PAT Basic; cookie ignored |
| `PUT/GET/DELETE` | `/generic/{owner}/{name}/{version}/…` | Generic/raw package files | PAT Basic; cookie ignored |
| `POST` | `/api/actions/register` | Runner registration (registration token) | Registration token only (not session cookie) |
| `POST` | `/api/actions/declare` | Runner label declaration | Bearer runner token |
| `POST` | `/api/actions/fetch_task` | Claim queued workflow job | Bearer runner token |
| `POST` | `/api/actions/update_task` | Job state transition | Bearer runner token |
| `POST` | `/api/actions/update_log` | Append job log chunk | Bearer runner token |
| `POST` | `/api/repos/{owner}/{repo}/mirror/hook` | Inbound push webhook (wake two-way mirror) | Shared secret (HMAC / token headers) |

SSO start routes redirect to the IdP when configured. If WorkOS/OIDC ENV is missing, start returns HTTP 503 with `auth.not_configured`. Failures typically redirect to `/login?error=sso`.

### RPC procedures (`POST /api/rpc`)

| Procedure | Description | Auth |
| --- | --- | --- |
| `system.health` | Status, API crate version, DB ping string | No |
| `system.echo` | Echo `message` (max 8192 bytes) | No |
| `system.db_probe` | Dialect probe / `instances` counter | No |
| `auth.signup` | Local signup; sets session cookie. Rejected with `auth.setup_required` while empty-instance setup is needed; rejected when instance `allow_signup` is false | No (local mode) |
| `auth.login` | Local login; sets session cookie. ENV-seeded admins with `must_change_credentials` are redirected to `/setup/credentials` in the SPA | No (local mode) |
| `auth.logout` | Revoke current session; clear cookie | Session |
| `auth.logout_all` | Revoke all sessions for user; clear cookie | Session |
| `auth.me` | Current user public profile (`must_change_credentials` included) | Session |
| `auth.provider_config` | Public `{ mode, allow_signup }` for UI (fail-closed when unset/error) | No (blocked while `needs_setup`) |
| `auth.bootstrap_status` | `{ needs_setup }` — empty users table and incomplete `OXIDEAN_ADMIN_*` ENV | No |
| `auth.bootstrap_setup` | One-time `/setup` wizard; creates verified `sys-admin` + session; persists `allow_signup` | No (empty instance only) |
| `auth.confirm_admin_credentials` | Forced credential change for ENV-seeded admins (`system-administrator` must be changed; email/password may be kept) | Session (seeded admin) |
| `user.get_profile` | Current user profile | Session |
| `user.update_profile` | Update `display_name`, `username`, `bio` | Session |
| `user.lookup` | Username prefix autocomplete (public fields only; never emails) | Session (rate-limited) |
| `org.create` | Create organization; slug shares username reserved list | Session + verified email |
| `org.get` | Org profile by slug | Anonymous OK; unknown → `org.not_found` |
| `org.listMine` | Orgs the caller belongs to (includes caller `role`) | Session + verified |
| `org.updateSettings` | Update `display_name` / `member_base_permission` | Org Admin+ |
| `org.members.list` | Members (`username`, `role`, ids — no emails) | Org member |
| `org.members.add` / `updateRole` / `remove` | Membership mutations | Org Admin+ (Owner-only for Owner grants) |
| `org.invites.create` / `list` / `revoke` | Bulk email invites — `{ emails }` (max 50) → per-recipient `results[]`; plaintext token only in outbound mail link / returned `invite_url` | Org Admin+ |
| `org.invites.createLink` | Shareable link invite (unbound `email`); optional `expires_at`, `max_uses` seats | Org Admin+ |
| `org.invites.accept` | Redeem invite token; may create verified user under closed signup | Token (optional session) |
| `invites.get` | Anonymous-safe invite preview — `kind`, bound `email`, expiry, seats, `acceptable`/`reason` | Token |
| `invites.accept` | Unified redeem (instance/org/repo); bound-email enforced, link invites take `email` when anonymous; may create verified user | Token (optional session) |
| `repo.listMine` / `repo.listByOwner` | Personal / owner-scoped repo lists (ACL-filtered) | Session |
| `repo.create` / `repo.get` / browse / branch / settings | Forge RPC (Capability ACL) | Session (+ capability) |
| `repo.star` / `repo.unstar` | Idempotent star membership + `star_count` / `viewer_has_starred` on `RepoPublic` | Session + Read (anonymous rejected; private without Read → `repo.not_found`) |
| `user.listStarred` | Caller's starred repos (newest-starred first; Read ACL filter; offset/limit) | Session + verified |
| `user.getPublicProfile` | Public profile by username (`username`, `display_name`, `bio`, `avatar_url` — **never email**) | Anonymous OK; unknown → `user.not_found` |
| `repo.explore` | Public repos sorted by `star_count` desc then `updated_at` desc; optional `q` substring | Anonymous OK |
| `repo.fork` | Fork public readable source (bare copy); sets `forked_from_repo_id` + `fork_network_id`; one active fork per (owner, network) | Session + Read on public source |
| `repo.rename` | Rename repo; moves bare dir; inserts redirect | Repo Admin |
| `repo.transfer` | Transfer ownership (type-confirm `confirmName`); moves bare dir; redirect | Repo Admin |
| `repo.softDelete` | Soft-delete with type-confirm | Repo Admin |
| `repo.collaborators.list` / `add` / `update` / `remove` | Per-repo collaborator grants | Repo Admin |
| `repo.invites.create` / `createLink` / `list` / `revoke` | Bulk collaborator email invites (`{ emails, permission }`) and shareable links (optional `expires_at`, `max_uses`) | Repo Admin |
| `release.list` / `get` / `create` / `update` / `delete` / `deleteAsset` | Tag-based releases + notes; assets via HTTP | Session (+ capability) |
| `admin.users.list` / `updateRole` / `ban` / `unban` / `delete` / `revokeSessions` / `getAccess` | User administration (type-confirm delete; `delete_orgs` opt-in for shared orgs) | Sys-admin |
| `admin.users.listSessions` | Per-user sessions with client metadata (`ip_address`, `user_agent`, `remember_me`, last-seen/expiry) | Sys-admin |
| `admin.users.getActivity` | Merged per-user activity — audit events + repository activity; `source` (`audit`\|`repository`) and `event_type` filters | Sys-admin |
| `admin.invites.create` / `createLink` / `list` / `revoke` | Bulk instance email invites + shareable links (optional `expires_at`, `max_uses`) | Sys-admin |
| `admin.auth.get_settings` | Auth/email settings including `allow_signup` (no secrets) | Admin session |
| `admin.auth.update_settings` | Update provider/email/`allow_signup`; rebuild email sender | Admin session |
| `admin.instance.factory_reset` | Wipe users, orgs, repos + issue domain (DB); optional disk wipe via `scope` | Sys-admin |
| `issue.create` / `get` / `list` / `update` / `close` / `reopen` / `history` / `delete` | Per-repo issues (`#N`); Capability ACL | Session (+ capability) |
| `issue.comments.*` | Comment CRUD + history; author or Write+ moderate-delete | Session (+ capability) |
| `issue.labels.set` / `assignees.set` / `assigneeCandidates` | Assign labels / assignees (Write+; assignees must have Read+) | Session (+ capability) |
| `issue.reactions.toggle` | Toggle emoji reaction on issue or comment | Session (+ Write+) |
| `issue.links.list` / `add` / `remove` | Linked issues/PRs (`pr` preferred; legacy `pr_stub` kept) | Session (+ capability) |
| `notification.list` | Own notifications; filter `unread` (default) \| `all`; offset pagination | Session |
| `notification.unreadCount` | Unread badge count for session user | Session |
| `notification.markRead` | Mark own notification ids read (foreign ids no-op) | Session |
| `notification.markAllRead` | Mark all own unread notifications read | Session |
| `pull.create` / `get` / `list` / `update` / `close` / `reopen` | Pull requests; shared `#N` with issues | Session (+ capability) |
| `pull.files` / `pull.commits` | Diff + commit list for a PR | Session (+ Read+) |
| `pull.comments.list` / `create` / `resolve` | General + line comments; resolve threads | Session (+ capability) |
| `pull.reviews.list` / `submit` / `dismiss` | Approve / request changes / comment; dismiss | Session (+ Write+) |
| `pull.reviewRequests.list` / `add` / `remove` | Optional requested reviewers (UX only) | Session (+ capability) |
| `pull.merge` | Merge / squash / rebase; optional delete head; closing keywords on default branch | Session (+ Write+) |
| `repo.mergeSettings.get` / `update` | Per-repo allow merge/squash/rebase (Admin for update) | Session (+ Admin for update) |
| `label.listForRepo` / `listForOrg` / `create` / `update` / `delete` | Org/repo label definitions (Admin for defs) | Session (+ capability) |
| `pat.createClassic` | Mint classic PAT (`oxidean_pat_…`); one-time plaintext in response. Classic scopes include optional `package:read` / `package:write` (repo scope does **not** imply packages) | Session + verified email |
| `pat.createFineGrained` | Mint fine-grained PAT (`oxidean_fg_…`); optional Packages Read/Write | Session + verified email |
| `pat.list` | List active PATs for the signed-in user (no secrets) | Session |
| `pat.revoke` | Soft-revoke a PAT by `id` | Session |
| `packages.list` | List packages for an owner login or `repository_id` (ACL-filtered) | Session + verified |
| `packages.deleteVersion` | Delete a version; `confirm` must equal `{name}@{version}` | Session + owner Admin capability |
| `packages.adminUsage` | Per-owner storage usage + format/package breakdown | Sys-admin |
| `packages.adminSetQuota` | Set per-owner package quota override (`max_bytes`) | Sys-admin |
| `sshKey.add` | Register an OpenSSH public key; returns fingerprint metadata | Session + verified email |
| `sshKey.list` | List registered SSH public keys (no private keys) | Session |
| `sshKey.revoke` | Hard-delete an SSH public key by `id` | Session |
| `repo.actions.listRuns` / `getRun` / `getJobLog` | Workflow run list, detail, job log text | Session + Read+ |
| `repo.actions.secrets.list` / `put` / `delete` | Repo Actions secrets (names only on list) | Session + Admin |
| `repo.actions.getEnabled` / `setEnabled` | Per-repo Actions enable toggle | Session + Read+ / Admin |
| `repo.mirror.get` / `upsert` / `delete` / `syncNow` | Two-way remote mirror config + enqueue sync | Session + Admin |
| `repo.mirror.generateSshKey` / `rotateWebhookSecret` / `fetchHostKey` | Deploy key, inbound webhook secret, ssh-keyscan | Session + Admin |
| `webhook.create` / `list` / `get` / `update` / `delete` / `deliveries.list` / `deliveries.get` / `ping` / `redeliver` | Outbound repo webhooks; events: `push`, `pull_request`, `issues`, `issue_comment` (incl. PR conversation comments), `ping`, `*` | Session + Admin |
| `repo.commitStatus.create` / `list` | Commit statuses (Phase 13 + Actions publisher) | Session + Write+ / Read+ |
| `admin.actions.createRegistrationToken` | Mint one-time runner registration token | Sys-admin |
| `admin.actions.listRunners` | List registered runners (no secrets) | Sys-admin |

Unknown procedure → `rpc.unknown_procedure` (HTTP 404).

## Request/response formats

### RPC envelope

Request:

```json
{
  "procedure": "system.health",
  "input": {}
}
```

`input` defaults to `{}` if omitted. Success:

```json
{
  "ok": true,
  "data": {
    "status": "ok",
    "version": "…",
    "database": "ok"
  }
}
```

Failure:

```json
{
  "ok": false,
  "error": {
    "code": "auth.unauthenticated",
    "message": "not authenticated"
  }
}
```

Optional `error.data` may appear on some errors.

### Version header + curl example

```bash
curl -sS http://127.0.0.1:8080/api/rpc \
  -H 'content-type: application/json' \
  -H 'Oxidean-RPC-Version: 1' \
  -d '{"procedure":"system.health","input":{}}'
```

Authenticated call (after login/signup returned `Set-Cookie`):

```bash
curl -sS http://127.0.0.1:8080/api/rpc \
  -H 'content-type: application/json' \
  -H 'Oxidean-RPC-Version: 1' \
  -H 'Cookie: oxidean_session=…' \
  -d '{"procedure":"auth.me","input":{}}'
```

### WebSocket `/api/rpc/ws`

1. Upgrade with `Oxidean-RPC-Version: 1` (and optional `Cookie` for session).
2. Send text frames: same JSON as HTTP `RpcRequest`.
3. Receive text frames: same JSON as `RpcResponse`.
4. Invalid JSON frame → `rpc.bad_input`. Cookie `Set-Cookie` is HTTP-only; WS handlers do not attach cookies on responses.

### Local auth inputs

`auth.signup`:

```json
{ "email": "user@example.com", "username": "alice", "password": "at-least-8-chars" }
```

`auth.login`:

```json
{ "identifier": "alice", "password": "…", "remember_me": false }
```

`identifier` is email or username. Password minimum length: **8**. Response data is `UserPublic` (`id`, `email`, `username`, `display_name`, `bio`, `avatar_url`, `role` (`user` \| `admin` \| `sys-admin`), `profile_incomplete`, `email_verified`).

### Profile

`user.update_profile`:

```json
{ "display_name": "Alice", "username": "alice", "bio": "" }
```

Bio max 160 characters; display name 1–100 characters. Avatar is **not** set via RPC — use multipart upload.

### Avatar upload

```bash
curl -sS http://127.0.0.1:8080/api/user/avatar \
  -H 'Cookie: oxidean_session=…' \
  -F 'avatar=@photo.png;type=image/png'
```

- Field name: `avatar`
- Allowed types: `image/jpeg`, `image/png`, `image/webp`
- Max body: **2 MiB**
- Stored as WebP under `var/uploads/avatars/{user_id}.webp`; public URL `/uploads/avatars/{user_id}.webp`
- Success: `{"ok":true,"avatar_url":"/uploads/avatars/….webp"}`

```bash
curl -sS -X DELETE http://127.0.0.1:8080/api/user/avatar \
  -H 'Cookie: oxidean_session=…'
```

- Clears `avatar_path` and deletes `{user_id}.webp` when present (idempotent if already absent)
- Success: `{"ok":true,"avatar_url":null}`

### Admin auth settings

`admin.auth.update_settings` input (non-secret fields only; secrets stay in ENV):

```json
{
  "provider_mode": "local",
  "email_provider": "log",
  "from_address": "Oxidean <noreply@example.com>",
  "oidc_issuer": null,
  "oidc_client_id": null,
  "workos_client_id": null
}
```

Response includes boolean badges such as `smtp_configured`, `resend_configured`, `workos_api_key_configured`, `oidc_client_secret_configured` — never raw secrets.

### Personal access tokens (`pat.*`)

Manage tokens with the session cookie via RPC (or `@oxidean/api-client`). Token **prefixes** (redacted examples only):

| Kind | Prefix | Phase 8 capability |
| --- | --- | --- |
| Classic | `oxidean_pat_` | Scope catalog: `repo` (HTTPS fetch + push where ACL allows) |
| Fine-grained | `oxidean_fg_` | `repo_access`: `selected` \| `all`; `contents`: `read` \| `write` |

`pat.createClassic` input:

```json
{ "name": "laptop", "scopes": ["repo"], "expires_at": null }
```

`pat.createFineGrained` input:

```json
{
  "name": "ci-bot",
  "repo_access": "selected",
  "repository_ids": ["…repo-id…"],
  "contents": "write",
  "expires_at": null
}
```

Create responses include a one-time plaintext `token` (store it immediately) plus a metadata `item` **without** the secret. `pat.list` / list items never return the secret — only `token_prefix`, scopes/permissions, `last_used_at` / `last_used_ip`, etc. `pat.revoke` input: `{ "id": "…" }`.

Minting requires a verified email (`auth.email_unverified` otherwise). Empty note/name → `pat.note_required`. Selected fine-grained with no repositories → `pat.repos_required`. Invalid classic scopes or foreign/empty-id fine-grained `selected` repos → `pat.invalid_scope`. Unknown or non-owned revoke id → `pat.not_found`.

### SSH public keys (`sshKey.*`)

Register OpenSSH **public** keys for Git-over-SSH (session cookie; never send private keys to the API). Keys map to **full account identity** — there are no PAT-style scopes on SSH transport (D-SSH-03 / D-SSH-04 / D-SSH-05).

`sshKey.add` input:

```json
{ "title": "laptop", "public_key": "ssh-ed25519 AAAA… comment" }
```

Accepted key types: `ssh-ed25519` and RSA ≥2048. Response is a list item with `id`, `title`, `fingerprint` (SHA256), `key_type`, optional `public_key`, `created_at`, and optional `last_used_*`. There is **no** one-time secret field (unlike PAT mint).

`sshKey.list` returns the same item shape for the signed-in user. `sshKey.revoke` input: `{ "id": "…" }` (hard-delete).

Add requires verified email (`auth.email_unverified` otherwise). Empty title → `sshKey.title_required`. Invalid/unsupported key → `sshKey.invalid_key`. Duplicate fingerprint → `sshKey.fingerprint_taken`. More than **25** keys → `sshKey.limit_exceeded`. Unknown or non-owned revoke id → `sshKey.not_found`.


**PAT ∩ ACL:** Classic `repo` push/fetch requires the PAT subject to also `meets` the needed Capability on that repository (org membership, collaborator grant, or personal owner — not `owner_id == pat.user_id` alone). Fine-grained `all` covers personal-owned plus org Owner/Admin repos; collaborators must use `selected`.

### Organizations (`org.*`) & collaborators

Organizations share the username slug namespace. `org.create` rejects reserved / taken slugs (`org.slug_taken`, `auth.reserved_username`). Blank `display_name` defaults to the slug. `member_base_permission` defaults to `none` and applies only to org **Members** on org-owned private repos (Owner/Admin always Admin).

`org.members.list` returns username + role + ids only (no emails). Live add uses `user.lookup` (prefix ≥ 2; short/email-shaped prefixes return empty ok). Invite create/list omit plaintext tokens; accept redeems the magic-link token and can create a verified local user even when instance `allow_signup` is false. If the invite email already has an account, accept returns `org.invite_login_required` instead of overwriting credentials.

`repo.collaborators.*` grants per-repo `read` \| `write` \| `admin` (never an org role). Mutations require repo Admin capability. Highest-wins coalesce with org roles / `member_base` (collaborator raises effective permission; cannot lower Owner/Admin).

`repo.rename` / `repo.transfer` require Admin. Rename updates `name` and moves the bare dir; transfer rewrites `owner_type` / `owner_id` and moves under the destination slug. Both insert a `repository_redirects` row so old `/{owner}/{repo}` (and Smart HTTP / SSH paths) keep resolving until `OXIDEAN_REPO_REDIRECT_RETENTION_DAYS` (default 90). Transfer requires exact `confirmName` match. Issues and LFS associations stay on `repo_id` (no OID copy). Webhooks are not invented here (later phases). Package owner-path updates on rename/transfer follow packages rules (see Packages registry).

### Releases (`release.*`)

Tag-based releases (GIT-14/15): `release.create` / `update` / `delete` / `list` / `get` / `deleteAsset`. Binary assets use dedicated HTTP routes under `OXIDEAN_RELEASE_ASSETS_DIR` (opaque `asset_id`, not the LFS OID store). Multipart upload replaces by filename; max size `OXIDEAN_RELEASE_ASSET_MAX_BYTES` (default 512 MiB). Draft visibility follows Write+; published downloads need Read+.

`admin.instance.factory_reset` (`confirmation: "RESET"`) wipes repositories (cascades collaborators, PAT-repo links, and **issue domain** tables), organizations (members/invites/org-scoped labels cascade), and auth users. `scope`: `database_only` (default) keeps bare dirs; `database_and_repositories` also clears children under `OXIDEAN_REPOS_DIR` and `OXIDEAN_RELEASE_ASSETS_DIR`.

### Issues (`issue.*`) & labels (`label.*`)

Phase 11 ships per-repository issues (ISS-01…04) on migration `0011_issues`:

| Concern | Contract |
| --- | --- |
| **Numbering** | Each repo allocates monotonic `#N` via `issue_counters`. Hard-delete does **not** reclaim numbers. |
| **ACL** | Capability gates: Read+ to view; Write+ to create/comment/assign/react/link; author or Write+ to edit own issue/comment; Admin (or typed confirm) for hard-delete. Private unauthorized access returns soft `repo.not_found` / `issue.not_found` (no enumeration). |
| **Markdown** | Web Write\|Preview uses `renderGfm` with `#N` / `owner/repo#N` autolink and sanitize-last. `@mention` / commit SHA autolink are off. |
| **Linked PRs** | `issue.links.*` supports `pr` (real PR `#N`) and legacy `pr_stub`. Prefer `pr` when linking to an open/merged pull. |
| **Closing keywords** | On `pull.merge` into the **default branch**, `fixes` / `closes` / `resolves` `#N` in the PR body (and merge commit message) close matching open issues. Keywords do **not** fire on close-without-merge or non-default bases. |

### Pull requests (`pull.*`)

Phase 12 ships pull requests (PR-01…07) on migration `0016_pull_requests`:

| Concern | Contract |
| --- | --- |
| **Numbering** | Shared per-repo `#N` with issues (`issue_counters`). |
| **ACL** | Read+ list/get/diff/comments; Write+ open/comment/review/merge/close/reopen; Admin merge-strategy settings. Author cannot Approve / Request changes on own PR. |
| **Merge** | Methods `merge` \| `squash` \| `rebase` gated by `repo.mergeSettings.*` (defaults all enabled). Conflict → `pull.merge_conflict`. Optional `delete_branch`. |
| **Diff UX** | `pull.files` unified patch; web supports unified/split. Line comments carry path/side/line; outdated after head/base change. |

Client surface: `client.pull.*` / `client.mergeSettings.*` / `client.issue.*` / `client.label.*` in `@oxidean/api-client` (regenerate with `make rpc-gen`).

### Notifications (`notification.*`)

Phase 17 ships in-app activity notifications (NOTF-01 / NOTF-02) on migration `0018_notifications`:

| Concern | Contract |
| --- | --- |
| **Ownership** | Every list/mark/unread query is forced to `recipient_id = session.user_id`. Clients cannot address another user's inbox. |
| **Read model** | `read_at` null = unread. `notification.list` filter `unread` (default) or `all`; newest-first offset pagination. |
| **Payload** | Rows include `reason`, `subject_kind` (`issue` \| `pull_request`), `owner` / `repo` slugs, `subject_number`, `subject_title`, `actor_username` for deep links `/{owner}/{repo}/issues\|pull/{n}`. |
| **Fan-out** | Domain writes (e.g. `issue.comments.create`) insert best-effort rows; actors are never notified. Activity email is out of scope. |

Client surface: `client.notification.*` in `@oxidean/api-client` (regenerate with `make rpc-gen`).

### Git Smart HTTP

Clone / fetch / push use Git Smart HTTP under `/{owner}/{repo}.git` (not `/api/rpc`):

| Method | Path | Service |
| --- | --- | --- |
| `GET` | `/{owner}/{repo}.git/info/refs?service=git-upload-pack` | Discovery (fetch) |
| `GET` | `/{owner}/{repo}.git/info/refs?service=git-receive-pack` | Discovery (push) |
| `POST` | `/{owner}/{repo}.git/git-upload-pack` | Fetch / clone |
| `POST` | `/{owner}/{repo}.git/git-receive-pack` | Push |

**Auth:** HTTP Basic with password = PAT (`oxidean_pat_…` or `oxidean_fg_…`). Username may be the account username or aliases `git`, `token`, or `oauth2` (identity comes from the PAT hash). Account passwords are rejected. **Session cookies are ignored** for Smart HTTP authorization.

Public repos may allow anonymous `upload-pack`. Private repos and push require a valid PAT with sufficient **scope ∩ Capability ACL**; ACL denials stay HTTP **401** Basic, while insufficient PAT scope → HTTP **403**. Unverified-email users may fetch but not push (`auth.email_unverified` JSON on receive-pack). Failed Basic auth may return `401` with `WWW-Authenticate: Basic realm="Oxidean Git"` and a PAT hint body.

Example (redacted token):

```bash
git clone https://git:oxidean_pat_REDACTED@example.com/alice/demo.git
# or:
git -c http.extraHeader="Authorization: Basic $(printf 'git:oxidean_pat_REDACTED' | base64 -w0)" \
  ls-remote https://example.com/alice/demo.git
```

### Git LFS

Phase 14 serves **Git LFS** over HTTPS under the same `{owner}/{repo}.git` surface (Traefik `.git` PathRegexp already covers `info/lfs`). Storage is the instance volume `OXIDEAN_LFS_DIR` — see [CONFIGURATION.md](CONFIGURATION.md#git-lfs).

| Method | Path | Role |
| --- | --- | --- |
| `POST` | `/{owner}/{repo}.git/info/lfs/objects/batch` | Batch discover upload/download actions (`transfer=basic`) |
| `PUT` | `/{owner}/{repo}.git/info/lfs/objects/{oid}` | Streaming basic upload |
| `GET` | `/{owner}/{repo}.git/info/lfs/objects/{oid}` | Download; optional `Range` |
| `POST` | `/{owner}/{repo}.git/info/lfs/objects/{oid}/verify` | Optional post-upload verify |

**Auth (D-LFS-09):** Same as Smart HTTP — HTTP Basic with password = **personal access token**. Username aliases `git` / `token` / `oauth2` work. **Session cookies are ignored** for LFS. Failed auth may return `401` with `WWW-Authenticate: Basic realm="Oxidean Git"` (LFS clients also accept `LFS-Authenticate`). Read capability for download; Write + verified email for upload. Classic `repo` / fine-grained `contents` scopes (no dedicated `lfs` scope).

**Enable:** Per-repo LFS must be enabled by a repository Admin before Batch issues upload actions. Quotas / max object size reject oversized uploads with clear LFS error JSON (no soft-warn-only).

**Client setup:** Track patterns with `git lfs track` and commit `.gitattributes` (server does not auto-commit). Example:

```bash
git lfs install
git lfs track "*.psd"
git add .gitattributes
# push with HTTPS remote + PAT, or SSH git remote + HTTPS LFS via credential helper
```

**SSH git remotes:** Pack protocol may use SSH, but LFS object transfer remains **HTTPS** in Phase 14. Configure a credential helper so `git-lfs` can present a PAT to `https://…/{owner}/{repo}.git/info/lfs`. LFS-over-SSH is not supported.

### Git over SSH

Clone / fetch / push over SSH use an in-process listener (Compose TCP **2222** by default — not Traefik). Remotes are **scp-style** `git@{host}:{owner}/{repo}.git` (D-SSH-02). The SSH username must be `git`; identity comes only from a registered public-key fingerprint (full account ACL — no PAT scopes). When advertised port ≠ 22, clients set `Port` in `~/.ssh/config` (or `ssh -p`); do not treat `ssh://` as the primary CloneBox URL.

Failed pubkey auth is rate-limited like Smart HTTP PAT failures (IP + fingerprint buckets). See [CONFIGURATION.md](CONFIGURATION.md) for `OXIDEAN_SSH_*`.

### Two-way repository mirroring

Attach one external git remote (HTTPS token or SSH deploy key) to an existing repository. Choose a **sync mode**:

| Mode | Tips | Deletes | Protected default |
| --- | --- | --- | --- |
| **`merge`** (default) | Fast-forward when one side is behind; diverged **branches** get a merge commit pushed both ways; **tags never merge** (identical → skip, different SHAs → conflict) | Not propagated | Local FF that would violate protection opens a PR from `mirror/<id-prefix>/sync/<branch>`; merge conflicts open `mirror/<id-prefix>/<branch>` |
| **`exact`** | Last-writer-wins by tip committer time (tie → remote); force-update the older side | Both ways via a per-mirror ref snapshot (first Exact run baselines only — no mass deletes on mode switch) | Updates the real tip (mirror Admin bypass; still blocked when `enforce_admins`). No helper `mirror/*` branches or PRs |

Force-push / `git push --mirror` are used only in **exact** mode. Helper branches under `refs/heads/mirror/**` are never synced in Exact mode and are cleaned up when Exact runs.

**Triggers (event-driven):**

1. Local ref mutation (HTTPS/SSH receive-pack, PR merge, branch/tag RPC) — async enqueue; the git client is never blocked on the remote.
2. Inbound push webhook from the remote forge.
3. `repo.mirror.syncNow` (Admin).
4. Short poll backstop per mirror (`poll_interval_secs`, default ~60; `0` disables). Instance ticker: `OXIDEAN_MIRROR_POLL_TICK_SECS` (see [CONFIGURATION.md](CONFIGURATION.md)).

Per repo: at most one sync in flight plus one coalesced follow-up. Mirror-driven local ref updates do **not** re-enqueue (avoids loops after we push to GitHub and it webhooks us back). Equal tips are a cheap skip.

Set `sync_mode` on `repo.mirror.upsert` (`merge` | `exact`). Changing mode clears the Exact ref snapshot so the next Exact run re-baselines.

#### Inbound webhook

| Method | Path | Auth |
| --- | --- | --- |
| `POST` | `/api/repos/{owner}/{repo}/mirror/hook` | Shared secret (see below) |

Configure a **Push** (and tag-push) webhook on GitHub / GitLab / Gitea / Forgejo pointing at that URL. The payload SHAs are **not** trusted as a sync plan — a valid authenticated POST only wakes the engine; `fetch` is the source of truth. Ping / unrelated events return **204** after auth. Missing repo, disabled mirror, or bad auth → **404** (anti-enumeration) or **401** for failed signature after the repo resolves.

Accept any of:

- GitHub / Gitea: `X-Hub-Signature-256: sha256=<hmac>` (or `X-Gitea-Signature`)
- GitLab: `X-Gitlab-Token: <secret>`
- `Authorization: Bearer <secret>`

The secret is generated on mirror upsert / `repo.mirror.rotateWebhookSecret`. RPC list/get return a masked value; plaintext is returned **once** on create/rotate. Store it like Actions secrets (AES-256-GCM via `OXIDEAN_ACTIONS_SECRETS_KEY`).

#### Admin RPC

| Procedure | Notes |
| --- | --- |
| `repo.mirror.get` | Current config + last status + per-ref outcomes (no decrypted git credentials) |
| `repo.mirror.upsert` | Create/update remote URL, auth kind, poll interval, enable; may return `webhook_secret` once |
| `repo.mirror.delete` | Remove mirror row |
| `repo.mirror.syncNow` | Enqueue a two-way run (rate-limited) |
| `repo.mirror.generateSshKey` | Generate Ed25519 deploy key; UI shows public key for the remote |
| `repo.mirror.rotateWebhookSecret` | New inbound secret (plaintext once) |
| `repo.mirror.fetchHostKey` | Run `ssh-keyscan` for an SSH remote URL; returns `known_hosts` lines (admin still saves) |

**SSH remotes:** paste or generate a private key; set `known_hosts` (TOFU — first fingerprint must match later or sync fails closed). Git uses `GIT_SSH_COMMAND` with `IdentitiesOnly=yes` and a dedicated `UserKnownHostsFile`. HTTPS remotes use a token/password via askpass/extraheader. `file://` and non-git schemes are rejected.

Settings UI: repository **Settings → Two-way mirror** (after Webhooks).

### Packages registry (OCI / npm / generic)

Same-host path prefixes (Traefik/Vite must route to the API **before** the SPA):

| Prefix | Clients | Notes |
| --- | --- | --- |
| `/v2` | `docker` / OCI | Distribution Spec; Bearer realm `GET /v2/token`; image names `{owner}/{image}` (two segments) |
| `/npm/{owner}/` | `npm` / yarn / pnpm | Packument, publish, tarball; set registry to `{PUBLIC_ORIGIN}/npm/{owner}/` |
| `/generic/{owner}/{name}/{version}/{filename}` | curl / CI | Immutable per filename (409 on overwrite); `DELETE` removes a version |

**Auth matrix:** Registry routes use **PAT only** (HTTP Basic password = token, or OCI Bearer). Session cookies are **ignored**. Classic `package:read` / `package:write` (or FG Packages perm) is **required** in addition to forge ACL — classic `repo` alone does **not** grant package access. Public packages may allow anonymous pull when ACL permits.

**Immutability:** Published versions are immutable (npm republish conflict; generic filename conflict; OCI digest tags). Soft-delete via UI/RPC uses type-to-confirm `name@version`.

**Tarball / clone-style URLs:** Absolute URLs in npm packuments use `OXIDEAN_PUBLIC_ORIGIN` (same as clone boxes).

**Owner transfer/rename:** When Phase 15 lands owner transfer/rename, package owner path metadata must be updated in lockstep — not implemented in Phase 20.

### TypeScript client

```ts
import { createClient } from "@oxidean/api-client";

const client = createClient({ baseUrl: "" }); // same-origin; credentials: "include" by default
const health = await client.system.health();
const me = await client.auth.me();
const pats = await client.pat.list();
const created = await client.pat.createClassic({ name: "laptop", scopes: ["repo"] });
// created.data.token is shown once — never send it as RPC Bearer
const keys = await client.sshKey.list();
await client.sshKey.add({ title: "laptop", public_key: "ssh-ed25519 AAAA… comment" });
```

TanStack Query helpers (`authMeQueryOptions`, `patListQueryOptions`, `sshKeyListQueryOptions`, `adminAuthGetSettingsQueryOptions`, etc.) are exported from the same package.

## Error codes

HTTP status for `/api/rpc` is derived from the RPC error:

| HTTP | When |
| --- | --- |
| `200` | `ok: true` |
| `400` | Most RPC errors (validation, provider mismatch, version mismatch, etc.) |
| `401` | `auth.unauthenticated` |
| `403` | `admin.forbidden`, `auth.email_unverified` |
| `404` | `rpc.unknown_procedure`, `repo.not_found`, `issue.not_found` |

Common `error.code` values:

| Code | Meaning |
| --- | --- |
| `rpc.version_mismatch` | Missing/wrong `Oxidean-RPC-Version` |
| `rpc.bad_input` | Invalid JSON / procedure input |
| `rpc.payload_too_large` | Echo message too large |
| `rpc.unknown_procedure` | Unknown procedure name |
| `auth.unauthenticated` | No valid session |
| `auth.email_unverified` | Verified email required (PAT mint; SSH key add; Smart HTTP / SSH push) |
| `auth.provider_mismatch` | Local auth disabled for current mode |
| `auth.taken` / `auth.invalid_*` / `auth.weak_password` / `auth.reserved_username` | Signup/profile validation |
| `auth.setup_required` | Empty instance must complete `/setup` before signup/SSO |
| `auth.setup_unavailable` | `/setup` already completed (users exist or ENV seed path) |
| `auth.not_configured` | WorkOS/OIDC ENV missing (SSO start) |
| `admin.forbidden` | Authenticated but not admin |
| `pat.note_required` | PAT name/note empty |
| `pat.repos_required` | Fine-grained `selected` with no repository ids |
| `pat.invalid_scope` | Classic scopes or fine-grained repo selection invalid |
| `pat.not_found` | Revoke target missing or not owned |
| `sshKey.title_required` | SSH key title/note empty |
| `sshKey.invalid_key` | Public key parse/type/size rejected |
| `sshKey.fingerprint_taken` | Fingerprint already registered |
| `sshKey.limit_exceeded` | More than 25 SSH keys for the user |
| `sshKey.not_found` | Revoke target missing or not owned |
| `org.slug_taken` | Org slug collides with user or org |
| `org.forbidden` / `org.not_found` | Org ACL / missing org |
| `org.invite_login_required` | Invite email already registered — sign in to accept |
| `org.invite_*` | Invite expired / revoked / invalid |
| `invite.invalid` | Invite token unknown, expired, revoked, or out of seats |
| `invite.email_mismatch` | Bound invite accepted under a different email |
| `invite.login_required` | Link-invite email already registered — sign in to accept |
| `repo.not_found` | Missing or unauthorized private (web/RPC soft 404) |
| `repo.create_forbidden` | Org Member cannot create under that org |
| `issue.not_found` / `issue.comment_not_found` / `issue.link_not_found` | Missing issue/comment/link (private soft-404 where applicable) |
| `issue.confirm_mismatch` | Admin hard-delete confirmation number mismatch |
| `label.not_found` | Missing label definition |
| `db.not_configured` / `db.probe_failed` | Database unavailable |
| `avatar.*` | Multipart/type/size/store failures on avatar upload |

Avatar and SSO JSON errors use the same `{ ok: false, error: { code, message } }` shape where applicable.

## Rate limits

Smart HTTP failed-authentication attempts are rate-limited in-process: **20 failures per client IP** and **10 per username** per **15 minutes**, then HTTP `429` with `Retry-After`. Client IP uses the rightmost `X-Forwarded-For` hop from a trusted proxy; do not expose the API without a proxy that sanitizes forwarded headers. Successful PAT auth clears the user bucket. Git-over-SSH failed pubkey auth uses the same windows with the key **fingerprint** as the user bucket. Other RPC routes do not apply this limiter; rely on reverse-proxy / edge controls for deployment-wide limits.

`user.lookup` is rate-limited per session (**60** requests / **60s**). Other RPC routes do not apply in-process limiters; rely on reverse-proxy / edge controls for deployment-wide limits.

Invite issuance is rate-limited per scope (instance / org / repo): **20** invites issued per hour, enforced cumulatively — a bulk `create` batch stops issuing once the remaining hourly budget is exhausted, and remaining recipients get per-recipient `hourly invite limit reached` results. Requests are also capped at **50** recipient emails each, and re-issuing an invite to the same email within **60s** is rejected (re-issue after that revokes the previous pending invite — its link dies). `createLink` counts as one issued invite against the same hourly budget.

## Actions (Phase 19)

Oxidean Actions is a **control plane**: workflows are discovered under `.github/workflows/*.yml`, runs/jobs are queued, and **registered runners** execute them. There is **no managed CI minutes** product and no in-process job executor (ACT-07 / D-ACT-10).

### Workflow layout & triggers (ACT-01 / ACT-02)

- Workflow files live at `.github/workflows/*.yml` (or `.yaml`) on the evaluated ref.
- **push** — evaluated after Smart HTTP / SSH receive (and related notify hooks).
- **pull_request** — evaluated on PR open/sync/reopen-style events (Phase 12 hook).
- Instance gate: `OXIDEAN_ACTIONS_ENABLED`. Per-repo Admin toggle: `repo.actions.setEnabled` / Settings → Actions.

### Runner protocol HTTP (`/api/actions`) (ACT-06)

Mounted under `/api/actions` on the same HTTP port as RPC (Traefik `/api` PathPrefix → API). **Session cookies are ignored** — only registration tokens and runner bearer tokens authenticate (D-ACT-18). Use placeholders in docs/examples; never commit real tokens.

| Method | Path | Auth | Purpose |
|--------|------|------|---------|
| POST | `/api/actions/register` | Registration token (`token` body or bootstrap env `OXIDEAN_RUNNER_REGISTRATION_TOKEN`) | Register runner; returns `runner_token` once |
| POST | `/api/actions/declare` | Bearer runner token | Update labels (`label[:schema[:args]]`, D-ACT-09) |
| POST | `/api/actions/fetch_task` | Bearer runner token | Claim queued job matching labels; may include decrypted `secrets` map |
| POST | `/api/actions/update_task` | Bearer runner token | Job state updates (`queued` → `in_progress` / `success` / `failure` / `cancelled`) |
| POST | `/api/actions/update_log` | Bearer runner token | Append job log chunks |

Example register body (placeholders only):

```json
{
  "name": "compose-runner",
  "labels": ["ubuntu-latest:docker://node:20-bookworm", "self-hosted"],
  "token": "reg_REPLACE_ME"
}
```

Runner protocol ignores session cookies (D-ACT-18).

**Custom `runs-on` labels (D-ACT-09):** format `label[:schema[:args]]`, e.g. `ubuntu-latest:docker://node:20-bookworm` (bare labels run on the runner host; `docker://` runs in a container when the socket is mounted). Runners declare labels at register/declare; jobs queue until a registered runner with a matching label calls `fetch_task`. There is **no forge-hosted executor** and no managed Oxidean Cloud minutes (ACT-07).

Session RPC (Read+/Admin as noted):

| Procedure | ACL | Notes |
|-----------|-----|-------|
| `repo.actions.listRuns` / `getRun` / `getJobLog` | Read+ | UI list/detail |
| `repo.actions.secrets.list` / `put` / `delete` | Admin | Names only on list; values never echoed |
| `repo.actions.getEnabled` / `setEnabled` | Read+ / Admin | Per-repo enable |
| `admin.actions.createRegistrationToken` | SysAdmin | One-time plaintext token (`reg_…`) |
| `admin.actions.listRunners` | SysAdmin | Registered runners (no token hashes) |

### Commit statuses for Phase 13 (D-ACT-15 / D-ACT-16)

Job updates publish commit statuses via `repo.commitStatus.*` with context (D-ACT-15):

```text
{workflow_name} / {job_key}
```

Example: `CI / build` (job key is the YAML `jobs.<id>`, not the DB row UUID). Target URL points at `/{owner}/{repo}/actions/runs/{run_id}` when public origin is configured. Phase 13 required checks should match these contexts (`repo.commitStatus.list`).

### UI routes (ACT-03)

- `/{owner}/{repo}/actions` — run list
- `/{owner}/{repo}/actions/{runId}` — jobs + logs
- `/{owner}/{repo}/settings/actions` — Actions enable + secrets (Admin)
- `/admin/runners` — registration tokens + runner list

### Official runner image (ACT-04 / ACT-05)

Operators attach compute via `oxidean-runner` (`crates/oxidean-runner`, image from `docker/oxidean-runner`). Compose profile `actions` sidecar or standalone `docker run` against `OXIDEAN_PUBLIC_ORIGIN` — see [DEPLOYMENT.md](DEPLOYMENT.md) and [`docker/oxidean-runner/README.md`](../docker/oxidean-runner/README.md).

## Regenerating the TypeScript client

Procedure names and DTOs in Rust (`rpc.rs`, `oxidean-core`) are authoritative. Regenerate `@oxidean/api-client`:

```bash
make rpc-gen
# equivalent: cargo run -q -p oxidean-api --bin rpc-gen
```

This overwrites `packages/api-client/src/index.ts` (do not hand-edit). CI drift check:

```bash
make rpc-sync-check
```

Related docs: [ARCHITECTURE.md](ARCHITECTURE.md), [CONFIGURATION.md](CONFIGURATION.md), [database.md](database.md), [dev-auth.md](dev-auth.md).
