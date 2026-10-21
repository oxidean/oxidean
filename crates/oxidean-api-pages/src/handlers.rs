//! `pages.*` RPC handlers — extracted from `core::pages` (#162).

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::git::bare_repo_path;
use oxidean_api_core::repo::{ensure_not_archived, Capability};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::AppError;

use oxidean_api_core::pages::*;

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
    if !oxidean_api_core::pat::bearer::is_job_token(pat) {
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
