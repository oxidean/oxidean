//! `/api/oxpilot/connector/*` — the C3 connector HTTP surface (U6).
//!
//! Verifier-class Bearer auth ONLY: no session cookies, no PATs
//! (BR2.3). `/pair` is the unauthenticated carve-out — the pairing
//! code IS the credential (BR1.5). Every route gates enablement FIRST
//! (fail-closed `{error:"disabled"}`, BR2.5), resolves the
//! `ConnectorIdentity` as the SOLE scoping source (BR2.4 — a body's
//! `target_id` must equal it, never trust it), and feeds the
//! failed-auth IP limiter through `resolve_verifier` (NFR1.3). Audits
//! ride `audit::record_with` inside the gateway — this surface has no
//! `RpcCtx`.
//!
//! Body/response shapes are pinned by C3:
//!   POST /pair       {pairing_code, device_label, requested_caps}
//!                    → 200 {connector_id, target_id, verifier}
//!                    → 4xx {error: pairing_expired|pairing_used|forbidden}
//!   POST /claim      {target_id, wait_seconds, capabilities}
//!                    → 200 {actions:[{action_id,kind,args,grant_id}],
//!                           cancel:\[action_id\], retry_after}
//!                    → 204 + Retry-After (long-poll boundary)
//!   POST /report     {action_id, status, exit_code, output}
//!                    → 200 {ack:true} | 404 {error:"not_found"}
//!   POST /heartbeat  {target_id} → 200 {revoked, cancel:\[action_id\]}
#![allow(clippy::result_large_err)]

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use oxidean_api_core::app::AppState;
use oxidean_api_core::oxpilot::{
    resolve_enablement_slot, ClaimOutcome, ConnectorGateway, ConnectorIdentity,
};
use oxidean_api_core::rpc::ClientMeta;
use oxidean_core::AppError;
use serde::Deserialize;
use serde_json::json;

/// Field bounds — ids are UUIDs; nothing on this surface legitimately
/// carries longer scalars.
const FIELD_MAX: usize = 256;
/// Report `output` ingest bound — the store form is the ≤2 KiB tail;
/// anything past this can never matter and just costs parsing.
const OUTPUT_MAX: usize = 256 * 1024;
/// Requested/claimed capability list bound — the connector enum is
/// three members wide; a longer list is malformed input.
const CAPS_LIST_MAX: usize = 16;

