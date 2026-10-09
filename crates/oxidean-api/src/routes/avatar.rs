//! Avatar multipart upload, delete, and static serve (AUTH-08, D-18, T-04-19/20).
//!
//! Files land under `var/uploads/avatars/{user_id}.webp` (uploads_dir default `var/uploads`).

use std::path::{Path, PathBuf};

use axum::body::Bytes;
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};
use oxidean_core::AppError;

use crate::app::AppState;
use crate::auth::session::{ResolvedSession, SESSION_COOKIE_NAME};

/// Max upload body size (2 MiB).
pub const AVATAR_MAX_BYTES: usize = 2 * 1024 * 1024;

const ALLOWED_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp"];

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

#[allow(clippy::result_large_err)]
async fn require_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ResolvedSession, Response> {
    let Some(token) = session_token_from_headers(headers) else {
        return Err(err_response(
            StatusCode::UNAUTHORIZED,
            "auth.unauthenticated",
            "not authenticated",
        ));
    };
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
        Ok(Some(s)) => Ok(s),
        Ok(None) => Err(err_response(
            StatusCode::UNAUTHORIZED,
            "auth.unauthenticated",
            "not authenticated",
        )),
        Err(e) => {
            tracing::warn!(error = %e, "session resolve failed on avatar route");
            Err(err_response(
                StatusCode::UNAUTHORIZED,
                "auth.unauthenticated",
                "not authenticated",
            ))
        }
    }
}

fn public_avatar_url(user_id: &str) -> String {
    format!("/uploads/avatars/{user_id}.webp")
}

fn avatar_fs_path(uploads_dir: &Path, user_id: &str) -> PathBuf {
    uploads_dir.join("avatars").join(format!("{user_id}.webp"))
}

/// Decode, resize longest edge to 512px, encode as WebP bytes.
fn process_avatar(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let img = image::load_from_memory(bytes).map_err(|e| {
        AppError::new(
            "avatar.invalid_image",
            format!("could not decode image: {e}"),
        )
    })?;

    let resized = resize_longest_edge(img, 512);
    let mut out = Vec::new();
    resized
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::WebP)
        .map_err(|e| {
            AppError::new(
                "avatar.encode_failed",
                format!("could not encode avatar: {e}"),
            )
        })?;
    Ok(out)
}

fn resize_longest_edge(img: DynamicImage, max_edge: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w <= max_edge && h <= max_edge {
        return img;
    }
    let (nw, nh) = if w >= h {
        let nw = max_edge;
        let nh = ((h as f64) * (max_edge as f64) / (w as f64)).round() as u32;
        (nw, nh.max(1))
    } else {
        let nh = max_edge;
        let nw = ((w as f64) * (max_edge as f64) / (h as f64)).round() as u32;
        (nw.max(1), nh)
    };
    img.resize(nw, nh, FilterType::Lanczos3)
}

