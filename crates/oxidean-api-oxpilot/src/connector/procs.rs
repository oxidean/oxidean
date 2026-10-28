//! `oxpilot.{pairing_create,target_*,grant_*}` — the U6 owner surface
//! (C1) over the `ConnectorGateway` seam. `SessionOnly` at the proc
//! gate, `require_enabled` first inside every handler (BR2.3); the
//! gateway enforces owner scoping — absent and foreign ids resolve to
//! `not_found` identically (BR7.1). Audits emit inside the gateway
//! with the proc's real `ClientMeta`.

use std::sync::Arc;

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::oxpilot::{require_enabled, ConnectorGateway};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};

fn gateway(ctx: &RpcCtx) -> Result<Arc<dyn ConnectorGateway>, AppError> {
    ctx.oxpilot
        .clone()
        .ok_or_else(|| AppError::new("oxpilot.internal", "oxpilot runtime unavailable"))?
        .gateway()
}

fn bad_input(msg: impl Into<String>) -> AppError {
    AppError::new("rpc.bad_input", msg.into())
}

fn parse<T: for<'a> Deserialize<'a>>(input: Value, proc: &str) -> Result<T, AppError> {
    serde_json::from_value(input).map_err(|e| bad_input(format!("invalid {proc} input: {e}")))
}

#[derive(Debug, Deserialize)]
struct PairingCreateReq {
    target_name: String,
    caps: Vec<String>,
}

/// `oxpilot.pairing_create{target_name, caps}` → `{pairing_id,
/// pairing_code, expires_at}` — BR1.1's shown-once mint, per-user
/// mint limiter inside the seam.
pub async fn pairing_create(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: PairingCreateReq = parse(input, "oxpilot.pairing_create")?;
    let view = gateway(ctx)?
        .mint_pairing(&ctx.client, &user.id, &req.target_name, &req.caps)
        .await?;
    serde_json::to_value(&view).map_err(|e| bad_input(format!("pairing view: {e}")))
}

/// `oxpilot.target_list{}` → `{targets: [TargetView]}` — BR3.2's
/// owner read re-mints pending connect grants on `pending_consent`
/// rows before projecting.
pub async fn target_list(ctx: &RpcCtx) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let views = gateway(ctx)?.list_targets(&ctx.client, &user.id).await?;
    Ok(json!({ "targets": views }))
}

#[derive(Debug, Deserialize)]
struct TargetRef {
    target_id: String,
}

/// `oxpilot.target_get{target_id}` → `TargetView`.
pub async fn target_get(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: TargetRef = parse(input, "oxpilot.target_get")?;
    let view = gateway(ctx)?
        .get_target(&ctx.client, &user.id, &req.target_id)
        .await?;
    serde_json::to_value(&view).map_err(|e| bad_input(format!("target view: {e}")))
}

/// `oxpilot.target_disconnect{target_id}` → `{ok}` — BR6.1 tombstone
/// + direct cancel; idempotent on terminal targets (BR6.7).
pub async fn target_disconnect(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: TargetRef = parse(input, "oxpilot.target_disconnect")?;
    gateway(ctx)?
        .disconnect_target(&ctx.client, &user.id, &req.target_id)
        .await?;
    Ok(json!({ "ok": true }))
}

/// `oxpilot.target_revoke{target_id}` → `{ok}` — BR6.2 verifier
/// preserved, grants revoked, claimed work `cancel_requested`.
pub async fn target_revoke(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: TargetRef = parse(input, "oxpilot.target_revoke")?;
    gateway(ctx)?
        .revoke_target(&ctx.client, &user.id, &req.target_id)
        .await?;
    Ok(json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
struct GrantListReq {
    #[serde(default)]
    target_id: Option<String>,
}

/// `oxpilot.grant_list{target_id?}` → `{grants: [GrantView]}` — lazy
/// pending-grant expiry on observation (BR3.1).
pub async fn grant_list(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: GrantListReq = parse(input, "oxpilot.grant_list")?;
    let views = gateway(ctx)?
        .list_grants(&ctx.client, &user.id, req.target_id.as_deref())
        .await?;
    Ok(json!({ "grants": views }))
}

#[derive(Debug, Deserialize)]
struct GrantRef {
    grant_id: String,
}

/// `oxpilot.grant_get{grant_id}` → `GrantView`.
pub async fn grant_get(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: GrantRef = parse(input, "oxpilot.grant_get")?;
    let view = gateway(ctx)?
        .get_grant(&ctx.client, &user.id, &req.grant_id)
        .await?;
    serde_json::to_value(&view).map_err(|e| bad_input(format!("grant view: {e}")))
}

#[derive(Debug, Deserialize)]
struct GrantRespondReq {
    grant_id: String,
    allow: bool,
    #[serde(default)]
    scope: Option<String>,
}

/// `oxpilot.grant_respond{grant_id, allow, scope?}` → `{state}` —
/// BR3.4/BR3.5 respond + BR6.4's allowed-session-grant revoke arm.
pub async fn grant_respond(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: GrantRespondReq = parse(input, "oxpilot.grant_respond")?;
    let view = gateway(ctx)?
        .respond_grant(
            &ctx.client,
            &user.id,
            &req.grant_id,
            req.allow,
            req.scope.as_deref(),
        )
        .await?;
    Ok(json!({ "state": view.state }))
}
