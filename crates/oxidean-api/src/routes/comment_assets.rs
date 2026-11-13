//! Comment-attachment upload + serve (WI-0001 pr-new-ui composer parity).
//!
//! `POST /api/uploads/comment-attachments?name=<file>` accepts a raw body
//! (session cookie or PAT bearer, same auth as release-asset upload) and
//! stores it under `{uploads_dir}/comment-attachments/{uuid}` with a
//! `{uuid}.json` sidecar carrying name/content_type/size — no DB row, the
//! unguessable id is the capability (GitHub user-attachments model).
//! `GET /uploads/comment-attachments/{id}` serves it publicly: raster
//! images inline, everything else `Content-Disposition: attachment`.

use std::path::{Path, PathBuf};

use axum::extract::{Path as AxumPath, Request};
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::resolve_session_user_id;
use crate::app::AppState;

/// Max upload body size (10 MiB) — GitHub's non-video comment-attachment cap.
pub const COMMENT_ATTACHMENT_MAX_BYTES: usize = 10 * 1024 * 1024;

const INLINE_TYPES: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

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

fn attachments_dir(uploads_dir: &Path) -> PathBuf {
    uploads_dir.join("comment-attachments")
}

#[derive(Deserialize)]
pub struct UploadQuery {
    /// Original filename — stored for `Content-Disposition` on download.
    pub name: Option<String>,
    /// Declared content type; anything outside the inline allowlist is
    /// served as an attachment regardless.
    pub content_type: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct AttachmentMeta {
    name: String,
    content_type: String,
    size: u64,
    uploader_id: String,
    created_at: String,
}

fn meta_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

/// `POST /api/uploads/comment-attachments?name=&content_type=` — raw body.
pub async fn upload_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<UploadQuery>,
    req: Request,
) -> Response {
    let Some((user_id, _pat)) = resolve_session_user_id(&state, &headers).await else {
        return err_response(
            StatusCode::UNAUTHORIZED,
            "auth.unauthenticated",
            "not authenticated",
        );
    };
    let bytes = match axum::body::to_bytes(req.into_body(), COMMENT_ATTACHMENT_MAX_BYTES).await {
        Ok(b) => b,
        Err(_) => {
            return err_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "upload.too_large",
                "attachment exceeds the 10 MiB limit",
            );
        }
    };
    if bytes.is_empty() {
        return err_response(
            StatusCode::BAD_REQUEST,
            "upload.empty",
            "attachment body is empty",
        );
    }
    let name = query
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("file")
        .chars()
        .take(255)
        .collect::<String>();
    let content_type = query
        .content_type
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.len() <= 128 && !t.chars().any(|c| c.is_control()))
        .unwrap_or("application/octet-stream")
        .to_string();

    let dir = attachments_dir(&state.uploads_dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::error!(error = %e, "comment-attachments dir create failed");
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "upload.internal",
            "could not store attachment",
        );
    }
    let id = Uuid::new_v4().to_string();
    let data_path = dir.join(&id);
    if let Err(e) = std::fs::write(&data_path, &bytes) {
        tracing::error!(error = %e, "comment-attachment write failed");
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "upload.internal",
            "could not store attachment",
        );
    }
    let meta = AttachmentMeta {
        name,
        content_type: content_type.clone(),
        size: bytes.len() as u64,
        uploader_id: user_id,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = std::fs::write(
        meta_path(&dir, &id),
        serde_json::to_vec(&meta).unwrap_or_default(),
    ) {
        tracing::error!(error = %e, "comment-attachment meta write failed");
        let _ = std::fs::remove_file(&data_path);
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "upload.internal",
            "could not store attachment",
        );
    }

    Json(serde_json::json!({
        "ok": true,
        "data": {
            "id": id,
            "url": format!("/uploads/comment-attachments/{id}"),
            "name": meta.name,
            "content_type": meta.content_type,
            "size": meta.size,
        }
    }))
    .into_response()
}

/// `GET /uploads/comment-attachments/{id}` — public serve; raster images
/// inline, everything else downloads. `nosniff` + stored type keep a
/// hostile `text/html` upload from ever rendering as a page.
pub async fn serve_attachment(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    // Ids are uuid v4 — reject anything else before touching the fs.
    if Uuid::parse_str(&id).is_err() {
        return err_response(StatusCode::NOT_FOUND, "upload.not_found", "not found");
    }
    let dir = attachments_dir(&state.uploads_dir);
    let data_path = dir.join(&id);
    let Ok(bytes) = tokio::fs::read(&data_path).await else {
        return err_response(StatusCode::NOT_FOUND, "upload.not_found", "not found");
    };
    let meta: Option<AttachmentMeta> = tokio::fs::read(meta_path(&dir, &id))
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());

    let content_type = meta
        .as_ref()
        .map(|m| m.content_type.as_str())
        .unwrap_or("application/octet-stream");
    let filename = meta
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "file".to_string());
    let inline = INLINE_TYPES.contains(&content_type);
    let disposition = if inline {
        "inline".to_string()
    } else {
        // RFC 5987 filename* for non-ASCII names; plain fallback for ASCII.
        let ascii: String = filename
            .chars()
            .map(|c| {
                if c.is_ascii() && c != '"' && c != '\\' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("attachment; filename=\"{ascii}\"")
    };

    let mut resp = bytes.into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_str(content_type)
            .unwrap_or(header::HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_str(&disposition)
            .unwrap_or(header::HeaderValue::from_static("attachment")),
    );
    headers.insert(
        header::HeaderName::from_static("x-content-type-options"),
        header::HeaderValue::from_static("nosniff"),
    );
    resp
}
