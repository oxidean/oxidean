# Phase 15: Releases & Transfer - Research

**Researched:** 2026-09-14
**Domain:** Forge releases (tag-bound notes + downloadable assets), repository rename with HTTP/git redirects, ownership transfer cascades
**Confidence:** HIGH (codebase ACL/disk/settings patterns); MEDIUM (forge product parity defaults for retention/size)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
### A — Release model
- **D-REL-01:** **Tag must already exist**; release binds to that tag (no create-tag-on-publish in Phase 15) — **Reversibility:** reversible
- **D-REL-02:** Support **draft** and **prerelease** flags (GitHub-like) — **Reversibility:** reversible
- **D-REL-03:** **Author or Write+** can edit release notes/assets after publish; **Admin** can **delete** a release — **Reversibility:** reversible

### B — Release assets
- **D-REL-04:** Store assets on a **separate release-assets volume** (not the LFS OID store) — **Reversibility:** costly — storage split from LFS
- **D-REL-05:** **Configurable max asset size**; **replacing** an asset on edit is allowed — **Reversibility:** reversible
- **D-REL-06:** Authenticated download from web UI with **Read** (public repos: anonymous read of published release assets) — **Reversibility:** reversible

### C — Rename
- **D-REL-07:** **Admin only** may rename a repository (user-owned or org-owned) — **Reversibility:** reversible
- **D-REL-08:** Keep **HTTP redirects** from the old `/{owner}/{repo}` path for a **configurable retention window** — **Reversibility:** costly — redirect table + purge job

### D — Transfer
- **D-REL-09:** **Admin only** may transfer; destination is **user or org** — **Reversibility:** reversible
- **D-REL-10:** Transfer moves **git data + issues + LFS object associations**; confirm with **type-the-repo-name** — **Reversibility:** costly — ownership rewrite across tables
- **D-REL-11:** Webhooks / packages (future) follow ownership when those phases exist; Phase 15 does not invent stub webhooks — **Reversibility:** reversible

### E — Permissions & UI
- **D-REL-12:** **Write+** can create/publish releases; **Admin** for rename/transfer/delete release — **Reversibility:** reversible
- **D-REL-13:** Add **Releases** tab to repo chrome; list + detail + asset upload UI — **Reversibility:** reversible

### Claude's Discretion
- Exact default redirect retention duration
- Exact default max release asset size
- Whether drafts are visible only to Write+ (assume yes — GitHub-like)
- Exact settings IA for rename vs transfer (same danger zone vs separate)
- Whether release asset download URLs are stable after rename/transfer (prefer yes via release id)

### Deferred Ideas (OUT OF SCOPE)
- Auto-generated release notes from merged PRs
- Soft-delete releases / recycle bin
- Transfer across instances
- Attaching container packages to a release (Phase 20)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| GIT-14 | User can create a release for a tag with notes and downloadable assets | Tag-bound `release` rows + multipart asset upload to `OXIDEAN_RELEASE_ASSETS_DIR`; Write+ gate; draft/prerelease flags |
| GIT-15 | User can download release assets from the web UI | Axum download route with Read ACL (anon for public published); Content-Disposition from DB filename |
| GIT-16 | User with permission can rename a repository | Admin `repo.rename` + disk `fs::rename` of bare dir + `repository_redirects` row + retention purge |
| GIT-17 | User with permission can transfer a repository to another user or organization | Admin `repo.transfer` + owner_type/owner_id rewrite + bare dir move + redirect + type-confirm; issues/LFS stay on `repo_id` |
</phase_requirements>

## Summary

Phase 15 adds two product surfaces that forges treat as related danger-zone / shipping features: **tag-bound releases with binary assets**, and **Admin rename/transfer with redirects**. Oxidean already has the hard primitives this phase must reuse: `Capability::{Read,Write,Admin}` coalesce ACL, polymorphic `owner_type`/`owner_id`, bare layout `{repos_dir}/{owner}/{name}.git`, typed confirm for soft-delete, username-driven owner-dir rename, multipart upload + Compose volume pattern for avatars, and orphan-reconcile retention jobs.

Forge research (GitHub + Gitea) converges on: releases are DB metadata bound to a git tag with draft/prerelease; assets are separate uploads keyed by release id; rename/transfer insert redirect records so old `owner/name` (and Smart HTTP `.git` URLs) keep working until superseded by a new repo at the old path. Oxidean locks a **finite redirect retention window** (unlike GitHub’s indefinite redirects) and a **separate release-assets volume** (unlike LFS OID store). Issues and LFS associations should key off `repositories.id` so transfer is primarily an ownership + disk-path rewrite, not a row-by-row content migrate.

