//! Pages domain — GitHub Pages parity: per-repo site config (`repo_pages`,
//! existence = enabled), an append-only build history (`pages_builds`), and
//! the `page_build` lifecycle/webhook event on each completed build.
//!
//! Build kinds (GitHub `build_type`):
//! - `legacy` — `pages.builds.request` materializes `source_branch:source_path`
//!   (`/` or `/docs`, GitHub's only allowed subtrees) into the site dir.
//! - `workflow` — `pages.deploys.create`, the server-side half of
//!   `actions/deploy-pages`. The calling job token pins job/run/repository; a
//!   `github-pages` artifact's tar.gz is extracted into the site dir.
//!
//! Serving lives in [`crate::routes::pages_serve`] — `GET /pages/{o}/{r}/{*p}`.

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use oxidean_core::AppError;
use oxidean_db::{PagesBuildRow, RepoPagesRow, UserRow};

use crate::actions::events::LifecycleEvent;
use crate::auth::gate::require_verified;
use crate::git::bare_repo_path;
use crate::repo::{ensure_not_archived, meets, resolve_repo_for_read, AccessibleRepo, Capability};
use crate::rpc::RpcCtx;
use crate::webhook::dispatch;

/// Site blob root — `OXIDEAN_PAGES_DIR` wins; default `var/pages` beside the
/// other `var/` stores.
pub fn resolve_pages_dir() -> PathBuf {
    std::env::var("OXIDEAN_PAGES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/pages"))
}

/// Extracted-site ceiling (files + bytes) — mirrors the artifact bomb guards;
/// a hostile tree/tarball must not expand without bound. GitHub Pages sites
/// are soft-capped at 1 GiB — we hard-fail past it.
const SITE_MAX_BYTES: u64 = 1 << 30;
const SITE_MAX_FILES: u32 = 100_000;

fn db_err(e: String) -> AppError {
    tracing::error!(error = %e, "pages db error");
    AppError::new("pages.internal", "pages operation failed")
}

fn not_found() -> AppError {
    // GitHub 404s `GET /pages` when the site isn't configured — the same code
    // covers unauthorized reads (capability indistinguishable).
    AppError::new("pages.not_found", "Pages site not found")
}

fn build_not_found() -> AppError {
    AppError::new("pages.build_not_found", "Pages build not found")
}

fn invalid(message: &str) -> AppError {
    AppError::new("pages.invalid", message)
}

fn conflict(message: &str) -> AppError {
    AppError::new("pages.conflict", message)
}

/// `pages.*` read gate — repo Read+.
async fn resolve_read(ctx: &RpcCtx, owner: &str, name: &str) -> Result<AccessibleRepo, AppError> {
    resolve_repo_for_read(ctx, owner, name).await
}

/// Config + manual-build gate — GitHub gates Pages administration at
/// maintain/admin; `pages.builds.request` only needs push access upstream,
/// but the row also rebuilds the served site, so Write is the floor. Callers
/// pick the level explicitly.
async fn resolve_capability(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
    need: Capability,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, need) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;
    Ok(accessible)
}

async fn load_site(ctx: &RpcCtx, repo_id: &str) -> Result<RepoPagesRow, AppError> {
    ctx.db
        .find_repo_pages(repo_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)
}

/// Public URL root — `html_url` in payloads, `url` on built builds.
fn site_url(accessible: &AccessibleRepo) -> String {
    format!(
        "/pages/{}/{}/",
        accessible.owner_username, accessible.row.name
    )
}

fn build_type_of(value: &str) -> Result<&'static str, AppError> {
    match value {
        "legacy" => Ok("legacy"),
        "workflow" => Ok("workflow"),
        _ => Err(invalid("build_type must be \"legacy\" or \"workflow\"")),
    }
}

/// GitHub restricts legacy sources to `/` or `/docs` on the publishing branch.
fn validate_source_path(path: &str) -> Result<&str, AppError> {
    match path {
        "/" => Ok("/"),
        "/docs" => Ok("/docs"),
        _ => Err(invalid("source.path must be \"/\" or \"/docs\"")),
    }
}

