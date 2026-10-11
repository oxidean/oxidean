//! Fork synchronization (GIT-24) — `repo.forkStatus` + `repo.syncFork`.
//!
//! GitHub "Sync fork" parity: report how the fork branch diverges from the
//! same-named branch on its upstream parent (`forkStatus`), and bring it up to
//! date (`syncFork`). Sync fast-forwards when the fork has no unique commits and
//! otherwise creates a merge commit of the upstream tip into the fork branch;
//! unresolvable merges surface `repo.sync_conflict` rather than touching history.

use std::path::Path;

use oxidean_core::{
    AppError, RepoForkStatusRequest, RepoForkStatusResponse, RepoSyncForkRequest,
    RepoSyncForkResponse,
};
use oxidean_db::{RepositoryRow, UserRow};

use crate::auth::gate::require_verified;
use crate::git::{bare_repo_path, branch_tip};
use crate::protection::{effective_for_branch, evaluate_push, ProtectionIntent};
use crate::repo::{owner_ref_for_repo, AccessibleRepo, OwnerRef};
use crate::rpc::RpcCtx;

/// Upstream parent repo + owner slug for a fork. `repo.not_fork` when the row
/// has no recorded parent (also covers non-forks — `forked_from` is only set on
/// forks); a dangling parent → `repo.upstream_unavailable`.
async fn upstream_for_fork(
    ctx: &RpcCtx,
    fork: &AccessibleRepo,
) -> Result<(RepositoryRow, OwnerRef), AppError> {
    let parent_id = ctx
        .db
        .get_repo_forked_from(&fork.row.id)
        .await
        .map_err(super::db_err)?
        .ok_or_else(|| AppError::new("repo.not_fork", "repository is not a fork"))?;
    let parent = ctx
        .db
        .find_repository_by_id(&parent_id)
        .await
        .map_err(super::db_err)?
        .filter(|r| r.deleted_at.is_none())
        .ok_or_else(|| {
            AppError::new("repo.upstream_unavailable", "upstream repository is unavailable")
        })?;
    let owner = owner_ref_for_repo(&ctx.db, &parent)
        .await
        .map_err(super::db_err)?
        .ok_or_else(|| {
            AppError::new("repo.upstream_unavailable", "upstream repository is unavailable")
        })?;
    Ok((parent, owner))
}

/// Branch to sync: request override or the fork's default branch.
fn sync_branch(raw: Option<&str>, default_branch: &str) -> Result<String, AppError> {
    let chosen = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default_branch);
    if chosen.is_empty()
        || chosen.contains('\0')
        || chosen.contains("..")
        || chosen.starts_with('-')
    {
        return Err(AppError::new(
            "repo.invalid_ref",
            format!("invalid branch name: {chosen}"),
        ));
    }
    Ok(chosen.to_string())
}

/// Fork branch vs upstream tip after fetching upstream objects into the fork.
struct ForkSyncState {
    /// Fork-side branch tip — `None` when the branch does not exist locally.
    fork_sha: Option<String>,
    /// Upstream same-named branch tip.
    upstream_sha: String,
    /// Commits on the fork branch that upstream lacks.
    ahead: u64,
    /// Commits on the upstream branch the fork branch lacks.
    behind: u64,
}

async fn fork_sync_state(
    ctx: &RpcCtx,
    fork_bare: &Path,
    upstream_bare: &Path,
    branch: &str,
) -> Result<ForkSyncState, AppError> {
    // Local-path fetch — no credentials — pulls upstream objects into the fork's
    // odb so `ahead_behind` / merges can see them (same pattern as pull.files).
    let upstream_sha = ctx
        .git
        .fetch_ref_from(fork_bare, upstream_bare, &format!("refs/heads/{branch}"))
        .await
        .map_err(|_| {
            AppError::new(
                "repo.upstream_branch_not_found",
                format!("upstream branch not found: {branch}"),
            )
        })?;
    let fork_sha = branch_tip(ctx.git.as_ref(), fork_bare, branch).await?;
    let (ahead, behind) = match &fork_sha {
        Some(tip) if tip == &upstream_sha => (0, 0),
        Some(tip) => ctx
            .git
            .ahead_behind(fork_bare, tip, &upstream_sha)
            .await
            .map_err(|e| {
                AppError::new("repo.sync_failed", format!("divergence check failed: {e}"))
            })?,
        // Missing fork branch: everything upstream has is "behind".
        None => (
            0,
            ctx.git
                .rev_list_count(fork_bare, &upstream_sha)
                .await
                .unwrap_or(0),
        ),
    };
    Ok(ForkSyncState {
        fork_sha,
        upstream_sha,
        ahead,
        behind,
    })
}

