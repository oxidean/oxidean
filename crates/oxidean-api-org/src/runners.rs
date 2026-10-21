//! Org-scoped self-hosted runners (`org.actions.runners.*`): org admins
//! register runners that may claim jobs from every repository owned by the
//! organization — GitHub's `/orgs/{org}/actions/runners` surface. Ownership
//! rides the existing `action_runners.owner_type='org'` + `owner_id` columns.

use oxidean_core::{
    ActionListRunnersResponse, ActionRegistrationTokenResponse, ActionRunnerDeleteResponse,
    ActionRunnerGetResponse, AppError, OrgActionsRunnerRequest, OrgActionsRunnersRequest,
};

use crate::members::require_admin_plus;
use oxidean_api_core::actions::runner_public;
use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::org::{db_err, load_org_by_slug};
use oxidean_api_core::rpc::RpcCtx;

fn not_found() -> AppError {
    AppError::new("rpc.not_found", "not found")
}

/// `org.actions.runners.list` — Admin+; runners bound to the org.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionListRunnersResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsRunnersRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.runners.list input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let rows = ctx
        .db
        .list_action_runners_for_org(&org.id)
        .await
        .map_err(db_err)?;
    Ok(ActionListRunnersResponse {
        runners: rows.iter().map(runner_public).collect(),
    })
}

/// `org.actions.runners.get` — Admin+; one org-bound runner.
pub async fn get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRunnerGetResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsRunnerRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.runners.get input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let runner = ctx
        .db
        .list_action_runners_for_org(&org.id)
        .await
        .map_err(db_err)?
        .into_iter()
        .find(|r| r.id == req.runner_id)
        .ok_or_else(not_found)?;
    Ok(ActionRunnerGetResponse {
        runner: runner_public(&runner),
    })
}

/// `org.actions.runners.delete` — Admin+; only org-bound runners — repo-bound
/// and instance-shared runners keep their own delete surfaces.
pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRunnerDeleteResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsRunnerRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.runners.delete input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let runner = ctx
        .db
        .find_action_runner_by_id(&req.runner_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    if runner.owner_type.as_deref() != Some("org")
        || runner.owner_id.as_deref() != Some(org.id.as_str())
    {
        return Err(not_found());
    }
    let deleted = ctx
        .db
        .delete_action_runner(&req.runner_id)
        .await
        .map_err(db_err)?;
    Ok(ActionRunnerDeleteResponse { deleted })
}

/// `org.actions.runners.createRegistrationToken` — Admin+; one-shot
/// org-scoped token. The runner it registers binds `owner_type='org'` and
/// claims jobs from repositories owned by this org only.
pub async fn create_registration_token(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRegistrationTokenResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsRunnersRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.runners.createRegistrationToken input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let token = oxidean_api_core::actions::tokens::mint_registration_token_for_scope(
        &ctx.db,
        "org",
        Some(&org.id),
    )
    .await
    .map_err(|e| AppError::new("org.actions.token_failed", e))?;
    Ok(ActionRegistrationTokenResponse {
        token,
        scope: "org".into(),
    })
}

/// `org.actions.runners.createRemoveToken` — Admin+; one-shot remove token.
/// `DELETE /orgs/{org}/actions/runners/{id}` accepts it as Bearer auth so a
/// runner can self-remove without user credentials.
pub async fn create_remove_token(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRegistrationTokenResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsRunnersRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.runners.createRemoveToken input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let token = oxidean_api_core::actions::tokens::mint_remove_token(&ctx.db, "org", Some(&org.id))
        .await
        .map_err(|e| AppError::new("org.actions.token_failed", e))?;
    Ok(ActionRegistrationTokenResponse {
        token,
        scope: "org".into(),
    })
}