fn validate_branch(branch: &str) -> Result<&str, AppError> {
    let b = branch.trim();
    if b.is_empty()
        || b.contains('\0')
        || b.contains("..")
        || b.starts_with('-')
        || b.starts_with('/')
        || b.ends_with('/')
        || b.contains(' ')
        || b.contains('~')
        || b.contains('^')
        || b.contains(':')
        || b.contains('?')
        || b.contains('*')
        || b.contains('[')
    {
        return Err(invalid("source.branch is not a valid git ref"));
    }
    Ok(b)
}

fn cname_of(value: Option<&str>) -> Result<String, AppError> {
    let c = value.unwrap_or_default().trim().to_ascii_lowercase();
    if c.len() > 253 {
        return Err(invalid("cname too long"));
    }
    if c.chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-'))
    {
        return Err(invalid("cname must be a DNS hostname"));
    }
    Ok(c)
}

async fn latest_status(ctx: &RpcCtx, repo_id: &str) -> Option<String> {
    ctx.db
        .latest_pages_build(repo_id, None)
        .await
        .ok()
        .flatten()
        .map(|b| b.status)
}

fn site_to_public(
    row: &RepoPagesRow,
    accessible: &AccessibleRepo,
    status: Option<String>,
    site_dir: &Path,
) -> oxidean_core::RepoPagesPublic {
    oxidean_core::RepoPagesPublic {
        repo_id: row.repo_id.clone(),
        build_type: row.build_type.clone(),
        source: oxidean_core::PagesSourcePublic {
            branch: row.source_branch.clone(),
            path: row.source_path.clone(),
        },
        status,
        cname: row.cname.clone(),
        custom_404: site_dir.join("404.html").is_file(),
        https_enforced: row.https_enforced,
        html_url: site_url(accessible),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

async fn build_to_public(ctx: &RpcCtx, row: &PagesBuildRow) -> oxidean_core::PagesBuildPublic {
    let pusher_username = match &row.pusher_id {
        Some(id) => ctx
            .db
            .find_user_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|u| u.username),
        None => None,
    };
    oxidean_core::PagesBuildPublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        status: row.status.clone(),
        build_type: row.build_type.clone(),
        commit: row.commit_sha.clone(),
        branch: row.branch.clone(),
        path: row.path.clone(),
        url: row.url.clone(),
        error_message: if row.error_message.is_empty() {
            None
        } else {
            Some(row.error_message.clone())
        },
        duration_ms: row.duration_ms,
        pusher_id: row.pusher_id.clone(),
        pusher_username,
        run_id: row.run_id.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

fn site_dir(ctx: &RpcCtx, repo_id: &str) -> PathBuf {
    ctx.pages_dir.join(repo_id)
}

// ---------------------------------------------------------------------------
// Site config RPCs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OwnerName {
    owner: String,
    name: String,
}

/// `pages.get` — GitHub `GET /repos/{o}/{r}/pages`; 404 when unconfigured.
pub async fn get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoPagesPublic, AppError> {
    let req: OwnerName = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pages input: {e}")))?;
    let accessible = resolve_read(ctx, &req.owner, &req.name).await?;
    let row = load_site(ctx, &accessible.row.id).await?;
    let status = latest_status(ctx, &accessible.row.id).await;
    Ok(site_to_public(
        &row,
        &accessible,
        status,
        &site_dir(ctx, &accessible.row.id),
    ))
}

/// `pages.enable` — GitHub `POST /repos/{o}/{r}/pages`; 409 when configured.
pub async fn enable(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoPagesPublic, AppError> {
    let req: oxidean_core::RepoPagesEnableRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pages.enable input: {e}")))?;
    let accessible = resolve_capability(ctx, &req.owner, &req.name, Capability::Admin).await?;
    require_verified(ctx).await?;
    if ctx
        .db
        .find_repo_pages(&accessible.row.id)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(conflict("Pages site already exists — update it instead"));
    }
    let build_type = build_type_of(req.build_type.as_deref().unwrap_or("legacy"))?;
    let (branch, path) = match &req.source {
        Some(s) => (
            validate_branch(&s.branch)?.to_string(),
            validate_source_path(&s.path)?.to_string(),
        ),
        None => (
            if build_type == "legacy" {
                accessible.row.default_branch.clone()
            } else {
                String::new()
            },
            "/".to_string(),
        ),
    };
    if build_type == "legacy" {
        // The publishing branch must resolve — catching a typo here beats a
        // failed first build.
        let bare = bare_repo_path(
            &ctx.repos_dir,
            &accessible.owner_username,
            &accessible.row.name,
        )?;
        ctx.git
            .rev_parse(&bare, &format!("refs/heads/{branch}"))
            .await
            .map_err(|_| invalid("source.branch does not resolve to a branch head"))?;
    }
    let row = ctx
        .db
        .upsert_repo_pages(&accessible.row.id, build_type, &branch, &path, "", true)
        .await
        .map_err(db_err)?;
    let mut status = None;
    if build_type == "legacy" {
        // GitHub queues the first build on POST /pages.
        let user = require_verified(ctx).await?;
        let built = run_legacy_build(&deps_from_ctx(ctx), &accessible, &row, &user).await?;
        status = Some(built.status);
    }
    Ok(site_to_public(
        &row,
        &accessible,
        status,
        &site_dir(ctx, &accessible.row.id),
    ))
}

/// `pages.update` — GitHub `PUT /repos/{o}/{r}/pages` (source/build_type) plus
/// the cname/https_enforced PATCH surface; omitted fields keep their value.
pub async fn update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoPagesPublic, AppError> {
    let req: oxidean_core::RepoPagesUpdateRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pages.update input: {e}")))?;
    let accessible = resolve_capability(ctx, &req.owner, &req.name, Capability::Admin).await?;
    require_verified(ctx).await?;
    let current = load_site(ctx, &accessible.row.id).await?;
    let build_type = match &req.build_type {
        Some(v) => build_type_of(v)?,
        None => current.build_type.as_str(),
    };
    let (branch, path) = match &req.source {
        Some(s) => (
            validate_branch(&s.branch)?.to_string(),
            validate_source_path(&s.path)?.to_string(),
        ),
        None => (current.source_branch.clone(), current.source_path.clone()),
    };
    if build_type == "legacy" && !branch.is_empty() {
        let bare = bare_repo_path(
            &ctx.repos_dir,
            &accessible.owner_username,
            &accessible.row.name,
        )?;
        ctx.git
            .rev_parse(&bare, &format!("refs/heads/{branch}"))
            .await
            .map_err(|_| invalid("source.branch does not resolve to a branch head"))?;
    }
    let cname = cname_of(req.cname.as_deref().or(Some(&current.cname)))?;
    let https = req.https_enforced.unwrap_or(current.https_enforced);
    let row = ctx
        .db
        .upsert_repo_pages(
            &accessible.row.id,
            build_type,
            &branch,
            &path,
            &cname,
            https,
        )
        .await
        .map_err(db_err)?;
    let status = latest_status(ctx, &accessible.row.id).await;
    Ok(site_to_public(
        &row,
        &accessible,
        status,
        &site_dir(ctx, &accessible.row.id),
    ))
}

/// `pages.disable` — GitHub `DELETE /repos/{o}/{r}/pages`: unpublishes the
/// site (config row + served tree); build history stays (GitHub keeps it).
pub async fn disable(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::RepoPagesDeleteResponse, AppError> {
    let req: OwnerName = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pages input: {e}")))?;
    let accessible = resolve_capability(ctx, &req.owner, &req.name, Capability::Admin).await?;
    require_verified(ctx).await?;
    load_site(ctx, &accessible.row.id).await?;
    ctx.db
        .delete_repo_pages(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let dir = site_dir(ctx, &accessible.row.id);
    if dir.exists() {
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
    Ok(oxidean_core::RepoPagesDeleteResponse { ok: true })
}

// ---------------------------------------------------------------------------
// Builds
// ---------------------------------------------------------------------------

/// `pages.builds.list` — newest-first history (GitHub `GET /pages/builds`).
pub async fn builds_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::PagesBuildsListResponse, AppError> {
    let req: oxidean_core::PagesBuildsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pages.builds.list input: {e}"),
        )
    })?;
    let accessible = resolve_read(ctx, &req.owner, &req.name).await?;
    load_site(ctx, &accessible.row.id).await?;
    let status = match req.status.as_deref() {
        None => None,
        Some("built") => Some("built"),
        Some("errored") => Some("errored"),
        Some("building") => Some("building"),
        Some(_) => return Err(invalid("status must be built|errored|building")),
    };
    let limit = req.limit.unwrap_or(30).clamp(1, 100);
    let rows = ctx
        .db
        .list_pages_builds(&accessible.row.id, status, limit)
        .await
        .map_err(db_err)?;
    let mut builds = Vec::with_capacity(rows.len());
    for row in &rows {
        builds.push(build_to_public(ctx, row).await);
    }
    Ok(oxidean_core::PagesBuildsListResponse { builds })
}

/// `pages.builds.get` — one build, or `"latest"` for the newest of any status.
pub async fn builds_get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::PagesBuildPublic, AppError> {
    let req: oxidean_core::PagesBuildGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pages.builds.get input: {e}"),
        )
    })?;
    let accessible = resolve_read(ctx, &req.owner, &req.name).await?;
    load_site(ctx, &accessible.row.id).await?;
    let row = if req.build == "latest" {
        ctx.db
            .latest_pages_build(&accessible.row.id, None)
            .await
            .map_err(db_err)?
    } else {
        match ctx.db.find_pages_build(&req.build).await.map_err(db_err)? {
            Some(b) if b.repo_id == accessible.row.id => Some(b),
            _ => None,
        }
    }
    .ok_or_else(build_not_found)?;
    Ok(build_to_public(ctx, &row).await)
}