/// Fork-only "ahead" reads as `up_to_date` — there is nothing upstream to bring
/// down (GitHub reports the same on the Sync fork card).
fn status_for(ahead: u64, behind: u64) -> &'static str {
    if behind == 0 {
        "up_to_date"
    } else if ahead == 0 {
        "behind"
    } else {
        "diverged"
    }
}

/// `repo.forkStatus` — Read+ on the fork; live divergence vs upstream (GIT-24).
/// Anonymous/read-only callers get the public divergence; `can_write` is implied
/// by the caller's capability (the UI hides the button when repo.can_write is false).
pub async fn fork_status(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoForkStatusResponse, AppError> {
    let req: RepoForkStatusRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.forkStatus input: {e}"),
        )
    })?;
    let accessible = super::resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let (parent, upstream_owner) = upstream_for_fork(ctx, &accessible).await?;
    let branch = sync_branch(req.branch.as_deref(), &accessible.row.default_branch)?;

    let fork_bare = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let upstream_bare = bare_repo_path(&ctx.repos_dir, upstream_owner.slug(), &parent.name)?;
    let state = fork_sync_state(ctx, &fork_bare, &upstream_bare, &branch).await?;

    Ok(RepoForkStatusResponse {
        branch: branch.clone(),
        upstream_owner: upstream_owner.slug().to_string(),
        upstream_name: parent.name,
        upstream_branch: branch,
        ahead_count: state.ahead as i64,
        behind_count: state.behind as i64,
        status: status_for(state.ahead, state.behind).to_string(),
    })
}

/// `repo.syncFork` — Write+ on the **fork**; fast-forward the branch to the
/// upstream tip, or merge upstream in when the histories diverged (GIT-24).
/// Branch protection still applies to the target branch — the API pre-evaluates
/// a Push intent so the same hook denial is surfaced before any ref write.
pub async fn sync_fork(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoSyncForkResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: RepoSyncForkRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid repo.syncFork input: {e}")))?;
    let accessible = super::resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let (parent, upstream_owner) = upstream_for_fork(ctx, &accessible).await?;
    let branch = sync_branch(req.branch.as_deref(), &accessible.row.default_branch)?;

    let fork_bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let upstream_bare = bare_repo_path(&ctx.repos_dir, upstream_owner.slug(), &parent.name)?;
    let state = fork_sync_state(ctx, &fork_bare, &upstream_bare, &branch).await?;
    let before_sha = state.fork_sha.clone().unwrap_or_default();

    if state.behind == 0 {
        // Includes fork-only "ahead" — nothing to bring down from upstream.
        return Ok(RepoSyncForkResponse {
            status: "up_to_date".to_string(),
            branch: branch.clone(),
            upstream_owner: upstream_owner.slug().to_string(),
            upstream_name: parent.name.clone(),
            upstream_branch: branch,
            before_sha: before_sha.clone(),
            after_sha: before_sha,
            merge_commit_sha: None,
        });
    }

    let eff = effective_for_branch(&ctx.db, &accessible.row.id, &branch).await?;
    evaluate_push(&eff, ProtectionIntent::Push, accessible.capability)?;

    let (status, after_sha, merge_sha) = if state.ahead == 0 {
        ctx.git
            .fast_forward_ref(
                &fork_bare,
                &format!("refs/heads/{branch}"),
                &state.upstream_sha,
            )
            .await
            .map_err(|e| AppError::new("repo.sync_failed", format!("sync failed: {e}")))?;
        ("fast_forwarded", state.upstream_sha.clone(), None)
    } else {
        // Diverged: merging upstream in requires merge commits to be enabled on
        // the fork — otherwise the caller resolves divergence themselves.
        let settings = ctx
            .db
            .get_repo_merge_settings(&accessible.row.id)
            .await
            .map_err(super::db_err)?;
        if !settings.allow_merge_commit {
            return Err(AppError::new(
                "repo.sync_diverged",
                "branches have diverged and merge commits are disabled for this repository",
            ));
        }
        let message = format!(
            "Merge branch '{branch}' of {}/{} into {branch}",
            upstream_owner.slug(),
            parent.name
        );
        let sha = ctx
            .git
            .merge_commit(&fork_bare, &branch, &state.upstream_sha, &message)
            .await
            .map_err(|e| {
                let msg = e.to_string().to_lowercase();
                if msg.contains("conflict") {
                    AppError::new("repo.sync_conflict", "merge conflict syncing with upstream")
                } else {
                    AppError::new("repo.sync_failed", format!("sync failed: {e}"))
                }
            })?;
        ("merged", sha.clone(), Some(sha))
    };

    // Internal writes skip the push path — record activity + mirrors + move open
    // PR heads the way synchronize_after_push would.
    super::record_ref_updates(
        &ctx.db,
        &accessible.row.id,
        &user.id,
        &[(
            before_sha.clone(),
            after_sha.clone(),
            format!("refs/heads/{branch}"),
        )],
        Some(ctx.git.clone()),
        Some(&fork_bare),
    )
    .await;
    crate::mirror::notify_mirror_after_local_mutation(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        accessible.row.id.clone(),
    );
    sync_fork_pr_heads(ctx, &accessible, &branch, &after_sha, &user).await;

    Ok(RepoSyncForkResponse {
        status: status.to_string(),
        branch: branch.clone(),
        upstream_owner: upstream_owner.slug().to_string(),
        upstream_name: parent.name,
        upstream_branch: branch,
        before_sha,
        after_sha,
        merge_commit_sha: merge_sha,
    })
}

