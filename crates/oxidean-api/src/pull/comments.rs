//! Pull conversation + line comments (PR-03 / D-PR-13..16).

use oxidean_core::{
    AppError, CreatePullCommentRequest, PullCommentPublic, PullCommentsListResponse,
    PullRefRequest, ResolvePullCommentRequest,
};
use oxidean_db::PullCommentRow;
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
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;

    let path = req
        .path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let side = req
        .side
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase());
    if let Some(ref s) = side {
        if s != "LEFT" && s != "RIGHT" {
            return Err(AppError::new(
                "rpc.bad_input",
                "side must be LEFT or RIGHT",
            ));
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
    // GitHub parity (DEBT-04): `issue_comment` covers PR conversation comments;
    // line-anchored comments map to `pull_request_review_comment` instead.
    if row.path.is_none() {
        let state = if pull.state == "merged" {
            "closed"
        } else {
            pull.state.as_str()
        };
        let payload = dispatch::issue_comment_payload(
            "created",
            pull.number,
            &pull.title,
            &pull.body,
            state,
            true,
            &row.id,
            &row.body,
            None,
            &user.username,
            &user.id,
            &accessible.owner_username,
            &accessible.row.name,
            &accessible.row.id,
            &user.username,
            &user.id,
        );
        dispatch::emit(
            &ctx.db,
            &accessible.row.id,
            "issue_comment",
            "created",
            payload,
            &ctx.env_name,
        )
        .await;
    }
    comment_to_public(ctx, &row).await
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
