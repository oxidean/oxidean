//! `refs/pull/{N}/*` synthesized refs for GitHub-compatible tooling (API-06).
//!
//! Pull refs are **real refs** written directly into the base repository's
//! bare dir via `git update-ref` (no hooks run — `update-ref` bypasses
//! `hooks/update`). Smart HTTP and SSH `upload-pack` then advertise them, so
//! CI/deploy bots can `git fetch origin pull/123/head` unchanged. The
//! namespace is read-only: pushes targeting `refs/pull/*` are denied at the
//! Smart HTTP edge, inside the SSH pack bridge, and (when installed) by
//! `hooks/update` via [`crate::protection::check_ref_update`].
//!
//! - `refs/pull/{N}/head` — the PR's current head tip (`pulls.head_sha`).
//! - `refs/pull/{N}/merge` — written once the PR merges (the recorded merge
//!   commit). No speculative test-merge commit is computed while open, so the
//!   ref simply does not exist until then (documented in docs/API.md).
//!
//! Refs are best-effort: a failed write never fails the PR mutation.

use std::path::Path;

use oxidean_db::{Database, PullRow};
use oxidean_git::GitBackend;

use crate::git::bare_repo_path;

/// GitHub-conventional pull head ref (API-06).
pub fn head_ref_name(number: i64) -> String {
    format!("refs/pull/{number}/head")
}

/// GitHub-conventional pull merge ref (API-06). Exists only post-merge.
pub fn merge_ref_name(number: i64) -> String {
    format!("refs/pull/{number}/merge")
}

/// True for any synthesized pull ref (`head`, `merge`, or future suffixes).
pub fn is_pull_ref(refname: &str) -> bool {
    refname.starts_with("refs/pull/")
}

/// Resolve a repo row's `{owner}/{name}` disk coords (`org` → slug, else user).
pub(crate) async fn repo_disk_coords(db: &Database, repo_id: &str) -> Option<(String, String)> {
    let repo = db.find_repository_by_id(repo_id).await.ok()??;
    let owner = match repo.owner_type.as_str() {
        "org" => db
            .find_organization_by_id(&repo.owner_id)
            .await
            .ok()?
            .map(|o| o.slug)?,
        _ => db
            .find_user_by_id(&repo.owner_id)
            .await
            .ok()?
            .map(|u| u.username)?,
    };
    Some((owner, repo.name))
}

/// Point the base repo's `refs/pull/{N}/head` at `pull.head_sha` (API-06).
///
/// Fork heads are first fetched into the base repo so the ref resolves to a
/// present object (same pattern as `pull.files` / `pull.merge`). Failures are
/// logged and swallowed — PR writes must not depend on ref synthesis.
pub(crate) async fn sync_head_ref(
    git: &dyn GitBackend,
    repos_dir: &Path,
    db: &Database,
    base_owner: &str,
    base_name: &str,
    pull: &PullRow,
) {
    let base_bare = match bare_repo_path(repos_dir, base_owner, base_name) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e.message, pull = %pull.id, "pull refs: bad base path");
            return;
        }
    };
    if pull.head_repo_id != pull.repo_id {
        let Some((head_owner, head_name)) = repo_disk_coords(db, &pull.head_repo_id).await else {
            tracing::warn!(pull = %pull.id, "pull refs: head repo row missing");
            return;
        };
        let head_bare = match bare_repo_path(repos_dir, &head_owner, &head_name) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e.message, pull = %pull.id, "pull refs: bad head path");
                return;
            }
        };
        if let Err(e) = git
            .fetch_ref_from(&base_bare, &head_bare, &pull.head_sha)
            .await
        {
            tracing::warn!(error = %e, pull = %pull.id, "pull refs: fork head fetch failed");
            return;
        }
    }
    if let Err(e) = git
        .update_ref(&base_bare, &head_ref_name(pull.number), &pull.head_sha)
        .await
    {
        tracing::warn!(error = %e, pull = %pull.id, "pull refs: head ref write failed");
    }
}

/// Write `refs/pull/{N}/merge` → `merge_sha` after a successful merge (API-06).
/// The merge commit is already an object in the base repo (written by the
/// worktree merge push), so no fetch is needed. Best-effort.
pub(crate) async fn sync_merge_ref(
    git: &dyn GitBackend,
    repos_dir: &Path,
    base_owner: &str,
    base_name: &str,
    number: i64,
    merge_sha: &str,
) {
    let Ok(base_bare) = bare_repo_path(repos_dir, base_owner, base_name) else {
        return;
    };
    if let Err(e) = git
        .update_ref(&base_bare, &merge_ref_name(number), merge_sha)
        .await
    {
        tracing::warn!(error = %e, "pull refs: merge ref write failed");
    }
}
