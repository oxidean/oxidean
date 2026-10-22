//! Bare-repo git object size quota — enforcement + usage accounting (GIT-25).
//!
//! Scoping matches LFS quotas (D-LFS-12/13): an instance-wide default from
//! `OXIDEAN_GIT_REPO_QUOTA_BYTES` (admin-overridable via `admin.git.*Settings`),
//! plus an optional per-repo override (`repositories.size_quota_bytes`).
//!
//! Enforcement lives in [`crate::protection::check_ref_update`] (the
//! `hooks/update` helper path): while a push is in flight the incoming pack
//! still sits in the receive quarantine under `GIT_DIR`, so a filesystem walk
//! measures the *projected* post-push size. When that exceeds the effective
//! quota the ref update is denied — this covers Smart HTTP, SSH receive-pack,
//! and internal worktree pushes (merges, seeds, mirror ref writes) because all
//! of them flow through `hooks/update`. Ref *deletes* carry no objects and
//! stay allowed so an over-quota repo can still clean up refs.
//!
//! `repositories.size_bytes` is bookkeeping for UI/admin surfaces, refreshed
//! after pushes/merges/syncs and on `repo.getQuota` — enforcement always
//! measures live disk state, never the cached column.

use std::path::Path;

use oxidean_core::AppError;
use oxidean_db::{Database, RepositoryRow};

/// Built-in instance default per-repo quota: 10 GiB (mirrors the LFS per-repo default).
pub const DEFAULT_REPO_QUOTA_BYTES: i64 = 10 * 1024 * 1024 * 1024;
/// Env var for the instance default; `0` / negative = unlimited.
pub const REPO_QUOTA_ENV: &str = "OXIDEAN_GIT_REPO_QUOTA_BYTES";

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

/// `0` or negative means unlimited; otherwise a positive byte limit.
fn normalize_limit(v: i64) -> Option<i64> {
    if v <= 0 {
        None
    } else {
        Some(v)
    }
}

/// Resolved instance default: admin DB override wins, else env, else built-in.
pub async fn instance_repo_quota_bytes(db: &Database) -> Result<i64, String> {
    let row = db.get_git_settings().await?;
    Ok(row
        .repo_quota_bytes
        .unwrap_or_else(|| env_i64(REPO_QUOTA_ENV, DEFAULT_REPO_QUOTA_BYTES)))
}

/// Effective per-repo quota in bytes: explicit `size_quota_bytes` override wins;
/// `None` means unlimited.
pub async fn effective_repo_quota_bytes(
    db: &Database,
    repo: &RepositoryRow,
) -> Result<Option<i64>, String> {
    let raw = match repo.size_quota_bytes {
        Some(v) => v,
        None => instance_repo_quota_bytes(db).await?,
    };
    Ok(normalize_limit(raw))
}

/// Apparent disk usage of the bare repo dir (includes quarantined pack objects
/// when called from `hooks/update`).
pub async fn repo_disk_usage_bytes(git_dir: &Path) -> Result<i64, String> {
    let bytes = oxidean_git::repo_disk_usage(git_dir)
        .await
        .map_err(|e| e.to_string())?;
    Ok(i64::try_from(bytes).unwrap_or(i64::MAX))
}

/// Measure `git_dir` and persist into `repositories.size_bytes`.
/// Returns the stored value; callers treat failures as best-effort (log + skip).
pub async fn refresh_repo_size_bytes(
    db: &Database,
    repo_id: &str,
    git_dir: &Path,
) -> Result<i64, String> {
    let size = repo_disk_usage_bytes(git_dir).await?;
    db.update_repository_size_bytes(repo_id, size).await?;
    Ok(size)
}

/// Deny a ref update when the repository — including any quarantined incoming
/// pack — is over its effective quota (GIT-25).
pub async fn enforce_push_quota(
    db: &Database,
    repo: &RepositoryRow,
    git_dir: &Path,
) -> Result<(), AppError> {
    let Some(quota) = effective_repo_quota_bytes(db, repo).await.map_err(|e| {
        tracing::error!(error = %e, "resolve git repo quota failed");
        AppError::new("repo.internal", "repository operation failed")
    })?
    else {
        return Ok(());
    };
    let used = repo_disk_usage_bytes(git_dir).await.map_err(|e| {
        tracing::error!(error = %e, path = %git_dir.display(), "measure repo disk usage failed");
        AppError::new("repo.internal", "repository operation failed")
    })?;
    if used > quota {
        return Err(AppError::new(
            "repo.size_quota_exceeded",
            "This repository is over its git storage quota. Contact an instance admin.",
        )
        .with_data(serde_json::json!({
            "size_bytes": used,
            "quota_bytes": quota,
        })));
    }
    Ok(())
}
