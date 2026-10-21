//! Org Actions secrets (`org.actions.secrets.*` — Phase 8): names list
//! metadata only, values are encrypted at rest and never returned.
//! Visibility gates the repo reach at claim time — 'all', 'private', or
//! 'selected' (the explicit repo allow-set).

use oxidean_core::{
    ActionSecretKeyPublic, AppError, OrgActionsSecretDeleteRequest, OrgActionsSecretPutRequest,
    OrgActionsSecretReposRequest, OrgActionsSecretReposResponse, OrgActionsSecretSetReposRequest,
    OrgActionsSecretsListRequest, OrgActionsSecretsListResponse, OrgSecretMetaPublic,
};
use uuid::Uuid;

use crate::actions::secrets::{encrypt_secret, validate_secret_name};
use crate::auth::gate::require_verified;
use crate::org::{db_err, load_org_by_slug, members::require_admin_plus};
use crate::rpc::RpcCtx;

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

/// `org.actions.secrets.list` — Admin+; names + visibility, never values.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgActionsSecretsListResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsSecretsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.secrets.list input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let rows = ctx
        .db
        .list_action_org_secrets(&org.id)
        .await
        .map_err(db_err)?;
    Ok(OrgActionsSecretsListResponse {
        secrets: rows
            .into_iter()
            .map(|r| OrgSecretMetaPublic {
                name: r.name,
                visibility: r.visibility,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect(),
    })
}

/// `org.actions.secrets.publicKey` — Admin+; the sealed-box receiver key for
/// org secrets (`crypto_box_seal` against the derived `org:{slug}` keypair).
pub async fn public_key(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionSecretKeyPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsSecretsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.secrets.publicKey input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    crate::crypto::seal_public_key(
        crate::actions::secrets::KEY_ENVS,
        &format!("org:{}", org.id),
    )
    .map(|(key_id, key)| ActionSecretKeyPublic { key_id, key })
    .map_err(|e| AppError::new("org.actions.secret_key_failed", e))
}

/// `org.actions.secrets.put` — Admin+. Creates or replaces a secret;
/// `repository_ids` replaces the selected-repo allow-set when given (only
/// meaningful with `visibility = 'selected'`).
pub async fn put(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsSecretPutRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.secrets.put input: {e}"),
        )
    })?;
    validate_secret_name(&req.secret_name)
        .map_err(|msg| AppError::new("org.actions.invalid_secret_name", msg))?;
    if req.value.is_empty() {
        return Err(AppError::new(
            "org.actions.invalid_secret_value",
            "secret value is required",
        ));
    }
    let visibility = validate_visibility(req.visibility.as_deref().unwrap_or("all"))?;
    if visibility != "selected" && req.repository_ids.as_ref().is_some_and(|v| !v.is_empty()) {
        return Err(AppError::new(
            "org.actions.invalid_visibility",
            "repository_ids only applies to visibility = 'selected'",
        ));
    }
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let ciphertext = encrypt_secret(&req.value)
        .map_err(|e| AppError::new("org.actions.secret_encrypt_failed", e))?;

    // Reuse the existing row's id so the selected-repo join keeps its FK.
    let existing = ctx
        .db
        .find_action_org_secret(&org.id, &req.secret_name)
        .await
        .map_err(db_err)?;
    let secret_id = existing
        .as_ref()
        .map(|r| r.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    ctx.db
        .upsert_action_org_secret(
            &secret_id,
            &org.id,
            &req.secret_name,
            &ciphertext,
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
            .set_action_org_secret_repos(&secret_id, ids)
            .await
            .map_err(db_err)?;
    }
    Ok(serde_json::json!({ "ok": true, "name": req.secret_name }))
}

/// `org.actions.secrets.delete` — Admin+.
pub async fn delete(ctx: &RpcCtx, input: serde_json::Value) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsSecretDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.secrets.delete input: {e}"),
        )
    })?;
    validate_secret_name(&req.secret_name)
        .map_err(|msg| AppError::new("org.actions.invalid_secret_name", msg))?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let ok = ctx
        .db
        .delete_action_org_secret(&org.id, &req.secret_name)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": ok }))
}

/// `org.actions.secrets.listRepos` — Admin+; repo ids in the selected
/// allow-set (empty for 'all'/'private' visibility).
pub async fn list_repos(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OrgActionsSecretReposResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsSecretReposRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.secrets.listRepos input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let secret = ctx
        .db
        .find_action_org_secret(&org.id, &req.secret_name)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.actions.secret_not_found", "secret not found"))?;
    let repository_ids = ctx
        .db
        .list_action_org_secret_repos(&secret.id)
        .await
        .map_err(db_err)?;
    Ok(OrgActionsSecretReposResponse { repository_ids })
}

/// `org.actions.secrets.setRepos` — Admin+; replaces the secret's
/// `visibility = 'selected'` allow set.
pub async fn set_repos(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let caller = require_verified(ctx).await?;
    let req: OrgActionsSecretSetReposRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid org.actions.secrets.setRepos input: {e}"),
        )
    })?;
    let org = load_org_by_slug(ctx, &req.slug).await?;
    let _ = require_admin_plus(ctx, &org, &caller).await?;
    let secret = ctx
        .db
        .find_action_org_secret(&org.id, &req.secret_name)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("org.actions.secret_not_found", "secret not found"))?;
    ctx.db
        .set_action_org_secret_repos(&secret.id, &req.repository_ids)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}
