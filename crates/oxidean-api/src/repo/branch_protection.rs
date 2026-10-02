//! `repo.branchProtection.*` Admin CRUD (ORG-05 / D-03 / D-26).

use oxidean_core::{
    AppError, BranchProtectionDeleteRequest, BranchProtectionListResponse,
    BranchProtectionRuleInput, BranchProtectionRulePublic, BranchProtectionUpdateRequest,
    RepoGetRequest,
};
use oxidean_db::BranchProtectionRuleRow;
use uuid::Uuid;

use crate::repo::collaborators::resolve_repo_for_admin;
use crate::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e == "branch protection rule not found" {
        AppError::new("repo.branch_protection_not_found", "Branch protection rule not found")
    } else {
        tracing::error!(error = %e, "branch protection db error");
        AppError::new("repo.internal", "repository operation failed")
    }
}

fn validate_pattern(pattern: &str) -> Result<(), AppError> {
    let p = pattern.trim();
    if p.is_empty() || p.len() > 255 {
        return Err(AppError::new(
            "rpc.bad_input",
            "branch pattern must be 1–255 characters",
        ));
    }
    if p.contains('\0') || p.contains("..") {
        return Err(AppError::new("rpc.bad_input", "invalid branch pattern"));
    }
    Ok(())
}

fn validate_review_count(require: bool, count: i32) -> Result<i32, AppError> {
    if !require {
        return Ok(count.clamp(1, 6));
    }
    if !(1..=6).contains(&count) {
        return Err(AppError::new(
            "rpc.bad_input",
            "required_approving_review_count must be 1..6",
        ));
    }
    Ok(count)
}

fn contexts_json(contexts: &[String]) -> Result<String, AppError> {
    for c in contexts {
        let t = c.trim();
        if t.is_empty() || t.len() > 255 {
            return Err(AppError::new(
                "rpc.bad_input",
                "status context must be 1–255 characters",
            ));
        }
    }
    serde_json::to_string(contexts).map_err(|_| AppError::new("repo.internal", "contexts encode"))
}

fn row_public(row: &BranchProtectionRuleRow) -> BranchProtectionRulePublic {
    let contexts: Vec<String> =
        serde_json::from_str(&row.required_status_contexts).unwrap_or_default();
    BranchProtectionRulePublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        pattern: row.pattern.clone(),
        require_reviews: row.require_reviews,
        required_approving_review_count: row.required_approving_review_count,
        dismiss_stale_reviews: row.dismiss_stale_reviews,
        require_conversation_resolution: row.require_conversation_resolution,
        require_last_push_approval: row.require_last_push_approval,
        required_status_contexts: contexts,
        strict_status_checks: row.strict_status_checks,
        allow_force_pushes: row.allow_force_pushes,
        allow_deletions: row.allow_deletions,
        enforce_admins: row.enforce_admins,
        required_linear_history: row.required_linear_history,
        lock_branch: row.lock_branch,
        require_signed_commits: row.require_signed_commits,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// `repo.branchProtection.list` — Admin.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<BranchProtectionListResponse, AppError> {
    let req: RepoGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.branchProtection.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let rows = ctx
        .db
        .list_branch_protection_rules(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(BranchProtectionListResponse {
        rules: rows.iter().map(row_public).collect(),
    })
}

/// `repo.branchProtection.create` — Admin.
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<BranchProtectionRulePublic, AppError> {
    let req: BranchProtectionRuleInput = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.branchProtection.create input: {e}"),
        )
    })?;
    validate_pattern(&req.pattern)?;
    let count = validate_review_count(req.require_reviews, req.required_approving_review_count)?;
    let contexts = contexts_json(&req.required_status_contexts)?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_branch_protection_rule(
            &id,
            &accessible.row.id,
            req.pattern.trim(),
            req.require_reviews,
            count,
            req.dismiss_stale_reviews,
            req.require_conversation_resolution,
            req.require_last_push_approval,
            &contexts,
            req.strict_status_checks,
            req.allow_force_pushes,
            req.allow_deletions,
            req.enforce_admins,
            req.required_linear_history,
            req.lock_branch,
            req.require_signed_commits,
        )
        .await
        .map_err(db_err)?;
    Ok(row_public(&row))
}

/// `repo.branchProtection.update` — Admin.
pub async fn update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<BranchProtectionRulePublic, AppError> {
    let req: BranchProtectionUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.branchProtection.update input: {e}"),
        )
    })?;
    validate_pattern(&req.pattern)?;
    let count = validate_review_count(req.require_reviews, req.required_approving_review_count)?;
    let contexts = contexts_json(&req.required_status_contexts)?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let row = ctx
        .db
        .update_branch_protection_rule(
            &accessible.row.id,
            &req.id,
            req.pattern.trim(),
            req.require_reviews,
            count,
            req.dismiss_stale_reviews,
            req.require_conversation_resolution,
            req.require_last_push_approval,
            &contexts,
            req.strict_status_checks,
            req.allow_force_pushes,
            req.allow_deletions,
            req.enforce_admins,
            req.required_linear_history,
            req.lock_branch,
            req.require_signed_commits,
        )
        .await
        .map_err(db_err)?;
    Ok(row_public(&row))
}

/// `repo.branchProtection.delete` — Admin.
pub async fn delete(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let req: BranchProtectionDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.branchProtection.delete input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    ctx.db
        .delete_branch_protection_rule(&accessible.row.id, &req.id)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}