**Primary recommendation:** Implement `release.*` RPC + multipart asset HTTP routes on a new `OXIDEAN_RELEASE_ASSETS_DIR` volume; implement `repo.rename` / `repo.transfer` with disk move + `repository_redirects` (web SSR + Smart HTTP + SSH resolution) and ENV-tunable retention/size defaults (90 days / 512 MiB).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Release CRUD + draft ACL | API / Backend | Browser / Client | Capability checks + tag existence live in Axum/RPC; UI is form/list |
| Release asset upload/download | API / Backend | CDN / Static | Multipart + ACL + body limits on API; not static CDN (authZ required for private) |
| Release notes markdown render | Browser / Client | — | Reuse `renderGfm` + sanitize like issues |
| Repo rename / transfer mutate | API / Backend | Database / Storage | DB ownership + bare dir move + redirect rows |
| Redirect resolution (web) | Frontend Server (SSR) | API / Backend | SPA loaders call resolve RPC then `throw redirect` / `window.location` |
| Redirect resolution (git HTTPS/SSH) | API / Backend | — | Smart HTTP + SSH must honor redirects like Gitea |
| Asset volume / factory reset wipe | Database / Storage | API / Backend | Compose bind + wipe with repos scope |
| Settings danger-zone UI | Browser / Client | — | Octane settings page; Admin-gated |

## Project Constraints (from .cursor/rules/)

- One product (cloud + self-host); Bun workspaces + Cargo crates — do not invent parallel app structure. [VERIFIED: `.cursor/rules/oxidean-core.mdc`]
- Web UI is **Octane** (`.tsrx`), not React; load Octane skill before UI edits; `@if`/`@else` only; TanStack Query for server state. [VERIFIED: `.cursor/rules/octane-ui.mdc`]
- RPC types: change Rust → `make rpc-gen`; never treat hand-edited `packages/api-client` as source of truth. [VERIFIED: `.cursor/rules/rpc-codegen.mdc`]
- Dialect SQL only inside `crates/oxidean-db`. [VERIFIED: `.cursor/rules/oxidean-core.mdc`]
- No secrets in commits/examples; prefer `make test` / `make rpc-sync-check`. [VERIFIED: `.cursor/rules/oxidean-core.mdc`]
- Prefer extending existing patterns (ACL gates, Make targets, upload volumes) over new frameworks. [VERIFIED: `.cursor/rules/oxidean-core.mdc`]

## Standard Stack

### Core

| Library / Component | Version / Location | Purpose | Why Standard |
|---------------------|--------------------|---------|--------------|
| Axum + `DefaultBodyLimit` | in-tree `oxidean-api` | Multipart asset upload + download routes | Already used for avatars [VERIFIED: `crates/oxidean-api/src/app.rs:127-131`] |
| `Capability` ACL | `repo/acl.rs` | Read/Write/Admin gates | Phase 10 source of truth [VERIFIED: `crates/oxidean-api/src/repo/acl.rs:22-28`] |
| `CliGitBackend` / `repo.refs` | `oxidean-git` + `repo/mod.rs` | Prove tag exists before create | Tags already listed in browse UI |
| SQLx migrations (3 dialects) | `oxidean-db/migrations/*` | `releases`, `release_assets`, `repository_redirects` | Dialect branching stays in db crate |
| Octane `.tsrx` + TanStack Query | `apps/web` | Releases tab + settings danger zone | Project UI stack |
| Compose volume bind | `docker-compose.yml` | `OXIDEAN_RELEASE_ASSETS_DIR` | Same pattern as `./var/uploads` / `./var/repos` |

### Supporting

| Library / Component | Version | Purpose | When to Use |
|---------------------|---------|---------|-------------|
| Existing AlertDialog / Input confirm | in-tree | Type-repo-name for transfer (and optionally rename) | Mirror soft-delete UX [VERIFIED: `apps/web/src/routes/$owner.$repo.settings.tsrx:74-112`] |
| `orphan_reconcile` job scheduler | `jobs/reconcile.rs` | Purge expired redirects (+ optional asset orphans) | Extend existing ENV-driven job |
| `rename_owner_repos_dir` | `git/mod.rs` | Pattern for atomic-ish disk rename before DB commit | Repo rename/transfer bare moves [VERIFIED: `crates/oxidean-api/src/git/mod.rs:48-57`] |
| `renderGfm` | `apps/web/src/lib/markdown.ts` | Release notes HTML | Same as issues |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Separate release-assets volume | Store under bare repo `releases/` | Rejected by D-REL-04; couples backup/GC to git objects |
| Indefinite redirects (GitHub) | Finite retention (locked) | Self-host disk/row growth control; ENV default recommended 90d |
| Accept-email transfer (GitHub user→user) | Immediate transfer after type-confirm | Locked UX is type-confirm only; immediate fits self-host without mail round-trip [ASSUMED] |
| New npm upload SDK | Axum multipart | No new packages; matches avatar route |