/// `pages.builds.request` — GitHub `POST /pages/builds`: synchronously rebuilds
/// a `legacy` site from `source_branch:source_path`. `workflow` sites 422 —
/// their builds arrive through [`deploys_create`].
pub async fn builds_request(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::PagesBuildPublic, AppError> {
    let req: OwnerName = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pages input: {e}")))?;
    let accessible = resolve_capability(ctx, &req.owner, &req.name, Capability::Write).await?;
    let user = require_verified(ctx).await?;
    let site = load_site(ctx, &accessible.row.id).await?;
    if site.build_type != "legacy" {
        return Err(conflict(
            "workflow sites build from Actions deploys, not manual requests",
        ));
    }
    let row = run_legacy_build(&deps_from_ctx(ctx), &accessible, &site, &user).await?;
    Ok(build_to_public(ctx, &row).await)
}

/// `pages.deploys.create` — the `actions/deploy-pages` seam: job-token only,
/// job/run/repo all derived from the calling token's row. Requires
/// `environment: github-pages` on the job and a `workflow`-type site.
pub async fn deploys_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::PagesBuildPublic, AppError> {
    let req: oxidean_core::PagesDeployCreateRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid pages.deploys.create input: {e}"),
            )
        })?;
    let pat = ctx
        .pat
        .as_ref()
        .ok_or_else(|| AppError::new("pages.forbidden", "requires a workflow job token"))?;
    if !crate::pat::bearer::is_job_token(pat) {
        return Err(AppError::new(
            "pages.forbidden",
            "only Actions job tokens may publish Pages deploys",
        ));
    }
    let token_row = ctx
        .db
        .find_action_job_token_by_id(&pat.token_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("pages.forbidden", "job token not recognized"))?;

    // Repo resolution rides the ordinary read path — a job token's
    // repository_ids pinning is enforced at the bearer edge before dispatch.
    let accessible = resolve_read(ctx, &req.owner, &req.name).await?;
    if token_row.repository_id != accessible.row.id {
        return Err(AppError::new(
            "pages.forbidden",
            "job token is not minted for this repository",
        ));
    }
    ensure_not_archived(&accessible)?;
    let site = load_site(ctx, &accessible.row.id).await?;
    if site.build_type != "workflow" {
        return Err(conflict(
            "site is not configured for workflow deployments (build_type: workflow)",
        ));
    }
    let job = ctx
        .db
        .find_action_job_by_id(&token_row.job_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("pages.forbidden", "job not found for token"))?;
    if job.environment_name.as_deref() != Some("github-pages") {
        return Err(AppError::new(
            "pages.forbidden",
            "Pages deploys require environment: github-pages",
        ));
    }
    let run = ctx
        .db
        .find_action_run_by_id(&token_row.run_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("pages.forbidden", "run not found for token"))?;
    let artifact_id = req
        .artifact_id
        .as_deref()
        .map(str::trim)
        .filter(|i| !i.is_empty());
    let artifact = match artifact_id {
        // GitHub's `artifact_url` arrives resolved to a row id by the REST
        // edge — still pinned to this token's run before the job check below.
        Some(id) => {
            let a = ctx
                .db
                .find_action_artifact_by_id(id)
                .await
                .map_err(db_err)?
                .ok_or_else(|| invalid("artifact not found for this run"))?;
            if a.run_id != token_row.run_id {
                return Err(invalid("artifact not found for this run"));
            }
            a
        }
        None => {
            let artifact_name = req
                .artifact
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .unwrap_or("github-pages");
            ctx.db
                .find_action_artifact(&token_row.run_id, artifact_name)
                .await
                .map_err(db_err)?
                .ok_or_else(|| invalid("artifact not found for this run"))?
        }
    };
    if artifact.repository_id != accessible.row.id
        || artifact.job_id.as_deref() != Some(token_row.job_id.as_str())
    {
        // The token's job produced the artifact — a sibling job's artifact of
        // the same name is not deployable by this token.
        return Err(invalid("artifact does not belong to this job"));
    }
    let tarball = ctx.actions_artifacts_dir.join(&artifact.storage_path);
    let row = run_workflow_build(
        &deps_from_ctx(ctx),
        &accessible,
        &tarball,
        &run,
        &token_row.user_id,
    )
    .await?;
    Ok(build_to_public(ctx, &row).await)
}

