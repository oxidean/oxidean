//! `oauthApp.*`/`oauth.*` RPC handlers — extracted from `core::oauth` (#162).
//! The OAuth HTTP endpoints (authorize/token/userinfo/device_code/metadata)
//! and token-authentication machinery stay in `oxidean-api-core` — routes
//! mount them and `actions`/`packages` authenticate through them.

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::auth::session::bytes_to_hex;
use oxidean_api_core::oauth::*;
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    format_oauth_scopes, parse_oauth_scopes, AppError, ClassicPatScope, CreateOAuthAppRequest,
    CreateOAuthAppResponse, OAuthAppIdRequest, OAuthAppPublic, OAuthAuthorizeInfo,
    OAuthAuthorizeInfoRequest, OAuthAuthorizeRequest, OAuthAuthorizeResponse,
    OAuthDeviceAuthorizeRequest, OAuthDeviceInfo, OAuthDeviceInfoRequest, OAuthGrantPublic,
    UpdateOAuthAppRequest, OAUTH_CLIENT_ID_PREFIX, OAUTH_CLIENT_SECRET_PREFIX, OAUTH_CODE_PREFIX,
    OAUTH_CODE_TTL_SECS,
};
use uuid::Uuid;

/// `oauthApp.create` — register an app; returns the one-time client_secret.
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CreateOAuthAppResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreateOAuthAppRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.create input: {e}"),
        )
    })?;
    let name = validate_name(&req.name)?;
    let uris = validate_redirect_uris(&req.redirect_uris)?;
    let uris_json = serde_json::to_string(&uris)
        .map_err(|e| AppError::new("oauth.internal", format!("uris serialize: {e}")))?;

    let (secret, secret_hash, secret_prefix) = mint_secret(OAUTH_CLIENT_SECRET_PREFIX);
    let client_id = {
        let mut bytes = [0u8; 16];
        rand::fill(&mut bytes);
        format!("{OAUTH_CLIENT_ID_PREFIX}{}", bytes_to_hex(&bytes))
    };
    let id = Uuid::new_v4().to_string();
    ctx.db
        .insert_oauth_app(
            &id,
            &user.id,
            &name,
            &client_id,
            &secret_hash,
            &secret_prefix,
            &uris_json,
        )
        .await
        .map_err(db_err)?;

    let row = ctx
        .db
        .find_oauth_app_by_id(&id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.internal", "created app missing"))?;
    Ok(CreateOAuthAppResponse {
        app: app_to_public(&row)?,
        client_secret: secret,
    })
}

/// `oauthApp.list` — apps owned by the signed-in user.
pub async fn list(ctx: &RpcCtx) -> Result<Vec<OAuthAppPublic>, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let rows = ctx
        .db
        .list_oauth_apps_for_owner(user_id)
        .await
        .map_err(db_err)?;
    rows.iter().map(app_to_public).collect()
}

/// `oauthApp.update` — rename and/or replace the redirect URI set.
pub async fn update(ctx: &RpcCtx, input: serde_json::Value) -> Result<OAuthAppPublic, AppError> {
    let req: UpdateOAuthAppRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.update input: {e}"),
        )
    })?;
    let app = owned_app(ctx, req.id.trim()).await?;
    let name = match req.name.as_deref() {
        Some(n) => validate_name(n)?,
        None => app.name.clone(),
    };
    let uris_json = match req.redirect_uris.as_ref() {
        Some(list) => serde_json::to_string(&validate_redirect_uris(list)?)
            .map_err(|e| AppError::new("oauth.internal", format!("uris serialize: {e}")))?,
        None => app.redirect_uris_json.clone(),
    };
    ctx.db
        .update_oauth_app(&app.id, &name, &uris_json, &now_rfc3339())
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .find_oauth_app_by_id(&app.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.internal", "updated app missing"))?;
    app_to_public(&row)
}

/// `oauthApp.delete` — removes the app; codes + tokens cascade-delete.
pub async fn delete_app(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let req: OAuthAppIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.delete input: {e}"),
        )
    })?;
    let app = owned_app(ctx, req.id.trim()).await?;
    ctx.db.delete_oauth_app(&app.id).await.map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}

/// `oauthApp.regenerateSecret` — rotate the client secret (one-time reveal).
pub async fn regenerate_secret(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CreateOAuthAppResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: OAuthAppIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.regenerateSecret input: {e}"),
        )
    })?;
    let app = owned_app(ctx, req.id.trim()).await?;
    let (secret, hash, prefix) = mint_secret(OAUTH_CLIENT_SECRET_PREFIX);
    ctx.db
        .update_oauth_app_secret(&app.id, &hash, &prefix, &now_rfc3339())
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .find_oauth_app_by_id(&app.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.internal", "updated app missing"))?;
    Ok(CreateOAuthAppResponse {
        app: app_to_public(&row)?,
        client_secret: secret,
    })
}

