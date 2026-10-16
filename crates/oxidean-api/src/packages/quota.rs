//! Package storage quotas (D-PKG-09).

use oxidean_db::Database;

/// Default per-owner quota (10 GiB) when unset.
pub const DEFAULT_OWNER_QUOTA_BYTES: u64 = 10 * 1024 * 1024 * 1024;

pub fn owner_quota_default_from_env() -> u64 {
    std::env::var("OXIDEAN_PACKAGES_OWNER_QUOTA_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_OWNER_QUOTA_BYTES)
}

pub async fn effective_owner_quota(
    db: &Database,
    owner_type: &str,
    owner_id: &str,
) -> Result<u64, String> {
    if let Some(max) = db.find_package_quota_override(owner_type, owner_id).await? {
        return Ok(max as u64);
    }
    Ok(owner_quota_default_from_env())
}

pub async fn owner_usage_bytes(
    db: &Database,
    owner_type: &str,
    owner_id: &str,
) -> Result<u64, String> {
    db.sum_package_blob_bytes_for_owner(owner_type, owner_id)
        .await
        .map(|n| n as u64)
}

/// Returns Err message if adding `additional` would exceed quota.
pub async fn check_can_store(
    db: &Database,
    owner_type: &str,
    owner_id: &str,
    additional: u64,
) -> Result<(), String> {
    let quota = effective_owner_quota(db, owner_type, owner_id).await?;
    let used = owner_usage_bytes(db, owner_type, owner_id).await?;
    if used.saturating_add(additional) > quota {
        return Err(format!(
            "owner package storage quota exceeded ({used} + {additional} > {quota})"
        ));
    }
    Ok(())
}

/// Delete CA blobs with refcount 0 older than grace (default 7d). Returns count removed.
pub async fn gc_unref_blobs(db: &Database, packages_dir: &std::path::Path) -> Result<u64, String> {
    let grace_secs: i64 = std::env::var("OXIDEAN_PACKAGES_GC_GRACE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7 * 24 * 3600);
    let digests = db.list_unref_package_blobs(grace_secs).await?;
    let mut n = 0u64;
    for d in digests {
        if let Ok(path) = crate::packages::store::blob_path(packages_dir, &d) {
            let _ = std::fs::remove_file(path);
        }
        db.delete_package_blob(&d).await?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_quota_defaults() {
        assert_eq!(DEFAULT_OWNER_QUOTA_BYTES, 10 * 1024 * 1024 * 1024);
    }
}