// ---------------------------------------------------------------------------
// Build pipeline
// ---------------------------------------------------------------------------

fn repository_and_sender(
    accessible: &AccessibleRepo,
    user: Option<&UserRow>,
) -> (serde_json::Value, serde_json::Value) {
    (
        crate::actions::payload::repository_embed(
            &accessible.owner_username,
            &accessible.row.name,
            &accessible.row.default_branch,
        ),
        crate::actions::payload::user_embed(
            user.map(|u| u.username.as_str())
                .unwrap_or("github-pages[bot]"),
        ),
    )
}

/// Everything the build pipeline + `page_build` emission need — the RPC
/// handlers source it from [`RpcCtx`], the post-receive seam from `AppState`.
struct BuildDeps<'a> {
    db: &'a oxidean_db::Database,
    git: &'a dyn oxidean_git::GitBackend,
    repos_dir: &'a Path,
    pages_dir: &'a Path,
    env_name: &'a str,
    job_token_actor: bool,
    actions_enabled: bool,
}

fn deps_from_ctx<'a>(ctx: &'a RpcCtx) -> BuildDeps<'a> {
    BuildDeps {
        db: &ctx.db,
        git: ctx.git.as_ref(),
        repos_dir: &ctx.repos_dir,
        pages_dir: &ctx.pages_dir,
        env_name: &ctx.env_name,
        job_token_actor: crate::actions::events::rpc_caller_is_job_token(ctx),
        actions_enabled: crate::actions::env_actions_enabled(),
    }
}

