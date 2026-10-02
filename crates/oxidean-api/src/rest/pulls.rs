//! `/api/v1/repos/{owner}/{repo}/pulls/**` — pull requests, comments, reviews.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{call, ctx_for, dispatch, merge_fields, repo_ref, respond, RepoNumberPath, RepoPath};
use crate::app::AppState;
use oxidean_core::RpcResponse;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repos/{owner}/{repo}/pulls",
            get(list_pulls).post(create_pull),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}",
            get(get_pull).patch(update_pull),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}/merge",
            axum::routing::post(merge_pull),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}/files",
            get(list_pull_files),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}/commits",
            get(list_pull_commits),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}/comments",
            get(list_pull_comments).post(create_pull_comment),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}/reviews",
            get(list_pull_reviews).post(submit_pull_review),
        )
}

#[derive(serde::Serialize, Deserialize)]
pub struct PullListQuery {
    /// `open` (default) | `closed` | `merged` | `all`.
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
    /// `approved` | `changes_requested` | `review_required` | …
    #[serde(default, alias = "review")]
    review_state: Option<String>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
}

/// `GET /api/v1/repos/{owner}/{repo}/pulls` (`pull.list`).
async fn list_pulls(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<PullListQuery>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        serde_json::to_value(&q).unwrap_or_else(|_| json!({})),
    );
    call(&state, &headers, "pull.list", input, StatusCode::OK).await
}

/// `POST /api/v1/repos/{owner}/{repo}/pulls` (`pull.create`) —
/// body: `title`, `base_ref`, `head_ref` (required); `body`, `head_owner`,
/// `head_name`, `draft` (optional).
async fn create_pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(repo_ref(&p.owner, &p.repo), body);
    call(&state, &headers, "pull.create", input, StatusCode::CREATED).await
}

/// `GET /api/v1/repos/{owner}/{repo}/pulls/{number}` (`pull.get`).
async fn get_pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(&state, &headers, "pull.get", p.ref_input(), StatusCode::OK).await
}

/// `PATCH /api/v1/repos/{owner}/{repo}/pulls/{number}` — `pull.update` for
/// `title`/`body`/`base_ref`/`draft`; `state: "closed"|"open"` maps to
/// `pull.close` / `pull.reopen`.
async fn update_pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let has_fields = body.get("title").is_some()
        || body.get("body").is_some()
        || body.get("base_ref").is_some()
        || body.get("draft").is_some();
    let state_change = body.get("state").and_then(|v| v.as_str());
    if !has_fields && state_change.is_none() {
        return bad_input("PATCH requires at least one of: title, body, base_ref, draft, state");
    }
    if let Some(other) = state_change {
        if !matches!(other, "open" | "closed") {
            return bad_input(&format!("invalid state: {other} (expected open|closed)"));
        }
    }

    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let mut last: Option<RpcResponse> = None;
    if has_fields {
        let input = merge_fields(p.ref_input(), body.clone());
        let resp = dispatch(&mut ctx, "pull.update", input).await;
        if matches!(resp, RpcResponse::Err { .. }) {
            return respond(resp, StatusCode::OK);
        }
        last = Some(resp);
    }
    match state_change {
        Some("closed") => {
            let resp = dispatch(&mut ctx, "pull.close", p.ref_input()).await;
            return respond(resp, StatusCode::OK);
        }
        Some("open") => {
            let resp = dispatch(&mut ctx, "pull.reopen", p.ref_input()).await;
            return respond(resp, StatusCode::OK);
        }
        _ => {}
    }
    match last {
        Some(resp) => respond(resp, StatusCode::OK),
        None => bad_input("PATCH requires at least one of: title, body, base_ref, draft, state"),
    }
}

fn bad_input(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "code": "rpc.bad_input", "message": message })),
    )
        .into_response()
}

/// `POST /api/v1/repos/{owner}/{repo}/pulls/{number}/merge` (`pull.merge`) —
/// body: `method` (`merge`|`squash`|`rebase`, required); `commit_title`,
/// `commit_message`, `delete_branch` (optional).
async fn merge_pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(p.ref_input(), body);
    call(&state, &headers, "pull.merge", input, StatusCode::OK).await
}

/// `GET /api/v1/repos/{owner}/{repo}/pulls/{number}/files` (`pull.files`).
async fn list_pull_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "pull.files",
        p.ref_input(),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/repos/{owner}/{repo}/pulls/{number}/commits` (`pull.commits`).
async fn list_pull_commits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "pull.commits",
        p.ref_input(),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/repos/{owner}/{repo}/pulls/{number}/comments`
/// (`pull.comments.list`).
async fn list_pull_comments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "pull.comments.list",
        p.ref_input(),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/repos/{owner}/{repo}/pulls/{number}/comments`
/// (`pull.comments.create`) — body: `body` (required); `path`, `side`,
/// `line`, `start_line`, `commit_sha` (optional, diff placement).
async fn create_pull_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(p.ref_input(), body);
    call(
        &state,
        &headers,
        "pull.comments.create",
        input,
        StatusCode::CREATED,
    )
    .await
}

/// `GET /api/v1/repos/{owner}/{repo}/pulls/{number}/reviews`
/// (`pull.reviews.list`).
async fn list_pull_reviews(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "pull.reviews.list",
        p.ref_input(),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/repos/{owner}/{repo}/pulls/{number}/reviews`
/// (`pull.reviews.submit`) — body: `state` (`approved`|`changes_requested`|
/// `commented`, required), `body`.
async fn submit_pull_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(p.ref_input(), body);
    call(
        &state,
        &headers,
        "pull.reviews.submit",
        input,
        StatusCode::CREATED,
    )
    .await
}