/// Open PRs whose head is this fork branch move to the new tip — same
/// `synchronize` bookkeeping a push would perform. Heads may belong to PRs
/// hosted on the fork-network root (upstream PRs) or on the fork itself.
async fn sync_fork_pr_heads(
    ctx: &RpcCtx,
    fork: &AccessibleRepo,
    branch: &str,
    new_head_sha: &str,
    user: &UserRow,
) {
    let Some(network_id) = ctx
        .db
        .get_repo_fork_network_id(&fork.row.id)
        .await
        .ok()
        .flatten()
    else {
        return;
    };
    // Forks always point at a network root != own id, so the two listings are
    // disjoint sets (no dedupe needed).
    let mut open: Vec<oxidean_db::PullRow> = Vec::new();
    for repo_id in [&network_id, &fork.row.id] {
        match ctx
            .db
            .list_pulls_for_repo(repo_id, Some("open"), 0, 500)
            .await
        {
            Ok((pulls, _)) => open.extend(pulls),
            Err(e) => {
                tracing::warn!(error = %e, repo_id, "sync_fork: list pulls failed (soft-fail)");
            }
        }
    }
    for pull in open.into_iter().filter(|p| {
        p.head_repo_id == fork.row.id && p.head_ref == branch && p.head_sha != new_head_sha
    }) {
        let Ok(Some(base_repo)) = ctx.db.find_repository_by_id(&pull.repo_id).await else {
            continue;
        };
        let Ok(Some(base_owner)) = owner_ref_for_repo(&ctx.db, &base_repo).await else {
            continue;
        };
        if let Err(e) = crate::pull::synchronize_pull_after_head_move(
            &ctx.db,
            &ctx.repos_dir,
            ctx.git.as_ref(),
            &pull,
            base_owner.slug(),
            &base_repo.name,
            &fork.owner_username,
            &fork.row.name,
            new_head_sha,
            &user.username,
            &user.id,
            &ctx.env_name,
        )
        .await
        {
            tracing::warn!(
                error = %e,
                pull = pull.number,
                "sync_fork: PR head synchronize failed (soft-fail)"
            );
        }
    }
}
