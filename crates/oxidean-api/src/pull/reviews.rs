//! Pull reviews + optional review requests (PR-04 / D-PR-05..09).

use oxidean_core::{
    AppError, DismissPullReviewRequest, PullRefRequest, PullReviewPublic, PullReviewRequestMutate,
    PullReviewRequestsListResponse, PullReviewState, PullReviewsListResponse,
    SubmitPullReviewRequest,
};
use oxidean_db::PullReviewRow;
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::notify;
use crate::pull::acl;
use crate::rpc::RpcCtx;

use super::{db_err, load_pull_in_repo, validate_body};

fn review_not_found() -> AppError {
    AppError::new("pull.review_not_found", "Review not found")
}

async fn review_to_public(
    ctx: &RpcCtx,
    row: &PullReviewRow,
) -> Result<PullReviewPublic, AppError> {
    let author_username = ctx
        .db
        .find_user_by_id(&row.author_id)
        .await
        .map_err(db_err)?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into());
    let state = PullReviewState::parse(&row.state)
        .map_err(|e| AppError::new("pull.internal", e))?;
    Ok(PullReviewPublic {
        id: row.id.clone(),
        pull_id: row.pull_id.clone(),
        author_id: row.author_id.clone(),
        author_username,
        state,
        body: row.body.clone(),
        commit_sha: row.commit_sha.clone(),
        submitted_at: row.submitted_at.clone(),
        dismissed_at: row.dismissed_at.clone(),
        dismiss_reason: row.dismiss_reason.clone(),
    })
}

/// `pull.reviews.list` — Read+.
pub async fn reviews_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullReviewsListResponse, AppError> {
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.reviews.list input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let rows = ctx
        .db
        .list_pull_reviews(&pull.id)
        .await
        .map_err(db_err)?;
    let mut reviews = Vec::with_capacity(rows.len());
    for row in &rows {
        reviews.push(review_to_public(ctx, row).await?);
    }
    Ok(PullReviewsListResponse { reviews })
}

/// `pull.reviews.submit` — Write+; author cannot Approve / Request changes.
pub async fn reviews_submit(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullReviewPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: SubmitPullReviewRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.reviews.submit input: {e}"),
        )
    })?;
    let state = PullReviewState::parse(&req.state).map_err(|e| {
        AppError::new("rpc.bad_input", e)
    })?;
    if matches!(state, PullReviewState::Dismissed) {
        return Err(AppError::new(
            "rpc.bad_input",
            "cannot submit a dismissed review; use dismiss",
        ));
    }
    let body = validate_body(req.body.as_deref())?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    if pull.author_id == user.id
        && matches!(
            state,
            PullReviewState::Approved | PullReviewState::ChangesRequested
        )
    {
        return Err(AppError::new(
            "pull.author_cannot_approve",
            "pull request authors cannot approve or request changes on their own pull",
        ));
    }
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_pull_review(
            &id,
            &pull.id,
            &user.id,
            state.as_str(),
            &body,
            Some(pull.head_sha.as_str()),
        )
        .await
        .map_err(db_err)?;
    let subject = notify::subject_for_pull(&pull);
    notify::fanout_activity(
        &ctx.db,
        &user.id,
        std::iter::once(pull.author_id.clone()),
        "pr_review",
        &subject,
    )
    .await;
    review_to_public(ctx, &row).await
}

/// `pull.reviews.dismiss` — Write+.
pub async fn reviews_dismiss(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullReviewPublic, AppError> {
    let _user = require_verified(ctx).await?;
    let req: DismissPullReviewRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.reviews.dismiss input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let existing = ctx
        .db
        .find_pull_review_by_id(&req.review_id)
        .await
        .map_err(db_err)?
        .ok_or_else(review_not_found)?;
    if existing.pull_id != pull.id {
        return Err(review_not_found());
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let reason = req
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let row = ctx
        .db
        .dismiss_pull_review(&req.review_id, reason, &now)
        .await
        .map_err(db_err)?;
    review_to_public(ctx, &row).await
}

/// `pull.reviewRequests.list` — Read+.
pub async fn review_requests_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullReviewRequestsListResponse, AppError> {
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.reviewRequests.list input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let ids = ctx
        .db
        .list_pull_review_request_user_ids(&pull.id)
        .await
        .map_err(db_err)?;
    let users = ctx.db.find_users_by_ids(&ids).await.map_err(db_err)?;
    let mut usernames: Vec<String> = users.into_iter().map(|u| u.username).collect();
    usernames.sort();
    Ok(PullReviewRequestsListResponse { usernames })
}

/// `pull.reviewRequests.add` — Write+ UX-only.
pub async fn review_requests_add(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullReviewRequestsListResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: PullReviewRequestMutate = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.reviewRequests.add input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let target = ctx
        .db
        .find_user_by_username(req.username.trim())
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("user.not_found", "User not found"))?;
    ctx.db
        .upsert_pull_review_request(&pull.id, &target.id, &user.id)
        .await
        .map_err(db_err)?;
    let subject = notify::subject_for_pull(&pull);
    notify::fanout_suppress_ignored(
        &ctx.db,
        &user.id,
        std::iter::once(target.id.clone()),
        "pr_review_requested",
        &subject,
    )
    .await;
    review_requests_list(
        ctx,
        serde_json::json!({
            "owner": req.owner,
            "name": req.name,
            "number": req.number,
        }),
    )
    .await
}

/// `pull.reviewRequests.remove` — Write+.
pub async fn review_requests_remove(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullReviewRequestsListResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: PullReviewRequestMutate = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.reviewRequests.remove input: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let target = ctx
        .db
        .find_user_by_username(req.username.trim())
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("user.not_found", "User not found"))?;
    ctx.db
        .delete_pull_review_request(&pull.id, &target.id)
        .await
        .map_err(db_err)?;
    review_requests_list(
        ctx,
        serde_json::json!({
            "owner": req.owner,
            "name": req.name,
            "number": req.number,
        }),
    )
    .await
}
