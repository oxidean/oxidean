//! `pull.branchStatus` + `pull.updateBranch` (GIT-24) — GitHub "Update branch"
//! parity: merge the base branch into the PR head branch so the PR is no
//! longer behind.

use std::path::{Path, PathBuf};

use oxidean_core::{AppError, PullBranchStatusResponse, PullRefRequest, UpdatePullBranchResponse};
use oxidean_db::{PullRow, RepositoryRow};

use crate::auth::gate::require_verified;
use crate::git::{bare_repo_path, branch_tip};
use crate::protection::{effective_for_branch, evaluate_push, ProtectionIntent};
use crate::repo::{
    effective_capability, meets, not_found, owner_ref_for_repo, AccessibleRepo, Capability,
};
use crate::rpc::RpcCtx;

use super::{acl, db_err, load_pull_in_repo, synchronize_pull_after_head_move, to_public};

/// Head repo row + owner slug + caller's effective capability on it.
/// `pull.head_unavailable` when the head repo (or its owner) is gone.
async fn resolve_head(
    ctx: &RpcCtx,
    base: &AccessibleRepo,
    row: &PullRow,
) -> Result<(RepositoryRow, String, Option<Capability>), AppError> {
    if row.head_repo_id == base.row.id {
        return Ok((base.row.clone(), base.owner_username.clone(), base.capability));
    }
    let head = ctx
        .db
        .find_repository_by_id(&row.head_repo_id)
        .await
        .map_err(db_err)?
        .filter(|r| r.deleted_at.is_none())
        .ok_or_else(|| AppError::new("pull.head_unavailable", "head repository is unavailable"))?;
    let owner = owner_ref_for_repo(&ctx.db, &head)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("pull.head_unavailable", "head repository is unavailable"))?;
    let caller = ctx.session.as_ref().map(|s| s.user_id.as_str());
    let cap = effective_capability(&ctx.db, caller, &head, &owner)
        .await
        .map_err(db_err)?;
    Ok((head, owner.slug().to_string(), cap))
}

/// Live tips `(base_sha, head_sha)`. For cross-repo PRs the base objects are
/// fetched into the head bare first (local-path fetch — pull.files precedent)
/// so ancestry checks and the update merge can resolve them.
async fn live_tips(
    ctx: &RpcCtx,
    row: &PullRow,
    base_bare: &Path,
    head_bare: &Path,
) -> Result<(String, String), AppError> {
    let base_sha = if row.head_repo_id == row.repo_id {
        branch_tip(ctx.git.as_ref(), base_bare, &row.base_ref)
            .await?
            .ok_or_else(|| {
                AppError::new(
                    "pull.ref_not_found",
                    format!("base ref not found: {}", row.base_ref),
                )
            })?
    } else {
        ctx.git
            .fetch_ref_from(head_bare, base_bare, &format!("refs/heads/{}", row.base_ref))
            .await
            .map_err(|_| {
                AppError::new(
                    "pull.ref_not_found",
                    format!("base ref not found: {}", row.base_ref),
                )
            })?
    };
    let head_sha = branch_tip(ctx.git.as_ref(), head_bare, &row.head_ref)
        .await?
        .ok_or_else(|| AppError::new("pull.head_unavailable", "head branch is unavailable"))?;
    Ok((base_sha, head_sha))
}

