//! Orphan disk purge + soft-delete retention + path-safe deletes (D-36, T-07-26).

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use oxidean_db::Database;
use oxidean_git::GitBackend;

use crate::git::bare_repo_path;

/// Default soft-delete retention (A4 RESEARCH / D-36).
pub const DEFAULT_SOFT_DELETE_RETENTION_DAYS: u64 = 14;

pub fn soft_delete_retention_days() -> u64 {
    std::env::var("OXIDEAN_SOFT_DELETE_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SOFT_DELETE_RETENTION_DAYS)
}

/// True when `path` is under canonical `root` (boundary-safe; not string prefix).
pub fn path_is_under(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root).is_ok()
}

/// Remove a file or directory only if it canonicalizes under `repos_dir` (T-07-26).
pub async fn delete_under_repos_dir(repos_dir: &Path, target: &Path) -> Result<(), String> {
    if !target.exists() {
        return Ok(());
    }
    let root = tokio::fs::canonicalize(repos_dir)
        .await
        .map_err(|e| format!("canonicalize repos_dir: {e}"))?;
    let canon = tokio::fs::canonicalize(target)
        .await
        .map_err(|e| format!("canonicalize target: {e}"))?;
    if !path_is_under(&canon, &root) {
        return Err(format!(
            "refusing to delete path outside repos_dir: {} (root {})",
            canon.display(),
            root.display()
        ));
    }
    let meta = tokio::fs::metadata(&canon)
        .await
        .map_err(|e| format!("metadata: {e}"))?;
    if meta.is_dir() {
        tokio::fs::remove_dir_all(&canon)
            .await
            .map_err(|e| format!("remove_dir_all {}: {e}", canon.display()))?;
    } else {
        tokio::fs::remove_file(&canon)
            .await
            .map_err(|e| format!("remove_file {}: {e}", canon.display()))?;
    }
    Ok(())
}

fn parse_deleted_at(s: &str) -> Option<DateTime<Utc>> {
    let trimmed = s.trim();
    // Prefer RFC3339; SQLite may emit without fractional seconds.
    DateTime::parse_from_rfc3339(trimmed)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%SZ")
                .ok()
                .map(|n| DateTime::from_naive_utc_and_offset(n, Utc))
        })
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|n| DateTime::from_naive_utc_and_offset(n, Utc))
        })
}

fn disk_key(owner: &str, name: &str) -> String {
    format!("{owner}/{name}.git")
}

/// Purge expired soft-deletes and remove bare dirs with no matching DB row.
pub async fn orphan_reconcile(db: &Database, repos_dir: &Path) -> Result<ReconcileStats, String> {
    orphan_reconcile_with_retention(db, repos_dir, soft_delete_retention_days()).await
}