**Installation:**

```bash
# No new npm/Cargo packages required for Phase 15 — reuse Axum multipart, existing UI kits, CliGitBackend.
# Ops only: Compose bind + ENV knobs documented in docs/CONFIGURATION.md
```

**Version verification:** No new registry packages. Existing stack confirmed in-repo (Axum router, Vitest, nextest). Package legitimacy gate: N/A.

## Package Legitimacy Audit

> Phase installs **no new external packages**.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| — | — | — | — | — | — | none |

**Packages removed due to [SLOP] verdict:** none  
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```text
[Browser Octane UI]
   |  RPC session cookie
   v
[Axum /api/rpc] ---- release.create/list/get/update/delete
   |                 repo.rename / repo.transfer
   |                 (Capability: Write+ / Admin)
   v
[oxidean-db]
   repositories (owner_type, owner_id, name)
   releases (repo_id, tag_name, draft, prerelease, ...)
   release_assets (release_id, filename, size, ...)
   repository_redirects (old_owner_slug, old_name, repo_id, expires_at)
   issues* / lfs_repo_oids*  ---- keyed by repo_id (no owner rewrite)
   |
   +--> [OXIDEAN_REPOS_DIR]/{owner}/{name}.git   (rename/transfer fs::rename)
   +--> [OXIDEAN_RELEASE_ASSETS_DIR]/{asset_id}  (stable; not under owner path)
   +--> [OXIDEAN_LFS_DIR] OID store              (associations only; no path rewrite)

[Browser / git client]
   |  GET /api/releases/assets/{asset_id}
   |  GET|POST /{owner}/{repo}.git/...
   v
[Axum download + Smart HTTP + SSH]
   resolve owner/name → active repo OR redirect → 302/serve
```

\*Issues and LFS association tables land in Phases 11/14; Phase 15 transfer must treat `repo_id` FKs as already moved when those tables exist. Roadmap order places 11–14 before 15.

### Recommended Project Structure

```
crates/oxidean-core/src/release_types.rs   # DTOs + error codes
crates/oxidean-db/migrations/*/00NN_releases_redirects.sql
crates/oxidean-db/src/releases.rs
crates/oxidean-db/src/redirects.rs
crates/oxidean-api/src/release/mod.rs      # RPC handlers
crates/oxidean-api/src/routes/release_assets.rs  # multipart + download
crates/oxidean-api/src/repo/rename_transfer.rs   # rename/transfer + disk
crates/oxidean-api/src/jobs/redirect_purge.rs    # or extend reconcile.rs
apps/web/src/routes/$owner.$repo.releases*.tsrx
apps/web/src/components/repo/repo-chrome.tsrx     # Releases tab + can_admin Settings
apps/web/src/routes/$owner.$repo.settings.tsrx   # Danger zone: rename + transfer
docs/CONFIGURATION.md / docs/API.md
docker-compose.yml                              # bind ./var/release-assets
```

### Pattern 1: Tag-must-exist release create (D-REL-01)

**What:** Create release only if `refs/tags/{tag}` exists via `GitBackend` / `repo.refs`; reject with stable `release.tag_missing`. Do **not** create tags from `target_commitish` (GitHub can; Oxidean must not in Phase 15).

**When to use:** `release.create` / publish draft.

**Example:**

```rust
// Source: Oxidean pattern — verify tag via existing refs listing (repo.refs)
// [ASSUMED] exact GitBackend helper name; prefer listing refs/tags and matching short name
if !tag_exists_in_repo(&ctx.git, &bare_path, &tag_name).await? {
    return Err(AppError::new("release.tag_missing", "Tag does not exist on this repository."));
}
```

### Pattern 2: Capability gates (D-REL-07/09/12)

**What:** Reuse `resolve_repo_for_read` + `meets`, and Admin helper:

```rust
// Source: crates/oxidean-api/src/repo/collaborators.rs:51-63
/// Resolve repo for Admin-only mutate (collaborators, visibility, soft-delete).
/// Missing OR insufficient capability → identical soft [`acl::not_found`].
pub async fn resolve_repo_for_admin(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let _ = require_verified(ctx).await?;
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, Capability::Admin) {
        return Err(acl::not_found());
    }
    Ok(accessible)
}
```

[VERIFIED: `crates/oxidean-api/src/repo/collaborators.rs:51-63`]

- Releases mutate (create/edit/upload): `meets(..., Write)` after verified session.
- Rename / transfer / delete release: `resolve_repo_for_admin`.
- Soft not-found for private denials (anti-enumeration).

### Pattern 3: Disk path + rename (GIT-16/17)

**What:** Bare path is `{repos_dir}/{owner}/{name}.git`:

```rust
// Source: crates/oxidean-api/src/git/mod.rs:13-31
/// Bare repo path: `{repos_dir}/{owner}/{name}.git` (D-30).
pub fn bare_repo_path(repos_dir: &Path, owner: &str, name: &str) -> Result<PathBuf, AppError> {
    // ... path traversal rejects ...
    Ok(repos_dir.join(owner).join(format!("{name}.git")))
}
```

[VERIFIED: `crates/oxidean-api/src/git/mod.rs:13-31`]

Username migrate already renames **owner directories** before DB commit (`rename_owner_repos_dir`). Repo rename/transfer should:

1. Validate new name with `validate_repo_name` [VERIFIED: `crates/oxidean-core/src/repo_types.rs:404-430`].
2. Ensure destination `(owner_id, lower(name))` free (unique index) [VERIFIED: `crates/oxidean-db/migrations/sqlite/0010_orgs_acl.sql:74-76`].
3. `tokio::fs::rename` bare dir (and ensure parent owner dir exists on transfer).
4. Update DB `name` and/or `owner_type`/`owner_id`.
5. Insert `repository_redirects` for old slug/name with `expires_at`.
6. Compensate disk on DB failure (mirror username migrate compensate).

### Pattern 4: Type-confirm danger actions (D-REL-10)

**What:** Reuse soft-delete confirm pattern:

```rust
// Source: crates/oxidean-api/src/repo/mod.rs:712-729
let confirm = req.confirm_name.trim();
if confirm != accessible.row.name.as_str() {
    return Err(AppError::new(
        "repo.confirm_mismatch",
        "Type the repository name exactly to confirm deletion.",
    ));
}
```

[VERIFIED: `crates/oxidean-api/src/repo/mod.rs:712-729`]

Transfer (and optionally rename) should use the same `confirmName` + `repo.confirm_mismatch` (or `repo.transfer_confirm_mismatch`) and UI AlertDialog in Danger zone.

### Pattern 5: Redirect table (Gitea-shaped, retention-bounded)

**What:** Gitea stores `owner_id + lower_name → redirect_repo_id` and resolves on HTTP (and increasingly SSH). [CITED: github.com/go-gitea/gitea/blob/v1.27.0/models/repo/redirect.go]

Oxidean should store **slug strings** (not only owner ids) because web URLs are `/{owner}/{repo}` and owners can be user or org:

| Column | Notes |
|--------|-------|
| `id` | TEXT PK |
| `old_owner_slug` | lowercased username/org slug |
| `old_name` | lowercased repo name |
| `repo_id` | FK repositories.id ON DELETE CASCADE |
| `expires_at` | RFC3339; purge job |
| UNIQUE(old_owner_slug, old_name) | |

**Conflict rule (GitHub/Gitea):** If a live repository occupies `owner/name`, it **wins** over redirects; creating a new repo at the old path deletes/supersedes the redirect. [CITED: docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository]

**Resolution surfaces:**

1. Web SSR / `repo.get` miss → `repo.resolveRedirect` → 302 to new `/{owner}/{repo}/…` preserving suffix.
2. Smart HTTP `/{owner}/{repo}.git` → 301/302 to new location (git clients follow).
3. SSH tracer → resolve redirect to new bare path (parity with Gitea SSH work). [CITED: github.com/go-gitea/gitea/issues/35416]

### Pattern 6: Release assets volume (D-REL-04..06)

**What:** New ENV `OXIDEAN_RELEASE_ASSETS_DIR` (default `var/release-assets`), Compose bind like uploads. Store files as `{dir}/{asset_id}` (opaque id) so **rename/transfer never rewrites asset paths**. DB holds `filename`, `content_type`, `byte_size`, `release_id`, `uploader_id`.

Download: `GET /api/releases/assets/{asset_id}` (stable) with:
- published + public repo → anonymous OK
- else session with Read
- drafts → Write+ only (discretion lock: GitHub-like)

Upload: `POST /api/repos/{owner}/{repo}/releases/{release_id}/assets` multipart + `DefaultBodyLimit::max(OXIDEAN_RELEASE_ASSET_MAX_BYTES)`.

Replace-on-edit (D-REL-05): delete old file then write new, or overwrite same `asset_id` after size check.

### Anti-Patterns to Avoid

