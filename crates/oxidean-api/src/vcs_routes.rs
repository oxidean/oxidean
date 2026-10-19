//! `/api/vcs/{owner}/{name}/objects` REST routes (contract C2) — the raw
//! object transport beside the `vcs.*` RPC family.
//!
//! Auth: PAT (Basic/Bearer) and OAuth access tokens ONLY — a session cookie
//! alone never authenticates these routes (AC8.5.1); cookie-only requests get
//! `401 vcs.unauthenticated`. PAT scope classes ride the route-alias names
//! `vcs.object_get`/`vcs.object_put` (`proc_classes.rs` — they never appear
//! in `PROCEDURES`). Success GET is `application/octet-stream` with an
//! immutable `Cache-Control`; every failure is the JSON envelope
//! (`{"ok":false,"error":{…}}`) with a real status.
//!
//! Flag gate (AC1.2.1): GET serves whenever the repo's flag ROW exists —
//! `enabled:false` still serves reads; `vcs.disabled` fires only on a
//! never-enabled repo. POST requires the flag enabled (a write op).

use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;

use oxidean_core::{AppError, ObjectId, RpcResponse, VcsErrorCode, VcsObjectPutResponse};

use crate::app::AppState;
use crate::repo::{
    effective_capability, is_private_visibility, lookup_repo_row_or_redirect, meets, Capability,
};

/// `AppError` → `(status, envelope)` for the route error contract.
fn err_response(e: AppError) -> Response {
    let status = match e.code.as_str() {
        "vcs.unauthenticated" | "auth.unauthenticated" => StatusCode::UNAUTHORIZED,
        "vcs.disabled" | "vcs.forbidden" | "auth.pat_scope" => StatusCode::FORBIDDEN,
        "vcs.not_found" | "repo.not_found" => StatusCode::NOT_FOUND,
        "vcs.quota_exceeded" | "vcs.too_large" => StatusCode::PAYLOAD_TOO_LARGE,
        "vcs.invalid_input"
        | "vcs.ref_conflict"
        | "vcs.conflict"
        | "vcs.no_common_ancestor"
        | "vcs.import_state"
        | "rpc.bad_input" => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(RpcResponse::err(e))).into_response()
}

/// Bearer-token extraction — `Authorization: Bearer <token>` only; Basic
/// `user:token` is already normalized upstream by the Smart-HTTP path, but
/// these routes accept the Bearer form (contract C2).
fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = raw.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// Shared auth + repo + flag resolution for both routes. Cookie-only
/// requests fail `vcs.unauthenticated` outright — the session cookie never
/// counts (AC8.5.1).
async fn resolve(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    name: &str,
    procedure: &'static str,
    need: Capability,
) -> Result<(String, String), Box<Response>> {
    let client = crate::rpc::ClientMeta::from_headers(headers);
    let Some(token) = bearer_token(headers) else {
        return Err(Box::new(err_response(
            VcsErrorCode::Unauthenticated.app_error("authentication required"),
        )));
    };
    let (session, pat) = match crate::pat::bearer::resolve_bearer(
        &state.db,
        &state.git_auth_limiter,
        token,
        client.ip_address.as_deref(),
    )
    .await
    {
        Ok(pair) => pair,
        Err(rejection) => {
            let status = match rejection.status() {
                StatusCode::TOO_MANY_REQUESTS => StatusCode::TOO_MANY_REQUESTS,
                _ => StatusCode::UNAUTHORIZED,
            };
            return Err(Box::new(
                (status, Json(RpcResponse::err(rejection.error))).into_response(),
            ));
        }
    };

    let (repo_row, owner_ref) = match lookup_repo_row_or_redirect(&state.db, owner, name).await {
        Ok(Some(pair)) => pair,
        _ => {
            return Err(Box::new(err_response(
                VcsErrorCode::NotFound.app_error("repository not found"),
            )))
        }
    };

    // PAT scope class — `authorize_pat_on_repo` applies the classic/FG rules
    // under the route-alias procedure name; its AppError maps via the route's
    // status table (pat_scope → 403, anti-enumeration not_found → 404).
    if let Err(e) =
        crate::pat::bearer::authorize_pat_on_repo(&state.db, &pat, procedure, &repo_row, &owner_ref)
            .await
    {
        let status = if e.code == "auth.pat_scope" {
            StatusCode::FORBIDDEN
        } else if e.code.ends_with(".not_found") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return Err(Box::new(
            (status, Json(RpcResponse::err(e))).into_response(),
        ));
    }

    // Capability tier — private-repo deny mirrors the resolver's
    // anti-enumeration shape (404, not 403).
    let capability = match effective_capability(
        &state.db,
        Some(&session.user_id),
        &repo_row,
        &owner_ref,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "vcs objects effective_capability");
            return Err(Box::new(err_response(AppError::new(
                "rpc.internal",
                "internal error",
            ))));
        }
    };
    if !meets(capability, need) {
        if is_private_visibility(&repo_row.visibility) {
            return Err(Box::new(err_response(
                VcsErrorCode::NotFound.app_error("repository not found"),
            )));
        }
        return Err(Box::new(err_response(
            VcsErrorCode::Forbidden.app_error("insufficient repository capability"),
        )));
    }

    Ok((repo_row.id, session.user_id))
}

