//! LFS max-object + quota enforcement (D-LFS-12/13/14).

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app::AppState;
use crate::lfs::batch::LfsErrorBody;

/// Default max object size: 2 GiB.
pub const DEFAULT_MAX_OBJECT_BYTES: i64 = 2 * 1024 * 1024 * 1024;
/// Default per-repo logical quota: 10 GiB.
pub const DEFAULT_QUOTA_REPO_BYTES: i64 = 10 * 1024 * 1024 * 1024;
/// Default per-user (repo-owner) logical quota: 50 GiB.
pub const DEFAULT_QUOTA_USER_BYTES: i64 = 50 * 1024 * 1024 * 1024;

const LFS_JSON: &str = "application/vnd.git-lfs+json";

fn lfs_json_headers() -> [(header::HeaderName, HeaderValue); 1] {
    [(header::CONTENT_TYPE, HeaderValue::from_static(LFS_JSON))]
}

#[derive(Debug, Clone, Copy)]
pub struct EffectiveLimits {
    pub max_object_bytes: Option<i64>,
    pub quota_repo_bytes: Option<i64>,
    pub quota_user_bytes: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum QuotaReject {
    Oversize { size: i64, max: i64 },
    RepoQuota,
    UserQuota,
}

impl QuotaReject {
    pub fn object_code(&self) -> i32 {
        match self {
            Self::Oversize { .. } => 422,
            Self::RepoQuota | Self::UserQuota => 507,
        }
    }

    pub fn object_message(&self) -> String {
        match self {
            Self::Oversize { size, max } => {
                format!("LFS object size {size} exceeds max object size {max}")
            }
            Self::RepoQuota => "LFS repository storage quota exceeded".into(),
            Self::UserQuota => "LFS user storage quota exceeded".into(),
        }
    }

    pub fn into_response(self) -> Response {
        let status = match &self {
            Self::Oversize { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RepoQuota | Self::UserQuota => StatusCode::INSUFFICIENT_STORAGE,
        };
        (
            status,
            lfs_json_headers(),
            Json(LfsErrorBody {
                message: self.object_message(),
                request_id: None,
            }),
        )
            .into_response()
    }
}

/// `0` or `-1` means unlimited; otherwise positive limit.
fn normalize_limit(v: i64) -> Option<i64> {
    if v <= 0 {
        None
    } else {
        Some(v)
    }
}

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

/// Raw limit values before unlimited normalization (for Admin getSettings).
pub async fn raw_effective_limits(state: &AppState) -> Result<(i64, i64, i64, bool, bool, bool), String> {
    let row = state.db.get_lfs_settings().await?;
    let max = row
        .max_object_bytes
        .unwrap_or_else(|| env_i64("OXIDEAN_LFS_MAX_OBJECT_BYTES", DEFAULT_MAX_OBJECT_BYTES));
    let repo = row
        .quota_repo_bytes
        .unwrap_or_else(|| env_i64("OXIDEAN_LFS_QUOTA_REPO_BYTES", DEFAULT_QUOTA_REPO_BYTES));
    let user = row
        .quota_user_bytes
        .unwrap_or_else(|| env_i64("OXIDEAN_LFS_QUOTA_USER_BYTES", DEFAULT_QUOTA_USER_BYTES));
    Ok((
        max,
        repo,
        user,
        row.max_object_bytes.is_some(),
        row.quota_repo_bytes.is_some(),
        row.quota_user_bytes.is_some(),
    ))
}

#[allow(clippy::result_large_err)]
/// Resolve effective limits: Admin DB override (when Some) wins over env default.
pub async fn effective_limits(state: &AppState) -> Result<EffectiveLimits, Response> {
    let (max, repo, user, _, _, _) = match raw_effective_limits(state).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "get_lfs_settings");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    Ok(EffectiveLimits {
        max_object_bytes: normalize_limit(max),
        quota_repo_bytes: normalize_limit(repo),
        quota_user_bytes: normalize_limit(user),
    })
}

/// Check max size + repo/owner quotas for a prospective upload of `size` bytes.
/// Skips quota increment when the OID is already linked (dedup).
pub async fn check_upload(
    state: &AppState,
    repo_id: &str,
    owner_id: &str,
    size: i64,
    already_linked: bool,
) -> Result<(), QuotaReject> {
    let limits = match effective_limits(state).await {
        Ok(l) => l,
        Err(_) => {
            // Treat settings failure as unlimited to avoid locking uploads on DB blip;
            // callers already log. Prefer fail-open only for internal errors path —
            // use oversize-safe defaults instead: re-fetch via env only.
            EffectiveLimits {
                max_object_bytes: normalize_limit(env_i64(
                    "OXIDEAN_LFS_MAX_OBJECT_BYTES",
                    DEFAULT_MAX_OBJECT_BYTES,
                )),
                quota_repo_bytes: normalize_limit(env_i64(
                    "OXIDEAN_LFS_QUOTA_REPO_BYTES",
                    DEFAULT_QUOTA_REPO_BYTES,
                )),
                quota_user_bytes: normalize_limit(env_i64(
                    "OXIDEAN_LFS_QUOTA_USER_BYTES",
                    DEFAULT_QUOTA_USER_BYTES,
                )),
            }
        }
    };
    if let Some(max) = limits.max_object_bytes {
        if size > max {
            return Err(QuotaReject::Oversize { size, max });
        }
    }
    if already_linked {
        return Ok(());
    }
    // New physical OID: charge full size. Linking existing OID still charges logical
    // once per repo (already_linked false + on_disk true).
    if let Some(repo_q) = limits.quota_repo_bytes {
        let used = state.db.lfs_repo_logical_bytes(repo_id).await.unwrap_or(0);
        if used.saturating_add(size) > repo_q {
            return Err(QuotaReject::RepoQuota);
        }
    }
    if let Some(user_q) = limits.quota_user_bytes {
        let used = state
            .db
            .lfs_owner_logical_bytes(owner_id)
            .await
            .unwrap_or(0);
        if used.saturating_add(size) > user_q {
            return Err(QuotaReject::UserQuota);
        }
    }
    Ok(())
}