/// Same as [`orphan_reconcile`] with an explicit retention window (tests / operators).
pub async fn orphan_reconcile_with_retention(
    db: &Database,
    repos_dir: &Path,
    retention_days: u64,
) -> Result<ReconcileStats, String> {
    let mut stats = ReconcileStats::default();
    if db.is_skipped() {
        return Ok(stats);
    }
    if !repos_dir.exists() {
        return Ok(stats);
    }

    let retention = Duration::from_secs(retention_days.saturating_mul(24 * 3600));
    let now = Utc::now();
    let refs = db.list_repo_disk_refs().await?;

    // 1) Soft-deleted past retention → disk purge + hard delete.
    for row in &refs {
        let Some(deleted_at) = row.deleted_at.as_deref() else {
            continue;
        };
        let Some(when) = parse_deleted_at(deleted_at) else {
            tracing::warn!(
                repo_id = %row.id,
                deleted_at,
                "orphan reconcile: unparseable deleted_at; skipping retention purge"
            );
            continue;
        };
        if now
            .signed_duration_since(when)
            .to_std()
            .unwrap_or(Duration::ZERO)
            < retention
        {
            continue;
        }
        let path = bare_repo_path(repos_dir, &row.owner_username, &row.name)
            .map_err(|e| e.message.clone())?;
        if let Err(e) = delete_under_repos_dir(repos_dir, &path).await {
            tracing::error!(error = %e, path = %path.display(), "soft-delete disk purge failed");
            stats.errors += 1;
            continue;
        }
        if let Err(e) = db.hard_delete_repository(&row.id).await {
            tracing::error!(error = %e, repo_id = %row.id, "hard-delete after purge failed");
            stats.errors += 1;
            continue;
        }
        stats.purged_soft_deleted += 1;
    }

    // Refresh known keys after hard deletes.
    let refs = db.list_repo_disk_refs().await?;
    let known: HashSet<String> = refs
        .iter()
        .map(|r| disk_key(&r.owner_username, &r.name))
        .collect();

    // 2) Orphan bare dirs on disk with no DB row (active or soft-deleted).
    let root = tokio::fs::canonicalize(repos_dir)
        .await
        .map_err(|e| format!("canonicalize repos_dir: {e}"))?;

    let mut owners = tokio::fs::read_dir(&root)
        .await
        .map_err(|e| format!("read_dir repos_dir: {e}"))?;
    while let Some(owner_ent) = owners
        .next_entry()
        .await
        .map_err(|e| format!("owner entry: {e}"))?
    {
        let owner_ft = owner_ent
            .file_type()
            .await
            .map_err(|e| format!("owner file_type: {e}"))?;
        if !owner_ft.is_dir() {
            continue;
        }
        let owner_name = owner_ent.file_name();
        let owner_str = owner_name.to_string_lossy();
        if owner_str == "." || owner_str == ".." || owner_str.contains('/') {
            continue;
        }
        let owner_path = owner_ent.path();
        let mut repos = tokio::fs::read_dir(&owner_path)
            .await
            .map_err(|e| format!("read_dir owner: {e}"))?;
        while let Some(repo_ent) = repos
            .next_entry()
            .await
            .map_err(|e| format!("repo entry: {e}"))?
        {
            let name_os = repo_ent.file_name();
            let name = name_os.to_string_lossy();
            if !name.ends_with(".git") {
                continue;
            }
            let key = format!("{owner_str}/{name}");
            if known.contains(&key) {
                continue;
            }
            let path = repo_ent.path();
            match delete_under_repos_dir(repos_dir, &path).await {
                Ok(()) => stats.orphans_removed += 1,
                Err(e) => {
                    tracing::error!(error = %e, path = %path.display(), "orphan delete failed");
                    stats.errors += 1;
                }
            }
        }
        // Best-effort: remove empty owner directories.
        if let Ok(mut remaining) = tokio::fs::read_dir(&owner_path).await {
            if remaining.next_entry().await.ok().flatten().is_none() {
                let _ = delete_under_repos_dir(repos_dir, &owner_path).await;
            }
        }
    }

    // 3) Purge expired repository redirects (D-REL-08).
    let now_rfc = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    match db.purge_expired_repository_redirects(&now_rfc).await {
        Ok(n) => stats.purged_expired_redirects = n as u32,
        Err(e) => {
            tracing::error!(error = %e, "purge expired repository redirects failed");
            stats.errors += 1;
        }
    }

    Ok(stats)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReconcileStats {
    pub orphans_removed: u32,
    pub purged_soft_deleted: u32,
    pub purged_expired_redirects: u32,
    pub errors: u32,
}

/// Run `git gc` on one active bare repo path (must be under repos_dir).
pub async fn run_gc_one(git: &dyn GitBackend, repos_dir: &Path, path: &Path) -> Result<(), String> {
    let root = tokio::fs::canonicalize(repos_dir)
        .await
        .map_err(|e| format!("canonicalize repos_dir: {e}"))?;
    if !path.exists() {
        return Err(format!("repo path missing: {}", path.display()));
    }
    let canon = tokio::fs::canonicalize(path)
        .await
        .map_err(|e| format!("canonicalize repo: {e}"))?;
    if !path_is_under(&canon, &root) {
        return Err(format!(
            "refusing gc outside repos_dir: {}",
            canon.display()
        ));
    }
    git.gc(&canon)
        .await
        .map_err(|e| format!("git gc failed: {e}"))
}

/// GC every active (non-deleted) repository under `repos_dir`.
pub async fn run_gc_all(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
) -> Result<(u32, u32), String> {
    if db.is_skipped() {
        return Ok((0, 0));
    }
    let refs = db.list_repo_disk_refs().await?;
    let mut ok = 0u32;
    let mut err = 0u32;
    for row in refs.into_iter().filter(|r| r.deleted_at.is_none()) {
        let path = match bare_repo_path(repos_dir, &row.owner_username, &row.name) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e.message, "gc skip invalid path");
                err += 1;
                continue;
            }
        };
        match run_gc_one(git, repos_dir, &path).await {
            Ok(()) => ok += 1,
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "scheduled gc failed");
                err += 1;
            }
        }
    }
    Ok((ok, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn delete_under_repos_dir_refuses_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let repos = tmp.path().join("repos");
        let outside = tmp.path().join("outside");
        tokio::fs::create_dir_all(&repos).await.unwrap();
        tokio::fs::create_dir_all(&outside).await.unwrap();
        tokio::fs::write(outside.join("x"), b"x").await.unwrap();
        let err = delete_under_repos_dir(&repos, &outside.join("x"))
            .await
            .expect_err("must refuse");
        assert!(err.contains("outside"), "{err}");
    }

    #[tokio::test]
    async fn orphan_reconcile_removes_disk_without_db_row() {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", dir.path().join("orphan.db").display());
        let repos = dir.path().join("repos");
        let orphan = repos.join("ghost").join("leftover.git");
        tokio::fs::create_dir_all(&orphan).await.unwrap();
        tokio::fs::write(orphan.join("HEAD"), b"ref: refs/heads/main\n")
            .await
            .unwrap();

        let db = Database::connect(&url).await.unwrap();
        db.migrate().await.unwrap();

        let stats = orphan_reconcile(&db, &repos).await.expect("reconcile");
        assert_eq!(stats.orphans_removed, 1);
        assert!(!orphan.exists());
    }
}
