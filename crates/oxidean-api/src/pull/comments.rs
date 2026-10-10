//! Pull conversation + line comments (PR-03 / D-PR-13..16).

use oxidean_core::{
    AppError, CommentHistoryResponse, CommentRevisionPublic, CreatePullCommentRequest,
    DeletePullCommentResponse, PullCommentPublic, PullCommentRefRequest,
    PullCommentsListResponse, PullRefRequest, ResolvePullCommentRequest,
    UpdatePullCommentRequest,
};
use oxidean_db::{PullCommentRow, PullRow};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::notify;
use crate::pull::acl;
use crate::rpc::RpcCtx;
use crate::webhook::dispatch;

use super::{db_err, load_pull_in_repo, validate_body};

fn comment_not_found() -> AppError {
    AppError::new("pull.comment_not_found", "Comment not found")
}

/// GitHub parity (DEBT-04 + #102): PR conversation comments emit
/// `issue_comment` with the `issue.pull_request` marker; diff-anchored
/// comments emit `pull_request_review_comment`. `changes_from` is set only on
/// `edited`; `sender` is the acting user, which differs from the comment
/// author on moderator deletes.
#[expect(clippy::too_many_arguments)]
async fn emit_comment_webhook(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    pull: &PullRow,
    row: &PullCommentRow,
    action: &str,
    changes_from: Option<&str>,
    author_login: &str,
    sender_login: &str,
    sender_id: &str,
) {
    let state = if pull.state == "merged" {
        "closed"
    } else {
        pull.state.as_str()
    };
    let (event, payload) = if let Some(path) = row.path.as_deref() {
        let payload = dispatch::pull_request_review_comment_payload(
            action,
            pull.number,
            &pull.title,
            state,
            &row.id,
            &row.body,
            path,
            row.side.as_deref(),
            row.line,
            row.start_line,
            row.commit_sha.as_deref(),
            &row.created_at,
            &row.updated_at,
            changes_from,
            author_login,
            &row.author_id,
            &accessible.owner_username,
            &accessible.row.name,
            &accessible.row.id,
            sender_login,
            sender_id,
        );
        ("pull_request_review_comment", payload)
    } else {
        let payload = dispatch::issue_comment_payload(
            action,
            pull.number,
            &pull.title,
            &pull.body,
            state,
            true,
            &row.id,
            &row.body,
            changes_from,
            author_login,
            &row.author_id,
            &accessible.owner_username,
            &accessible.row.name,
            &accessible.row.id,
            sender_login,
            sender_id,
        );
        ("issue_comment", payload)
    };
    dispatch::emit(&ctx.db, &accessible.row.id, event, action, payload, &ctx.env_name).await;
}

async fn load_comment_usernames(
    ctx: &RpcCtx,
    rows: &[PullCommentRow],
) -> Result<std::collections::HashMap<String, String>, AppError> {
    let mut ids: Vec<String> = rows.iter().map(|r| r.author_id.clone()).collect();
    ids.sort();
    ids.dedup();
    Ok(ctx
        .db
        .find_users_by_ids(&ids)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|u| (u.id, u.username))
        .collect())
}

fn comment_row_to_public(
    row: &PullCommentRow,
    usernames: &std::collections::HashMap<String, String>,
) -> PullCommentPublic {
    let author_username = usernames
        .get(&row.author_id)
        .cloned()
        .unwrap_or_else(|| "unknown".into());
    PullCommentPublic {
        id: row.id.clone(),
        pull_id: row.pull_id.clone(),
        author_id: row.author_id.clone(),
        author_username,
        body: row.body.clone(),
        path: row.path.clone(),
        side: row.side.clone(),
        line: row.line,
        start_line: row.start_line,
        commit_sha: row.commit_sha.clone(),
        outdated: row.outdated,
        resolved: row.resolved,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

async fn comment_to_public(
    ctx: &RpcCtx,
    row: &PullCommentRow,
) -> Result<PullCommentPublic, AppError> {
    let usernames = load_comment_usernames(ctx, std::slice::from_ref(row)).await?;
    Ok(comment_row_to_public(row, &usernames))
}

/// `pull.comments.list` — Read+.
pub async fn comments_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullCommentsListResponse, AppError> {
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.comments.list input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let rows = ctx.db.list_pull_comments(&pull.id).await.map_err(db_err)?;
    let usernames = load_comment_usernames(ctx, &rows).await?;
    let comments = rows
        .iter()
        .map(|row| comment_row_to_public(row, &usernames))
        .collect();
    Ok(PullCommentsListResponse { comments })
}

/// `pull.comments.create` — Read+ verified general or line-anchored.
pub async fn comments_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullCommentPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreatePullCommentRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.comments.create input: {e}"),
        )
    })?;
    let body = validate_body(Some(req.body.as_str()))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    crate::repo::ensure_not_archived(&accessible)?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;

    let path = req.path.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let side = req
        .side
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase());
    if let Some(ref s) = side {
        if s != "LEFT" && s != "RIGHT" {
            return Err(AppError::new("rpc.bad_input", "side must be LEFT or RIGHT"));
        }
    }
    if path.is_some() && (side.is_none() || req.line.is_none()) {
        return Err(AppError::new(
            "rpc.bad_input",
            "line comments require path, side, and line",
        ));
    }
    if path.is_none() && (side.is_some() || req.line.is_some() || req.start_line.is_some()) {
        return Err(AppError::new(
            "rpc.bad_input",
            "general comments cannot include line anchors",
        ));
    }

    let commit_sha = req
        .commit_sha
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            if path.is_some() {
                Some(pull.head_sha.clone())
            } else {
                None
            }
        });

    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_pull_comment(
            &id,
            &pull.id,
            &user.id,
            &body,
            path,
            side.as_deref(),
            req.line,
            req.start_line,
            commit_sha.as_deref(),
        )
        .await
        .map_err(db_err)?;
    let subject = notify::subject_for_pull(&pull);
    let participants = notify::pull_participant_ids(&ctx.db, &pull.id, &pull.author_id).await;
    let mentions = notify::resolve_mention_user_ids(&ctx.db, &body).await;
    // One watch-level lookup serves both fanouts below.
    let watch = ctx.db.list_repo_watch_levels(&subject.repo_id).await;
    notify::fanout_activity_with_watch(&ctx.db, &user.id, participants.clone(), "pr_comment", &subject, &watch).await;
    let participant_set: std::collections::HashSet<_> = participants.into_iter().collect();
    let mention_only: Vec<_> = mentions
        .into_iter()
        .filter(|m| !participant_set.contains(m))
        .collect();
    notify::fanout_suppress_ignored_with_watch(&ctx.db, &user.id, mention_only, "pr_mention", &subject, &watch).await;
    emit_comment_webhook(
        ctx,
        &accessible,
        &pull,
        &row,
        "created",
        None,
        &user.username,
        &user.username,
        &user.id,
    )
    .await;
    comment_to_public(ctx, &row).await
}

