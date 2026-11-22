//! `pull.auto_merge.enable` / `pull.auto_merge.disable` — COL-07 arming
//! surface for plain-PR auto-merge-when-green. The fire/disarm machinery
//! lives in `oxidean_api_core::auto_merge`; these handlers do input +
//! ACL + the enable-time admissibility checks.

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::merge_queue::{MqDeps, QueueScope};
use oxidean_api_core::notify;
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{AppError, MergeMethod, PullAutoMergeEnableRequest, PullPublic, PullRefRequest};
use serde_json::Value;

use oxidean_api_core::pull::{acl, db_err, emit_pull_event, load_pull_in_repo, to_public};

/// `pull.auto_merge.enable` — Author or Write+; open non-draft pull on a
/// repo with `allow_auto_merge`; method must be one the repo's merge
/// settings allow. Re-arming replaces method/title/message.
pub async fn enable(ctx: &RpcCtx, input: Value) -> Result<PullPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: PullAutoMergeEnableRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.auto_merge.enable input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    if !acl::can_edit_pull(&user.id, &row, accessible.capability) {
        return Err(oxidean_api_core::repo::not_found());
    }
    oxidean_api_core::repo::ensure_not_archived(&accessible)?;
    let settings = ctx
        .db
        .get_repo_merge_settings(&accessible.row.id)
        .await
        .map_err(db_err)?;
    if !settings.allow_auto_merge {
        return Err(AppError::new(
            "pull.auto_merge_disabled",
            "auto-merge is not enabled for this repository",
        ));
    }
    if row.state != "open" {
        return Err(AppError::new(
            "pull.invalid_state",
            "auto-merge can only be enabled on open pull requests",
        ));
    }
    // Drafts may arm (GitHub parity) — the merge gate blocks the fire until
    // the pull flips ready.
    let method = req.method;
    let allowed = match method {
        MergeMethod::Merge => settings.allow_merge_commit,
        MergeMethod::Squash => settings.allow_squash_merge,
        MergeMethod::Rebase => settings.allow_rebase_merge,
    };
    if !allowed {
        return Err(AppError::new(
            "pull.merge_disabled",
            format!(
                "{} merges are disabled for this repository",
                method.as_str()
            ),
        ));
    }

    // Queue-required bases own their merges — arming is meaningless there;
    // GitHub shows "merge when ready" (enqueue) instead.
    let eff = oxidean_api_core::protection::effective_for_branch(
        &ctx.db,
        &accessible.row.id,
        &row.base_ref,
    )
    .await?;
    if eff.required_merge_queue {
        return Err(AppError::new(
            "pull.merge_queue_required",
            "merge queue is required for this branch",
        ));
    }

    ctx.db
        .set_pull_auto_merge(
            &row.id,
            method.as_str(),
            &user.id,
            req.commit_title.as_deref(),
            req.commit_message.as_deref(),
        )
        .await
        .map_err(db_err)?;

    let updated = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    emit_pull_event(
        ctx,
        &accessible,
        &updated,
        "auto_merge_enabled",
        &user.username,
        &user.id,
        false,
    )
    .await;
    let (head_owner, head_name) = if updated.head_repo_id == updated.repo_id {
        (
            accessible.owner_username.clone(),
            accessible.row.name.clone(),
        )
    } else {
        oxidean_api_core::merge_queue::repo_coords(&ctx.db, &updated.head_repo_id)
            .await
            .map(|s| (s.owner, s.name))
            .unwrap_or_else(|| {
                (
                    accessible.owner_username.clone(),
                    accessible.row.name.clone(),
                )
            })
    };
    oxidean_api_core::pull::notify_actions_for_pull(
        &ctx.db,
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &updated,
        &accessible.owner_username,
        &accessible.row.name,
        &head_owner,
        &head_name,
        Some(&user.id),
        oxidean_api_core::actions::events::rpc_caller_is_job_token(ctx),
        oxidean_api_core::actions::PullRequestAction::AutoMergeEnabled,
        false,
    )
    .await;
    let subject = notify::subject_for_pull(&updated);
    let recipients = notify::pull_participant_ids(&ctx.db, &updated.id, &updated.author_id).await;
    notify::fanout_activity(
        &ctx.db,
        &user.id,
        recipients,
        "pr_auto_merge_enabled",
        &subject,
    )
    .await;

    // Green-already PRs merge immediately rather than waiting for the next
    // signal — the arm itself is the first fire event.
    let deps = MqDeps::from_ctx(ctx);
    let scope = QueueScope::of(&accessible);
    if let Err(e) = oxidean_api_core::auto_merge::maybe_fire_pull(&deps, &scope, &updated).await {
        tracing::warn!(pull = %updated.id, error = %e.message, "auto-merge immediate fire failed");
    }
    let current = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    to_public(ctx, &current).await
}

/// `pull.auto_merge.disable` — Author, the arming user, or Write+.
/// Idempotent: disarming an unarmed pull returns it unchanged.
pub async fn disable(ctx: &RpcCtx, input: Value) -> Result<PullPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.auto_merge.disable input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let is_enabler = row.auto_merge_set_by.as_deref() == Some(user.id.as_str());
    if !is_enabler && !acl::can_edit_pull(&user.id, &row, accessible.capability) {
        return Err(oxidean_api_core::repo::not_found());
    }
    oxidean_api_core::repo::ensure_not_archived(&accessible)?;

    if row.auto_merge_method.is_none() {
        return Err(AppError::new(
            "pull.invalid_state",
            "auto-merge is not enabled on this pull request",
        ));
    }
    let scope = QueueScope::of(&accessible);
    oxidean_api_core::auto_merge::disable_for_pull(
        &ctx.db,
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &ctx.env_name,
        oxidean_api_core::actions::events::rpc_caller_is_job_token(ctx),
        &scope,
        &row,
        &user.username,
        &user.id,
    )
    .await;

    let updated = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    to_public(ctx, &updated).await
}