/// `pull.branchStatus` — Read+ on the base repo; live head-vs-base freshness
/// plus whether the caller may run `pull.updateBranch` (Write on either repo).
pub async fn branch_status(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullBranchStatusResponse, AppError> {
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.branchStatus input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let (head_repo, head_slug, head_cap) = resolve_head(ctx, &accessible, &row).await?;

    let base_bare = bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let head_bare = bare_repo_path(&ctx.repos_dir, &head_slug, &head_repo.name)?;
    let (base_sha, head_sha) = live_tips(ctx, &row, &base_bare, &head_bare).await?;

    let (ahead, behind) = if base_sha == head_sha {
        (0, 0)
    } else {
        ctx.git
            .ahead_behind(&head_bare, &head_sha, &base_sha)
            .await
            .map_err(|e| {
                AppError::new("pull.update_failed", format!("divergence check failed: {e}"))
            })?
    };
    let can_update =
        meets(accessible.capability, Capability::Write) || meets(head_cap, Capability::Write);
    Ok(PullBranchStatusResponse {
        status: if behind == 0 { "up_to_date" } else { "behind" }.to_string(),
        ahead_count: ahead as i64,
        behind_count: behind as i64,
        base_sha,
        head_sha,
        can_update,
    })
}

/// `pull.updateBranch` — merge the live base tip into the PR head branch with a
/// merge commit (GIT-24 / GitHub "Update branch"). Requires Write+ on the head
/// repo **or** the base repo (maintainer update). The head branch's protection
/// rules still apply — evaluated here as a Push intent, and again by the bare
/// repo's `update` hook when the backend pushes the merge.
pub async fn update_branch(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<UpdatePullBranchResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.updateBranch input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    if row.state != "open" {
        return Err(AppError::new(
            "pull.invalid_state",
            "only open pull requests can be updated",
        ));
    }
    let (head_repo, head_slug, head_cap) = resolve_head(ctx, &accessible, &row).await?;
    // Maintainer-update parity: Write on the base repo or on the head repo.
    if !meets(accessible.capability, Capability::Write) && !meets(head_cap, Capability::Write) {
        return Err(not_found());
    }

    let base_bare: PathBuf =
        bare_repo_path(&ctx.repos_dir, &accessible.owner_username, &accessible.row.name)?;
    let head_bare: PathBuf = bare_repo_path(&ctx.repos_dir, &head_slug, &head_repo.name)?;
    let (base_sha, head_sha) = live_tips(ctx, &row, &base_bare, &head_bare).await?;

    // Protection on the head branch — the ref being written.
    let eff = effective_for_branch(&ctx.db, &head_repo.id, &row.head_ref).await?;
    evaluate_push(&eff, ProtectionIntent::Push, head_cap)?;

    // Already contains the base tip → no-op (still refresh stored base_sha when
    // it drifted so stored state matches the live branch).
    if base_sha == head_sha
        || ctx
            .git
            .is_ancestor(&head_bare, &base_sha, &head_sha)
            .await
            .unwrap_or(false)
    {
        if base_sha != row.base_sha {
            ctx.db
                .update_pull_fields(
                    &row.id,
                    &row.title,
                    &row.body,
                    row.draft,
                    &row.base_ref,
                    &base_sha,
                )
                .await
                .map_err(db_err)?;
        }
        let updated = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
        return Ok(UpdatePullBranchResponse {
            pull: to_public(ctx, &updated).await?,
            status: "up_to_date".to_string(),
            merge_commit_sha: None,
        });
    }

    let message = if row.head_repo_id == row.repo_id {
        format!("Merge branch '{}' into {}", row.base_ref, row.head_ref)
    } else {
        format!(
            "Merge branch '{}' of {}/{} into {}",
            row.base_ref, accessible.owner_username, accessible.row.name, row.head_ref
        )
    };
    let merge_sha = ctx
        .git
        .merge_commit(&head_bare, &row.head_ref, &base_sha, &message)
        .await
        .map_err(|e| {
            let msg = e.to_string().to_lowercase();
            if msg.contains("conflict") {
                AppError::new(
                    "pull.update_conflict",
                    "merge conflict updating the head branch",
                )
            } else {
                AppError::new("pull.update_failed", format!("update failed: {e}"))
            }
        })?;

    // Same bookkeeping a head push performs: head_sha + base_sha refresh, stale
    // review dismissal, `synchronize` webhook + Actions notify.
    let updated = synchronize_pull_after_head_move(
        &ctx.db,
        &ctx.repos_dir,
        ctx.git.as_ref(),
        &row,
        &accessible.owner_username,
        &accessible.row.name,
        &head_slug,
        &head_repo.name,
        &merge_sha,
        &user.username,
        &user.id,
        &ctx.env_name,
    )
    .await
    .map_err(db_err)?;

    crate::repo::record_ref_updates(
        &ctx.db,
        &head_repo.id,
        &user.id,
        &[(
            head_sha,
            merge_sha.clone(),
            format!("refs/heads/{}", row.head_ref),
        )],
        Some(ctx.git.clone()),
        Some(&head_bare),
    )
    .await;
    crate::mirror::notify_mirror_after_local_mutation(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        head_repo.id.clone(),
    );

    Ok(UpdatePullBranchResponse {
        pull: to_public(ctx, &updated).await?,
        status: "updated".to_string(),
        merge_commit_sha: Some(merge_sha),
    })
}