/// GET flag gate — the flag ROW must exist (a never-enabled repo denies);
/// `enabled:false` still serves reads (AC1.2.1).
async fn flag_row_present(state: &AppState, repo_id: &str) -> bool {
    matches!(
        state.db.vcs_get_repo_surface_flag(repo_id).await,
        Ok(Some(_))
    )
}

/// `GET /api/vcs/{owner}/{name}/objects/{hash}` — raw blob bytes.
pub async fn object_get(
    State(state): State<AppState>,
    AxumPath((owner, name, hash)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let (repo_id, _user_id) = match resolve(
        &state,
        &headers,
        &owner,
        &name,
        "vcs.object_get",
        Capability::Read,
    )
    .await
    {
        Ok(pair) => pair,
        Err(res) => return *res,
    };
    let Some(slot) = state.vcs.as_ref() else {
        return err_response(VcsErrorCode::Disabled.app_error("vcs is not enabled"));
    };
    if !flag_row_present(&state, &repo_id).await {
        return err_response(
            VcsErrorCode::Disabled.app_error("vcs is not enabled on this repository"),
        );
    }
    let oid = match ObjectId::parse(&hash) {
        Ok(o) => o,
        Err(_) => {
            return err_response(VcsErrorCode::InvalidInput.app_error("invalid object hash"));
        }
    };
    let bare = match oid.as_str().strip_prefix("sha256:") {
        Some(h) => h.to_string(),
        None => return err_response(AppError::new("rpc.internal", "internal error")),
    };
    let file = match slot.api.objects().get(&bare).await {
        Ok(f) => f,
        Err(oxidean_vcs::object_store::ObjectStoreError::NotFound) => {
            return err_response(VcsErrorCode::NotFound.app_error("object not found"));
        }
        Err(_) => return err_response(AppError::new("rpc.internal", "internal error")),
    };
    // Chunked body — the whole object never lands in memory (the per-object
    // cap is `OXIDEAN_VCS_MAX_OBJECT_BYTES`, default 256 MiB). A mid-stream
    // read error surfaces as a body error after headers are sent.
    let stream =
        futures_util::stream::unfold(Some(file), |state: Option<tokio::fs::File>| async move {
            let mut f = state?;
            let mut buf = vec![0u8; 64 * 1024];
            match tokio::io::AsyncReadExt::read(&mut f, &mut buf).await {
                Ok(0) => None,
                Ok(n) => {
                    buf.truncate(n);
                    Some((
                        Ok::<_, std::io::Error>(axum::body::Bytes::from(buf)),
                        Some(f),
                    ))
                }
                Err(e) => Some((Err(e), None)),
            }
        });
    let mut res = (StatusCode::OK, Body::from_stream(stream)).into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("immutable, max-age=31536000"),
    );
    res
}

/// `POST /api/vcs/{owner}/{name}/objects` — raw blob bytes; the server
/// computes `sha256:` and returns `{ok:true,data:{hash}}` (dedup no-op on
/// re-upload). The `DefaultBodyLimit` sits ABOVE `max_object_bytes` so the
/// typed `vcs.quota_exceeded` runs before Axum's untyped 413.
pub async fn object_put(
    State(state): State<AppState>,
    AxumPath((owner, name)): AxumPath<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let (repo_id, _user_id) = match resolve(
        &state,
        &headers,
        &owner,
        &name,
        "vcs.object_put",
        Capability::Write,
    )
    .await
    {
        Ok(pair) => pair,
        Err(res) => return *res,
    };
    let Some(slot) = state.vcs.as_ref() else {
        return err_response(VcsErrorCode::Disabled.app_error("vcs is not enabled"));
    };
    match state.db.vcs_get_repo_surface_flag(&repo_id).await {
        Ok(Some(flag)) if flag.enabled => {}
        Ok(_) => {
            return err_response(
                VcsErrorCode::Disabled.app_error("vcs is not enabled on this repository"),
            );
        }
        Err(_) => return err_response(AppError::new("rpc.internal", "internal error")),
    }
    let stream = body
        .into_data_stream()
        .map(|r| r.map_err(|e| std::io::Error::other(e.to_string())));
    match slot.api.objects().put(stream, &repo_id).await {
        Ok(outcome) => match ObjectId::parse(&format!("sha256:{}", outcome.hash)) {
            Ok(hash) => (
                StatusCode::OK,
                Json(RpcResponse::ok(VcsObjectPutResponse { hash })),
            )
                .into_response(),
            Err(_) => err_response(AppError::new("rpc.internal", "internal error")),
        },
        Err(oxidean_vcs::object_store::ObjectStoreError::QuotaExceeded(_)) => {
            err_response(VcsErrorCode::QuotaExceeded.app_error("vcs: quota exceeded"))
        }
        Err(oxidean_vcs::object_store::ObjectStoreError::Stream(_))
        | Err(oxidean_vcs::object_store::ObjectStoreError::InvalidHash) => {
            err_response(VcsErrorCode::InvalidInput.app_error("invalid object stream"))
        }
        Err(oxidean_vcs::object_store::ObjectStoreError::DbNotConfigured) => err_response(
            AppError::new("db.not_configured", "vcs database not configured"),
        ),
        Err(_) => err_response(AppError::new("rpc.internal", "internal error")),
    }
}
