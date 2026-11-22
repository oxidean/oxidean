//! Shared post-gate merge execution — `pull.merge` (RPC, actor = caller)
//! and the COL-07 auto-merge fire path (actor = the enabling user) both
//! run this exact sequence. Extracted from `oxidean-api-pulls`'s merge
//! handler; admissibility checks (repo settings, `evaluate_merge_gate`,
//! merge-queue-required) stay with the callers.

use crate::git::{bare_repo_path, quota::refresh_repo_size_bytes};
use crate::merge_queue::{repo_coords, MqDeps, QueueScope};
use crate::mirror::notify_mirror_after_local_mutation;
use crate::notify;
use crate::pull::{closing_refs, db_err, ensure_head_objects_in_base, emit_pull_event_db, notify_actions_for_pull, refs};
use crate::webhook::dispatch;
use oxidean_core::{AppError, MergeMethod};
use oxidean_db::PullRow;

/// Actor of record for the merge — the RPC caller for `pull.merge`, the
/// enabling user for the auto-merge fire path.
pub struct MergeActor<'a> {
    pub id: &'a str,
    pub login: &'a str,
}

/// Merge `row` onto `row.base_ref` inside `scope` using `method`, record
/// the merge, and replay the post-merge fan-out. Returns the merge commit
/// sha. The pull row must already be known-open and the caller must have
/// cleared admissibility — this function re-loads nothing before writing.
pub async fn execute(
    deps: &MqDeps,
    scope: &QueueScope,
    row: &PullRow,
    method: MergeMethod,
    commit_title: Option<&str>,
    commit_message: Option<&str>,
    delete_branch: bool,
    actor: &MergeActor<'_>,
) -> Result<String, AppError> {
    let path = bare_repo_path(&deps.repos_dir, &scope.owner, &scope.name)?;
    ensure_head_objects_in_base(&deps.db, deps.git.as_ref(), &deps.repos_dir, &path, row).await?;

    let message = commit_title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(row.title.as_str());
    let full_message = match commit_message.map(str::trim).filter(|s| !s.is_empty()) {
        Some(body) => format!("{message}\n\n{body}"),
        None => message.to_string(),
    };

    // Pre-merge base tip — rebase merges rewrite several commits so the
    // merge result's first parent is not the old tip; the recorded
    // attribution range needs the tip the worktree merged onto.
    let base_tip = deps
        .git
        .rev_parse(&path, &row.base_ref)
        .await
        .ok()
        .filter(|s| !s.is_empty());

    let sha = match method {
        MergeMethod::Merge => {
            deps.git
                .merge_commit(&path, &row.base_ref, &row.head_sha, &full_message)
                .await
        }
        MergeMethod::Squash => {
            deps.git
                .squash_merge(&path, &row.base_ref, &row.head_sha, &full_message)
                .await
        }
        MergeMethod::Rebase => deps.git.rebase_merge(&path, &row.base_ref, &row.head_sha).await,
    }
    .map_err(|e| {
        let msg = e.to_string().to_lowercase();
        if msg.contains("conflict") {
            AppError::new("pull.merge_conflict", "merge conflict")
        } else {
            AppError::new("pull.merge_failed", format!("merge failed: {e}"))
        }
    })?;

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    deps.db
        .mark_pull_merged(&row.id, actor.id, &sha, method.as_str(), &now)
        .await
        .map_err(db_err)?;
    // An armed auto-merge is consumed by the merge that satisfies it —
    // clear the columns so `auto_merge` reads disarmed on the merged row.
    let _ = deps.db.clear_pull_auto_merge(&row.id).await;

    // API-06: refs/pull/{N}/merge resolves to the recorded merge commit.
    refs::sync_merge_ref(
        deps.git.as_ref(),
        &deps.repos_dir,
        &scope.owner,
        &scope.name,
        row.number,
        &sha,
    )
    .await;

    let merge_subject = full_message.lines().next().unwrap_or("").trim();
    crate::repo::record_pr_merge(
        &deps.db,
        &scope.repo_id,
        actor.id,
        &row.base_ref,
        &sha,
        row.number,
        if merge_subject.is_empty() {
            None
        } else {
            Some(merge_subject)
        },
    )
    .await;

    // Authored-commit attribution: credit head commits landing on default.
    crate::repo::record_authored_merge(
        &deps.db,
        deps.git.clone(),
        path.clone(),
        &scope.repo_id,
        &row.base_ref,
        &sha,
        base_tip.as_deref(),
    )
    .await;

    notify_mirror_after_local_mutation(
        deps.db.clone(),
        deps.git.clone(),
        deps.repos_dir.clone(),
        scope.repo_id.clone(),
    );

    // GIT-25: refresh cached size_bytes after merge objects landed.
    if let Err(e) = refresh_repo_size_bytes(&deps.db, &scope.repo_id, &path).await {
        tracing::warn!(error = %e, "refresh repo size_bytes failed");
    }

    if row.base_ref == scope.default_branch {
        let mut close_refs = closing_refs::closing_refs(&row.body);
        close_refs.extend(closing_refs::closing_refs(&full_message));
        let mut seen = std::collections::HashSet::new();
        close_refs.retain(|r| {
            seen.insert((
                r.repo
                    .as_ref()
                    .map(|(o, n)| (o.to_lowercase(), n.to_lowercase())),
                r.number,
            ))
        });
        for r in close_refs {
            let same_repo = match &r.repo {
                None => true,
                Some((owner, name)) => {
                    owner.eq_ignore_ascii_case(&scope.owner)
                        && name.eq_ignore_ascii_case(&scope.name)
                }
            };
            if !same_repo {
                continue;
            }
            if let Ok(Some(issue)) = deps
                .db
                .find_issue_by_repo_number(&scope.repo_id, r.number)
                .await
            {
                if issue.state == "open" {
                    let _ = deps.db.close_issue(&issue.id, actor.id).await;
                }
            }
        }
    }

    if delete_branch
        && row.head_repo_id == row.repo_id
        && deps.git.branch_delete(&path, &row.head_ref).await.is_ok()
    {
        // API-04: deleting the head branch on merge fires `delete`.
        let payload = dispatch::ref_event_payload(
            "delete",
            &row.head_ref,
            "branch",
            &scope.default_branch,
            &scope.description,
            &scope.owner,
            &scope.name,
            &scope.repo_id,
            actor.login,
            actor.id,
        );
        dispatch::emit(
            &deps.db,
            &scope.repo_id,
            "delete",
            "",
            payload,
            &deps.env_name,
        )
        .await;
    }

    let updated = deps
        .db
        .find_pull_by_id(&row.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("pull.internal", "pull missing after merge"))?;
    emit_pull_event_db(
        &deps.db,
        &scope.owner,
        &scope.name,
        &scope.repo_id,
        &updated,
        "closed",
        actor.login,
        actor.id,
        true,
        &deps.env_name,
        None,
    )
    .await;
    // `on: pull_request types: [closed]` — merged reads true in the payload.
    let (head_owner, head_name) = if updated.head_repo_id == updated.repo_id {
        (scope.owner.clone(), scope.name.clone())
    } else {
        repo_coords(&deps.db, &updated.head_repo_id)
            .await
            .map(|s| (s.owner, s.name))
            .unwrap_or_else(|| (scope.owner.clone(), scope.name.clone()))
    };
    notify_actions_for_pull(
        &deps.db,
        deps.git.as_ref(),
        &deps.repos_dir,
        &updated,
        &scope.owner,
        &scope.name,
        &head_owner,
        &head_name,
        Some(actor.id),
        deps.job_token_actor,
        crate::actions::PullRequestAction::Closed,
        true,
    )
    .await;
    let subject = notify::subject_for_pull(&updated);
    let recipients = notify::pull_participant_ids(&deps.db, &updated.id, &updated.author_id).await;
    notify::fanout_activity(&deps.db, actor.id, recipients, "pr_merged", &subject).await;
    Ok(sha)
}