/// `oauthApp.listGrants` — applications with a live token for the signed-in
/// user (the "Authorized OAuth applications" list).
pub async fn list_grants(ctx: &RpcCtx) -> Result<Vec<OAuthGrantPublic>, AppError> {
    let user_id = require_session_user_id(ctx)?;
    let tokens = ctx
        .db
        .list_active_oauth_tokens_for_user(user_id)
        .await
        .map_err(db_err)?;
    let mut grants: Vec<OAuthGrantPublic> = Vec::new();
    for t in &tokens {
        let idx = grants
            .iter()
            .position(|g| g.application_id == t.application_id);
        match idx {
            Some(i) => {
                let g = &mut grants[i];
                for s in parse_oauth_scopes(&t.scopes).unwrap_or_default() {
                    let name = s.as_str().to_string();
                    if !g.scopes.contains(&name) {
                        g.scopes.push(name);
                    }
                }
                if t.created_at < g.granted_at {
                    g.granted_at = t.created_at.clone();
                }
                match (&g.last_used_at, &t.last_used_at) {
                    (Some(cur), Some(next)) if next > cur => g.last_used_at = Some(next.clone()),
                    (None, Some(next)) => g.last_used_at = Some(next.clone()),
                    _ => {}
                }
            }
            None => {
                let app = ctx
                    .db
                    .find_oauth_app_by_id(&t.application_id)
                    .await
                    .map_err(db_err)?;
                let Some(app) = app else { continue };
                grants.push(OAuthGrantPublic {
                    application_id: t.application_id.clone(),
                    app_name: app.name.clone(),
                    client_id: app.client_id.clone(),
                    scopes: parse_oauth_scopes(&t.scopes)
                        .unwrap_or_default()
                        .iter()
                        .map(|s| s.as_str().to_string())
                        .collect(),
                    granted_at: t.created_at.clone(),
                    last_used_at: t.last_used_at.clone(),
                });
            }
        }
    }
    Ok(grants)
}

/// `oauthApp.revoke` — revoke the signed-in user's grant to an application
/// (`id` = `oauth_applications.id`); every live token for that pair dies.
pub async fn revoke_grant(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let user_id = require_session_user_id(ctx)?.to_string();
    let req: OAuthAppIdRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.revoke input: {e}"),
        )
    })?;
    let id = req.id.trim();
    let app = ctx
        .db
        .find_oauth_app_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("oauth.app_not_found", "oauth application not found"))?;
    ctx.db
        .revoke_oauth_tokens_for_user_app(&user_id, &app.id, &now_rfc3339())
        .await
        .map_err(db_err)?;
    let actor_name = ctx
        .db
        .find_user_by_id(&user_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.username)
        .unwrap_or_default();
    oxidean_api_core::audit::record_with(
        &ctx.db,
        &ctx.client,
        Some((user_id.as_str(), actor_name.as_str())),
        "oauth.grant_revoked",
        Some(("oauth_application", &app.id)),
        None,
    )
    .await;
    Ok(serde_json::json!({ "ok": true }))
}

/// `oauthApp.authorizeInfo` — consent screen payload (session required).
pub async fn authorize_info(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OAuthAuthorizeInfo, AppError> {
    require_session_user_id(ctx)?;
    let req: OAuthAuthorizeInfoRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.authorizeInfo input: {e}"),
        )
    })?;
    let (app, uri, scopes) = resolve_authorize_request(
        &ctx.db,
        req.client_id.trim(),
        req.redirect_uri.as_deref(),
        req.scope.as_deref(),
    )
    .await?;
    let owner_username = ctx
        .db
        .find_user_by_id(&app.owner_id)
        .await
        .map_err(db_err)?
        .map(|u| u.username)
        .unwrap_or_default();
    Ok(OAuthAuthorizeInfo {
        app_name: app.name.clone(),
        client_id: app.client_id.clone(),
        redirect_uri: uri,
        scopes: scopes.iter().map(|s| s.as_str().to_string()).collect(),
        scope_details: oxidean_core::scope_details(&scopes),
        owner_username,
    })
}