/// `page_build` is typeless on GitHub — emitted with an empty action so the
/// lifecycle `types:` filter can't suppress it (gollum precedent).
async fn emit_page_build(
    deps: &BuildDeps<'_>,
    accessible: &AccessibleRepo,
    build: &PagesBuildRow,
    pusher_login: Option<&str>,
    sender: serde_json::Value,
    repository: serde_json::Value,
    actor_id: Option<String>,
) {
    let payload =
        crate::actions::payload::page_build_payload(build, pusher_login, &repository, &sender);
    dispatch::emit(
        deps.db,
        &accessible.row.id,
        "page_build",
        "",
        payload.clone(),
        deps.env_name,
    )
    .await;
    let Ok(bare) = bare_repo_path(
        deps.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    ) else {
        return;
    };
    crate::actions::events::notify_lifecycle_actions(
        deps.db,
        deps.git,
        &LifecycleEvent {
            bare_repo: bare,
            repository_id: accessible.row.id.clone(),
            default_branch: accessible.row.default_branch.clone(),
            event_name: "page_build",
            action: String::new(),
            payload,
            actor_id,
            job_token_actor: deps.job_token_actor,
            actions_enabled_instance: deps.actions_enabled,
        },
    )
    .await;
}

/// Run a `legacy` build: `source_branch:source_path` → site dir → `built`|
/// `errored` row → `page_build`.
async fn run_legacy_build(
    deps: &BuildDeps<'_>,
    accessible: &AccessibleRepo,
    site: &RepoPagesRow,
    user: &UserRow,
) -> Result<PagesBuildRow, AppError> {
    let bare = bare_repo_path(
        deps.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let commit = deps
        .git
        .rev_parse(&bare, &format!("refs/heads/{}", site.source_branch))
        .await
        .map_err(|_| invalid("source.branch does not resolve to a branch head"))?;
    let build_id = uuid::Uuid::new_v4().to_string();
    let row = deps
        .db
        .insert_pages_build(
            &build_id,
            &accessible.row.id,
            "legacy",
            &commit,
            &site.source_branch,
            &site.source_path,
            Some(&user.id),
            None,
        )
        .await
        .map_err(db_err)?;
    let started = std::time::Instant::now();
    let staging = deps
        .pages_dir
        .join(format!("{}.new-{}", accessible.row.id, build_id));
    let url = site_url(accessible);
    let result = materialize_tree(deps, &bare, &commit, &site.source_path, &staging).await;
    let site_dir = deps.pages_dir.join(&accessible.row.id);
    let outcome = match result {
        Ok(()) => match swap_site_dir(&site_dir, &staging).await {
            Ok(()) => ("built", url.clone(), String::new()),
            Err(e) => ("errored", String::new(), e),
        },
        Err(e) => ("errored", String::new(), e),
    };
    let finished = deps
        .db
        .finish_pages_build(
            &row.id,
            outcome.0,
            &outcome.1,
            &outcome.2,
            started.elapsed().as_millis() as i64,
        )
        .await
        .map_err(db_err)?
        .unwrap_or(row);
    let (repository, sender) = repository_and_sender(accessible, Some(user));
    emit_page_build(
        deps,
        accessible,
        &finished,
        Some(&user.username),
        sender,
        repository,
        Some(user.id.clone()),
    )
    .await;
    Ok(finished)
}

/// Run a `workflow` build: artifact tarball → site dir → row → `page_build`.
async fn run_workflow_build(
    deps: &BuildDeps<'_>,
    accessible: &AccessibleRepo,
    tarball: &Path,
    run: &oxidean_db::actions::ActionRunRow,
    pusher_id: &str,
) -> Result<PagesBuildRow, AppError> {
    let build_id = uuid::Uuid::new_v4().to_string();
    // `head_ref` is `refs/heads/x` — the public surface carries the short name.
    let branch = run
        .head_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(&run.head_ref)
        .to_string();
    let row = deps
        .db
        .insert_pages_build(
            &build_id,
            &accessible.row.id,
            "workflow",
            &run.head_sha,
            &branch,
            "/",
            Some(pusher_id),
            Some(&run.id),
        )
        .await
        .map_err(db_err)?;
    let started = std::time::Instant::now();
    let staging = deps
        .pages_dir
        .join(format!("{}.new-{}", accessible.row.id, build_id));
    let url = site_url(accessible);
    let result = extract_tar_gz(tarball, &staging).await;
    let site_dir = deps.pages_dir.join(&accessible.row.id);
    let outcome = match result {
        Ok(()) => match swap_site_dir(&site_dir, &staging).await {
            Ok(()) => ("built", url.clone(), String::new()),
            Err(e) => ("errored", String::new(), e),
        },
        Err(e) => ("errored", String::new(), e),
    };
    let finished = deps
        .db
        .finish_pages_build(
            &row.id,
            outcome.0,
            &outcome.1,
            &outcome.2,
            started.elapsed().as_millis() as i64,
        )
        .await
        .map_err(db_err)?
        .unwrap_or(row);
    let user = deps.db.find_user_by_id(pusher_id).await.ok().flatten();
    let (repository, sender) = repository_and_sender(accessible, user.as_ref());
    emit_page_build(
        deps,
        accessible,
        &finished,
        user.as_ref().map(|u| u.username.as_str()),
        sender,
        repository,
        Some(pusher_id.to_string()),
    )
    .await;
    Ok(finished)
}

/// Materialize `commit:{path_prefix}` into `staging` — every blob under the
/// subtree becomes a file. Caps files + bytes (hostile trees fail, not fill).
async fn materialize_tree(
    deps: &BuildDeps<'_>,
    bare: &Path,
    commit: &str,
    path_prefix: &str,
    staging: &Path,
) -> Result<(), String> {
    let prefix = path_prefix.trim_start_matches('/');
    let entries = deps
        .git
        .ls_tree_sized_blobs(bare, commit, SITE_MAX_FILES)
        .await
        .map_err(|e| format!("ls-tree failed: {e}"))?;
    tokio::fs::create_dir_all(staging)
        .await
        .map_err(|e| format!("site dir create failed: {e}"))?;
    let mut bytes = 0u64;
    for entry in entries {
        // Component-boundary match — `docsx/f` must not satisfy prefix `docs`.
        let rel = if prefix.is_empty() {
            entry.path.as_str()
        } else {
            match entry.path.strip_prefix(&format!("{prefix}/")) {
                Some(r) => r,
                None => continue,
            }
        };
        if rel.is_empty() {
            continue;
        }
        let dest =
            safe_site_path(staging, rel).ok_or_else(|| format!("unsafe path in tree: {rel}"))?;
        bytes = bytes.saturating_add(entry.size);
        if bytes > SITE_MAX_BYTES {
            return Err("site exceeds the 1 GiB ceiling".to_string());
        }
        let content = deps
            .git
            .cat_blob(bare, commit, &entry.path)
            .await
            .map_err(|e| format!("cat_blob {} failed: {e}", entry.path))?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("site dir create failed: {e}"))?;
        }
        tokio::fs::write(&dest, &content)
            .await
            .map_err(|e| format!("site file write failed: {e}"))?;
    }
    Ok(())
}

