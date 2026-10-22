//! Org Actions variables (`org.actions.variables.*`) — plaintext siblings of
//! the org secrets endpoints: values ARE returned on list (variables are
//! config, not secrets — GitHub exposes them). `visibility` gates the repo
//! reach at claim time — 'all', 'private', or 'selected' (explicit allow-set).

use oxidean_core::{
    AppError, OrgActionsVariableDeleteRequest, OrgActionsVariablePutRequest,
    OrgActionsVariableReposRequest, OrgActionsVariableReposResponse,
    OrgActionsVariableSetReposRequest, OrgActionsVariablesListRequest,
    OrgActionsVariablesListResponse, OrgVariablePublic,
};
use uuid::Uuid;

use crate::members::require_admin_plus;
use oxidean_api_core::actions::vars::{validate_variable_name, validate_variable_value};
use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::org::{db_err, load_org_by_slug};
use oxidean_api_core::rpc::RpcCtx;

/// `visibility` whitelist — anything else would silently behave like 'all'
/// (the claim query treats unknown values as unreachable).
fn validate_visibility(v: &str) -> Result<&'static str, AppError> {
    match v {
        "all" => Ok("all"),
        "private" => Ok("private"),
        "selected" => Ok("selected"),
        _ => Err(AppError::new(
            "org.actions.invalid_visibility",
            "visibility must be 'all', 'private', or 'selected'",
        )),
    }
}

/// `org.actions.variables.list` — Admin+; values included (plaintext).
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgActionsVariablesListResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsVariablesListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.variables.list input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let rows = ctx
        .db
        .list_action_org_variables(&org.id)
        .await
        .map_err(db_err)?;
    Ok(OrgActionsVariablesListResponse {
        variables: rows
            .into_iter()
            .map(|r| OrgVariablePublic {
                name: r.name,
                value: r.value,
                visibility: r.visibility,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect(),
    })
}

/// `org.actions.variables.put` — Admin+. Creates or replaces a variable;
/// `repository_ids` replaces the selected-repo allow-set when given (only
/// meaningful with `visibility = 'selected'`).
pub async fn put(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsVariablePutRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.variables.put input: {e}"),
        )
    })?;
    validate_variable_name(&req.variable_name)
        .map_err(|msg| AppError::new("org.actions.invalid_variable_name", msg))?;
    validate_variable_value(&req.value)
        .map_err(|msg| AppError::new("org.actions.invalid_variable_value", msg))?;
    let visibility = validate_visibility(req.visibility.as_deref().unwrap_or("all"))?;
    if visibility != "selected" && req.repository_ids.as_ref().is_some_and(|v| !v.is_empty()) {
        return Err(AppError::new(
            "org.actions.invalid_visibility",
            "repository_ids only applies to visibility = 'selected'",
        ));
    }
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;

    // Reuse the existing row's id so the selected-repo join keeps its FK.
    let existing = ctx
        .db
        .find_action_org_variable(&org.id, &req.variable_name)
        .await
        .map_err(db_err)?;
    let variable_id = existing
        .as_ref()
        .map(|r| r.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    ctx.db
        .upsert_action_org_variable(
            &variable_id,
            &org.id,
            &req.variable_name,
            &req.value,
            visibility,
        )
        .await
        .map_err(db_err)?;
    if let Some(ids) = req.repository_ids.as_ref() {
        // Allow-set repos must belong to this org — no cross-org grafting.
        for repo_id in ids {
            let owned = ctx
                .db
                .find_repository_by_id(repo_id)
                .await
                .map_err(db_err)?
                .is_some_and(|r| r.owner_id == org.id);
            if !owned {
                return Err(AppError::new(
                    "org.actions.repo_not_in_org",
                    "repository_ids must belong to this organization",
                ));
            }
        }
        ctx.db
            .set_action_org_variable_repos(&variable_id, ids)
            .await
            .map_err(db_err)?;
    }
    Ok(serde_json::json!({ "ok": true, "name": req.variable_name }))
}

/// `org.actions.variables.delete` — Admin+.
pub async fn delete(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsVariableDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.variables.delete input: {e}"),
        )
    })?;
    validate_variable_name(&req.variable_name)
        .map_err(|msg| AppError::new("org.actions.invalid_variable_name", msg))?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let ok = ctx
        .db
        .delete_action_org_variable(&org.id, &req.variable_name)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": ok }))
}

/// `org.actions.variables.listRepos` — Admin+; repo ids in the selected
/// allow-set (empty for 'all'/'private' visibility).
pub async fn list_repos(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgActionsVariableReposResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsVariableReposRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.variables.listRepos input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let variable = ctx
        .db
        .find_action_org_variable(&org.id, &req.variable_name)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.actions.variable_not_found", "variable not found"))?;
    let repository_ids = ctx
        .db
        .list_action_org_variable_repos(&variable.id)
        .await
        .map_err(db_err)?;
    Ok(OrgActionsVariableReposResponse { repository_ids })
}

/// `org.actions.variables.setRepos` — Admin+; replaces the variable's
/// `visibility = 'selected'` allow set.
pub async fn set_repos(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsVariableSetReposRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.variables.setRepos input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let variable = ctx
        .db
        .find_action_org_variable(&org.id, &req.variable_name)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.actions.variable_not_found", "variable not found"))?;
    ctx.db
        .set_action_org_variable_repos(&variable.id, &req.repository_ids)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}
