//! COL-07 auto-merge orchestrator — plain-PR auto-merge-when-green.
//!
//! A pull is *armed* when `auto_merge_method` is set (see
//! `pull.auto_merge.enable`). Six fire seams — check-run completion,
//! commit-status arrival, review approval, review dismissal, thread
//! resolution, `pull.updateBranch`, `enqueue` check — plus the enable RPC
//! itself call [`maybe_fire_pull`]/[`maybe_fire_for_head`]; each re-reads
//! the pull row, rebuilds the *enabling* user's capability, re-runs the
//! shared `evaluate_merge_gate`, and merges via `pull::merge_exec` only
//! when every protection is green. A merge queue-required base branch
//! never merges here — the queue owns those merges.

use crate::actions::PullRequestAction;
use crate::merge_queue::{repo_coords, MqDeps, QueueScope};
use crate::protection::effective_for_branch;
use crate::pull::{
    db_err, emit_pull_event_db, merge_exec, merge_ops::evaluate_merge_gate,
    notify_actions_for_pull,
};
use crate::repo::{effective_capability, owner_ref_for_repo, AccessibleRepo};
use oxidean_core::{AppError, MergeMethod};
use oxidean_db::PullRow;

/// Re-evaluate every armed pull on `repo_id` whose head sha is `head_sha`
/// — the check/status seams know the completed sha, nothing else.
/// Soft-fails per pull: one failing merge must not starve the others.
pub async fn maybe_fire_for_head(deps: &MqDeps, repo_id: &str, head_sha: &str) {
    let pulls = match deps
        .db
        .list_armed_pulls_for_head(repo_id, head_sha)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, repo_id, head_sha, "list_armed_pulls_for_head failed");
            return;
        }
    };
    for row in pulls {
        let Some(scope) = repo_coords(&deps.db, &row.repo_id).await else {
            continue;
        };
        if let Err(e) = maybe_fire_pull(deps, &scope, &row).await {
            tracing::error!(pull = %row.id, error = %e.message, "auto-merge fire failed");
        }
    }
}

/// Re-evaluate one armed pull: reload the row, re-check arm state and the
/// enabling user's capability, run the shared merge gate, and execute the
/// merge when green. Returns `Ok` when the pull simply isn't mergeable
/// yet — gate failures are the normal "wait for the next signal" path.
pub async fn maybe_fire_pull(
    deps: &MqDeps,
    scope: &QueueScope,
    pull: &PullRow,
) -> Result<(), AppError> {
    // Re-read: the seam's row is stale the moment any concurrent merge or
    // disarm ran; bail silently when the arm was cleared or state moved.
    let row = deps
        .db
        .find_pull_by_id(&pull.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("pull.not_found", "pull request not found"))?;
    if row.state != "open" {
        return Ok(());
    }
    let (Some(method), Some(set_by)) = (
        row.auto_merge_method.as_deref().and_then(|m| MergeMethod::parse(m).ok()),
        row.auto_merge_set_by.as_deref(),
    ) else {
        return Ok(());
    };

    let repo_row = deps
        .db
        .find_repository_by_id(&row.repo_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.not_found", "repository not found"))?;
    if repo_row.archived {
        return Ok(());
    }
    // Flipping the repo setting off mid-flight holds the arm — the pull
    // stays enabled but nothing merges until the setting returns.
    let allow_auto = deps
        .db
        .get_repo_merge_settings(&repo_row.id)
        .await
        .map_err(db_err)?
        .allow_auto_merge;
    if !allow_auto {
        return Ok(());
    }

    let owner_ref = owner_ref_for_repo(&deps.db, &repo_row)
        .await
        .map_err(|e| AppError::new("repo.internal", e))?
        .ok_or_else(|| AppError::new("repo.internal", "repository owner missing"))?;
    // The enabler's capability at *fire* time — a maintainer whose write
    // bit was revoked since enabling must not merge through a stale arm.
    let capability = effective_capability(&deps.db, Some(set_by), &repo_row, &owner_ref)
        .await
        .map_err(|e| AppError::new("repo.internal", e))?;
    // Enabler lost all repo access since arming — hold, don't merge.
    if capability.is_none() {
        return Ok(());
    }
    let accessible = AccessibleRepo {
        row: repo_row,
        owner_username: owner_ref.slug().to_string(),
        capability,
    };

    let eff = effective_for_branch(&deps.db, &accessible.row.id, &row.base_ref).await?;
    // Auto-merge never bypasses the required merge queue — the queue's own
    // lifecycle lands these pulls.
    if eff.required_merge_queue {
        return Ok(());
    }

    if evaluate_merge_gate(
        &deps.db,
        &deps.repos_dir,
        &accessible,
        &eff,
        &row,
        Some(method.as_str().to_string()),
    )
    .await
    .is_err()
    {
        return Ok(());
    }

    let actor_login = deps
        .db
        .find_user_by_id(set_by)
        .await
        .ok()
        .flatten()
        .map(|u| u.username)
        .unwrap_or_else(|| set_by.to_string());
    merge_exec::execute(
        deps,
        scope,
        &row,
        method,
        row.auto_merge_commit_title.as_deref(),
        row.auto_merge_commit_message.as_deref(),
        false,
        &merge_exec::MergeActor {
            id: set_by,
            login: &actor_login,
        },
    )
    .await
    .map(|_| ())
}

/// Disarm `pull` and emit the `auto_merge_disabled` lifecycle (webhook +
/// Actions activity). Shared by `pull.auto_merge.disable` (sender = the
/// cancelling user) and the disarm seams — head-move, retarget, close
/// (sender = whoever moved the head / closed / retargeted). No-op when the
/// pull isn't armed.
#[allow(clippy::too_many_arguments)]
pub async fn disable_for_pull(
    db: &oxidean_db::Database,
    git: &dyn oxidean_git::GitBackend,
    repos_dir: &std::path::Path,
    env_name: &str,
    job_token_actor: bool,
    scope: &QueueScope,
    pull: &PullRow,
    sender_login: &str,
    sender_id: &str,
) {
    if pull.auto_merge_method.is_none() {
        return;
    }
    let _ = db.clear_pull_auto_merge(&pull.id).await;
    let Some(updated) = db.find_pull_by_id(&pull.id).await.ok().flatten() else {
        return;
    };
    emit_pull_event_db(
        db,
        &scope.owner,
        &scope.name,
        &scope.repo_id,
        &updated,
        "auto_merge_disabled",
        sender_login,
        sender_id,
        false,
        env_name,
        None,
    )
    .await;
    let (head_owner, head_name) = if updated.head_repo_id == updated.repo_id {
        (scope.owner.clone(), scope.name.clone())
    } else {
        repo_coords(db, &updated.head_repo_id)
            .await
            .map(|s| (s.owner, s.name))
            .unwrap_or_else(|| (scope.owner.clone(), scope.name.clone()))
    };
    notify_actions_for_pull(
        db,
        git,
        repos_dir,
        &updated,
        &scope.owner,
        &scope.name,
        &head_owner,
        &head_name,
        Some(sender_id),
        job_token_actor,
        PullRequestAction::AutoMergeDisabled,
        false,
    )
    .await;
}
