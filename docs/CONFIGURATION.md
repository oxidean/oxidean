# Configuration

Oxidean is configured primarily through environment variables. Canonical templates live in [`.env.example`](../.env.example) (Compose / local API) and [`docs/dev-auth.env.example`](dev-auth.env.example) (local auth/email stubs). Copy templates into untracked files such as `.env` or `.env.dev-auth` — never commit secrets.

Related docs: [database.md](database.md), [dev-auth.md](dev-auth.md).

## Environment variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | Optional* | _(unset)_ | Connection URL. Scheme selects dialect: `postgres://` / `postgresql://`, `mysql://`, or `sqlite:` (also `sqlite://`, `sqlite::memory:`). `mariadb://` is not supported. If unset, the API boots without a DB pool. |
| `OXIDEAN_DB_DIALECT` | Optional | _(inferred from URL)_ | Explicit dialect: `postgres`, `mysql`, or `sqlite`. When set, must agree with `DATABASE_URL` or boot exits. |
| `OXIDEAN_AUTO_MIGRATE` | Optional | `true` | When `true`/`1`, run sqlx migrations on API boot if a database is configured. Oxidean Cloud sets this to `true` on all environments (including production) so promote applies schema. Self-host prod-like Compose may set `false` and run `make db-migrate`. |
| `OXIDEAN_ENV` | Optional | `development` | Runtime mode. Affects CORS, session cookie `Secure`, and whether `OXIDEAN_OIDC_ALLOW_INSECURE` is honored. Common values: `development`, `dev`, `compose`, `production`. |
| `OXIDEAN_CORS_ORIGINS` | Conditional | _(none)_ | Comma-separated browser origins. **Required** when `OXIDEAN_ENV` is not `development` or `dev` (e.g. `compose`). Ignored for CORS allowlisting in `development`/`dev` (request origin is mirrored). |
| `API_BIND` | Optional | `0.0.0.0:8080` | Listen address for `oxidean-api`. Host `make` workflows often use `127.0.0.1:8080`. |
| `OXIDEAN_RPC_VERSION` | Optional | `1` (Compose) | Documented / passed through Compose. The generated TS client embeds `RPC_VERSION = 1` and sends header `Oxidean-RPC-Version`. |
| `RUST_LOG` | Optional | `info` (Compose) | `tracing` / `EnvFilter` log directives. |
| `OXIDEAN_PUBLIC_ORIGIN` | Optional | `http://localhost:8080` (API fallback when unset) | Browser-facing origin for **verify/reset magic links**, **invite URLs** (`{origin}/invites/{token}` — emailed and shareable links), SSO redirect URIs, and **HTTPS git clone URLs** (`{origin}/{owner}/{repo}.git`, D-19). Trailing slash is stripped. Compose defaults to `http://localhost` via `OXIDEAN_COMPOSE_PUBLIC_ORIGIN` so a host Vite value (`http://localhost:3000`) in `.env` does not poison container mail. On the **web** service, the hostname is also merged into Vite `preview`/`server` `allowedHosts` (see `OXIDEAN_VITE_ALLOWED_HOSTS`). |
| `OXIDEAN_VITE_ALLOWED_HOSTS` | Optional (web) | _(empty → Vite localhost defaults)_ | Comma-separated hostnames for Vite’s DNS-rebinding guard (`server.allowedHosts` / `preview.allowedHosts`). Required whenever browsers hit `vite preview` via a non-localhost Host (Railway gateway domains, custom domains). A leading dot is a suffix match (e.g. `.up.railway.app` allows every `*.up.railway.app`). Do not hardcode operator domains in source — set this per deploy. Example cloud value: `.up.railway.app,app.oxidean.dev`. |
| `OXIDEAN_API_ORIGIN` | Required (web, cloud/SSR) | `http://127.0.0.1:8080` | Origin the **web** process uses for SSR / server-fn RPCs (`/api/rpc`). Browser clients use `window.location.origin` (same-origin through the gateway). On Railway set to the private API, e.g. `http://api.railway.internal:8080`. Leaving the default causes `ConnectionRefused` to `127.0.0.1:8080` and “Can't reach Oxidean” on `/setup`. |
| `OXIDEAN_COMPOSE_PUBLIC_ORIGIN` | Optional (Compose) | `http://localhost` | Sets `OXIDEAN_PUBLIC_ORIGIN` inside the API container (`make up` / `make up-with-dev-auth`). |
| `OXIDEAN_ADMIN_EMAIL` | Optional | _(unset)_ | With `OXIDEAN_ADMIN_PASSWORD`, seeds one `sys-admin` on an **empty instance** (username `system-administrator`, `must_change_credentials` until `/setup/credentials`). Set both **before first boot**. If either/both unset and users is empty, the SPA `/setup` wizard creates the first `sys-admin` instead. Same path for cloud and self-host — no deployment-mode fork. Seed failure with both set → **fail boot** (exit 1). |
| `OXIDEAN_ADMIN_PASSWORD` | Optional | _(unset)_ | Paired with `OXIDEAN_ADMIN_EMAIL` for empty-instance `sys-admin` seed. Placeholders only in examples — never commit real passwords. |
| `OXIDEAN_ALLOW_SIGNUP` | Optional | `false` | When `true`/`1`, ENV seed (and wizard default control) opens local signup (`allow_signup`). Default **false** (fail closed). After bootstrap, operators can change it via Admin → Auth. Cloud deploys that want open signup should set `true` in manifests. |
| `OXIDEAN_RESEND_API_KEY` | Optional | _(unset)_ | Resend API key. Prefer this over SMTP when set (ENV boot selection). |
| `OXIDEAN_RESEND_BASE_URL` | Optional | `https://api.resend.com` | Override Resend HTTP API base (local stubs). Leave unset in production. |
| `OXIDEAN_SMTP_URL` | Optional | _(unset)_ | SMTP URL for lettre (e.g. `smtp://127.0.0.1:1025`). Used when Resend key is unset / provider is smtp. |
| `OXIDEAN_MAIL_FROM` | Optional | `Oxidean <noreply@localhost>` | Default From address when DB settings do not override. |
| `WORKOS_API_KEY` | Optional† | _(unset)_ | WorkOS API key (enterprise SSO). Required for WorkOS provider mode. |
| `WORKOS_CLIENT_ID` | Optional† | _(unset)_ | WorkOS AuthKit client id. |
| `OXIDEAN_WORKOS_BASE_URL` | Optional | WorkOS SDK cloud default | Server-side WorkOS API base override for local stubs. Leave unset in production. |
| `OXIDEAN_WORKOS_AUTHORIZE_BASE_URL` | Optional | Same as `OXIDEAN_WORKOS_BASE_URL` / cloud | Browser-facing authorize base when it differs from the API base (e.g. Compose networking). |
| `OXIDEAN_OIDC_ISSUER` | Optional† | _(unset)_ | OIDC issuer URL. Required for OIDC provider mode. |
| `OXIDEAN_OIDC_CLIENT_ID` | Optional† | _(unset)_ | OIDC client id. |
| `OXIDEAN_OIDC_CLIENT_SECRET` | Optional† | _(unset)_ | OIDC client secret. |
| `OXIDEAN_OIDC_ALLOW_INSECURE` | Optional | unset / false | When `1`/`true`/`yes` **and** `OXIDEAN_ENV` is `development`, `dev`, or `compose`, allows http/loopback issuers for local mocks. Never honored in production-like envs. |
| `POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB` | Compose | `oxidean` | Postgres container credentials; must match `DATABASE_URL` for the default stack. |
| `MYSQL_USER` / `MYSQL_PASSWORD` / `MYSQL_DATABASE` / `MYSQL_ROOT_PASSWORD` | MySQL profile | `oxidean` | MySQL container credentials (`docker-compose.mysql.yml`). |
| `MYSQL_DATABASE_URL` | MySQL profile | `mysql://oxidean:oxidean@mysql:3306/oxidean` | Overrides API `DATABASE_URL` when using the MySQL Compose overlay. |
| `OXIDEAN_SQLITE_HOST_DIR` | SQLite overlay | `./var` | Host path bind-mounted to `/app/var` for SQLite file storage. |
| `OXIDEAN_REPOS_DIR` | Optional | `var/repos` | Root for bare git repositories (`{owner}/{name}.git`). Compose binds `./var/repos:/var/repos`; with API CWD `/` the default resolves to `/var/repos` without overriding the env var. |
| `OXIDEAN_GIT_REPO_QUOTA_BYTES` | Optional | `10737418240` (10 GiB) | Default per-repo git object size quota (bare repo disk usage). `0` or negative = unlimited. `admin.git.updateSettings` overrides; repo admins can set a per-repo override in Settings (GIT-25). |
| `OXIDEAN_LFS_DIR` | Optional | `var/lfs` | Instance-wide Git LFS object store (OID-sharded). Compose binds `./var/lfs:/var/lfs` and sets `OXIDEAN_LFS_DIR=/var/lfs`. |
| `OXIDEAN_LFS_MAX_OBJECT_BYTES` | Optional | `2147483648` (2 GiB) | Max single LFS object size. `0` disables. Admin UI can override. |
| `OXIDEAN_LFS_QUOTA_REPO_BYTES` | Optional | `10737418240` (10 GiB) | Default per-repo logical LFS quota. `0` disables. Admin UI can override. |
| `OXIDEAN_LFS_QUOTA_USER_BYTES` | Optional | `53687091200` (50 GiB) | Default per-user (repo-owner) logical LFS quota. `0` disables. Admin UI can override. |
| `OXIDEAN_LFS_GC_INTERVAL_SECS` | Optional | `86400` (24h) | Periodic GC of unreferenced LFS OIDs. Set `0` to disable. |
| `OXIDEAN_LFS_GC_GRACE_SECS` | Optional | `604800` (7d) | Grace period after refcount reaches 0 before OID delete. |
| `OXIDEAN_PACKAGES_DIR` | Optional | `var/packages` | Root for content-addressed package blobs (OCI/npm/generic). Compose binds `./var/packages:/var/packages` and sets `/var/packages`. **Must not** share `OXIDEAN_LFS_DIR` or release-asset paths (D-PKG-07). |
| `OXIDEAN_PACKAGES_MAX_BLOB_BYTES` | Optional | `2147483648` (2 GiB) | Reject uploads larger than this size (D-PKG-09). |
| `OXIDEAN_PACKAGES_OWNER_QUOTA_BYTES` | Optional | `10737418240` (10 GiB) | Default per-owner storage quota; Admin may override per owner (D-PKG-09). |
| `OXIDEAN_PACKAGES_GC_INTERVAL_SECS` | Optional | `86400` | Package blob GC interval; `0` disables. |
| `OXIDEAN_PACKAGES_GC_GRACE_SECS` | Optional | `604800` (7d) | Grace before deleting refcount-0 blobs. |
| `OXIDEAN_TEMPLATE_PACKS_DIR` | Optional | `var/template-packs` | Content-addressed instance template zip store (`sha256/{aa}/{bb}/{digest}.zip`). Compose binds `./var/template-packs:/var/template-packs` and sets `/var/template-packs`. **Must not** share packages/LFS/repos paths. |
| `OXIDEAN_TEMPLATE_PACK_MAX_BYTES` | Optional | `10485760` (10 MiB) | Max uploaded instance template zip size. |
| `OXIDEAN_CLI_DIST_DIR` | Optional | `/usr/local/share/oxidean-cli` | Directory of `ox-{target-triple}` binaries the API serves at `/cli/bin/*` for `curl /cli/install.sh | sh` and `ox self-update`. Release image ships the server's own platform; drop extra targets into this dir. See [CLI.md](CLI.md). |
| `OXIDEAN_ACTIONS_LOG_DIR` | Optional | `var/actions-logs` | Root for Actions job logs (`{run_id}/{job_id}.log`). Compose binds `./var/actions-logs:/var/actions-logs` and sets `/var/actions-logs`. **Must not** share repos/LFS/packages/release-asset paths (D-ACT-13). |
| `OXIDEAN_ACTIONS_ENABLED` | Optional | `true` | Instance-wide Actions gate. When `false`/`0`/`off`, no workflows are evaluated (D-ACT-06). Per-repo Admin toggle still applies when instance gate is on. |
| `OXIDEAN_MCP_ENABLED` | Optional | `true` | Instance gate for the MCP endpoint (`POST /api/mcp`). Sys-admins can override at runtime under **Admin → MCP endpoint** (`admin.mcp.updateSettings`); clearing the override reverts to this env default. Disabled → `404 mcp.disabled`. See [MCP.md](MCP.md). |
| `OXIDEAN_RUNNER_REGISTRATION_TOKEN` | Optional | — | Bootstrap registration token for official runners (Compose profile `actions`). **Reusable while set** — never leave on an internet-facing API; prefer `admin.actions.createRegistrationToken` (one-time). Unset after local runner bootstrap. Rotate on compromise (D-ACT-08). **Never commit real tokens.** |
| `OXIDEAN_RUNNER_NAME` | Optional | `compose-runner` | Display name passed to runner register. |
| `OXIDEAN_RUNNER_LABELS` | Optional | `ubuntu-latest:docker://node:20-bookworm,self-hosted` | Comma-separated runner labels (`label[:schema[:args]]`). |
| `OXIDEAN_ACTIONS_SECRETS_KEY` | Required for secrets* | Compose: `compose-dev-actions-secrets-key-not-for-production` (or `OXIDEAN_SESSION_SECRET`) | AES-256-GCM key material for repo Actions secrets at rest (D-ACT-17). Encrypt/decrypt **fail closed** if neither env is set (no hardcoded app fallback). Compose/`make up` supplies a local-only default so secrets work out of the box; **production must set a unique key** — never reuse the Compose default. Prefer a dedicated secret; never commit real keys. |
| `OXIDEAN_SSH_ENABLED` | Optional | unset / false | When `true`/`1`/`yes`, start the in-process Git-over-SSH listener (`russh`). Compose defaults to `true`. Host `make dev` omits the listener unless set. |
| `OXIDEAN_PROTECTION_HELPER` | Optional (API) | sibling of `oxidean-api` / Compose+Cloud: `/usr/local/bin/oxidean-protection-hook` | Absolute path to `oxidean-protection-hook` for bare-repo `hooks/update`. Required in `production`/`cloud` (fail-closed if missing). Compose and Railway IaC set the image path; Smart HTTP / SSH also inject it for receive-pack. |
| `OXIDEAN_SSH_PORT` | Optional | `2222` | **Listen and advertise** port (single knob). Compose publishes host `2222:2222`. When ≠ 22, clients need `~/.ssh/config` `Port` (CloneBox shows a Port hint; primary URL stays scp-style). |
| `OXIDEAN_SSH_HOST` | Optional | hostname of `OXIDEAN_PUBLIC_ORIGIN` (fallback `localhost`) | Advertised hostname for CloneBox / smoke scp-style URLs `git@{host}:{owner}/{repo}.git`. |
| `OXIDEAN_SSH_HOST_KEY_DIR` | Optional | `var/ssh` (Compose `/var/ssh`) | Persist Ed25519 host keys across restarts (TOFU). Also stores the instance **web-flow** SSH signing key (`web-flow` / `web-flow.pub`) used to sign template seed commits. Compose uses a named volume. |
| `OXIDEAN_WEB_FLOW_PRIVATE_KEY` | Optional | unset | Provisioned web-flow commit-signing key as an OpenSSH private key (PEM, or single-line base64-encoded PEM; unencrypted). When set, the API materializes `web-flow` / `web-flow.pub` under `OXIDEAN_SSH_HOST_KEY_DIR` — deterministic identity across fresh volumes and the required provisioning path for `production`/`cloud` (auto-generation is disabled there). Never commit a real key. |
| `OXIDEAN_ORPHAN_RECONCILE_INTERVAL_SECS` | Optional | `86400` (24h) | In-process orphan reconcile interval. Removes bare dirs with no DB row and purges soft-deleted repos past retention. Set `0` to disable. |
| `OXIDEAN_SOFT_DELETE_RETENTION_DAYS` | Optional | `14` | Days to keep soft-deleted repository rows/files before orphan reconcile hard-deletes them. |
| `OXIDEAN_REPO_REDIRECT_RETENTION_DAYS` | Optional | `90` | Days to keep `repository_redirects` after rename/transfer so old `/{owner}/{repo}` and Smart HTTP/SSH paths keep resolving. Expired rows are purged by orphan reconcile. |
| `OXIDEAN_RELEASE_ASSETS_DIR` | Optional | `var/release-assets` | Directory for release binary assets keyed by opaque `asset_id` (not the LFS OID store). Compose binds `./var/release-assets:/var/release-assets`. |
| `OXIDEAN_RELEASE_ASSET_MAX_BYTES` | Optional | `536870912` (512 MiB) | Max multipart size for a single release asset upload. |
| `OXIDEAN_GIT_GC_INTERVAL_SECS` | Optional | `604800` (7d) | In-process scheduled `git gc --auto` across active repos. Set `0` to disable. Sys-admins can also trigger `admin.repos.gc` manually. |
| `OXIDEAN_WEBHOOK_MAX_ATTEMPTS` | Optional | `5` | Max delivery attempts per webhook event (retries on 5xx/timeout/connection errors). |
| `OXIDEAN_WEBHOOK_TIMEOUT_SECS` | Optional | `10` | Outbound webhook HTTP timeout. |
| `OXIDEAN_WEBHOOK_WORKER_INTERVAL_SECS` | Optional | `5` | Pending-delivery drain interval. Set `0` to disable the retry worker. |
| `OXIDEAN_WEBHOOK_RETENTION_DAYS` | Optional | `30` | Soft retention hint for delivery history (UI/list caps also apply). |
| `OXIDEAN_MIRROR_POLL_TICK_SECS` | Optional | `30` | How often the API wakes the two-way mirror poller to enqueue mirrors whose per-repo `poll_interval_secs` elapsed. Set `0` to disable the ticker (event-driven sync + Sync Now still work). Per-mirror interval defaults to `60`; `0` on a mirror disables that mirror’s poll backstop. |

