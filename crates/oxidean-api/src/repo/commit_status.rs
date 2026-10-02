//! `repo.commitStatus.*` classic statuses (D-11..13 / D-26).

use oxidean_core::{
    AppError, CommitStatusCreateRequest, CommitStatusListRequest, CommitStatusListResponse,
    CommitStatusPublic, CommitStatusState,
};
use oxidean_db::CommitStatusRow;
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::repo::{acl, meets, resolve_repo_for_read, Capability};
use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "commit status db error");
        AppError::new("repo.internal", "repository operation failed")
    }
}

fn row_public(row: &CommitStatusRow) -> Result<CommitStatusPublic, AppError> {
    let state = CommitStatusState::parse(&row.state)
        .map_err(|e| AppError::new("repo.internal", e))?;
    Ok(CommitStatusPublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        sha: row.sha.clone(),
        context: row.context.clone(),
        state,
        description: row.description.clone(),
        target_url: row.target_url.clone(),
        creator_id: row.creator_id.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    })
}

fn validate_sha(sha: &str) -> Result<(), AppError> {
    let s = sha.trim();
    if s.len() < 7 || s.len() > 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::new("rpc.bad_input", "invalid commit sha"));
    }
    Ok(())
}

fn validate_context(context: &str) -> Result<(), AppError> {
    let c = context.trim();
    if c.is_empty() || c.len() > 255 {
        return Err(AppError::new(
            "rpc.bad_input",
            "status context must be 1–255 characters",
        ));
    }
    Ok(())
}

/// `repo.commitStatus.create` — Write+.
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CommitStatusPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: CommitStatusCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.commitStatus.create input: {e}"),
        )
    })?;
    validate_sha(&req.sha)?;
    validate_context(&req.context)?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(acl::not_found());
    }
    // GIT-20: commit statuses are writes — frozen while archived.
    acl::ensure_not_archived(&accessible)?;
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .upsert_commit_status(
            &id,
            &accessible.row.id,
            req.sha.trim(),
            req.context.trim(),
            req.state.as_str(),
            req.description.trim(),
            req.target_url.as_deref(),
            Some(user.id.as_str()),
        )
        .await
        .map_err(db_err)?;
    row_public(&row)
}

/// `repo.commitStatus.list` — Read+.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CommitStatusListResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: CommitStatusListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.commitStatus.list input: {e}"),
        )
    })?;
    validate_sha(&req.sha)?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(acl::not_found());
    }
    let rows = ctx
        .db
        .list_commit_statuses(&accessible.row.id, req.sha.trim())
        .await
        .map_err(db_err)?;
    let mut statuses = Vec::with_capacity(rows.len());
    for row in &rows {
        statuses.push(row_public(row)?);
    }
    Ok(CommitStatusListResponse { statuses })
}