/// `POST /api/user/avatar` — multipart field `avatar`.
pub async fn upload_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let session = match require_session(&state, &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    let mut avatar_bytes: Option<Bytes> = None;
    let mut content_type: Option<String> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return err_response(
                    StatusCode::BAD_REQUEST,
                    "avatar.multipart",
                    &format!("invalid multipart: {e}"),
                );
            }
        };

        let name = field.name().unwrap_or("").to_string();
        if name != "avatar" {
            continue;
        }

        let ct = field
            .content_type()
            .map(|m| m.to_string())
            .unwrap_or_default();
        content_type = Some(ct);

        let data = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return err_response(
                    StatusCode::BAD_REQUEST,
                    "avatar.read_failed",
                    &format!("failed to read upload: {e}"),
                );
            }
        };

        if data.len() > AVATAR_MAX_BYTES {
            return err_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "avatar.too_large",
                "avatar must be at most 2 MiB",
            );
        }
        avatar_bytes = Some(data);
    }

    let Some(bytes) = avatar_bytes else {
        return err_response(
            StatusCode::BAD_REQUEST,
            "avatar.missing",
            "multipart field 'avatar' is required",
        );
    };

    let ct = content_type.unwrap_or_default();
    let ct_base = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if !ALLOWED_TYPES.iter().any(|t| *t == ct_base) {
        return err_response(
            StatusCode::BAD_REQUEST,
            "avatar.unsupported_type",
            "avatar must be image/jpeg, image/png, or image/webp",
        );
    }

    let encoded = match process_avatar(&bytes) {
        Ok(v) => v,
        Err(e) => {
            return err_response(StatusCode::BAD_REQUEST, &e.code, &e.message);
        }
    };

    let avatars_dir = state.uploads_dir.join("avatars");
    if let Err(e) = tokio::fs::create_dir_all(&avatars_dir).await {
        tracing::error!(error = %e, "create avatars dir failed");
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "avatar.store_failed",
            "could not store avatar",
        );
    }

    let fs_path = avatar_fs_path(&state.uploads_dir, &session.user_id);
    // Ignore client filename — only write `{user_id}.webp` (T-04-19).
    if let Err(e) = tokio::fs::write(&fs_path, &encoded).await {
        tracing::error!(error = %e, path = %fs_path.display(), "write avatar failed");
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "avatar.store_failed",
            "could not store avatar",
        );
    }

    let avatar_url = public_avatar_url(&session.user_id);

    let existing = match state.db.find_user_by_id(&session.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return err_response(
                StatusCode::UNAUTHORIZED,
                "auth.unauthenticated",
                "not authenticated",
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "find user after avatar upload");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "avatar.store_failed",
                "could not update profile",
            );
        }
    };

    if let Err(e) = state
        .db
        .update_user_profile(
            &session.user_id,
            &existing.display_name,
            &existing.username,
            &existing.bio,
            Some(&avatar_url),
        )
        .await
    {
        tracing::error!(error = %e, "update avatar_path failed");
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "avatar.store_failed",
            "could not update profile",
        );
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "avatar_url": avatar_url
        })),
    )
        .into_response()
}

/// `DELETE /api/user/avatar` — clear `avatar_path` and remove stored WebP if present.
pub async fn delete_avatar(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(&state, &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };

    let existing = match state.db.find_user_by_id(&session.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return err_response(
                StatusCode::UNAUTHORIZED,
                "auth.unauthenticated",
                "not authenticated",
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "find user on avatar delete");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "avatar.delete_failed",
                "could not update profile",
            );
        }
    };

    if let Err(e) = state
        .db
        .update_user_profile(
            &session.user_id,
            &existing.display_name,
            &existing.username,
            &existing.bio,
            None,
        )
        .await
    {
        tracing::error!(error = %e, "clear avatar_path failed");
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "avatar.delete_failed",
            "could not update profile",
        );
    }

    let fs_path = avatar_fs_path(&state.uploads_dir, &session.user_id);
    match tokio::fs::remove_file(&fs_path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            // Profile already cleared — log and still succeed (idempotent UX).
            tracing::warn!(error = %e, path = %fs_path.display(), "avatar file remove failed");
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "avatar_url": null
        })),
    )
        .into_response()
}

/// Basename must match `^[a-zA-Z0-9_-]+\.webp$` — no path traversal (T-04-19).
fn is_safe_avatar_basename(name: &str) -> bool {
    if name.is_empty() || name.contains("..") || name.contains('/') || name.contains('\\') {
        return false;
    }
    let Some(stem) = name.strip_suffix(".webp") else {
        return false;
    };
    !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `GET /uploads/avatars/{file}` — public read of avatar bytes.
pub async fn serve_avatar(
    State(state): State<AppState>,
    AxumPath(file): AxumPath<String>,
) -> Response {
    if !is_safe_avatar_basename(&file) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let path = state.uploads_dir.join("avatars").join(&file);
    // Extra guard: resolved path must stay under avatars dir.
    let Ok(avatars_canon) = tokio::fs::canonicalize(state.uploads_dir.join("avatars")).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(file_canon) = tokio::fs::canonicalize(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !file_canon.starts_with(&avatars_canon) {
        return StatusCode::NOT_FOUND.into_response();
    }

    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "image/webp")],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_basename_accepts_uuid_webp() {
        assert!(is_safe_avatar_basename("abc-123_def.webp"));
    }

    #[test]
    fn safe_basename_rejects_traversal() {
        assert!(!is_safe_avatar_basename("../etc/passwd"));
        assert!(!is_safe_avatar_basename("..%2Fetc.webp"));
        assert!(!is_safe_avatar_basename("foo/bar.webp"));
        assert!(!is_safe_avatar_basename("foo.webp.exe"));
    }

    #[test]
    fn avatar_max_is_two_mib() {
        assert_eq!(AVATAR_MAX_BYTES, 2_097_152);
    }
}
