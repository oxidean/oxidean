//! Per-repository collaborator CRUD (`repo.collaborators.*` — ORG-03 / D-ORG-02c / D-ORG-04).
//!
//! Admin-gated via Capability ACL (T-10-01 / T-10-02). Collaborator is never an org role.

use oxidean_core::{
    AppError, CollaboratorPermission, RepoCollaboratorPublic, RepoCollaboratorsAddRequest,
    RepoCollaboratorsListResponse, RepoCollaboratorsRemoveRequest, RepoCollaboratorsUpdateRequest,
    RepoGetRequest,
};
use oxidean_db::RepoCollaboratorListRow;

use crate::auth::gate::require_verified;
use crate::notify;
use crate::repo::{acl, meets, resolve_repo_for_read, AccessibleRepo, Capability};
use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new(
            "repo.collaborator_exists",
            "That user is already a collaborator on this repository.",
        )
    } else if e == "repo collaborator not found" {
        AppError::new("repo.collaborator_not_found", "collaborator not found")
    } else {
        tracing::error!("repo collaborator db error: {e}");
        AppError::new("repo.internal", "repository operation failed")
    }
}

fn collab_public(row: &RepoCollaboratorListRow) -> Result<RepoCollaboratorPublic, AppError> {
    let permission = CollaboratorPermission::parse(&row.permission).map_err(|e| {
        tracing::error!(error = %e, "invalid collaborator permission in row");
        AppError::new("repo.internal", "repository operation failed")
    })?;
    Ok(RepoCollaboratorPublic {
        user_id: row.user_id.clone(),
        username: row.username.clone(),
        permission,
        created_at: row.created_at.clone(),
    })
}

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

/// `repo.collaborators.list` — Admin only (T-10-01).
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoCollaboratorsListResponse, AppError> {
    let req: RepoGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.collaborators.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let rows = ctx
        .db
        .list_repo_collaborators(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let mut collaborators = Vec::with_capacity(rows.len());
    for row in rows {
        collaborators.push(collab_public(&row)?);
    }
    Ok(RepoCollaboratorsListResponse { collaborators })
}

/// `repo.collaborators.add` — Admin; username of existing user; permission read|write|admin.
pub async fn add(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoCollaboratorPublic, AppError> {
    let req: RepoCollaboratorsAddRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.collaborators.add input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;

    let username = req.username.trim();
    if username.is_empty() {
        return Err(AppError::new("rpc.bad_input", "username is required"));
    }
    let target = ctx
        .db
        .find_user_by_username(username)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.user_not_found", "user not found"))?;

    if ctx
        .db
        .find_repo_collaborator(&accessible.row.id, &target.id)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "repo.collaborator_exists",
            "That user is already a collaborator on this repository.",
        ));
    }

    let row = ctx
        .db
        .insert_repo_collaborator(&accessible.row.id, &target.id, req.permission.as_str())
        .await
        .map_err(db_err)?;

    Ok(RepoCollaboratorPublic {
        user_id: row.user_id,
        username: target.username,
        permission: req.permission,
        created_at: row.created_at,
    })
}

/// `repo.collaborators.update` — Admin; change permission ladder.
pub async fn update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoCollaboratorPublic, AppError> {
    let req: RepoCollaboratorsUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.collaborators.update input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;

    let row = ctx
        .db
        .update_repo_collaborator_permission(
            &accessible.row.id,
            &req.user_id,
            req.permission.as_str(),
        )
        .await
        .map_err(db_err)?;

    // DEBT-06: re-check read — if the new permission revoked access, auto-unwatch
    // and drop stale notification rows (GitHub behavior).
    notify::prune_if_repo_read_lost(&ctx.db, &req.user_id, &accessible.row.id).await;

    let user = ctx
        .db
        .find_user_by_id(&row.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.user_not_found", "user not found"))?;

    Ok(RepoCollaboratorPublic {
        user_id: row.user_id,
        username: user.username,
        permission: req.permission,
        created_at: row.created_at,
    })
}

/// `repo.collaborators.remove` — Admin.
pub async fn remove(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let req: RepoCollaboratorsRemoveRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.collaborators.remove input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    ctx.db
        .remove_repo_collaborator(&accessible.row.id, &req.user_id)
        .await
        .map_err(db_err)?;
    // DEBT-06: re-check read — if removal revoked access, auto-unwatch and drop
    // stale notification rows (GitHub behavior).
    notify::prune_if_repo_read_lost(&ctx.db, &req.user_id, &accessible.row.id).await;
    Ok(serde_json::json!({ "ok": true }))
}
