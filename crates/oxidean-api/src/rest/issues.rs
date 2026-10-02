//! `/api/v1/repos/{owner}/{repo}/issues/**` — issues + comments.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    call, ctx_for, dispatch, merge_fields, repo_ref, respond, CommentPath, RepoNumberPath, RepoPath,
};
use crate::app::AppState;
use oxidean_core::RpcResponse;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repos/{owner}/{repo}/issues",
            get(list_issues).post(create_issue),
        )
        .route(
            "/repos/{owner}/{repo}/issues/{number}",
            get(get_issue).patch(update_issue),
        )
        .route(
            "/repos/{owner}/{repo}/issues/{number}/comments",
            get(list_issue_comments).post(create_issue_comment),
        )
        .route(
            "/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}",
            axum::routing::patch(update_issue_comment).delete(delete_issue_comment),
        )
}

#[derive(serde::Serialize, Deserialize)]
pub struct IssueListQuery {
    /// `open` (default) | `closed` | `all`.
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    assignee: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

/// `GET /api/v1/repos/{owner}/{repo}/issues` (`issue.list`).
async fn list_issues(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<IssueListQuery>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        serde_json::to_value(&q).unwrap_or_else(|_| json!({})),
    );
    call(&state, &headers, "issue.list", input, StatusCode::OK).await
}

/// `POST /api/v1/repos/{owner}/{repo}/issues` (`issue.create`) —
/// body: `title` (required), `body`.
async fn create_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(repo_ref(&p.owner, &p.repo), body);
    call(&state, &headers, "issue.create", input, StatusCode::CREATED).await
}

/// `GET /api/v1/repos/{owner}/{repo}/issues/{number}` (`issue.get`).
async fn get_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(&state, &headers, "issue.get", p.ref_input(), StatusCode::OK).await
}

/// `PATCH /api/v1/repos/{owner}/{repo}/issues/{number}` — `issue.update`
/// for `title`/`body`; `state: "closed"|"open"` maps to `issue.close` /
/// `issue.reopen`. Both may be combined in one request.
async fn update_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let has_fields = body.get("title").is_some() || body.get("body").is_some();
    let state_change = body.get("state").and_then(|v| v.as_str());
    if !has_fields && state_change.is_none() {
        return bad_input("PATCH requires at least one of: title, body, state");
    }

    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let mut last: Option<RpcResponse> = None;
    if has_fields {
        let input = merge_fields(p.ref_input(), body.clone());
        let resp = dispatch(&mut ctx, "issue.update", input).await;
        if matches!(resp, RpcResponse::Err { .. }) {
            return respond(resp, StatusCode::OK);
        }
        last = Some(resp);
    }
    match state_change {
        Some("closed") => {
            let resp = dispatch(&mut ctx, "issue.close", p.ref_input()).await;
            return respond(resp, StatusCode::OK);
        }
        Some("open") => {
            let resp = dispatch(&mut ctx, "issue.reopen", p.ref_input()).await;
            return respond(resp, StatusCode::OK);
        }
        Some(other) => return bad_input(&format!("invalid state: {other} (expected open|closed)")),
        None => {}
    }
    match last {
        Some(resp) => respond(resp, StatusCode::OK),
        None => bad_input("PATCH requires at least one of: title, body, state"),
    }
}

fn bad_input(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "code": "rpc.bad_input", "message": message })),
    )
        .into_response()
}

/// `GET /api/v1/repos/{owner}/{repo}/issues/{number}/comments`
/// (`issue.comments.list`).
async fn list_issue_comments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "issue.comments.list",
        p.ref_input(),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/repos/{owner}/{repo}/issues/{number}/comments`
/// (`issue.comments.create`) — body: `body` (required).
async fn create_issue_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(p.ref_input(), body);
    call(
        &state,
        &headers,
        "issue.comments.create",
        input,
        StatusCode::CREATED,
    )
    .await
}

/// `PATCH /api/v1/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}`
/// (`issue.comments.update`) — body: `body` (required).
async fn update_issue_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CommentPath>,
    Json(body): Json<Value>,
) -> Response {
    let fields = json!({
        "owner": p.owner,
        "name": p.repo,
        "number": p.number,
        "commentId": p.comment_id,
    });
    let input = merge_fields(fields, body);
    call(
        &state,
        &headers,
        "issue.comments.update",
        input,
        StatusCode::OK,
    )
    .await
}

/// `DELETE /api/v1/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}`
/// (`issue.comments.delete`).
async fn delete_issue_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CommentPath>,
) -> Response {
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "number": p.number,
        "commentId": p.comment_id,
    });
    call(
        &state,
        &headers,
        "issue.comments.delete",
        input,
        StatusCode::OK,
    )
    .await
}
