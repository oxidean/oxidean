//! Pull labels/assignees setters + per-user file-viewed marks (WI-0001).

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::pull::{acl, db_err, load_pull_in_repo, to_public};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    AppError, PullFilesViewedRequest, PullFilesViewedResponse, PullPublic, SetPullAssigneesRequest,
    SetPullLabelsRequest,
};

/// `pull.labels.set` — Write+; replace label ids from the repo's effective
/// set. Mirrors `issue.labels.set` without the actions-event fanout (pull
/// label events are not modeled).
pub async fn labels_set(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullPublic, AppError> {
    require_verified(ctx).await?;
    let req: SetPullLabelsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.labels.set input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;

    let allowed = oxidean_api_core::label::effective_label_id_set(
        ctx,
        &accessible.row.id,
        &accessible.row.owner_type,
        &accessible.row.owner_id,
    )
    .await?;

    let mut seen = std::collections::HashSet::new();
    let mut unique_ids = Vec::new();
    for id in &req.label_ids {
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::new("rpc.bad_input", "label id must not be empty"));
        }
        if !allowed.contains(id) {
            return Err(AppError::new(
                "rpc.bad_input",
                "label is not in the effective set for this repository",
            ));
        }
        if seen.insert(id.to_string()) {
            unique_ids.push(id.to_string());
        }
    }

    ctx.db
        .set_pull_labels(&pull.id, &unique_ids)
        .await
        .map_err(db_err)?;
    to_public(ctx, &pull).await
}

/// `pull.assignees.set` — Write+; each user_id must exist and have Read+ on
/// the repo. Mirrors `issue.assignees.set` without notification fanout.
pub async fn assignees_set(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullPublic, AppError> {
    require_verified(ctx).await?;
    let req: SetPullAssigneesRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.assignees.set input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;

    let owner_ref = oxidean_api_core::repo::owner_ref_for_repo(&ctx.db, &accessible.row)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "owner_ref_for_repo failed");
            AppError::new("pull.internal", "pull operation failed")
        })?
        .ok_or_else(|| AppError::new("pull.not_found", "Pull not found"))?;

    let mut seen = std::collections::HashSet::new();
    let mut unique_ids = Vec::new();
    for id in &req.user_ids {
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::new("rpc.bad_input", "user id must not be empty"));
        }
        if !seen.insert(id.to_string()) {
            continue;
        }
        let Some(_u) = ctx.db.find_user_by_id(id).await.map_err(db_err)? else {
            return Err(AppError::new(
                "rpc.bad_input",
                "assignee user was not found",
            ));
        };
        let cap = oxidean_api_core::repo::effective_capability(
            &ctx.db,
            Some(id),
            &accessible.row,
            &owner_ref,
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "effective_capability for assignee failed");
            AppError::new("pull.internal", "pull operation failed")
        })?;
        if !oxidean_api_core::repo::meets(cap, oxidean_api_core::repo::Capability::Read) {
            return Err(AppError::new(
                "rpc.bad_input",
                "assignee must have Read access on this repository",
            ));
        }
        unique_ids.push(id.to_string());
    }

    ctx.db
        .set_pull_assignees(&pull.id, &unique_ids)
        .await
        .map_err(db_err)?;
    to_public(ctx, &pull).await
}

/// `pull.files.setViewed` — session-required; records the mark against the
/// pull's current head sha so the client can invalidate after a push.
pub async fn files_set_viewed(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullFilesViewedResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: oxidean_core::PullFilesSetViewedRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid pull.files.setViewed input: {e}"),
            )
        })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let path = req.path.trim();
    if path.is_empty() {
        return Err(AppError::new("rpc.bad_input", "path must not be empty"));
    }
    ctx.db
        .set_pull_file_view(&pull.id, &user.id, path, &pull.head_sha, req.viewed)
        .await
        .map_err(db_err)?;
    let viewed_paths = ctx
        .db
        .list_pull_file_views(&pull.id, &user.id)
        .await
        .map_err(db_err)?;
    Ok(PullFilesViewedResponse { viewed_paths })
}

/// `pull.files.listViewed` — session-required; anonymous users get 401 like
/// other session-bound reads.
pub async fn files_list_viewed(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullFilesViewedResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: PullFilesViewedRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.files.listViewed input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let viewed_paths = ctx
        .db
        .list_pull_file_views(&pull.id, &user.id)
        .await
        .map_err(db_err)?;
    Ok(PullFilesViewedResponse { viewed_paths })
}
