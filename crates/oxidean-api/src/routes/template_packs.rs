//! Multipart upload for instance template packs (issue #18).

use axum::body::Bytes;
use axum::extract::{Multipart, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app::AppState;
use crate::auth::session::SESSION_COOKIE_NAME;
use crate::rpc::RpcCtx;
use crate::templates::handlers;

fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let part = part.trim();
        let prefix = format!("{SESSION_COOKIE_NAME}=");
        if let Some(value) = part.strip_prefix(prefix.as_str()) {
            return Some(value.to_string());
        }
    }
    None
}

fn err_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "ok": false,
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

async fn build_ctx(state: &AppState, headers: &HeaderMap) -> RpcCtx {
    let raw = session_token_from_headers(headers);
    let client = crate::rpc::ClientMeta::from_headers(headers);
    let session = match raw.as_deref() {
        Some(token) => state
            .sessions
            .resolve(
                &state.db,
                token,
                client.ip_address.as_deref(),
                client.user_agent.as_deref(),
            )
            .await
            .unwrap_or_default(),
        None => None,
    };
    RpcCtx {
        db: state.db.clone(),
        email: state.current_email(),
        email_slot: state.email.clone(),
        sessions: state.sessions.clone(),
        uploads_dir: state.uploads_dir.clone(),
        repos_dir: state.repos_dir.clone(),
        lfs_dir: state.lfs_dir.clone(),
        release_assets_dir: state.release_assets_dir.clone(),
        template_packs_dir: state.template_packs_dir.clone(),
        actions_log_dir: state.actions_log_dir.clone(),
        git: state.git.clone(),
        env_name: state.env_name.clone(),
        session,
        pat: None,
        client,
        set_cookie: None,
        lookup_limiter: state.lookup_limiter.clone(),
        search_timeout_ms: state.search_timeout_ms,
        search_max_matches: state.search_max_matches,
        search_max_files: state.search_max_files,
    }
}

/// `POST /api/admin/templates` — multipart fields: `pack` (zip), `slug`, `label`,
/// optional `group`, `description`, `default_gitignore`.
pub async fn upload_pack(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let ctx = build_ctx(&state, &headers).await;
    let mut slug = String::new();
    let mut label = String::new();
    let mut group = String::from("Custom");
    let mut description = String::new();
    let mut default_gitignore: Option<String> = None;
    let mut pack_bytes: Option<Bytes> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(_) => {
                return err_response(
                    StatusCode::BAD_REQUEST,
                    "admin.bad_multipart",
                    "Invalid multipart body.",
                );
            }
        };
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "pack" | "file" | "asset" => match field.bytes().await {
                Ok(b) => pack_bytes = Some(b),
                Err(_) => {
                    return err_response(
                        StatusCode::BAD_REQUEST,
                        "admin.bad_multipart",
                        "Could not read pack bytes.",
                    );
                }
            },
            "slug" => slug = field.text().await.unwrap_or_default(),
            "label" => label = field.text().await.unwrap_or_default(),
            "group" => {
                let g = field.text().await.unwrap_or_default();
                if !g.trim().is_empty() {
                    group = g;
                }
            }
            "description" => description = field.text().await.unwrap_or_default(),
            "default_gitignore" => {
                let g = field.text().await.unwrap_or_default();
                if !g.trim().is_empty() && !g.eq_ignore_ascii_case("none") {
                    default_gitignore = Some(g);
                }
            }
            _ => {}
        }
    }

    let Some(bytes) = pack_bytes else {
        return err_response(
            StatusCode::BAD_REQUEST,
            "admin.missing_pack",
            "Multipart field `pack` (zip) is required.",
        );
    };
    if slug.trim().is_empty() || label.trim().is_empty() {
        return err_response(
            StatusCode::BAD_REQUEST,
            "admin.missing_fields",
            "Fields `slug` and `label` are required.",
        );
    }

    match handlers::create_pack_from_bytes(
        &ctx,
        &slug,
        &label,
        &group,
        &description,
        default_gitignore.as_deref(),
        &bytes,
    )
    .await
    {
        Ok(pack) => (StatusCode::CREATED, Json(serde_json::json!({ "ok": true, "pack": pack })))
            .into_response(),
        Err(e) => {
            let status = match e.code.as_str() {
                "auth.unauthenticated" => StatusCode::UNAUTHORIZED,
                "auth.forbidden" | "admin.forbidden" => StatusCode::FORBIDDEN,
                "admin.template_slug_taken" => StatusCode::CONFLICT,
                _ => StatusCode::BAD_REQUEST,
            };
            err_response(status, &e.code, &e.message)
        }
    }
}