\* Strongly recommended for any real instance; without it the API runs with a skipped DB pool.  
† Required only when the corresponding auth provider mode is enabled (Admin → Auth / ENV bootstrap).

Dev-auth Compose port overrides (see `docker-compose.dev-auth.yml`): `OXIDEAN_MAILPIT_SMTP_PORT` (1025), `OXIDEAN_MAILPIT_UI_PORT` (8025), `OXIDEAN_OIDC_MOCK_PORT` (9090), `OXIDEAN_STUBS_PORT` (9092).

## Package registry storage

Package blobs live under `OXIDEAN_PACKAGES_DIR` (default `var/packages`). Layout is content-addressed: `{OXIDEAN_PACKAGES_DIR}/{algo}/{aa}/{bb}/{digest}` with DB refcounts for cross-package dedup (D-PKG-07 / D-PKG-08). This volume is **separate** from bare repos, Git LFS, and release assets.

**Compose:** Default stack mounts `./var/packages:/var/packages` and sets `OXIDEAN_PACKAGES_DIR=/var/packages`. Defaults for max blob size (2 GiB) and per-owner quota (10 GiB) are documented above; Admin UI (`/admin/packages`) and `packages.adminSetQuota` override per owner.

**Edge paths:** Registry protocols are served on the same host at `/v2` (OCI), `/npm` (npm), and `/generic` (raw). Traefik and Vite must route these prefixes to the API before the SPA (see Compose `api-packages` router, priority ≥110). Smoke: `make smoke-packages` (skip-ok without Docker).

