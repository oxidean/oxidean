//! Inbound push webhook for remote forges (GitHub / GitLab / Gitea).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use sha2::{Digest, Sha256};

use crate::actions::secrets::decrypt_secret;
use crate::app::AppState;
use crate::mirror::queue::enqueue_mirror_for_repo;
use crate::repo::lookup_repo_row_or_redirect;

/// `POST /api/repos/{owner}/{repo}/mirror/hook`
pub async fn mirror_hook(
    State(state): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Anti-enumeration: missing/private/no-mirror → 404.
    let not_found = (StatusCode::NOT_FOUND, "Not Found");

    let (repo, _owner_ref) = match lookup_repo_row_or_redirect(&state.db, &owner, &name).await {
        Ok(Some(pair)) => pair,
        _ => return not_found.into_response(),
    };
    let mirror = match state.db.get_mirror_by_repo(&repo.id).await {
        Ok(Some(m)) if m.enabled => m,
        _ => return not_found.into_response(),
    };
    if mirror.webhook_secret_ciphertext.is_empty() {
        return not_found.into_response();
    }
    let secret = match decrypt_secret(&mirror.webhook_secret_ciphertext) {
        Ok(s) => s,
        Err(_) => return not_found.into_response(),
    };

    if !verify_hook_auth(&headers, &body, &secret) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    // Ignore pings / unrelated events — still 204 after auth.
    let event = headers
        .get("x-github-event")
        .or_else(|| headers.get("x-gitlab-event"))
        .or_else(|| headers.get("x-gitea-event"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_push =
        event.is_empty() || event.contains("push") || event == "tag_push" || event == "create";

    if is_push {
        let db = state.db.clone();
        let git = Arc::clone(&state.git);
        let repos = state.repos_dir.clone();
        let rid = repo.id.clone();
        tokio::spawn(async move {
            if let Err(e) = enqueue_mirror_for_repo(db, git, repos, rid).await {
                tracing::warn!(error = %e, "mirror hook enqueue failed");
            }
        });
    }

    StatusCode::NO_CONTENT.into_response()
}

fn verify_hook_auth(headers: &HeaderMap, body: &[u8], secret: &str) -> bool {
    // GitLab token header (exact match).
    if let Some(tok) = headers.get("x-gitlab-token").and_then(|v| v.to_str().ok()) {
        return constant_time_eq(tok.as_bytes(), secret.as_bytes());
    }
    // GitHub / Gitea HMAC-SHA256.
    for key in ["x-hub-signature-256", "x-gitea-signature"] {
        if let Some(sig) = headers.get(key).and_then(|v| v.to_str().ok()) {
            if verify_hmac_sha256(secret, body, sig) {
                return true;
            }
        }
    }
    // Fallback: Authorization Bearer
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(tok) = auth.strip_prefix("Bearer ") {
            return constant_time_eq(tok.trim().as_bytes(), secret.as_bytes());
        }
    }
    false
}

fn verify_hmac_sha256(secret: &str, body: &[u8], signature_header: &str) -> bool {
    let sig = signature_header
        .strip_prefix("sha256=")
        .unwrap_or(signature_header)
        .trim();
    let expected = hmac_sha256_hex(secret.as_bytes(), body);
    constant_time_eq(expected.as_bytes(), sig.as_bytes())
}

/// Minimal HMAC-SHA256 (avoids hmac/sha2 version skew).
fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let dig = Sha256::digest(key);
        key_block[..32].copy_from_slice(&dig);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn hmac_accepts_hub_signature_256() {
        let secret = "whsec_test";
        let body = br#"{"ref":"refs/heads/main"}"#;
        let sig = format!("sha256={}", hmac_sha256_hex(secret.as_bytes(), body));
        let mut headers = HeaderMap::new();
        headers.insert("x-hub-signature-256", HeaderValue::from_str(&sig).unwrap());
        assert!(verify_hook_auth(&headers, body, secret));
    }

    #[test]
    fn hmac_rejects_bad_signature() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-hub-signature-256",
            HeaderValue::from_static("sha256=deadbeef"),
        );
        assert!(!verify_hook_auth(
            &headers,
            br#"{"ref":"refs/heads/main"}"#,
            "whsec_test",
        ));
    }

    #[test]
    fn gitlab_token_accepts_and_rejects() {
        let mut headers = HeaderMap::new();
        headers.insert("x-gitlab-token", HeaderValue::from_static("good-secret"));
        assert!(verify_hook_auth(&headers, b"{}", "good-secret"));
        assert!(!verify_hook_auth(&headers, b"{}", "bad-secret"));
    }

    #[test]
    fn missing_auth_rejects() {
        let headers = HeaderMap::new();
        assert!(!verify_hook_auth(&headers, b"{}", "secret"));
    }
}