/// `oauthApp.authorize` — the consent decision. `approve` mints a single-use
/// code (≤ [`OAUTH_CODE_TTL_SECS`]); `deny` returns an `access_denied` redirect.
pub async fn authorize_rpc(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OAuthAuthorizeResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: OAuthAuthorizeRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauthApp.authorize input: {e}"),
        )
    })?;
    let state = req
        .state
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(s) = state {
        if s.len() > MAX_STATE_LEN {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("state must be at most {MAX_STATE_LEN} characters"),
            ));
        }
    }
    let (app, uri, requested) = resolve_authorize_request(
        &ctx.db,
        req.client_id.trim(),
        Some(req.redirect_uri.as_str()),
        req.scope.as_deref(),
    )
    .await?;

    if !req.approve {
        let mut pairs = vec![("error", "access_denied")];
        if let Some(s) = state {
            pairs.push(("state", s));
        }
        return Ok(OAuthAuthorizeResponse {
            redirect_to: build_redirect(&uri, &pairs),
        });
    }

    // Consent-screen narrowing: the user may grant a subset of the requested
    // set (e.g. an MCP grant without `delete_repo`). Granted ⊆ requested —
    // never a superset — and non-empty.
    let scopes = match req
        .granted_scope
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => {
            let granted =
                parse_oauth_scopes(raw).map_err(|e| AppError::new("oauth.invalid_scope", e))?;
            if let Some(extra) = granted.iter().find(|g| !requested.contains(g)) {
                return Err(AppError::new(
                    "oauth.invalid_scope",
                    format!(
                        "granted scope \"{}\" was not requested by the application",
                        extra.as_str()
                    ),
                ));
            }
            granted
        }
        None => {
            if req.granted_scope.is_some() {
                return Err(AppError::new(
                    "rpc.bad_input",
                    "select at least one scope to grant",
                ));
            }
            requested
        }
    };

    let code_id = Uuid::new_v4().to_string();
    let (code, code_hash, _prefix) = mint_secret(OAUTH_CODE_PREFIX);
    ctx.db
        .insert_oauth_code(
            &code_id,
            &code_hash,
            &app.id,
            &user.id,
            &uri,
            &format_oauth_scopes(&scopes),
            &rfc3339_in(OAUTH_CODE_TTL_SECS),
        )
        .await
        .map_err(db_err)?;

    oxidean_api_core::audit::record_with(
        &ctx.db,
        &ctx.client,
        Some((&user.id, &user.username)),
        "oauth.grant_approved",
        Some(("oauth_application", &app.id)),
        None,
    )
    .await;

    let mut pairs = vec![("code", code.as_str())];
    if let Some(s) = state {
        pairs.push(("state", s));
    }
    Ok(OAuthAuthorizeResponse {
        redirect_to: build_redirect(&uri, &pairs),
    })
}

/// `oauth.deviceInfo` — approve page lookup: pending code → scopes + expiry.
/// Session required (any authenticated user may *look* at a pending code —
/// approving is the security decision).
pub async fn device_info(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<OAuthDeviceInfo, AppError> {
    require_session_user_id(ctx)?;
    let req: OAuthDeviceInfoRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauth.deviceInfo input: {e}"),
        )
    })?;
    let normalized = normalize_user_code(&req.user_code);
    let row = ctx
        .db
        .find_oauth_device_code_by_user_code(&normalized)
        .await
        .map_err(db_err)?
        .filter(|r| r.status == "pending" && !rfc3339_expired(&r.expires_at))
        .ok_or_else(|| AppError::new("oauth.device_code_not_found", "no pending device code"))?;
    let parsed = parse_oauth_scopes(&row.scopes).unwrap_or_default();
    Ok(OAuthDeviceInfo {
        user_code: display_user_code(&row.user_code),
        scopes: row
            .scopes
            .split_whitespace()
            .map(|s| s.to_string())
            .collect(),
        scope_details: oxidean_core::scope_details(&parsed),
        expires_at: row.expires_at.clone(),
    })
}

/// `oauth.deviceAuthorize` — the approve/deny decision (verified session).
pub async fn device_authorize(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let user = require_verified(ctx).await?;
    let req: OAuthDeviceAuthorizeRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid oauth.deviceAuthorize input: {e}"),
        )
    })?;
    let normalized = normalize_user_code(&req.user_code);
    let row = ctx
        .db
        .find_oauth_device_code_by_user_code(&normalized)
        .await
        .map_err(db_err)?
        .filter(|r| !rfc3339_expired(&r.expires_at))
        .ok_or_else(|| AppError::new("oauth.device_code_not_found", "no pending device code"))?;

    // Approval-page narrowing: granted ⊆ the grant's requested set, non-empty.
    let granted_scopes = if req.approve {
        match req
            .granted_scope
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(raw) => {
                let requested: Vec<ClassicPatScope> = row
                    .scopes
                    .split_whitespace()
                    .filter_map(|s| ClassicPatScope::parse(s).ok())
                    .collect();
                let granted =
                    parse_oauth_scopes(raw).map_err(|e| AppError::new("oauth.invalid_scope", e))?;
                if let Some(extra) = granted.iter().find(|g| !requested.contains(g)) {
                    return Err(AppError::new(
                        "oauth.invalid_scope",
                        format!(
                            "granted scope \"{}\" was not requested by the application",
                            extra.as_str()
                        ),
                    ));
                }
                Some(format_oauth_scopes(&granted))
            }
            None => {
                if req.granted_scope.is_some() {
                    return Err(AppError::new(
                        "rpc.bad_input",
                        "select at least one scope to grant",
                    ));
                }
                None
            }
        }
    } else {
        None
    };

    let ok = if req.approve {
        ctx.db
            .approve_oauth_device_code(&row.id, &user.id, &now_rfc3339(), granted_scopes.as_deref())
            .await
            .map_err(db_err)?
    } else {
        ctx.db
            .deny_oauth_device_code(&row.id)
            .await
            .map_err(db_err)?
    };
    if !ok {
        return Err(AppError::new(
            "oauth.device_code_not_found",
            "device code is no longer pending",
        ));
    }

    oxidean_api_core::audit::record_with(
        &ctx.db,
        &ctx.client,
        Some((&user.id, &user.username)),
        if req.approve {
            "oauth.device_approved"
        } else {
            "oauth.device_denied"
        },
        None,
        None,
    )
    .await;
    Ok(serde_json::json!({ "ok": true }))
}