**GC:** `OXIDEAN_PACKAGES_GC_INTERVAL_SECS` (default 86400) runs in-process GC; `OXIDEAN_PACKAGES_GC_GRACE_SECS` (default 7d) delays deletion of refcount-0 blobs. Live layers shared across packages are never GC'd while referenced.

**Auth:** PAT `package:read` / `package:write` (or FG Packages perm) ∩ forge ACL. Classic `repo` does not include packages. Cookies ignored on registry paths. See [API.md](API.md#packages-registry-oci--npm--generic).

## Git repositories & disk lifecycle

Bare repos live under `OXIDEAN_REPOS_DIR` (default `var/repos`). Layout: `{OXIDEAN_REPOS_DIR}/{owner}/{name}.git`.

**Backend seam (D-32 / GIT-09 / GIT-10):** Forge operations use the `GitBackend` trait; Phase 7 ships **`CliGitBackend`** (system `git`). A future **`GixGitBackend`** (gitoxide) is not the primary backend — see [ARCHITECTURE.md](ARCHITECTURE.md#git-forge-gitbackend).

**Git version floor (D-33):** The API **fails boot** (exit 1) if the `git` binary is missing or older than **2.5.0**. The API container image installs distro `git`; host `make` workflows require a local git ≥ 2.5 on `PATH`.

**Compose volume (D-30 / D-31):** Default stack mounts `./var/repos:/var/repos` on the API service so repository objects survive container recreation. Do not point `OXIDEAN_REPOS_DIR` outside that volume unless you also update the bind mount.

**Ownership (D-38):** Repository files are owned by the **API process user**. On Compose, ensure the bind-mounted `./var/repos` is writable by that user. If you run the API as a non-root UID/GID (for example via `user:` or a custom image), set host directory ownership to match (`chown UID:GID ./var/repos`) so create/gc/orphan purge can write. Avoid mounting the volume as root-owned when the process cannot write.

**Cleanup knobs (D-36 / D-37):**

| Knob | Default | Effect |
|------|---------|--------|
| `OXIDEAN_ORPHAN_RECONCILE_INTERVAL_SECS` | 86400 | Periodic scan: delete orphan bare dirs; purge soft-deletes past retention |
| `OXIDEAN_SOFT_DELETE_RETENTION_DAYS` | 14 | Soft-delete grace period before disk + row removal |
| `OXIDEAN_REPO_REDIRECT_RETENTION_DAYS` | 90 | Rename/transfer redirect TTL before purge |
| `OXIDEAN_GIT_GC_INTERVAL_SECS` | 604800 | Scheduled `git gc --auto` on active repos |

**Git object size quota (GIT-25):** The bare repo itself is metered as apparent on-disk size (all files under `{owner}/{name}.git`; hardlinked inodes counted once, symlinks not followed). Enforcement runs in the bare repo `hooks/update` via the protection helper: while a push is in flight the incoming pack still sits in the receive quarantine under `GIT_DIR`, so the check measures the *projected* post-push size and rejects the ref update when it exceeds the effective quota. This covers Smart HTTP, SSH receive-pack, and internal pushes (web merges, seeds, mirror ref writes) — all flow through `hooks/update`. Resolution order: per-repo override (`repositories.size_quota_bytes`, repo Admin in Settings) → instance Admin override (`admin.git.updateSettings` / `instance_git_settings`) → `OXIDEAN_GIT_REPO_QUOTA_BYTES` → 10 GiB built-in; `<= 0` disables. Ref *deletes* are never quota-blocked (an over-quota repo can still clean up), and Admin capability does not bypass the check. Mirror *fetches* land objects without `hooks/update`, so a repo already over quota pauses mirror syncs; one fetch can overshoot before the next sync blocks — same "block the next write once over" semantics as pushes. `repositories.size_bytes` is a cached bookkeeping column refreshed after pushes/merges/mirror syncs and by `repo.quota.get`; enforcement always measures live disk state. Environments without the protection helper (no `OXIDEAN_PROTECTION_HELPER` in dev/compose) fail open on all hook checks including quota.

Factory reset (Admin → Auth danger zone) offers **Database only** (keep files) vs **Database and repositories** (wipe children under `OXIDEAN_REPOS_DIR` **and** `OXIDEAN_LFS_DIR`). Reset always wipes issue-domain rows via repository/org CASCADE (no extra env knobs).

## Git LFS

Phase 14 adds filesystem-backed **Git LFS** over HTTPS (batch + basic transfer). Architecture: [ARCHITECTURE.md](ARCHITECTURE.md#git-lfs). Client paths and auth: [API.md](API.md#git-lfs).

| Env | Default | Role |
| --- | --- | --- |
| `OXIDEAN_LFS_DIR` | `var/lfs` | Content-addressed OID store (`{ab}/{cd}/{oid}`) |
| `OXIDEAN_LFS_MAX_OBJECT_BYTES` | 2 GiB | Reject oversized uploads |
| `OXIDEAN_LFS_QUOTA_REPO_BYTES` | 10 GiB | Per-repo logical quota |
| `OXIDEAN_LFS_QUOTA_USER_BYTES` | 50 GiB | Per-user (repo-owner) logical quota |
| `OXIDEAN_LFS_GC_INTERVAL_SECS` | 86400 | Unreferenced OID GC interval (`0` = off) |
| `OXIDEAN_LFS_GC_GRACE_SECS` | 604800 | Grace after refcount=0 before delete |

**Compose volume (D-LFS-01 / GIT-13):** Default stack mounts `./var/lfs:/var/lfs` and sets `OXIDEAN_LFS_DIR=/var/lfs` (API CWD is `/`). Keep the bind mount in sync if you change the path.

**Admin overrides (D-LFS-13):** Instance Admin UI can override max-object / quota defaults without restart. Env values remain the fallback when Admin fields are unset.

**Per-repo enable (D-LFS-10):** Only repository **Admin** may enable/disable LFS for a repo (Settings → LFS). Disabled repos reject Batch uploads.

**Auth (D-LFS-09):** Same as Smart HTTP — **personal access token** via HTTP Basic; **session cookies are ignored**. Read for download; Write + verified email for upload. Classic `repo` / fine-grained `contents` scopes (no dedicated `lfs` scope).

**Discovery path:** Clients use `/{owner}/{repo}.git/info/lfs` under the same Traefik `.git` PathRegexp as Smart HTTP — no extra router is required.

**SSH remotes (D-LFS-05):** Git-over-SSH does **not** carry LFS bytes in Phase 14. When the git remote is SSH, `git-lfs` still transfers objects over **HTTPS** using a credential helper / stored PAT. LFS-over-SSH is deferred.

**Client attributes (D-LFS-17):** Operators/users track patterns with `git lfs track` and commit `.gitattributes` themselves. The server does **not** auto-commit attributes when LFS is enabled.

## Issues

Phase 11 issues/labels reuse existing forge ACL and `OXIDEAN_PUBLIC_ORIGIN` for markdown autolink targets. **No new environment variables.** Closing-keyword auto-close from commits/PRs remains deferred (D-ISS-15) until Phase 12.

## Git Smart HTTP & personal access tokens

Phase 8 adds Git **Smart HTTP** on `/{owner}/{repo}.git` and **personal access tokens (PATs)** for HTTPS git auth. Procedures and error codes: [API.md](API.md). Architecture: [ARCHITECTURE.md](ARCHITECTURE.md#git-smart-http--pats).

**Clone URL host (D-19):** Displayed clone URLs use `OXIDEAN_PUBLIC_ORIGIN` (not the request `Host` header). Set this to the browser-facing origin operators expect in `git clone` copy (Compose: `OXIDEAN_COMPOSE_PUBLIC_ORIGIN` → API/web containers).

**Traefik `.git` routing:** Default Compose routes Smart HTTP to the API **before** the SPA catch-all:

```text
Host(`localhost`) && PathRegexp(`^/[^/]+/[^/]+\.git`)   # priority 110 → api
```

Bare `/{owner}/{repo}` (no `.git` suffix) stays on the web UI. Self-hosted reverse proxies must mirror this PathRegexp (or equivalent) so `git clone` / `ls-remote` / `push` hit the API, not HTML.

**CGI / disk:** Smart HTTP spawns `git-http-backend` with `GIT_PROJECT_ROOT` = `OXIDEAN_REPOS_DIR`. No extra env vars are required for CGI beyond the existing repos root and a system `git` ≥ 2.5 with `git-http-backend` on the exec path (API image installs distro git).

**PAT prefixes (redacted examples only):** classic `oxidean_pat_REDACTED`, fine-grained `oxidean_fg_REDACTED`. Mint via Settings → Tokens (RPC `pat.*` with session cookie). PATs authenticate Smart HTTP over HTTPS via HTTP Basic (password = token), the package registries, and typed `/api/rpc` via `Authorization: Bearer <pat>` when no session cookie is sent — the scope model is in [API.md](API.md#pat-bearer-authentication).

**Failed-auth rate limit (single replica):** Failed Basic/PAT attempts (including RPC `Bearer` auth on `/api/rpc`) are limited **in-process** (20 failures per client IP and 10 per username per 15 minutes → HTTP `429` + `Retry-After`). Client IP is taken from the **rightmost** `X-Forwarded-For` hop (the address appended by the trusted reverse proxy). **Do not expose the API directly to the internet without a proxy that overwrites or sanitizes forwarded headers** — otherwise clients can spoof leftmost XFF hops and bypass the per-IP window. Counters are **not** shared across API replicas — multi-replica deployments need an external / shared limiter or sticky single replica for this control.

## Git over SSH

Phase 9 adds Git **clone/fetch/push over SSH** beside Smart HTTP. Keys are registered via session RPC `sshKey.*` (Settings → SSH keys). Per-repo **deploy keys** (read or read/write scope, GIT-23) are managed by repo admins via `repo.deployKey.*` (repository Settings → Deploy keys); they are transport-only credentials — no session, RPC, or web access. Architecture: [ARCHITECTURE.md](ARCHITECTURE.md#git-over-ssh). API shapes: [API.md](API.md).

| Env | Role |
| --- | --- |
| `OXIDEAN_SSH_ENABLED` | Gate in-process `russh` listener |
| `OXIDEAN_SSH_PORT` | Listen **and** advertise port (Compose default **2222**) |
| `OXIDEAN_SSH_HOST` | Advertised hostname for CloneBox / smoke |
| `OXIDEAN_SSH_HOST_KEY_DIR` | Persist host keys (Compose volume `/var/ssh`) and the web-flow commit-signing key pair |
| `OXIDEAN_WEB_FLOW_PRIVATE_KEY` | Optional provisioned web-flow signing key (PEM or base64 PEM); materialized to `web-flow`/`web-flow.pub` when set |

**Web-flow signing key:** Under `OXIDEAN_SSH_HOST_KEY_DIR`, Oxidean keeps an Ed25519 key pair named `web-flow` / `web-flow.pub` (comment `oxidean-web-flow`). New repositories seeded from the UI are authored as the creating user and **SSH-signed** with this key (`gpg.format=ssh`). Resolution order: `OXIDEAN_WEB_FLOW_PRIVATE_KEY` (env var wins — files are refreshed from it, so rotation is a variable change), then existing `web-flow`/`web-flow.pub` files, then auto-generation via `ssh-keygen` on first use. Auto-generation applies everywhere **except** `production`/`cloud`, which fail closed until the key is provisioned via the env var or keypair files on the volume. Persist the directory across restarts so signatures keep verifying.

**User GPG keys:** Uploading and verifying OpenPGP commit signatures requires the `gpg` binary on the API host (`PATH`). The API Docker image installs `gnupg`. Local `cargo run` needs system GnuPG the same way.

**Remote URL (D-SSH-02):** Always scp-style `git@{OXIDEAN_SSH_HOST}:{owner}/{repo}.git`. Do **not** treat `ssh://` as the primary CloneBox string. When `OXIDEAN_SSH_PORT` ≠ 22, set `Port` under a matching `Host` in `~/.ssh/config` (or pass `-p`).

**Identity (D-SSH-03):** SSH username must be `git`. Account identity is the registered public-key **fingerprint** (full account ACL — no PAT scopes).

**Edge (D-SSH-07):** Publish **raw TCP** on the API service (`2222:2222` in Compose). **Do not** route SSH through Traefik HTTP. Host `make dev` may omit the listener unless `OXIDEAN_SSH_ENABLED` is set.

**Smoke:** `make smoke-git-ssh` (skips exit 0 when Docker is missing). See `scripts/smoke-git-ssh.sh`.

**Failed pubkey rate limit:** Same windows as Smart HTTP PAT failures (20/IP, 10/fingerprint per 15 minutes); successful auth clears the fingerprint bucket.

## Two-way repository mirroring

Existing repos can attach **one** two-way remote (HTTPS token or SSH deploy key). Behavior and RPC: [API.md](API.md#two-way-repository-mirroring). UI: repository Settings → Two-way mirror. Sync mode is per mirror: **`merge`** (default, FF/merge/PR) or **`exact`** (1:1 LWW + delete propagation).

| Env / setting | Role |
| --- | --- |
| `OXIDEAN_MIRROR_POLL_TICK_SECS` | Instance ticker that checks enabled mirrors (default **30**; `0` disables the ticker) |
| Per-mirror `poll_interval_secs` | Backstop only when inbound webhooks are missing (default **60**; `0` = no poll for that mirror) |
| `OXIDEAN_ACTIONS_SECRETS_KEY` | Encrypts git credentials, SSH private keys, and inbound webhook secrets at rest (same key as Actions secrets) |

**Primary sync path is event-driven** (local git activity + inbound push webhook + Sync Now). The poll is a backstop for SSH-only remotes or forges that cannot POST to this instance — not an 8-hour timer.

**SSH deploy keys:** generate or paste a private key in Settings; copy the public key to the remote as a deploy key (write access required for push-out). Paste the remote host line into `known_hosts` (StrictHostKeyChecking), or use **Fetch host key** in Settings (`ssh-keyscan`). Fingerprint mismatch fails closed until an admin updates it.

**OpenSSH on the API host:** Compose/API images include `openssh-client`. Host `make dev` needs `ssh`, `ssh-keygen`, and `ssh-keyscan` on `PATH` (e.g. Arch `openssh`, Debian/Ubuntu `openssh-client`) or SSH remotes fail with a clear “OpenSSH client is not installed” error.

**Inbound webhook:** copy the hook URL + secret into GitHub/GitLab/Gitea as a Push webhook. Secret rotation returns plaintext once.

## Config file format

There is no separate application `config.json` / TOML. Configuration is:

1. **Shell / `.env`** — variables consumed by `oxidean-api` and Compose `${…}` interpolation.
2. **Docker Compose YAML** — service wiring and dialect overlays:
   - `docker-compose.yml` — Traefik + web + api + Postgres
   - `docker-compose.mysql.yml` — MySQL profile
   - `docker-compose.sqlite.yml` — SQLite file bind-mount (no DB container)
   - `docker-compose.dev-auth.yml` — Mailpit, OIDC mock, Resend/WorkOS HTTP stubs

Minimal local `.env` for default Compose:

```bash
DATABASE_URL=postgres://oxidean:oxidean@postgres:5432/oxidean
POSTGRES_USER=oxidean
POSTGRES_PASSWORD=oxidean
POSTGRES_DB=oxidean
OXIDEAN_AUTO_MIGRATE=true
OXIDEAN_ENV=compose
OXIDEAN_CORS_ORIGINS=http://localhost,http://127.0.0.1
API_BIND=0.0.0.0:8080
OXIDEAN_RPC_VERSION=1
```

Auth settings that are **not** secrets (provider mode, from-address preference) are stored in the database and edited via Admin → Auth (`sys-admin` only). Secrets stay ENV-only.

### Recovering a DB with users but no `sys-admin`

ENV seed only runs when `users` is empty. If you already signed up during setup and need Auth settings access:

```sql
UPDATE users SET role = 'sys-admin' WHERE email = 'your@email';
```

Or wipe `users` / `sessions` and re-bootstrap with `OXIDEAN_ADMIN_*` or the `/setup` wizard (empty DB only). Do **not** auto-promote on upgrade.

## Required vs optional settings

**Fail boot (exit 1):**

| Condition | Error / behavior |
|-----------|------------------|
| `OXIDEAN_ENV` ∉ `{development,dev}` and `OXIDEAN_CORS_ORIGINS` missing/empty | `cors config error: OXIDEAN_CORS_ORIGINS is required when OXIDEAN_ENV is not development` |
| Invalid origin string in CORS list | `invalid CORS origin …` |
| `DATABASE_URL` set with unknown scheme | `unrecognized DATABASE_URL scheme: …` |
| `OXIDEAN_DB_DIALECT` disagrees with URL | `OXIDEAN_DB_DIALECT=… does not match DATABASE_URL scheme (detected …)` |
| DB connect / migrate failure | Printed to stderr; process exits |
| Both `OXIDEAN_ADMIN_*` set and empty-instance seed returns error | Fail closed: printed to stderr; process exits (no wizard fallback) |
| `git` missing or version &lt; 2.5.0 | `git version gate failed: …`; process exits (D-33) |

**Do not fail boot if unset:** `DATABASE_URL` (warns and skips pool), email/SSO secrets (features degrade to log sink / unavailable provider), admin seed vars (wizard path when either/both unset on empty instance), `OXIDEAN_ALLOW_SIGNUP` (defaults false), `OXIDEAN_REPOS_DIR` (defaults `var/repos`), `OXIDEAN_LFS_DIR` (defaults `var/lfs`), orphan/gc/LFS/mirror interval vars (use documented defaults; `0` disables that job).

## Defaults

| Setting | Default | Where |
|---------|---------|--------|
| `OXIDEAN_ENV` | `development` | `crates/oxidean-api/src/main.rs` |
| `API_BIND` | `0.0.0.0:8080` | `main.rs` |
| `OXIDEAN_AUTO_MIGRATE` | `true` (any value other than `true`/`1` disables) | `main.rs` |
| `OXIDEAN_ALLOW_SIGNUP` | `false` (only `true`/`1` opens signup at seed) | `crates/oxidean-api/src/auth/seed.rs` / bootstrap |
| `OXIDEAN_MAIL_FROM` | `Oxidean <noreply@localhost>` | `crates/oxidean-api/src/email/mod.rs` |
| Resend API base | `https://api.resend.com` | `crates/oxidean-api/src/email/resend.rs` |
| Compose `DATABASE_URL` | `postgres://oxidean:oxidean@postgres:5432/oxidean` | `docker-compose.yml` |
| Compose `OXIDEAN_ENV` | `compose` | `docker-compose.yml` |
| Compose CORS | `http://localhost,http://127.0.0.1` | `docker-compose.yml` |
| Dialect | Inferred from `DATABASE_URL` scheme | `crates/oxidean-db/src/dialect.rs` |
| `OXIDEAN_REPOS_DIR` | `var/repos` | `crates/oxidean-api/src/app.rs` |
| `OXIDEAN_LFS_DIR` | `var/lfs` | `crates/oxidean-api/src/app.rs` |
| `OXIDEAN_LFS_MAX_OBJECT_BYTES` | 2147483648 | `crates/oxidean-api/src/lfs/quota.rs` |
| `OXIDEAN_LFS_QUOTA_REPO_BYTES` | 10737418240 | `lfs/quota.rs` |
| `OXIDEAN_LFS_QUOTA_USER_BYTES` | 53687091200 | `lfs/quota.rs` |
| Orphan reconcile interval | 86400s | `crates/oxidean-api/src/jobs/schedule.rs` |
| Soft-delete retention | 14 days | `crates/oxidean-api/src/jobs/reconcile.rs` |
| Repo redirect retention | 90 days | `crates/oxidean-api/src/repo/rename_transfer.rs` / `app.rs` |
| Git gc interval | 604800s | `crates/oxidean-api/src/jobs/schedule.rs` |
| LFS GC interval | 86400s | `jobs/schedule.rs` / `jobs/lfs_gc.rs` |
| Mirror poll ticker | 30s | `OXIDEAN_MIRROR_POLL_TICK_SECS` / `mirror/queue.rs` |

Email sender selection when building from ENV: Resend key → SMTP URL → log sink.

**Organizations (Phase 10):** No new org-specific environment variables. Invite emails (instance, org, and repo scope) use the configured `EmailSender` (log / SMTP / Resend); invite URLs — emailed magic links and shareable link invites alike — are built on `OXIDEAN_PUBLIC_ORIGIN`, so keep it pointing at the browser-facing origin or acceptance links will be wrong. Shareable link invites need no mailer. Instance `allow_signup` still gates public `/signup`; redeeming a valid invite can create a verified local user under closed signup.

**Session client metadata:** sessions record last-known `ip_address` / `user_agent` for the admin user-details view. The IP is taken from the rightmost `X-Forwarded-For` hop — the same trusted-proxy convention as the Smart HTTP auth rate limiter. Behind a proxy that sanitizes forwarded headers this reflects the real client IP; exposed directly, it is whatever the client claims. See [guides/administration.md](guides/administration.md) for the admin surface.

## Per-environment overrides

| Environment | Typical `OXIDEAN_ENV` | CORS | Session cookie `Secure` | Stub / insecure flags |
|-------------|------------------------|------|-------------------------|------------------------|
| Host `make` + Vite (`:3000`) | `development` or `dev` | Mirror request origin; list optional | Off | `OXIDEAN_*_BASE_URL` stubs + `OXIDEAN_OIDC_ALLOW_INSECURE=1` allowed |
| Default Compose (Traefik `:80`) | `compose` | Allowlist required (`http://localhost`, …) | On (`compose` ≠ `development`/`dev`) | OIDC insecure flag allowed; use stub base URLs only for local stub stacks |
| Production-like | `production` (or other non-dev) | Allowlist required | On | Do **not** set stub base URLs or `OXIDEAN_OIDC_ALLOW_INSECURE` |

**DATABASE_URL dialects by bring-up:**

| Dialect | Sample URL | Bring-up |
|---------|------------|----------|
| PostgreSQL | `postgres://oxidean:oxidean@postgres:5432/oxidean` | `make up` |
| MySQL | `mysql://oxidean:oxidean@mysql:3306/oxidean` | `make up-mysql` |
| SQLite | `sqlite:./var/oxidean.db` (Compose: `sqlite:/app/var/oxidean.db`) | `make up-sqlite` |

Host-local Postgres (API outside Compose): point `DATABASE_URL` at `localhost:5432` and set `OXIDEAN_CORS_ORIGINS=http://localhost:3000` with `OXIDEAN_ENV=development`.

**Local stubs without cloud secrets:** copy `docs/dev-auth.env.example` → `.env.dev-auth`, run `make up-dev-auth`, then `source` the file before starting the API. That sets SMTP/Mailpit, OIDC mock issuer, WorkOS/Resend stub bases (`http://127.0.0.1:9092`), and `OXIDEAN_OIDC_ALLOW_INSECURE=1`. Details: [dev-auth.md](dev-auth.md).

<!-- VERIFY: Production WorkOS cloud API hostname when OXIDEAN_WORKOS_BASE_URL is unset (SDK default; tests mention api.workos.com) -->
<!-- VERIFY: Deployed public origin / SSO redirect URIs for non-local environments -->
<!-- VERIFY: Production SMTP / Resend / WorkOS / OIDC secret values (ENV-only; not in repo) -->


## Actions runners (ACT-01…ACT-07)

Oxidean Actions evaluates workflows from `.github/workflows/*.{yml,yaml}` on **push** and **pull_request** events. The forge queues jobs; **registered runners** execute them via `/api/actions` — there is **no managed-minutes product** and no in-process job executor (ACT-07 / D-ACT-10).

| Knob | Role |
| --- | --- |
| `OXIDEAN_ACTIONS_ENABLED` | Instance-wide gate (default `true`). When off, no workflows are evaluated. |
| `OXIDEAN_ACTIONS_LOG_DIR` | Job log blobs (`{run_id}/{job_id}.log`); distinct from repos/LFS/packages volumes. |
| `OXIDEAN_RUNNER_REGISTRATION_TOKEN` | Bootstrap registration token for Compose profile `actions` only — **reusable while set**; never leave on a public API. Prefer Admin-minted one-time tokens. **Never commit real values**. |
| `OXIDEAN_RUNNER_NAME` / `OXIDEAN_RUNNER_LABELS` | Default runner display name and labels (`label[:schema[:args]]`, e.g. `ubuntu-latest:docker://node:20-bookworm`). Bare labels execute on the runner host; `docker://` labels execute in a container (requires the Docker socket). |
| `OXIDEAN_RUNNER_GIT_TOKEN` | Optional PAT (`repo` scope) the runner uses to clone **private** repositories; public repos clone anonymously. |
| `OXIDEAN_RUNNER_STATE` / `OXIDEAN_RUNNER_WORK_DIR` | Runner registration-state file (default `/data/runner.json`) and per-job workspace root (default `<state dir>/work`). Mount a volume at `/data` in the official image. |
| `OXIDEAN_RUNNER_POLL_MS` / `OXIDEAN_RUNNER_JOB_TIMEOUT_SECS` | `fetch_task` poll interval (default `2000`) and hard job timeout (default `3600`). |
| `OXIDEAN_ACTIONS_SECRETS_KEY` | AES-256-GCM key for repo Actions secrets at rest (D-ACT-17). Required (or `OXIDEAN_SESSION_SECRET`); encrypt fails closed if unset. Compose/`make up` defaults to a local-only value — **set a unique key in production**. Prefer a dedicated secret. |

**Registration tokens:** instance admins mint via `admin.actions.createRegistrationToken` (one-time plaintext `reg_…`) or env bootstrap above. **Runner tokens** (`ort_…`) are returned once at register and used as Bearer on `/api/actions/*` — session cookies are ignored (D-ACT-18).

**Per-repo:** Admin enables Actions with `repo.actions.setEnabled`; secrets via `repo.actions.secrets.*` (list returns names only).

**Commit statuses (Phase 13):** Actions publishes contexts `{workflow_name} / {job_key}` — configure branch protection required checks to match. Query with `repo.commitStatus.list`.

Bring-up: [DEPLOYMENT.md](DEPLOYMENT.md#actions-runner-optional). Protocol: [API.md](API.md#actions-phase-19).