/// `pull.comments.update` — author only; appends full revision (D-ISS-09 parity).
pub async fn comments_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullCommentPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: UpdatePullCommentRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.comments.update input: {e}"),
        )
    })?;
    let new_body = validate_body(Some(req.body.as_str()))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    crate::repo::ensure_not_archived(&accessible)?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let row = ctx
        .db
        .find_pull_comment_by_id(&req.comment_id)
        .await
        .map_err(db_err)?
        .ok_or_else(comment_not_found)?;
    if row.pull_id != pull.id {
        return Err(comment_not_found());
    }
    if !crate::issue::acl::can_edit_comment(&user.id, &row.author_id) {
        return Err(crate::repo::not_found());
    }
    if new_body == row.body {
        return comment_to_public(ctx, &row).await;
    }
    let rev_id = Uuid::new_v4().to_string();
    ctx.db
        .insert_pull_comment_revision(&rev_id, &row.id, &user.id, &row.body)
        .await
        .map_err(db_err)?;
    let updated = ctx
        .db
        .update_pull_comment_body(&row.id, &new_body)
        .await
        .map_err(db_err)?;
    emit_comment_webhook(
        ctx,
        &accessible,
        &pull,
        &updated,
        "edited",
        Some(row.body.as_str()),
        &user.username,
        &user.username,
        &user.id,
    )
    .await;
    comment_to_public(ctx, &updated).await
}

/// `pull.comments.delete` — author or Write+ moderation (D-ISS-09 parity).
pub async fn comments_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeletePullCommentResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: PullCommentRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.comments.delete input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    crate::repo::ensure_not_archived(&accessible)?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let row = ctx
        .db
        .find_pull_comment_by_id(&req.comment_id)
        .await
        .map_err(db_err)?
        .ok_or_else(comment_not_found)?;
    if row.pull_id != pull.id {
        return Err(comment_not_found());
    }
    if !crate::issue::acl::can_delete_comment(&user.id, &row.author_id, accessible.capability) {
        return Err(crate::repo::not_found());
    }
    ctx.db.delete_pull_comment(&row.id).await.map_err(db_err)?;
    let author_login = match ctx.db.find_user_by_id(&row.author_id).await {
        Ok(Some(u)) => u.username,
        Ok(None) => String::new(),
        Err(e) => {
            tracing::warn!(error = %e, "pull comment delete: author lookup failed");
            String::new()
        }
    };
    emit_comment_webhook(
        ctx,
        &accessible,
        &pull,
        &row,
        "deleted",
        None,
        &author_login,
        &user.username,
        &user.id,
    )
    .await;
    Ok(DeletePullCommentResponse { ok: true })
}

/// `pull.comments.history` — Read+; prior body revisions oldest-first (D-ISS-12 parity).
pub async fn comments_history(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CommentHistoryResponse, AppError> {
    let req: PullCommentRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.comments.history input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let row = ctx
        .db
        .find_pull_comment_by_id(&req.comment_id)
        .await
        .map_err(db_err)?
        .ok_or_else(comment_not_found)?;
    if row.pull_id != pull.id {
        return Err(comment_not_found());
    }
    let revs = ctx
        .db
        .list_pull_comment_revisions(&row.id)
        .await
        .map_err(db_err)?;
    let mut revisions = Vec::with_capacity(revs.len());
    for rev in &revs {
        let editor_username = match ctx.db.find_user_by_id(&rev.editor_id).await {
            Ok(Some(u)) => u.username,
            Ok(None) => String::new(),
            Err(e) => return Err(db_err(e)),
        };
        revisions.push(CommentRevisionPublic {
            id: rev.id.clone(),
            comment_id: rev.comment_id.clone(),
            editor_id: rev.editor_id.clone(),
            editor_username,
            body: rev.body.clone(),
            created_at: rev.created_at.clone(),
        });
    }
    Ok(CommentHistoryResponse { revisions })
}

/// `pull.comments.resolve` — Write+ resolve/unresolve thread.
pub async fn comments_resolve(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullCommentPublic, AppError> {
    let _user = require_verified(ctx).await?;
    let req: ResolvePullCommentRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.comments.resolve input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let existing = ctx
        .db
        .find_pull_comment_by_id(&req.comment_id)
        .await
        .map_err(db_err)?
        .ok_or_else(comment_not_found)?;
    if existing.pull_id != pull.id {
        return Err(comment_not_found());
    }
    let row = ctx
        .db
        .set_pull_comment_resolved(&req.comment_id, req.resolved)
        .await
        .map_err(db_err)?;
    comment_to_public(ctx, &row).await
}