- **Hand-edit `packages/api-client`:** always `make rpc-gen`.
- **Mixing LFS OID store with release binaries:** violates D-REL-04; different GC/refcount semantics.
- **Create-tag-on-publish:** violates D-REL-01.
- **Owner-id equality for Admin gates:** must use `Capability::Admin` (org Admin collaborators).
- **Settings chrome gated only by personal username equality:** current chrome still uses `user.username === repo.owner_username` for Settings visibility [VERIFIED: `apps/web/src/components/repo/repo-chrome.tsrx:19-22`] — fix to `repo.can_admin` when adding Releases tab.
- **Redirect only in web UI:** git remotes break without Smart HTTP/SSH redirect honor.
- **Asset paths under `{owner}/{repo}/`:** breaks stability after rename/transfer.
- **Inventing webhook stubs:** D-REL-11 — do not.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| ACL ladder | Custom role checks | `effective_capability` / `meets` | Highest-wins coalesce already tested |
| Multipart parsing | Manual boundary parser | Axum `Multipart` + `DefaultBodyLimit` | Avatar route proven |
| Markdown HTML | Custom sanitizer | `renderGfm` pipeline | XSS already addressed for README/issues |
| Typed confirm UI | New modal kit | Existing AlertDialog + Input | Soft-delete UX |
| Disk path joins | String concat | `bare_repo_path` | Traversal guards |
| Redirect purge | Ad-hoc cron container | Extend `jobs/reconcile` / schedule | Existing ENV interval pattern |
| New state library | Zustand for releases | TanStack Query | Project rule |

**Key insight:** Phase 15 is mostly **composition of existing Oxidean seams** (ACL, bare paths, multipart, danger-zone confirm, retention jobs) plus two new tables and forge-standard redirect semantics — not a greenfield storage product.

## Common Pitfalls

### Pitfall 1: Unique name collision on rename/transfer
**What goes wrong:** Destination owner already has `lower(name)` active → unique index failure.  
**Why it happens:** Transfer keeps name by default.  
**How to avoid:** Pre-check; return `repo.name_taken` (same as create). Optional rename-during-transfer is GitHub-capable but **out of locked decisions** — defer unless needed.  
**Warning signs:** 500 on unique violation instead of structured error.

### Pitfall 2: Disk/DB ordering races with orphan reconcile
**What goes wrong:** Orphan job deletes “orphaned” bare dir mid-rename.  
**Why it happens:** Username migrate docs already warn: rename disk **before** DB commit so reconcile does not purge. [VERIFIED: `crates/oxidean-api/src/git/mod.rs:48-52`]  
**How to avoid:** Same ordering; hold reconcile lock or accept short window with compensate.  
**Warning signs:** Missing bare repo after rename under load.

### Pitfall 3: Redirect vs live repo precedence
**What goes wrong:** Old path keeps redirecting after user intentionally recreates repo there.  
**How to avoid:** On `repo.create`, `DeleteRedirect` for that slug/name (Gitea/GitHub).  
**Warning signs:** Create succeeds but browse still 302s to transferred repo.

### Pitfall 4: Draft leakage
**What goes wrong:** Public/anonymous list shows drafts.  
**How to avoid:** Filter `draft=true` unless caller has Write+ (GitHub: “Only users with push access will receive listings for draft releases”). [CITED: docs.github.com/en/rest/releases/releases]  
**Warning signs:** Draft titles visible to logged-out users on public repos.

### Pitfall 5: Transfer ACL after owner change
**What goes wrong:** Personal collaborators lose access unexpectedly, or org Member base suddenly applies.  
**How to avoid:** Document GitHub-like policy: keep `repository_collaborators` rows (keyed by `repo_id`); org destination then applies `member_base_permission` coalesce for org members. Recommend adding former personal owner as **admin collaborator** on user→user transfer (GitHub parity) [CITED: docs.github.com/en/repositories/creating-and-managing-repositories/transferring-a-repository] — confirm as discretion default.  
**Warning signs:** Transfer “succeeds” but previous admin cannot open settings.

### Pitfall 6: Issue/LFS “move” misunderstood as file copy
**What goes wrong:** Planner schedules expensive LFS OID copies.  
**How to avoid:** Phase 14 locks **instance-wide OID store**; transfer updates/retains **association rows by `repo_id`** — no OID file move. Issues keyed by `repo_id` move automatically with ownership change.  
**Warning signs:** Plans that `cp -r` LFS dirs on transfer.

### Pitfall 7: Factory reset misses release assets
**What goes wrong:** DB wiped, binaries remain (or reverse).  
**How to avoid:** Extend `database_and_repositories` scope to wipe `OXIDEAN_RELEASE_ASSETS_DIR` children (and LFS when present).  
**Warning signs:** Disk growth after repeated resets.

### Pitfall 8: Vite proxy gaps for new asset routes
**What goes wrong:** Local upload works in Compose but not Vite.  
**How to avoid:** Ensure `/api/releases` (or chosen prefix) is proxied like `/api/repos` / `/uploads` [VERIFIED: `apps/web/vite.config.ts:28-34`].  
**Warning signs:** Network errors only in `bun run dev`.

## Code Examples

### Soft-delete type-confirm (mirror for transfer)

