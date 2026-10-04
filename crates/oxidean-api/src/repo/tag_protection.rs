//! `repo.tagProtection.*` Admin CRUD (GIT-21) — protected tag rulesets.
//!
//! A matching rule restricts create/update/delete on `refs/tags/{pattern}` for
//! actors below Admin (and for admins when `enforce_admins` is set). Mirrors
//! `repo.branchProtection.*` shape and gating.

use oxidean_core::{
    AppError, RepoGetRequest, TagProtectionDeleteRequest, TagProtectionListResponse,
    TagProtectionRuleInput, TagProtectionRulePublic, TagProtectionUpdateRequest,
};
use oxidean_db::TagProtectionRuleRow;
use uuid::Uuid;

use crate::repo::collaborators::resolve_repo_for_admin;
use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e == "tag protection rule not found" {
        AppError::new(
            "repo.tag_protection_not_found",
            "Tag protection rule not found",
        )
    } else {
        tracing::error!(error = %e, "tag protection db error");
        AppError::new("repo.internal", "repository operation failed")
    }
}

fn validate_pattern(pattern: &str) -> Result<(), AppError> {
    let p = pattern.trim();
    if p.is_empty() || p.len() > 255 {
        return Err(AppError::new(
            "rpc.bad_input",
            "tag pattern must be 1-255 characters",
        ));
    }
    if p.contains('\0') || p.contains("..") {
        return Err(AppError::new("rpc.bad_input", "invalid tag pattern"));
    }
    Ok(())
}

fn row_public(row: &TagProtectionRuleRow) -> TagProtectionRulePublic {
    TagProtectionRulePublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        pattern: row.pattern.clone(),
        allow_create: row.allow_create,
        allow_update: row.allow_update,
        allow_delete: row.allow_delete,
        enforce_admins: row.enforce_admins,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// `repo.tagProtection.list` — Admin.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<TagProtectionListResponse, AppError> {
    let req: RepoGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.tagProtection.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let rows = ctx
        .db
        .list_tag_protection_rules(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(TagProtectionListResponse {
        rules: rows.iter().map(row_public).collect(),
    })
}

/// `repo.tagProtection.create` — Admin.
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<TagProtectionRulePublic, AppError> {
    let req: TagProtectionRuleInput = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.tagProtection.create input: {e}"),
        )
    })?;
    validate_pattern(&req.pattern)?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_tag_protection_rule(
            &id,
            &accessible.row.id,
            req.pattern.trim(),
            req.allow_create,
            req.allow_update,
            req.allow_delete,
            req.enforce_admins,
        )
        .await
        .map_err(db_err)?;
    Ok(row_public(&row))
}

/// `repo.tagProtection.update` — Admin.
pub async fn update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<TagProtectionRulePublic, AppError> {
    let req: TagProtectionUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.tagProtection.update input: {e}"),
        )
    })?;
    validate_pattern(&req.pattern)?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let row = ctx
        .db
        .update_tag_protection_rule(
            &accessible.row.id,
            &req.id,
            req.pattern.trim(),
            req.allow_create,
            req.allow_update,
            req.allow_delete,
            req.enforce_admins,
        )
        .await
        .map_err(db_err)?;
    Ok(row_public(&row))
}

/// `repo.tagProtection.delete` — Admin.
pub async fn delete(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let req: TagProtectionDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.tagProtection.delete input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    ctx.db
        .delete_tag_protection_rule(&accessible.row.id, &req.id)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}