/// Extract a `.tar.gz` artifact into `staging`. Every entry is normalized —
/// absolute/`..`/non-file members are skipped so a hostile tarball cannot
/// write outside the site dir.
async fn extract_tar_gz(tarball: &Path, staging: &Path) -> Result<(), String> {
    let tarball = tarball.to_path_buf();
    let staging = staging.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let file =
            std::fs::File::open(&tarball).map_err(|e| format!("artifact open failed: {e}"))?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        std::fs::create_dir_all(&staging).map_err(|e| format!("site dir create failed: {e}"))?;
        let mut bytes = 0u64;
        let mut files = 0u32;
        for entry in archive
            .entries()
            .map_err(|e| format!("artifact read failed: {e}"))?
        {
            let mut entry = entry.map_err(|e| format!("artifact entry failed: {e}"))?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let path = entry
                .path()
                .map_err(|e| format!("artifact path failed: {e}"))?;
            let rel = path.to_string_lossy().replace('\\', "/");
            let rel = rel.trim_start_matches("./").trim_start_matches('/');
            if rel.is_empty() {
                continue;
            }
            let Some(dest) = safe_site_path(&staging, rel) else {
                continue; // `..`/absolute members are dropped, not fatal
            };
            let size = entry.size();
            bytes = bytes.saturating_add(size);
            if bytes > SITE_MAX_BYTES {
                return Err("site exceeds the 1 GiB ceiling".to_string());
            }
            files += 1;
            if files > SITE_MAX_FILES {
                return Err("site exceeds the file-count ceiling".to_string());
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("site dir create failed: {e}"))?;
            }
            let mut out = std::fs::File::create(&dest)
                .map_err(|e| format!("site file create failed: {e}"))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("site file write failed: {e}"))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("extract task failed: {e}"))?
}