```rust
// Source: crates/oxidean-api/src/repo/mod.rs:712-737
pub async fn soft_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoSoftDeleteResponse, AppError> {
    // ...
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let confirm = req.confirm_name.trim();
    if confirm != accessible.row.name.as_str() {
        return Err(AppError::new(
            "repo.confirm_mismatch",
            "Type the repository name exactly to confirm deletion.",
        ));
    }
    ctx.db.soft_delete_repository(&accessible.row.id).await.map_err(db_err)?;
    Ok(RepoSoftDeleteResponse { name: accessible.row.name })
}
```

### Capability enum (gates)

```rust
// Source: crates/oxidean-api/src/repo/acl.rs:22-28
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    Read = 1,
    Write = 2,
    Admin = 3,
}
```

### RepoPublic flags for UI gating

```rust
// Source: crates/oxidean-core/src/repo_types.rs:78-97
pub struct RepoPublic {
    // ...
    pub can_admin: bool,
    pub can_write: bool,
}
```

[VERIFIED: `crates/oxidean-core/src/repo_types.rs:78-97`]

### Suggested RPC surface (planner)

| Procedure | Cap | Notes |
|-----------|-----|-------|
| `release.list` | Read | Omit drafts unless Write+ |
| `release.get` | Read | Draft → Write+ |
| `release.create` | Write | Requires existing tag |
| `release.update` | Write or author | Notes/flags/assets metadata |
| `release.delete` | Admin | Deletes DB + asset files |
| `repo.rename` | Admin | Disk + redirect |
| `repo.transfer` | Admin | Type-confirm; dest user/org; create-permission on dest org |
| `repo.resolveRedirect` | public | Optional dedicated; or fold into get |

HTTP (non-RPC) for large bodies:

| Route | Cap |
|-------|-----|
| `POST /api/repos/{owner}/{repo}/releases/{id}/assets` | Write |
| `GET /api/releases/assets/{asset_id}` | Read / anon public published |
| `DELETE` via RPC `release.deleteAsset` | Write or Admin |

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Tags = download archives only | Tags + first-class Releases with assets | Phase 15 | GIT-14/15 |
| Owner path assumed stable | Redirect table with retention | Phase 15 | GIT-16/17 |
| Personal-owner ACL stub | Capability Admin for rename/transfer | Phase 10 already | Must not regress |
| LFS shared OID store | Still separate from release binaries | Phase 14 design | D-REL-04 |

**Deprecated/outdated:**
- Treating release assets as “just another LFS object”
- GitHub-style create-tag-on-release for Oxidean Phase 15

## Discretion Recommendations (for planner locks)

| Topic | Recommendation | Rationale |
|-------|----------------|-----------|
| Redirect retention default | **90 days** via `OXIDEAN_REPO_REDIRECT_RETENTION_DAYS` | Longer than soft-delete 14d (git remotes linger); shorter than GitHub indefinite for self-host hygiene [ASSUMED] |
| Max asset size default | **512 MiB** via `OXIDEAN_RELEASE_ASSET_MAX_BYTES` | Safe self-host default; operators can raise; GitHub allows up to ~2 GiB [ASSUMED] |
| Draft visibility | **Write+ only** | Matches GitHub REST list behavior [CITED: docs.github.com/en/rest/releases/releases] |
| Settings IA | **Same Danger zone** as soft-delete: Rename, Transfer, Delete | GitHub Settings danger zone parity; less IA sprawl |
| Asset URL stability | **Id-based** `/api/releases/assets/{asset_id}` | Survives rename/transfer without depending on redirect TTL |
| Transfer accept email | **Immediate** after type-confirm | CONTEXT locks type-confirm only; self-host may lack mail [ASSUMED] |
| User→user collaborator | Add former owner as **admin** collaborator | GitHub parity [CITED: docs.github.com/…] |
| Org destination gate | Caller must be allowed to create under dest (Owner/Admin), same as `repo.create` A5 | Prevents dumping repos into foreign orgs |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Default redirect retention 90 days is acceptable | Discretion | Too short breaks remotes; too long grows redirect table |
| A2 | Default max asset 512 MiB is acceptable | Discretion | Blocks large binary releases or allows disk fill |
| A3 | Immediate transfer (no accept email) is desired | Transfer | Users may want GitHub-like accept for user destinations |
| A4 | Former owner becomes admin collaborator on user→user transfer | Transfer | Access surprises either way |
| A5 | Issues/LFS tables use `repo_id` FK by Phase 15 execution | Cascade | Planner must add explicit rewrite if schema differs |
| A6 | No new crates/npm deps needed | Stack | Streaming uploads might later need tower limits tweaks only |

**If wrong:** Discuss-phase / planner checkpoint before locking ENV defaults and transfer collaborator policy.

## Open Questions (RESOLVED)