fn err(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

/// AppError → the C3 `{error:code}` wire shape. The pinned vocabulary:
/// `unauthorized` (401), `rate_limited` (429 + Retry-After), `forbidden`
/// / `revoked` / `disabled` (403), `not_found` (404), `bad_input` (400),
/// `pairing_expired` (410), `pairing_used` (409), `internal` (500).
fn gateway_err(e: &AppError) -> Response {
    let (status, code) = match e.code.as_str() {
        "unauthorized" => (StatusCode::UNAUTHORIZED, "unauthorized"),
        "auth.rate_limited" => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
        "forbidden" => (StatusCode::FORBIDDEN, "forbidden"),
        "revoked" => (StatusCode::FORBIDDEN, "revoked"),
        "not_found" => (StatusCode::NOT_FOUND, "not_found"),
        "pairing_expired" => (StatusCode::GONE, "pairing_expired"),
        "pairing_used" => (StatusCode::CONFLICT, "pairing_used"),
        "bad_input" => (StatusCode::BAD_REQUEST, "bad_input"),
        // A missing store on an enabled deployment is fail-closed —
        // the connector-facing answer is `disabled`, not a 500 the
        // client would misread as transient.
        "db.not_configured" | "oxpilot.disabled" => (StatusCode::FORBIDDEN, "disabled"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    let mut resp = err(status, code);
    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry = e
            .data
            .as_ref()
            .and_then(|d| d.get("retry_after"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        if let Ok(v) = HeaderValue::from_str(&retry.max(1).to_string()) {
            resp.headers_mut().insert(header::RETRY_AFTER, v);
        }
    }
    resp
}

/// BR2.5 — enablement BEFORE authentication on every connector route.
async fn require_enabled_gate(state: &AppState) -> Result<(), Response> {
    let st = resolve_enablement_slot(&*state.oxpilot.slot.read().await);
    if st.enabled {
        Ok(())
    } else {
        Err(err(StatusCode::FORBIDDEN, "disabled"))
    }
}

fn gateway(state: &AppState) -> Result<Arc<dyn ConnectorGateway>, Response> {
    state
        .oxpilot
        .gateway()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "internal"))
}

/// `Authorization: Bearer <verifier>` — the only credential class on
/// this subtree.
fn bearer(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = raw.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty())
        .then(|| token.trim().to_string())
}

/// Enablement → Bearer → `ConnectorIdentity` — the SOLE scoping source
/// on this subtree (BR2.4). Absent Bearer material resolves through the
/// same unknown-verifier path: the failed-auth limiter + the
/// `connector_auth_failed` audit apply identically (BR2.1).
async fn connector_identity(
    state: &AppState,
    meta: &ClientMeta,
    headers: &HeaderMap,
) -> Result<ConnectorIdentity, Response> {
    require_enabled_gate(state).await?;
    let gw = gateway(state)?;
    let token = bearer(headers).unwrap_or_default();
    gw.resolve_verifier(meta, &token)
        .await
        .map_err(|e| gateway_err(&e))
}

/// BR2.4 — a body's `target_id` must equal the resolved identity's
/// target; a mismatch is `forbidden` (the identity IS the target).
fn bind_target(identity: &ConnectorIdentity, target_id: &str) -> Result<(), Response> {
    if target_id != identity.target_id {
        return Err(err(StatusCode::FORBIDDEN, "forbidden"));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct PairBody {
    pairing_code: String,
    #[serde(default)]
    device_label: String,
    #[serde(default)]
    requested_caps: Vec<String>,
}

/// `POST /pair` — BR1.5's no-auth carve-out: the pairing code IS the
/// credential. No session, no PAT, no route-level limiter (the code's
/// entropy + TTL is the bound).
pub async fn pair(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let meta = ClientMeta::from_headers(&headers);
    if let Err(r) = require_enabled_gate(&state).await {
        return r;
    }
    let gw = match gateway(&state) {
        Ok(g) => g,
        Err(r) => return r,
    };
    let body: PairBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(_) => return err(StatusCode::BAD_REQUEST, "bad_input"),
    };
    if body.pairing_code.len() > FIELD_MAX || body.requested_caps.len() > CAPS_LIST_MAX {
        return err(StatusCode::BAD_REQUEST, "bad_input");
    }
    match gw
        .redeem_pairing(
            &meta,
            &body.pairing_code,
            &body.device_label,
            &body.requested_caps,
        )
        .await
    {
        // The ONE response that ever carries plaintext verifier
        // material (BR1.2/BR7.3) — shown once, stored hashed.
        Ok(r) => (
            StatusCode::OK,
            Json(json!({
                "connector_id": r.connector_id,
                "target_id": r.target_id,
                "verifier": r.verifier,
            })),
        )
            .into_response(),
        Err(e) => gateway_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct ClaimBody {
    target_id: String,
    #[serde(default)]
    wait_seconds: u64,
    #[serde(default)]
    capabilities: Vec<String>,
}

/// `POST /claim` — the long-poll dequeue (BR5.1).
pub async fn claim(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let meta = ClientMeta::from_headers(&headers);
    let identity = match connector_identity(&state, &meta, &headers).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let gw = match gateway(&state) {
        Ok(g) => g,
        Err(r) => return r,
    };
    let body: ClaimBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(_) => return err(StatusCode::BAD_REQUEST, "bad_input"),
    };
    if body.target_id.len() > FIELD_MAX || body.capabilities.len() > CAPS_LIST_MAX {
        return err(StatusCode::BAD_REQUEST, "bad_input");
    }
    if let Err(r) = bind_target(&identity, &body.target_id) {
        return r;
    }
    match gw
        .claim(&meta, &identity, body.wait_seconds, &body.capabilities)
        .await
    {
        Ok(ClaimOutcome::Batch {
            actions,
            cancel,
            retry_after,
        }) => (
            StatusCode::OK,
            Json(json!({
                "actions": actions,
                "cancel": cancel,
                "retry_after": retry_after,
            })),
        )
            .into_response(),
        Ok(ClaimOutcome::Timeout { retry_after }) => {
            let mut resp = StatusCode::NO_CONTENT.into_response();
            if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
                resp.headers_mut().insert(header::RETRY_AFTER, v);
            }
            resp
        }
        Err(e) => gateway_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct ReportBody {
    action_id: String,
    status: String,
    #[serde(default)]
    exit_code: Option<i64>,
    #[serde(default)]
    output: String,
}

/// `POST /report` — the terminal close (BR5.2). No `target_id` field:
/// the action's row proves the binding (cross-target ⇒ `not_found`,
/// kill-local).
pub async fn report(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let meta = ClientMeta::from_headers(&headers);
    let identity = match connector_identity(&state, &meta, &headers).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let gw = match gateway(&state) {
        Ok(g) => g,
        Err(r) => return r,
    };
    let body: ReportBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(_) => return err(StatusCode::BAD_REQUEST, "bad_input"),
    };
    if body.action_id.len() > FIELD_MAX
        || body.status.len() > FIELD_MAX
        || body.output.len() > OUTPUT_MAX
    {
        return err(StatusCode::BAD_REQUEST, "bad_input");
    }
    match gw
        .report(
            &meta,
            &identity,
            &body.action_id,
            &body.status,
            body.exit_code,
            &body.output,
        )
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "ack": true }))).into_response(),
        Err(e) => gateway_err(&e),
    }
}

#[derive(Debug, Deserialize)]
struct HeartbeatBody {
    target_id: String,
}

/// `POST /heartbeat` — liveness + cancel-set re-announce (BR5.3). The
/// only route that still answers on `revoked` targets (`revoked:true`).
pub async fn heartbeat(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let meta = ClientMeta::from_headers(&headers);
    let identity = match connector_identity(&state, &meta, &headers).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let gw = match gateway(&state) {
        Ok(g) => g,
        Err(r) => return r,
    };
    let body: HeartbeatBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(_) => return err(StatusCode::BAD_REQUEST, "bad_input"),
    };
    if body.target_id.len() > FIELD_MAX {
        return err(StatusCode::BAD_REQUEST, "bad_input");
    }
    if let Err(r) = bind_target(&identity, &body.target_id) {
        return r;
    }
    match gw.heartbeat(&meta, &identity).await {
        Ok(h) => (
            StatusCode::OK,
            Json(json!({ "revoked": h.revoked, "cancel": h.cancel })),
        )
            .into_response(),
        Err(e) => gateway_err(&e),
    }
}
