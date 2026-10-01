//! Release asset multipart upload + id-based download (GIT-14/15 / D-REL-04..06).

use std::path::{Path, PathBuf};

use axum::body::Bytes;
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::session::SESSION_COOKIE_NAME;
use crate::repo::{
    effective_capability, meets, owner_ref_for_repo, resolve_owner_slug, Capability,
};
use crate::rpc::RpcCtx;

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

/// Opaque on-disk path: `{release_assets_dir}/{asset_id}` (D-REL-04).
pub fn asset_fs_path(release_assets_dir: &Path, asset_id: &str) -> PathBuf {
    release_assets_dir.join(asset_id)
}

pub fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("asset")
        .trim();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "asset".into()
    } else {
        cleaned
    }
}

pub async fn remove_asset_file(release_assets_dir: &Path, asset_id: &str) {
    let path = asset_fs_path(release_assets_dir, asset_id);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(error = %e, path = %path.display(), "remove release asset file"),
    }
}

async fn resolve_session_user_id(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let token = session_token_from_headers(headers)?;
    let client = crate::rpc::ClientMeta::from_headers(headers);
    match state
        .sessions
        .resolve(
            &state.db,
            &token,
            client.ip_address.as_deref(),
            client.user_agent.as_deref(),
        )
        .await
    {
        Ok(Some(s)) => Some(s.user_id),
        _ => None,
    }
}

/// `POST /api/repos/{owner}/{repo}/releases/{release_id}/assets` — multipart field `asset`.
pub async fn upload_asset(
    State(state): State<AppState>,
    AxumPath((owner, repo, release_id)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let Some(user_id) = resolve_session_user_id(&state, &headers).await else {
        return err_response(
            StatusCode::UNAUTHORIZED,
            "auth.unauthenticated",
            "not authenticated",
        );
    };

    let owner_ref = match resolve_owner_slug(&state.db, &owner).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return err_response(StatusCode::NOT_FOUND, "repo.not_found", "Repository not found");
        }
        Err(e) => {
            tracing::error!(error = %e, "resolve_owner_slug");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.internal",
                "upload failed",
            );
        }
    };
    let repo_row = match state
        .db
        .find_repository_by_owner_name(owner_ref.id(), &repo)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return err_response(StatusCode::NOT_FOUND, "repo.not_found", "Repository not found");
        }
        Err(e) => {
            tracing::error!(error = %e, "find repository");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.internal",
                "upload failed",
            );
        }
    };
    let capability = match effective_capability(&state.db, Some(&user_id), &repo_row, &owner_ref)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "effective_capability");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.internal",
                "upload failed",
            );
        }
    };
    if !meets(capability, Capability::Write) {
        return err_response(StatusCode::NOT_FOUND, "repo.not_found", "Repository not found");
    }

    let release = match state.db.find_release_by_id(&release_id).await {
        Ok(Some(r)) if r.repo_id == repo_row.id => r,
        Ok(Some(_)) | Ok(None) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "release.not_found",
                "Release not found",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "find release");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.internal",
                "upload failed",
            );
        }
    };

    let mut file_bytes: Option<Bytes> = None;
    let mut content_type = "application/octet-stream".to_string();
    let mut filename = "asset".to_string();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return err_response(
                    StatusCode::BAD_REQUEST,
                    "release.asset_multipart",
                    &format!("invalid multipart: {e}"),
                );
            }
        };
        let name = field.name().unwrap_or("").to_string();
        if name != "asset" && name != "file" {
            continue;
        }
        if let Some(fname) = field.file_name() {
            filename = sanitize_filename(fname);
        }
        if let Some(ct) = field.content_type() {
            content_type = ct.to_string();
        }
        let data = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return err_response(
                    StatusCode::BAD_REQUEST,
                    "release.asset_read_failed",
                    &format!("failed to read upload: {e}"),
                );
            }
        };
        if data.len() > state.release_asset_max_bytes {
            return err_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "release.asset_too_large",
                &format!(
                    "asset must be at most {} bytes",
                    state.release_asset_max_bytes
                ),
            );
        }
        file_bytes = Some(data);
    }

    let Some(bytes) = file_bytes else {
        return err_response(
            StatusCode::BAD_REQUEST,
            "release.asset_missing",
            "multipart field 'asset' is required",
        );
    };

    if let Err(e) = tokio::fs::create_dir_all(&state.release_assets_dir).await {
        tracing::error!(error = %e, "create release assets dir");
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "release.asset_store_failed",
            "could not store asset",
        );
    }

    let existing = match state
        .db
        .find_release_asset_by_filename(&release.id, &filename)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "find asset by filename");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.internal",
                "upload failed",
            );
        }
    };

    let (asset_id, row) = if let Some(prev) = existing {
        let path = asset_fs_path(&state.release_assets_dir, &prev.id);
        if let Err(e) = tokio::fs::write(&path, &bytes).await {
            tracing::error!(error = %e, "write asset replace");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.asset_store_failed",
                "could not store asset",
            );
        }
        match state
            .db
            .update_release_asset_bytes(&prev.id, &content_type, bytes.len() as i64)
            .await
        {
            Ok(r) => (prev.id, r),
            Err(e) => {
                tracing::error!(error = %e, "update asset bytes");
                return err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "release.internal",
                    "upload failed",
                );
            }
        }
    } else {
        let id = Uuid::new_v4().to_string();
        let path = asset_fs_path(&state.release_assets_dir, &id);
        if let Err(e) = tokio::fs::write(&path, &bytes).await {
            tracing::error!(error = %e, "write asset");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.asset_store_failed",
                "could not store asset",
            );
        }
        match state
            .db
            .insert_release_asset(
                &id,
                &release.id,
                &filename,
                &content_type,
                bytes.len() as i64,
                &user_id,
            )
            .await
        {
            Ok(r) => (id, r),
            Err(e) => {
                let _ = tokio::fs::remove_file(&path).await;
                tracing::error!(error = %e, "insert asset");
                return err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "release.internal",
                    "upload failed",
                );
            }
        }
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "asset": {
                "id": asset_id,
                "release_id": row.release_id,
                "filename": row.filename,
                "content_type": row.content_type,
                "byte_size": row.byte_size,
                "uploader_id": row.uploader_id,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
                "download_url": format!("/api/releases/assets/{asset_id}"),
            }
        })),
    )
        .into_response()
}