1. **Exact migration number** — RESOLVED
   - What we know: SQLite migrations currently end at `0010_orgs_acl`.
   - Resolution: Use next free `00NN_releases_redirects` across sqlite/postgres/mysql at execute time; do not hardcode `0011` (matches plan 15-01 assumptions).

2. **Rename confirmation** — RESOLVED
   - What we know: Transfer locks type-confirm; rename does not.
   - Resolution: Rename is a simple Admin form (no type-confirm); transfer keeps type-the-repo-name confirm (D-REL-10; plans 15-03/15-05).

3. **SSH redirect scope** — RESOLVED
   - What we know: Gitea HTTP redirects existed first; SSH followed later.
   - Resolution: Include SSH redirect in Phase 15 via the shared resolve helper alongside HTTPS/web/git Smart HTTP (plan 15-03; not deferred).

4. **Latest release semantics** — RESOLVED
   - GitHub “latest” excludes draft/prerelease.
   - Resolution: No `release.latest` in Phase 15; list/get/detail suffice for GIT-14/15 (deferred).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Node / Bun | Web Vitest / Vite | ✓ | Node v24.5.0 / Bun 1.4.0 | — |
| Cargo / Rust | API tests | ✓ | cargo 1.100.0-nightly | — |
| System git | Tag existence checks | ✓ (project standard ≥2.5) | probe at boot | fail boot (existing) |
| Docker Compose | Volume binds | optional for unit tests | — | tempfile dirs in tests |
| New packages | — | N/A | — | none |

**Missing dependencies with no fallback:** none for planning/research.  
**Missing dependencies with fallback:** Docker not required for nextest/Vitest unit/integration patterns already used.

Step 2.6: External tools limited to existing API/git/Compose volume pattern — available.

## Validation Architecture

