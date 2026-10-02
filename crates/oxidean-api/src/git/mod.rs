//! API-side git helpers: path layout under `OXIDEAN_REPOS_DIR` (D-30 / D-31).
//!
//! All process invocation lives in `oxidean-git` (`CliGitBackend`). This module
//! only builds safe on-disk paths under the configured repos root — except Smart
//! HTTP CGI in [`http_backend`].

pub mod http_backend;
pub mod quota;
pub mod web_flow;

use std::path::{Path, PathBuf};

use oxidean_core::AppError;

/// Bare repo path: `{repos_dir}/{owner}/{name}.git` (D-30).
///
/// `owner` and `name` must already be validated (username / `validate_repo_name`).
pub fn bare_repo_path(repos_dir: &Path, owner: &str, name: &str) -> Result<PathBuf, AppError> {
    if owner.is_empty()
        || name.is_empty()
        || owner.contains('/')
        || owner.contains('\\')
        || owner.contains("..")
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
    {
        return Err(AppError::new(
            "repo.invalid_path",
            "invalid repository path components",
        ));
    }
    Ok(repos_dir.join(owner).join(format!("{name}.git")))
}

fn validate_owner_segment(owner: &str) -> Result<(), AppError> {
    if owner.is_empty()
        || owner.contains('/')
        || owner.contains('\\')
        || owner.contains("..")
    {
        return Err(AppError::new(
            "repo.invalid_path",
            "invalid repository path components",
        ));
    }
    Ok(())
}

/// Rename `{repos_dir}/{old_username}` → `{repos_dir}/{new_username}` when a user
/// changes username. No-op if the old directory is missing (no repos yet).
///
/// Must run **before** committing the DB username change so orphan reconcile does
/// not delete live data under the old path.
pub async fn rename_owner_repos_dir(
    repos_dir: &Path,
    old_username: &str,
    new_username: &str,
) -> Result<(), AppError> {
    if old_username == new_username {
        return Ok(());
    }
    validate_owner_segment(old_username)?;
    validate_owner_segment(new_username)?;

    let old_dir = repos_dir.join(old_username);
    let new_dir = repos_dir.join(new_username);

    if !tokio::fs::try_exists(&old_dir).await.unwrap_or(false) {
        return Ok(());
    }

    if tokio::fs::try_exists(&new_dir).await.unwrap_or(false) {
        return Err(AppError::new(
            "repo.owner_dir_conflict",
            "cannot rename username: destination owner directory already exists on disk",
        ));
    }

    tokio::fs::rename(&old_dir, &new_dir).await.map_err(|e| {
        tracing::error!(
            old = %old_dir.display(),
            new = %new_dir.display(),
            error = %e,
            "failed to rename owner repos directory"
        );
        AppError::new(
            "repo.owner_dir_rename_failed",
            "could not move repositories to the new username",
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rename_owner_repos_dir_moves_directory() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("alice");
        let repo = old.join("hello.git");
        tokio::fs::create_dir_all(&repo).await.unwrap();
        tokio::fs::write(repo.join("HEAD"), "ref: refs/heads/main\n")
            .await
            .unwrap();

        rename_owner_repos_dir(root.path(), "alice", "bob")
            .await
            .unwrap();

        assert!(!old.exists());
        assert!(root
            .path()
            .join("bob")
            .join("hello.git")
            .join("HEAD")
            .exists());
    }

    #[tokio::test]
    async fn rename_owner_repos_dir_noop_when_missing() {
        let root = tempfile::tempdir().unwrap();
        rename_owner_repos_dir(root.path(), "ghost", "next")
            .await
            .unwrap();
        assert!(!root.path().join("next").exists());
    }

    #[tokio::test]
    async fn rename_owner_repos_dir_conflicts() {
        let root = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(root.path().join("alice").join("a.git"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(root.path().join("bob"))
            .await
            .unwrap();
        let err = rename_owner_repos_dir(root.path(), "alice", "bob")
            .await
            .unwrap_err();
        assert_eq!(err.code, "repo.owner_dir_conflict");
    }
}