/// `GET /api/releases/assets/{asset_id}` — ACL'd download (D-REL-06).
pub async fn download_asset(
    State(state): State<AppState>,
    AxumPath(asset_id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    if asset_id.contains('/') || asset_id.contains('\\') || asset_id.contains("..") {
        return err_response(StatusCode::NOT_FOUND, "release.asset_not_found", "Asset not found");
    }
    let asset = match state.db.find_release_asset_by_id(&asset_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "release.asset_not_found",
                "Asset not found",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "find asset");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.internal",
                "download failed",
            );
        }
    };
    let release = match state.db.find_release_by_id(&asset.release_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "release.asset_not_found",
                "Asset not found",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "find release for asset");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.internal",
                "download failed",
            );
        }
    };
    let repo = match state.db.find_repository_by_id(&release.repo_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "release.asset_not_found",
                "Asset not found",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "find repo for asset");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release.internal",
                "download failed",
            );
        }
    };
    let owner_ref = match owner_ref_for_repo(&state.db, &repo).await {
        Ok(Some(o)) => o,
        _ => {
            return err_response(
                StatusCode::NOT_FOUND,
                "release.asset_not_found",
                "Asset not found",
            );
        }
    };

    let caller = resolve_session_user_id(&state, &headers).await;
    let capability =
        match effective_capability(&state.db, caller.as_deref(), &repo, &owner_ref).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "effective_capability download");
                return err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "release.internal",
                    "download failed",
                );
            }
        };

    let need = if release.draft {
        Capability::Write
    } else {
        Capability::Read
    };
    if !meets(capability, need) {
        return err_response(
            StatusCode::NOT_FOUND,
            "release.asset_not_found",
            "Asset not found",
        );
    }

    let path = asset_fs_path(&state.release_assets_dir, &asset_id);
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "release.asset_not_found",
                "Asset not found",
            );
        }
    };

    let mut res = Response::new(axum::body::Body::from(bytes));
    *res.status_mut() = StatusCode::OK;
    let ct = if asset.content_type.trim().is_empty() {
        "application/octet-stream"
    } else {
        asset.content_type.as_str()
    };
    if let Ok(hv) = HeaderValue::from_str(ct) {
        res.headers_mut().insert(header::CONTENT_TYPE, hv);
    }
    let disp = format!(
        "attachment; filename=\"{}\"",
        asset.filename.replace('"', "_")
    );
    if let Ok(hv) = HeaderValue::from_str(&disp) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
    }
    res
}

/// Helper for RPC deleteAsset / release.delete — remove DB row + file.
pub async fn delete_asset_with_file(
    ctx: &RpcCtx,
    release_assets_dir: &Path,
    asset_id: &str,
) -> Result<(), oxidean_core::AppError> {
    ctx.db
        .delete_release_asset(asset_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "delete_release_asset");
            oxidean_core::AppError::new("release.internal", "could not delete asset")
        })?;
    remove_asset_file(release_assets_dir, asset_id).await;
    Ok(())
}