> `workflow.nyquist_validation` is **true** in `.planning/config.json`.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo nextest (Rust) + Vitest (web) |
| Config file | `apps/web/vitest.config.ts`; Cargo workspace nextest |
| Quick run command | `cargo nextest run -p oxidean-api -E 'test(release) \| test(rename) \| test(transfer) \| test(redirect)'` |
| Full suite command | `make test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GIT-14 | Create release for existing tag with notes + asset | integration | `cargo nextest run -p oxidean-api -E 'test(release_create)'` | ❌ Wave 0 |
| GIT-14 | Reject missing tag (`release.tag_missing`) | integration | same | ❌ Wave 0 |
| GIT-14 | Draft hidden from Read-only / anon | integration | `… test(release_draft_acl)` | ❌ Wave 0 |
| GIT-15 | Download asset with Read / anon public | integration | `… test(release_asset_download)` | ❌ Wave 0 |
| GIT-15 | Oversized upload rejected | integration | `… test(release_asset_size)` | ❌ Wave 0 |
| GIT-16 | Admin rename moves disk + DB; old path redirects | integration | `… test(repo_rename_redirect)` | ❌ Wave 0 |
| GIT-16 | Non-admin rename → soft not_found | integration | same | ❌ Wave 0 |
| GIT-17 | Admin transfer to user/org + type-confirm | integration | `… test(repo_transfer)` | ❌ Wave 0 |
| GIT-17 | Issues/LFS associations remain on repo_id | integration | `… test(repo_transfer_cascade)` | ❌ Wave 0 (after 11/14 schema) |
| GIT-16/17 | New repo at old path supersedes redirect | integration | `… test(redirect_supersede)` | ❌ Wave 0 |
| UI | Releases tab + settings danger zone | web unit/integration | `bun run --filter @oxidean/web test` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** targeted nextest filter for touched area
- **Per wave merge:** `cargo nextest run -p oxidean-api -p oxidean-db` + web Vitest for new routes
- **Phase gate:** `make test` + `make rpc-sync-check` green before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `crates/oxidean-api/tests/release_rpc.rs` — GIT-14/15
- [ ] `crates/oxidean-api/tests/repo_rename_transfer.rs` — GIT-16/17 + redirects
- [ ] `apps/web/src/routes/$owner.$repo.releases.integration.test.ts` — tab/routes discoverability
- [ ] Settings danger-zone integration assertions for rename/transfer confirm
- [ ] None of the above exist today — RED stubs first (match Phases 07–10 Wave 0 style)

## Security Domain

> `security_enforcement` enabled; ASVS level 1.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | Session cookie for RPC/UI; PAT only for git (not release RPC) |
| V3 Session Management | yes | Existing session service |
| V4 Access Control | yes | `Capability` Read/Write/Admin; soft `repo.not_found` |
| V5 Input Validation | yes | `validate_repo_name`; tag name validation; multipart size/type; path traversal guards in `bare_repo_path` |
| V6 Cryptography | no | No new crypto; download integrity optional checksum later |

### Known Threat Patterns for releases / rename / transfer

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Enumerate private repos via rename/transfer errors | Information disclosure | Identical `repo.not_found` for missing vs forbidden |
| Upload malware / zip bombs as assets | DoS / Tampering | Max bytes + content-type allow/deny; store outside web root with ACL’d download |
| Path traversal in asset filenames | Tampering | Ignore client path; store by `asset_id`; sanitize `Content-Disposition` filename |
| CSRF on transfer | Spoofing | Same-site session cookie + RPC pattern already used for soft-delete |
| Open redirect via crafted redirect rows | Spoofing | Redirect targets only `repo_id` → resolve current owner/name server-side |
| Cross-tenant asset download by id | Elevation | Always AuthZ via parent release→repo ACL on download |
| Race recreate at old name during redirect TTL | Tampering | Live repo wins; delete redirect on create |

## Sources

### Primary (HIGH confidence — in-repo)

- `crates/oxidean-api/src/repo/acl.rs` — Capability ladder / coalesce
- `crates/oxidean-api/src/repo/collaborators.rs` — `resolve_repo_for_admin`
- `crates/oxidean-api/src/repo/mod.rs` — softDelete confirm / visibility Admin
- `crates/oxidean-api/src/git/mod.rs` — `bare_repo_path`, `rename_owner_repos_dir`
- `crates/oxidean-db/migrations/sqlite/0010_orgs_acl.sql` — polymorphic owner + unique name
- `apps/web/src/routes/$owner.$repo.settings.tsrx` — Danger zone UX
- `apps/web/src/components/repo/repo-chrome.tsrx` — tab IA (Settings `isOwner` bug)
- `.planning/phases/15-releases-transfer/15-CONTEXT.md` — locked decisions
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — Admin/Write ACL
- `.planning/phases/11-issues/11-CONTEXT.md` — issues move with repo
- `.planning/phases/14-git-lfs/14-CONTEXT.md` — LFS volume ≠ release assets

### Secondary (MEDIUM confidence — official forge docs)

- [CITED: docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository] — web+git redirects; new name supersedes
- [CITED: docs.github.com/en/repositories/creating-and-managing-repositories/transferring-a-repository] — cascade; type-confirm; LFS; collaborator behavior
- [CITED: docs.github.com/en/rest/releases/releases] — draft/prerelease; push creates; draft list visibility
- [CITED: docs.gitea.com/api/1.26/operations/repo-create-release/] — draft/prerelease API shape
- [CITED: github.com/go-gitea/gitea/blob/v1.27.0/models/repo/redirect.go] — `repo_redirect` model

### Tertiary (LOW confidence)

- Exact numeric defaults for retention/size (discretion) — flagged in Assumptions Log
- SSH redirect urgency relative to HTTPS (Gitea timeline) — plan helper shared, SSH plan optional PE

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — reuse in-repo Axum/ACL/git/Octane; no new packages
- Architecture: HIGH — mirrors GitHub/Gitea + existing Oxidean path/ACL patterns
- Pitfalls: HIGH — rename/reconcile/draft/redirect collisions well-documented in forge + local jobs

**Research date:** 2026-09-14  
**Valid until:** 2026-10-14 (30 days; forge docs stable)

---

## RESEARCH COMPLETE

**Phase:** 15 - Releases & Transfer  
**Confidence:** HIGH

### Key Findings
- Reuse `Capability` Admin/Write gates, `bare_repo_path`, soft-delete type-confirm, avatar multipart + Compose volume patterns — no new npm packages.
- Releases: tag must pre-exist; draft/prerelease; assets on `OXIDEAN_RELEASE_ASSETS_DIR` keyed by asset id for rename/transfer stability.
- Rename/transfer: disk `fs::rename` + `repository_redirects` with finite retention; honor redirects on web + Smart HTTP (+ SSH if cheap); live repo supersedes redirect.
- Transfer cascade: prefer `repo_id` FKs for issues/LFS associations (no OID copy); do not stub webhooks (D-REL-11).
- Discretion defaults to lock in plan: 90-day redirects, 512 MiB assets, Write+-only drafts, same Danger zone IA, immediate transfer after type-confirm.

### File Created
`.planning/phases/15-releases-transfer/15-RESEARCH.md`

### Confidence Assessment
| Area | Level | Reason |
|------|-------|--------|
| Standard Stack | HIGH | Existing Oxidean seams verified in source |
| Architecture | HIGH | Forge docs + Gitea redirect model + local schema |
| Pitfalls | HIGH | Reconcile races, draft leak, redirect supersede documented |

### Open Questions (RESOLVED)
- RESOLVED: next-free `00NN` migration; rename without type-confirm; SSH via shared helper in 15-03; no `release.latest`; ENV defaults 90d / 512 MiB locked in plans.

### Ready for Planning
Research complete. Plans created; open questions closed to match plan locks.
