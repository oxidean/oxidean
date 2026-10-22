//! Periodic LFS OID GC — delete unreferenced objects after grace (D-LFS-15).

use std::path::Path;
use std::time::Duration;

use oxidean_db::Database;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::lfs::store;

/// Default GC interval: 24h. `OXIDEAN_LFS_GC_INTERVAL_SECS=0` disables.
pub const DEFAULT_GC_INTERVAL_SECS: u64 = 86_400;
/// Default grace before deleting unreferenced OIDs: 7 days.
pub const DEFAULT_GC_GRACE_SECS: u64 = 7 * 86_400;

pub fn gc_grace_from_env() -> Duration {
    let secs = std::env::var("OXIDEAN_LFS_GC_GRACE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_GC_GRACE_SECS);
    Duration::from_secs(secs)
}

#[derive(Debug, Default)]
pub struct LfsGcStats {
    pub deleted: u64,
    pub skipped_locked: bool,
    pub errors: u64,
}

/// Run one GC pass: lock `LFS_DIR/.gc.lock`, delete OIDs with no links older than grace.
pub async fn run_lfs_gc(
    db: &Database,
    lfs_dir: &Path,
    grace: Duration,
) -> Result<LfsGcStats, String> {
    let mut stats = LfsGcStats::default();
    if !lfs_dir.exists() {
        return Ok(stats);
    }
    let lock_path = lfs_dir.join(".gc.lock");
    let lock = match try_acquire_lock(&lock_path).await {
        Ok(Some(f)) => f,
        Ok(None) => {
            stats.skipped_locked = true;
            return Ok(stats);
        }
        Err(e) => return Err(e),
    };

    let cutoff_s = if grace.is_zero() {
        "9999-12-31 23:59:59".to_string()
    } else {
        let cutoff = chrono::Utc::now()
            - chrono::Duration::from_std(grace).unwrap_or(chrono::Duration::days(7));
        cutoff.format("%Y-%m-%d %H:%M:%S").to_string()
    };

    let oids = db.list_unreferenced_lfs_oids(&cutoff_s).await?;
    for oid in oids {
        match store::delete_object(lfs_dir, &oid).await {
            Ok(()) => {}
            Err(e) => {
                tracing::warn!(error = %e, %oid, "lfs gc disk delete failed");
                stats.errors += 1;
                continue;
            }
        }
        match db.delete_lfs_object(&oid).await {
            Ok(()) => stats.deleted += 1,
            Err(e) => {
                tracing::warn!(error = %e, %oid, "lfs gc db delete failed");
                stats.errors += 1;
            }
        }
    }

    drop(lock);
    let _ = tokio::fs::remove_file(&lock_path).await;
    Ok(stats)
}

async fn try_acquire_lock(path: &Path) -> Result<Option<tokio::fs::File>, String> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
    {
        Ok(mut f) => {
            let pid = std::process::id();
            let _ = f.write_all(format!("{pid}\n").as_bytes()).await;
            Ok(Some(f))
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
        Err(e) => Err(format!("lfs gc lock: {e}")),
    }
}

/// Wipe children of `lfs_dir` (keep root). Used by factory reset (D-LFS-04).
pub async fn wipe_lfs_dir_contents(lfs_dir: &Path) -> Result<(), String> {
    if !lfs_dir.exists() {
        return Ok(());
    }
    let root = tokio::fs::canonicalize(lfs_dir)
        .await
        .map_err(|e| format!("canonicalize lfs_dir: {e}"))?;
    let mut entries = tokio::fs::read_dir(&root)
        .await
        .map_err(|e| format!("read_dir lfs_dir: {e}"))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("lfs_dir entry: {e}"))?
    {
        let path = entry.path();
        let canon = match tokio::fs::canonicalize(&path).await {
            Ok(p) => p,
            Err(_) => path.clone(),
        };
        if !canon.starts_with(&root) {
            return Err(format!(
                "refusing to delete outside lfs_dir: {}",
                path.display()
            ));
        }
        let ft = entry
            .file_type()
            .await
            .map_err(|e| format!("file_type: {e}"))?;
        if ft.is_dir() {
            tokio::fs::remove_dir_all(&path)
                .await
                .map_err(|e| format!("remove_dir_all {}: {e}", path.display()))?;
        } else {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| format!("remove_file {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wipe_keeps_root() {
        let tmp = tempfile::tempdir().unwrap();
        let lfs = tmp.path().join("lfs");
        tokio::fs::create_dir_all(lfs.join("ab").join("cd"))
            .await
            .unwrap();
        tokio::fs::write(lfs.join("ab").join("cd").join("obj"), b"x")
            .await
            .unwrap();
        wipe_lfs_dir_contents(&lfs).await.unwrap();
        assert!(lfs.is_dir());
        assert!(tokio::fs::read_dir(&lfs)
            .await
            .unwrap()
            .next_entry()
            .await
            .unwrap()
            .is_none());
    }
}
