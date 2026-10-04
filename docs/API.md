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

**Personal access tokens (PATs) authenticate RPC for non-browser clients (API-02).** `POST /api/rpc` and `GET /api/rpc/ws` accept `Authorization: Bearer <pat>` when **no** `oxidean_session` cookie is present — the cookie always wins if both are sent. PATs also authenticate **Git Smart HTTP** over HTTPS via HTTP Basic (password = token) and the package registries. See [PAT Bearer authentication](#pat-bearer-authentication) for the scope model.

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
| `POST` | `/api/rpc` | JSON RPC dispatch | Cookie or PAT Bearer when procedure needs auth |
| `GET` | `/api/rpc/ws` | WebSocket upgrade; same procedures as HTTP | Cookie or PAT Bearer when procedure needs auth |
| `POST` | `/api/mcp` | MCP endpoint (JSON-RPC 2.0, streamable-HTTP) — see [MCP.md](MCP.md) | Cookie or `Bearer` PAT; anonymous for public data |
| `GET` | `/api/mcp` | SSE stream (unsupported → `405`) | — |
| `GET` | `/.well-known/webmcp` | WebMCP discovery document (served by the web app, not this router) — see [WEBMCP.md](WEBMCP.md) | No |
| `GET` | `/.well-known/mcp` | MCP server metadata for the instance endpoint (served by the web app) | No |
| `GET` | `/api/auth/workos/start` | Start WorkOS AuthKit (optional `?return_to=`) | No (redirect) |
| `GET` | `/api/auth/workos/callback` | WorkOS code exchange; sets session cookie | No (redirect) |
| `GET` | `/api/auth/oidc/start` | Start OIDC + PKCE (optional `?return_to=`) | No (redirect) |
| `GET` | `/api/auth/oidc/callback` | OIDC code exchange; sets session cookie | No (redirect) |
| `GET` | `/oauth/authorize` | OAuth2 authorization endpoint → consent page / login | Session cookie (or login redirect) |
| `POST` | `/oauth/token` | OAuth2 token endpoint (`grant_type=authorization_code`) | Client secret |
| `GET` | `/oauth/userinfo` | OAuth2 identity surface | `oxidean_oat_` Bearer |
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
| `*` | `/api/v1/**` | REST facade over the core domain (API-01) — see [REST API](#rest-api-apiv1) | Cookie or PAT Bearer |
| `GET` | `/api/repos/{owner}/{repo}/activity.atom` | Atom 1.0 feed of repo pushes / branch events (newest 30) | No (Read+ when private) |
| `GET` | `/api/repos/{owner}/{repo}/releases.atom` | Atom 1.0 feed of releases (drafts only for Write+) | No (Read+ when private) |
| `GET` | `/api/users/{username}/activity.atom` | Atom 1.0 feed of a user's public repo activity | No |

SSO start routes redirect to the IdP when configured. If WorkOS/OIDC ENV is missing, start returns HTTP 503 with `auth.not_configured`. Failures typically redirect to `/login?error=sso`.

### RPC procedures (`POST /api/rpc`)

| Procedure | Description | Auth |
| --- | --- | --- |
| `system.health` | Status, API crate version, DB ping string | No |
| `system.echo` | Echo `message` (max 8192 bytes) | No |
| `system.db_probe` | Dialect probe / `instances` counter | No |
| `system.manifest` | Compatibility contract: `protocol_version`, `server_version`, `procedures` map (every dispatch procedure → `true`), `capabilities` (`mcp`/`rest`/`oauth`), `min_cli_version` — clients feature-gate on this (CLI-02) | No |
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
| `repo.watch` / `repo.unwatch` | Subscription upsert at `level` (`all` \| `participating` \| `ignore`; default `all`) / remove row; `watch_count` + `viewer_watch_level` on `RepoPublic` count non-`ignore` rows only | Session + Read |
| `repo.watchers.list` | Public watchers (non-`ignore` rows), paginated + `q` filter | Anonymous OK |
| `user.listWatched` | Caller's repo subscriptions at any level incl. `ignore` (Read ACL filter; offset/limit) | Session + verified |
| `user.follow` / `user.unfollow` | Idempotent user-follow edge writes; returns target `PublicUserProfile` with refreshed counts | Session + verified |
| `user.followers.list` / `user.following.list` | Public follower/following lists (`username` filter via `q`; offset/limit) | Anonymous OK; unknown → `user.not_found` |
| `user.getPublicProfile` | Public profile by username (`username`, `display_name`, `bio`, `avatar_url` — **never email**) | Anonymous OK; unknown → `user.not_found` |
| `repo.explore` | Public repos sorted by `star_count` desc then `updated_at` desc; optional `q` substring | Anonymous OK |
| `search.global` | Sitewide grouped search — `repositories`, `users`, `organizations`, `issues`, `pulls` (SQL, ACL-filtered to readable repos); `commits`/`code` via a bounded scan of the newest ~10 readable repos (`truncated` marks partial coverage). `types` limits which groups get hits; DB groups always report `total`. Users group requires a verified session. Indexed cross-repo code search is SRCH-01 | Anonymous OK |
| `repo.fork` | Fork public readable source (bare copy); sets `forked_from_repo_id` + `fork_network_id`; one active fork per (owner, network) | Session + Read on public source |
| `repo.rename` | Rename repo; moves bare dir; inserts redirect | Repo Admin |
| `repo.transfer` | Transfer ownership (type-confirm `confirmName`); moves bare dir; redirect | Repo Admin |
| `repo.softDelete` | Soft-delete with type-confirm | Repo Admin |
| `repo.collaborators.list` / `add` / `update` / `remove` | Per-repo collaborator grants | Repo Admin |
| `repo.deployKey.list` / `create` / `delete` | Per-repo deploy keys for Git-over-SSH (read or read/write scope) | Repo Admin |
| `repo.invites.create` / `createLink` / `list` / `revoke` | Bulk collaborator email invites (`{ emails, permission }`) and shareable links (optional `expires_at`, `max_uses`) | Repo Admin |
| `repo.branchProtection.list` / `create` / `update` / `delete` | Classic branch protection rules (glob pattern on `refs/heads/*`); enforced by the bare-repo `update` hook on HTTPS + SSH pushes. `require_signed_commits` denies pushes introducing commits without a forge-verified SSH/GPG signature | Repo Admin |
| `repo.tagProtection.list` / `create` / `update` / `delete` | Protected tag rulesets (glob pattern on `refs/tags/*`); `allow_create`/`allow_update`/`allow_delete` carve out actions for non-admins, `enforce_admins` removes the admin bypass | Repo Admin |
| `release.list` / `get` / `create` / `update` / `delete` / `deleteAsset` | Tag-based releases + notes; assets via HTTP | Session (+ capability) |
| `admin.users.list` / `updateRole` / `ban` / `unban` / `delete` / `revokeSessions` / `getAccess` | User administration (type-confirm delete; `delete_orgs` opt-in for shared orgs) | Sys-admin |
| `admin.users.listSessions` | Per-user sessions with client metadata (`ip_address`, `user_agent`, `remember_me`, last-seen/expiry) | Sys-admin |
| `admin.users.getActivity` | Merged per-user activity — audit events + repository activity; `source` (`audit`\|`repository`) and `event_type` filters | Sys-admin |
| `admin.invites.create` / `createLink` / `list` / `revoke` | Bulk instance email invites + shareable links (optional `expires_at`, `max_uses`) | Sys-admin |
| `admin.auth.get_settings` | Auth/email settings including `allow_signup` (no secrets) | Admin session |
| `admin.auth.update_settings` | Update provider/email/`allow_signup`; rebuild email sender | Admin session |
| `admin.instance.factory_reset` | Wipe users, orgs, repos + issue domain (DB); optional disk wipe via `scope` | Sys-admin |
| `issue.create` / `get` / `list` / `update` / `close` / `reopen` / `history` / `delete` | Per-repo issues (`#N`); create/comment/react = verified + Read; edit/close/reopen = author or Write+ | Session (+ capability) |
| `issue.comments.*` | Comment CRUD + history; author or Write+ moderate-delete | Session (+ capability) |
| `issue.labels.set` / `assignees.set` / `assigneeCandidates` | Assign labels / assignees (Write+; assignees must have Read+) | Session (+ capability) |
| `issue.reactions.toggle` | Toggle emoji reaction on issue or comment | Session + verified (+ Read) |
| `issue.links.list` / `add` / `remove` | Linked issues/PRs (`pr` preferred; legacy `pr_stub` kept) | Session (+ capability) |
| `notification.list` | Own notifications; filter `unread` (default) \| `all`; offset pagination | Session |
| `notification.unreadCount` | Unread badge count for session user | Session |
| `notification.markRead` | Mark own notification ids read (foreign ids no-op) | Session |
| `notification.markAllRead` | Mark all own unread notifications read | Session |
| `pull.create` / `get` / `list` / `update` / `close` / `reopen` | Pull requests; shared `#N` with issues; create = Read+ on base + Write+ on head (fork heads OK); update/close/reopen = author or Write+ | Session (+ capability) |
| `pull.files` / `pull.commits` | Diff + commit list for a PR | Session (+ Read+) |
| `pull.comments.list` / `create` / `resolve` | General + line comments; create = verified + Read, resolve = Write+ | Session (+ capability) |
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
| `repo.actions.rerunRun` / `cancelRun` | Requeue or cancel a run; cancel on a finished run → `repo.actions.run_finished` | Session + Write+ |
| `repo.actions.secrets.list` / `put` / `delete` | Repo Actions secrets (names only on list) | Session + Admin |
| `repo.actions.getEnabled` / `setEnabled` | Per-repo Actions enable toggle | Session + Read+ / Admin |
| `repo.mirror.get` / `upsert` / `delete` / `syncNow` | Two-way remote mirror config + enqueue sync | Session + Admin |
| `repo.mirror.generateSshKey` / `rotateWebhookSecret` / `fetchHostKey` | Deploy key, inbound webhook secret, ssh-keyscan | Session + Admin |
| `webhook.create` / `list` / `get` / `update` / `delete` / `deliveries.list` / `deliveries.get` / `ping` / `redeliver` | Outbound repo webhooks; events: `push`, `pull_request`, `issues`, `issue_comment` (incl. PR conversation comments), `ping`, `*` | Session + Admin |
| `repo.commitStatus.create` / `list` | Commit statuses (Phase 13 + Actions publisher) | Session + Write+ / Read+ |
| `admin.actions.createRegistrationToken` | Mint one-time runner registration token | Sys-admin |
| `admin.actions.listRunners` | List registered runners (no secrets) | Sys-admin |

Unknown procedure → `rpc.unknown_procedure` (HTTP 404).

## REST API (`/api/v1`)

API-01 adds a resource-oriented REST surface alongside the RPC procedures. Every route translates path/query/body into the matching RPC procedure's input and dispatches through the same `rpc::dispatch` path — ACLs, PAT scope gates (`authorize_rpc`), the bootstrap lock, and side effects (notifications, webhook events) are identical for both surfaces. The REST API is a companion, not a replacement: `POST /api/rpc` remains the primary client contract and covers procedures REST does not expose.

- **OpenAPI spec**: [`docs/openapi.yaml`](openapi.yaml) (OpenAPI 3.0, hand-maintained — keep it in sync when adding routes).
- **Base path**: `/api/v1` (e.g. `GET /api/v1/repos/octo/hello`).
- **Auth**: `Cookie: oxidean_session=…` or `Authorization: Bearer <pat>`; the cookie wins when both are sent (same as `/api/rpc`). Anonymous requests can reach public read endpoints. Session-only procedures (org creation, `admin.*`, credential management) return `403 auth.pat_scope` for PATs — see [PAT Bearer authentication](#pat-bearer-authentication).
- **No version header**: unlike `/api/rpc`, REST requests do not send `Oxidean-RPC-Version`.
- **Response shape**: success returns the procedure's `data` payload directly (no `{ok, data}` envelope); `POST` create endpoints return `201`. Errors return the `AppError` body `{code, message, data?}` with a mapped status: `*.not_found` → 404, `*.forbidden` / `auth.pat_scope` / `auth.email_unverified` → 403, `auth.unauthenticated` → 401, `auth.rate_limited` → 429, `release.tag_taken` / `pull.merge_conflict` / `auth.taken` → 409, `*.internal` / `*_failed` → 500, otherwise 400.
- **Input mapping**: URL segments fill the procedure's identity fields (`{owner, name, number, …}`) and always override body fields of the same name; query params map to optional inputs.
- **`PATCH` `state`**: `PATCH …/issues/{n}` and `PATCH …/pulls/{n}` accept `state: "closed" | "open"`, mapping to `issue.close` / `issue.reopen` (or `pull.close` / `pull.reopen`). When combined with field edits in one request, the field update runs first; the two updates are not atomic.
- **Wildcard params**: `…/tree/{path}`, `…/contents/{path}`, and `…/releases/tags/{tag}` accept slashes — URL-encode slashed tags (`release%2F1.0`).

```bash
curl -sS http://127.0.0.1:8080/api/v1/repos/octo/hello   -H 'Authorization: Bearer oxidean_pat_…'
```

### REST endpoints

All paths are under `/api/v1`. The procedure column names the RPC equivalent in the table above.

| Method | Path | Procedure | Notes |
| --- | --- | --- | --- |
| `GET` | `/health` | `system.health` | Anonymous |
| `GET` | `/user` | `auth.me` | Authenticated account |
| `GET` | `/user/repos` | `repo.listMine` | Caller's repos |
| `POST` | `/user/repos` | `repo.create` | Caller-owned repo |
| `GET` | `/user/orgs` | `org.listMine` | |
| `GET` | `/user/starred` | `user.listStarred` | `offset`, `limit` |
| `GET` | `/users/{username}` | `user.getPublicProfile` | Anonymous OK |
| `GET` | `/users/{username}/repos` | `repo.listByOwner` | |
| `POST` | `/orgs` | `org.create` | Session only |
| `GET` | `/orgs/{slug}` | `org.get` | Anonymous OK |
| `GET` | `/orgs/{slug}/members` | `org.members.list` | |
| `GET` | `/orgs/{slug}/repos` | `repo.listByOwner` | |
| `POST` | `/orgs/{slug}/repos` | `repo.create` | `owner` = slug |
| `GET` | `/repos` | `repo.explore` | Public discovery; `q`, `offset`, `limit` |
| `GET` | `/repos/{owner}/{repo}` | `repo.get` | Anonymous OK for public |
| `PATCH` | `/repos/{owner}/{repo}` | `repo.updateMetadata` | `description`, `homepage`, `topics` |
| `DELETE` | `/repos/{owner}/{repo}` | `repo.softDelete` | URL is the typed confirmation |
| `GET` | `/repos/{owner}/{repo}/branches` | `repo.refs` | `refs/heads/*`, short names |
| `GET` | `/repos/{owner}/{repo}/tags` | `repo.refs` | `refs/tags/*`, short names |
| `GET` | `/repos/{owner}/{repo}/commits` | `repo.commits` | `sha` (alias `ref`), `skip`, `limit` |
| `GET` | `/repos/{owner}/{repo}/commits/{sha}` | `repo.commit` | |
| `GET` | `/repos/{owner}/{repo}/compare/{basehead}` | `repo.compare` | `base...head` (`..` accepted) |
| `GET` | `/repos/{owner}/{repo}/tree` | `repo.tree` | Root listing; `?ref=` |
| `GET` | `/repos/{owner}/{repo}/tree/{path}` | `repo.tree` | `?ref=` |
| `GET` | `/repos/{owner}/{repo}/contents/{path}` | `repo.blob` | `?ref=`; base64 content |
| `GET` | `/repos/{owner}/{repo}/languages` | `repo.languages` | |
| `GET` | `/repos/{owner}/{repo}/labels` | `label.listForRepo` | `includeHidden` |
| `GET` | `/repos/{owner}/{repo}/stargazers` | `repo.stargazers.list` | Write+ only |
| `GET`/`POST` | `/repos/{owner}/{repo}/hooks` | `webhook.list` / `webhook.create` | Repo Admin |
| `GET`/`PATCH`/`DELETE` | `/repos/{owner}/{repo}/hooks/{id}` | `webhook.get` / `webhook.update` / `webhook.delete` | Repo Admin |
| `GET`/`POST` | `/repos/{owner}/{repo}/issues` | `issue.list` / `issue.create` | Filters: `state`, `author`, `label`, `assignee`, `q`, `offset`, `limit` |
| `GET`/`PATCH` | `/repos/{owner}/{repo}/issues/{number}` | `issue.get` / `issue.update` (+`close`/`reopen` via `state`) | |
| `GET`/`POST` | `/repos/{owner}/{repo}/issues/{number}/comments` | `issue.comments.list` / `issue.comments.create` | |
| `PATCH`/`DELETE` | `/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}` | `issue.comments.update` / `issue.comments.delete` | Path carries `number` (RPC input requires it) — differs from GitHub's `/issues/comments/{id}` |
| `GET`/`POST` | `/repos/{owner}/{repo}/pulls` | `pull.list` / `pull.create` | `state` incl. `merged`; `review_state` filter |
| `GET`/`PATCH` | `/repos/{owner}/{repo}/pulls/{number}` | `pull.get` / `pull.update` (+`close`/`reopen` via `state`) | |
| `POST` | `/repos/{owner}/{repo}/pulls/{number}/merge` | `pull.merge` | `method`: `merge`\|`squash`\|`rebase` |
| `GET` | `/repos/{owner}/{repo}/pulls/{number}/files` | `pull.files` | |
| `GET` | `/repos/{owner}/{repo}/pulls/{number}/commits` | `pull.commits` | |
| `GET`/`POST` | `/repos/{owner}/{repo}/pulls/{number}/comments` | `pull.comments.list` / `pull.comments.create` | Optional diff placement fields |
| `GET`/`POST` | `/repos/{owner}/{repo}/pulls/{number}/reviews` | `pull.reviews.list` / `pull.reviews.submit` | `state`: `approved`\|`changes_requested`\|`commented` |
| `GET`/`POST` | `/repos/{owner}/{repo}/releases` | `release.list` / `release.create` | Tag must already exist |
| `GET`/`PATCH`/`DELETE` | `/repos/{owner}/{repo}/releases/tags/{tag}` | `release.get` / `release.update` / `release.delete` | Slashed tags %-encoded |
| `GET` | `/admin/users` | `admin.users.list` | Sys-admin session only |
| `GET` | `/admin/lfs/usage` | `admin.lfs.getUsage` | Sys-admin session only |

Not yet covered by v1 (use `/api/rpc`): notifications, SSH/GPG keys, PAT management, email addresses, packages, Actions runs, branch protection, collaborators, invitations, mirrors, LFS objects, issue delete/labels/assignees/reactions/links, pull review dismissal and review requests, `org.updateSettings` and org invites, `repo.rename`/`transfer`/`fork`, `repo.watch`/`star`/`unstar`, most `admin.*` procedures. Release asset upload/download, raw files, and archives keep their dedicated binary routes (`/api/repos/{owner}/{repo}/releases/{release_id}/assets`, `/api/releases/assets/{asset_id}`, `/api/repos/{owner}/{repo}/raw/{ref}/{path}`, `…/archive/{file}`).

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


#### PAT Bearer authentication

Non-browser clients (CLI, MCP, agents, scripts) call `/api/rpc` and `/api/rpc/ws` with a PAT instead of the session cookie:

```bash
curl -s https://oxidean.example.com/api/rpc \
  -H 'content-type: application/json' \
  -H 'Oxidean-RPC-Version: 1' \
  -H 'Authorization: Bearer oxidean_pat_…' \
  -d '{"procedure":"auth.me","input":{}}'
```

Rules:

- The `oxidean_session` cookie always wins — if it is present, `Authorization` is ignored for session resolution.
- Bearer-authenticated calls resolve the **token owner** as the request identity; repository ACL checks behave exactly as if the owner were signed in.
- Bearer responses never carry `Set-Cookie`, and PAT calls never create, refresh, or clear sessions.
- Bad, revoked, or expired tokens → `auth.unauthenticated` (HTTP 401 with `WWW-Authenticate: Bearer`; an RPC error frame on WebSocket). A Bearer header is an explicit credential — invalid tokens are never silently treated as anonymous.
- Failed-auth rate limiting is shared with Git Smart HTTP PAT auth (per-IP and per-user windows); a tripped limiter → `auth.rate_limited` (HTTP 429 with `Retry-After`).

**Scope model.** Procedures are classified at dispatch; insufficient scope → `auth.pat_scope` (HTTP 403).

| Token | Allowed procedures |
| --- | --- |
| Classic `repo` | All repo-domain procedures: repo read/write (issues, pulls, releases, actions, LFS flags), repo administration (collaborators, webhooks, branch protection, mirrors, visibility), plus account-level repo reads (`repo.listMine`, notifications, `user.lookup`) |
| Classic `package:read` | `packages.list` |
| Classic `package:write` | `packages.list`, `packages.deleteVersion` |
| Fine-grained `contents: read` | Repo-domain read procedures on covered repositories |
| Fine-grained `contents: write` | Repo-domain read **and** write procedures on covered repositories |
| Fine-grained `packages: read`/`write` | `packages.list` when `repository_id` names a covered repository |

Fine-grained coverage = `repo_access` `selected` ids, or `all` (personal-owned plus org Owner/Admin repositories — same rule as Git Smart HTTP). Public repositories stay readable with any valid fine-grained token; denied reads on private repositories return `repo.not_found` rather than a scope error, matching the anti-enumeration rule for anonymous callers.

**Never callable with a PAT** (session cookie required, `auth.pat_scope` otherwise): `admin.*` and `packages.admin*` (instance administration), auth lifecycle (`auth.login`/`signup`/`logout`/verify/reset/setup), credential management (`pat.*`, `sshKey.*`, `gpgKey.*`, `email.*`), org mutations and invites, profile writes, and any procedure not in the allowed classes above. Unrecognized procedures fail closed.

WebSocket: send the `Authorization` header on the upgrade request. The credential is re-validated per RPC frame, so revoking or expiring a token takes effect on the next frame.

### SSH public keys (`sshKey.*`)

Register OpenSSH **public** keys for Git-over-SSH (session cookie; never send private keys to the API). Keys map to **full account identity** — there are no PAT-style scopes on SSH transport (D-SSH-03 / D-SSH-04 / D-SSH-05).

`sshKey.add` input:

```json
{ "title": "laptop", "public_key": "ssh-ed25519 AAAA… comment" }
```

Accepted key types: `ssh-ed25519` and RSA ≥2048. Response is a list item with `id`, `title`, `fingerprint` (SHA256), `key_type`, optional `public_key`, `created_at`, and optional `last_used_*`. There is **no** one-time secret field (unlike PAT mint).

`sshKey.list` returns the same item shape for the signed-in user. `sshKey.revoke` input: `{ "id": "…" }` (hard-delete).

Add requires verified email (`auth.email_unverified` otherwise). Empty title → `sshKey.title_required`. Invalid/unsupported key → `sshKey.invalid_key`. Duplicate fingerprint → `sshKey.fingerprint_taken`. More than **25** keys → `sshKey.limit_exceeded`. Unknown or non-owned revoke id → `sshKey.not_found`.

### Deploy keys (`repo.deployKey.*`)

Per-repo OpenSSH **public** keys for Git-over-SSH, distinct from account `sshKey.*` keys (GIT-23). A deploy key resolves a fingerprint directly to **one repository** plus a scope — never to an account identity. They are **transport-only credentials**: a deploy key authorizes `git-upload-pack` (and `git-receive-pack` when `can_write`) over SSH and nothing else — no session, no RPC, no web/API access, no capability on any other repository.

`repo.deployKey.create` input:

```json
{
  "owner": "ada",
  "name": "hello",
  "title": "ci-runner",
  "public_key": "ssh-ed25519 AAAA… comment",
  "can_write": false
}
```

`owner` / `name` select the repository (same lookup as `repo.get`); `can_write` defaults to `false` (read-only). Accepted key types match `sshKey.add` (`ssh-ed25519`, RSA ≥2048). Response is a `DeployKeyPublic` item: `id`, `repo_id`, `title`, `fingerprint` (SHA256), `key_type`, `can_write`, `public_key`, `created_by`, `created_at`, optional `last_used_at` / `last_used_ip`. There is no secret field — nothing to copy on create.

`repo.deployKey.list` input is `{ "owner", "name" }` and returns `{ "keys": [...] }`. `repo.deployKey.delete` input is `{ "owner", "name", "id" }` (hard-delete; revocation takes effect on the next pack exec — in-flight connections are not cut mid-transfer).

Fingerprint rules (deliberate): one key may be attached to **multiple** repositories (one row each — a CI key can read several repos), but not twice to the same repo (`UNIQUE(repo_id, fingerprint)`). A fingerprint already registered as an **account** key is rejected on `deployKey.create`, and `sshKey.add` rejects fingerprints attached as deploy keys — a key is either an account credential or a deploy credential, never both, so a read-only deploy key cannot be silently widened by registering it as an account key. At most **50** deploy keys per repository.

All three procedures require repo **Admin** (`admin.forbidden` otherwise); anonymous → `auth.unauthenticated`. Empty title → `deployKey.title_required`; invalid key → `sshKey.invalid_key` (shared validator); duplicate fingerprint on the repo → `deployKey.fingerprint_taken`; over 50 keys → `deployKey.limit_exceeded`; unknown delete id → `deployKey.not_found`.

Pushes authorized by a deploy key still run the receive-pack protection env (`OXIDEAN_ACTOR_CAPABILITY=write`), so branch protection and archived-repo rules apply unchanged; post-push webhook / PR-sync / Actions attribution uses the admin who attached the key (`created_by`).

**PAT ∩ ACL:** Classic `repo` push/fetch requires the PAT subject to also `meets` the needed Capability on that repository (org membership, collaborator grant, or personal owner — not `owner_id == pat.user_id` alone). Fine-grained `all` covers personal-owned plus org Owner/Admin repos; collaborators must use `selected`.

### OAuth applications (`oauthApp.*`)

Oxidean can act as an OAuth2 authorization server (API-03) so external tools
authenticate users — "sign in with Oxidean" — and act on their behalf.

**Registering an app** (session cookie required):

```json
{ "procedure": "oauthApp.create", "input": { "name": "my-cli", "redirect_uris": ["https://app.example/callback"] } }
```

The create (and `oauthApp.regenerateSecret`) response includes a one-time
plaintext `client_secret` (`oxidean_osec_…`) plus the `app` row — `client_id`
(`oxidean_oc_…`), name, `client_secret_prefix`, `redirect_uris`, timestamps.
Only the SHA-256 hash of the secret is stored. `oauthApp.list` /
`oauthApp.update` / `oauthApp.delete` manage apps you own; update accepts
`{ "id", "name"?, "redirect_uris"? }`. Redirect URIs must be absolute `https`
(`http` only for `localhost` / `127.*` / `::1`), no fragments or userinfo,
max 10 per app.

**Authorization-code flow:**

1. Send the user's browser to `GET /oauth/authorize?response_type=code&client_id=…&redirect_uri=…&scope=…&state=…`. Signed-in users land on the `/oauth/consent` SPA page; anonymous users bounce through `/login?returnTo=` first. `redirect_uri` must byte-match a registered URI or the request fails with a 400 JSON error (errors never redirect to unregistered URIs).
2. On approve, the browser redirects to `redirect_uri?code=…&state=…`; deny yields `error=access_denied`. Codes are single-use and expire after 10 minutes.
3. Exchange the code at `POST /oauth/token` (form-encoded, or JSON; HTTP Basic `client_id:client_secret` also accepted): `grant_type=authorization_code&code=…&redirect_uri=…&client_id=…&client_secret=…` → `{ "access_token": "oxidean_oat_…", "token_type": "bearer", "expires_in": 28800, "scope": "…" }`. Wrong secret → `invalid_client`; used/expired/mismatched code → `invalid_grant`.

**Scopes** (space-delimited): `read:user` (default; identity via
`/oauth/userinfo`), `user:email` (adds `email` to userinfo), `repo` (git smart
HTTP), `package:read` / `package:write` (package registries).

**Using the token:** `GET /oauth/userinfo` with `Authorization: Bearer
oxidean_oat_…` returns `{ id, username, display_name, avatar_url?, email? }`.
OAuth access tokens also authenticate anywhere a PAT does — as the HTTP Basic
password for git clone/push, and as Basic or Bearer credentials on the OCI /
npm / generic package registries — with the granted scopes mapped onto the
classic-PAT scope set.

**Consent + grants:** the consent screen resolves `oauthApp.authorizeInfo`
(`{ "client_id", "redirect_uri"?, "scope"? }` → app name, owner username, parsed
scopes) and submits `oauthApp.authorize` (`{ "client_id", "redirect_uri",
"scope"?, "state"?, "approve" }` → `redirect_to`). Users review live grants via
`oauthApp.listGrants` and cut access with `oauthApp.revoke` (`{ "id" }` — the
application id from the grant row), which soft-revokes every live token for
that app/user pair; revoked tokens fail immediately everywhere.

Minting codes (`oauthApp.authorize` with `approve: true`) and registering apps
require a verified email (`auth.email_unverified` otherwise), matching PAT
minting posture.

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
| **Payload** | Rows include `reason`, `subject_kind` (`issue` \| `pull_request` \| `release` \| `workflow_run` \| `push`), `owner` / `repo` slugs, `subject_number`, `subject_title`, `subject_ref` (tag / run id / ref for non-numbered subjects), `actor_username`. Deep links: `/{owner}/{repo}/issues\|pull/{n}` for numbered subjects; `/{owner}/{repo}/releases/{tag}`, `/{owner}/{repo}/actions/{run_id}`, `/{owner}/{repo}/commits/{ref}` via `subject_ref`. |
| **Fan-out** | Domain writes (e.g. `issue.comments.create`) insert best-effort rows; actors are never notified (workflow-run completion includes the triggering user). Activity email is out of scope. |
| **Access** | `notification.list` / `notification.unreadCount` re-check repository read access at read time; rows for repos the recipient can no longer read are pruned together with the watch row (GitHub auto-unwatch on access loss). ACL mutations — collaborator remove/update, visibility flip to private, org member remove/demote or `member_base_permission` change, transfer, soft-delete — sweep eagerly. |

Client surface: `client.notification.*` in `@oxidean/api-client` (regenerate with `make rpc-gen`).

### Atom feeds (API-05)

Three `application/atom+xml; charset=utf-8` feeds sit under `/api` so the edge
gateway routes them to the API service. Each returns the newest **30** entries
with stable `urn:uuid:` entry ids, RFC 3339 `<published>` / `<updated>`
timestamps, and HTML permalinks built from the resolved public origin
(`OXIDEAN_PUBLIC_ORIGIN`).

| Feed | Path | HTML alternate |
| --- | --- | --- |
| Repo activity | `GET /api/repos/{owner}/{repo}/activity.atom` | `/{owner}/{repo}/activity` |
| Repo releases | `GET /api/repos/{owner}/{repo}/releases.atom` | `/{owner}/{repo}/releases` |
| User activity | `GET /api/users/{username}/activity.atom` | `/{username}` |

Repo feeds enforce the same Capability ACL as `repo.activity.list`: anonymous or
unauthorized reads of private repos answer the identical `repo.not_found`
**404** (no enumeration). Session cookie is honored, so a user with Read can
subscribe to a private repo's feed. Draft releases appear only for Write+
callers (same rule as `release.list`).

The **user feed is public-only** — it is filtered at the SQL layer
(`repositories.visibility = 'public'`), so private-repo pushes never leak
titles, branch names, or links regardless of the caller's session.

Feed autodiscovery: the repo page, activity page, releases page, and user
profile emit `<link rel="alternate" type="application/atom+xml">` pointing at
the matching feed.

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

Clone / fetch / push over SSH use an in-process listener (Compose TCP **2222** by default — not Traefik). Remotes are **scp-style** `git@{host}:{owner}/{repo}.git` (D-SSH-02). The SSH username must be `git`; identity comes only from a registered public-key fingerprint (full account ACL — no PAT scopes). When the fingerprint matches no account key, the handshake falls back to repo **deploy keys** (`repo.deployKey.*` above): the key authenticates the connection, and each `git-upload-pack` / `git-receive-pack` exec re-checks that the fingerprint is attached to the target repo (`can_write` required for push). Deploy keys are transport-only — no RPC/web access — and are rate-limited like account keys. When advertised port ≠ 22, clients set `Port` in `~/.ssh/config` (or `ssh -p`); do not treat `ssh://` as the primary CloneBox URL.

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
| `deployKey.title_required` | Deploy key title empty |
| `deployKey.fingerprint_taken` | Key already attached to that repo, or registered as an account SSH key |
| `deployKey.limit_exceeded` | More than 50 deploy keys on the repository |
| `deployKey.not_found` | Delete target missing or not on that repo |
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

## MCP endpoint (AGT-01)

`POST /api/mcp` is a [Model Context Protocol](https://modelcontextprotocol.io)
server over the streamable-HTTP transport (single JSON-RPC 2.0 message per
request, `application/json` responses, `202` for notifications). It exposes
repositories, issues, pull requests, Actions runs, packages, and search as MCP
tools, plus file/issue/PR bodies as `oxidean://` resources — each call is a thin
wrapper over the same typed RPC handlers and ACL checks as `/api/rpc`.

Auth: `oxidean_session` cookie or `Authorization: Bearer` with a classic
(`oxidean_pat_…`) / fine-grained (`oxidean_fg_…`) PAT. Presented-but-invalid
credentials return `401` + `WWW-Authenticate: Bearer`; missing credentials run
as anonymous. Full method/tool/resource list and scope mapping: [MCP.md](MCP.md).

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