/// `staging/{rel}` when `rel` stays inside `staging` — else `None`.
fn safe_site_path(staging: &Path, rel: &str) -> Option<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel_path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    Some(staging.join(rel_path))
}

/// Swap `staging` over `site` — the old tree moves aside first so readers see
/// either the previous or the new tree, never a half-written mix.
async fn swap_site_dir(site: &Path, staging: &Path) -> Result<(), String> {
    let backup = site.with_extension("old");
    if backup.exists() {
        tokio::fs::remove_dir_all(&backup)
            .await
            .map_err(|e| format!("site backup cleanup failed: {e}"))?;
    }
    if site.exists() {
        tokio::fs::rename(site, &backup)
            .await
            .map_err(|e| format!("site backup rename failed: {e}"))?;
    }
    if let Err(e) = tokio::fs::rename(staging, site).await {
        // Roll the previous tree back — a failed publish keeps the old site up.
        let _ = tokio::fs::rename(&backup, site).await;
        return Err(format!("site publish failed: {e}"));
    }
    if backup.exists() {
        let _ = tokio::fs::remove_dir_all(&backup).await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Push-driven rebuild (transport seam)
// ---------------------------------------------------------------------------

/// Legacy rebuild after a push — GitHub Pages parity: pushes to the
/// configured source branch rebuild the site and fire `page_build`.
/// `updates` are `(old, new, refname)` triples from the receive-pack; branch
/// deletion (zero new) leaves the last-built site up, matching GitHub.
///
/// Called from the post-receive fan-out — deps arrive explicitly because
/// transports don't hold an [`RpcCtx`].
#[allow(clippy::too_many_arguments)]
pub async fn rebuild_after_push(
    db: &oxidean_db::Database,
    git: &dyn oxidean_git::GitBackend,
    repos_dir: &Path,
    pages_dir: &Path,
    env_name: &str,
    repo_id: &str,
    owner_slug: &str,
    pusher_id: &str,
    updates: &[(String, String, String)],
    job_token_actor: bool,
    actions_enabled: bool,
) {
    let Ok(site) = db.find_repo_pages(repo_id).await else {
        return;
    };
    let Some(site) = site else { return };
    if site.build_type != "legacy" || site.source_branch.is_empty() {
        return;
    }
    let source_ref = format!("refs/heads/{}", site.source_branch);
    let Some((_, new_sha, _)) = updates.iter().find(|(_, _, r)| r == &source_ref) else {
        return;
    };
    let zero = "0".repeat(40);
    if new_sha == &zero {
        return; // deleted source branch — last built tree stays served (GitHub parity)
    }
    let Some(pusher) = db.find_user_by_id(pusher_id).await.ok().flatten() else {
        return;
    };
    let accessible = AccessibleRepo {
        row: match db.find_repository_by_id(repo_id).await {
            Ok(Some(r)) => r,
            _ => return,
        },
        owner_username: owner_slug.to_string(),
        capability: None,
    };
    let deps = BuildDeps {
        db,
        git,
        repos_dir,
        pages_dir,
        env_name,
        job_token_actor,
        actions_enabled,
    };
    if let Err(e) = run_legacy_build(&deps, &accessible, &site, &pusher).await {
        tracing::warn!(error = ?e, repo_id, "pages rebuild_after_push failed");
    }
}
