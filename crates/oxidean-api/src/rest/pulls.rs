//! `/api/v1/repos/{owner}/{repo}/pulls/**` — pull requests, comments, reviews.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::Json;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{
    call, ctx_for, dispatch, merge_fields, repo_ref, respond, CommentPath, RepoNumberPath, RepoPath,
};
use crate::app::AppState;
use oxidean_core::{
    CreatePullCommentRequest, CreatePullRequest, DeletePullCommentResponse, MergePullRequest,
    MergePullResponse, PullCommentPublic, PullCommentsListResponse, PullCommitsResponse,
    PullFilesResponse, PullListRequest, PullListResponse, PullPublic, PullReviewPublic,
    PullReviewsListResponse, RpcResponse, SubmitPullReviewRequest, UpdatePullCommentRequest,
};

/// `PATCH /pulls/{number}` body — fields go to `pull.update`; `state` fans
/// out to `pull.close`/`pull.reopen`.
fn pull_patch_body(_: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "body": { "type": "string" },
            "base_ref": { "type": "string" },
            "draft": { "type": "boolean" },
            "state": {
                "type": "string",
                "enum": ["open", "closed"],
                "description": "`closed` → pull.close; `open` → pull.reopen",
            },
        },
        "description": "At least one of `title`, `body`, `base_ref`, `draft`, `state` is required",
    })
}

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pulls",
        tags: &["pulls"],
        operation_id: "listPulls",
        summary: "List pull requests (`pull.list`)",
        procedure: "pull.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<PullListRequest>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullListResponse>,
        )),
        mount: || get(list_pulls),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/pulls",
        tags: &["pulls"],
        operation_id: "createPull",
        summary: "Create pull request (`pull.create`)",
        procedure: "pull.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreatePullRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullPublic>,
        )),
        mount: || post(create_pull),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pulls/{number}",
        tags: &["pulls"],
        operation_id: "getPull",
        summary: "Get pull request (`pull.get`)",
        procedure: "pull.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullPublic>,
        )),
        mount: || get(get_pull),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/pulls/{number}",
        tags: &["pulls"],
        operation_id: "updatePull",
        summary: "Update pull (`pull.update` + `pull.close`/`pull.reopen` when `state` is given)",
        procedure: "pull.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Json(pull_patch_body)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullPublic>,
        )),
        mount: || patch(update_pull),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/pulls/{number}/merge",
        tags: &["pulls"],
        operation_id: "mergePull",
        summary: "Merge pull request (`pull.merge`)",
        procedure: "pull.merge",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<MergePullRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<MergePullResponse>,
        )),
        mount: || post(merge_pull),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pulls/{number}/files",
        tags: &["pulls"],
        operation_id: "listPullFiles",
        summary: "Pull diff files (`pull.files`)",
        procedure: "pull.files",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullFilesResponse>,
        )),
        mount: || get(list_pull_files),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pulls/{number}/commits",
        tags: &["pulls"],
        operation_id: "listPullCommits",
        summary: "Pull commits (`pull.commits`)",
        procedure: "pull.commits",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullCommitsResponse>,
        )),
        mount: || get(list_pull_commits),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pulls/{number}/comments",
        tags: &["pulls"],
        operation_id: "listPullComments",
        summary: "Pull review comments (`pull.comments.list`)",
        procedure: "pull.comments.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullCommentsListResponse>,
        )),
        mount: || get(list_pull_comments),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/pulls/{number}/comments",
        tags: &["pulls"],
        operation_id: "createPullComment",
        summary: "Add pull comment (`pull.comments.create`; diff placement optional)",
        procedure: "pull.comments.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreatePullCommentRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullCommentPublic>,
        )),
        mount: || post(create_pull_comment),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/pulls/{number}/comments/{comment_id}",
        tags: &["pulls"],
        operation_id: "updatePullComment",
        summary: "Edit pull comment (`pull.comments.update`; author only)",
        procedure: "pull.comments.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number", "commentId"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<UpdatePullCommentRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullCommentPublic>,
        )),
        mount: || patch(update_pull_comment),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/pulls/{number}/comments/{comment_id}",
        tags: &["pulls"],
        operation_id: "deletePullComment",
        summary: "Delete pull comment (`pull.comments.delete`)",
        procedure: "pull.comments.delete",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number", "commentId"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<DeletePullCommentResponse>,
        )),
        mount: || delete(delete_pull_comment),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pulls/{number}/reviews",
        tags: &["pulls"],
        operation_id: "listPullReviews",
        summary: "Pull reviews (`pull.reviews.list`)",
        procedure: "pull.reviews.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullReviewsListResponse>,
        )),
        mount: || get(list_pull_reviews),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/pulls/{number}/reviews",
        tags: &["pulls"],
        operation_id: "submitPullReview",
        summary: "Submit review (`pull.reviews.submit`)",
        procedure: "pull.reviews.submit",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<SubmitPullReviewRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullReviewPublic>,
        )),
        mount: || post(submit_pull_review),
    },
];

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

/// `PATCH /api/v1/repos/{owner}/{repo}/pulls/{number}/comments/{comment_id}`
/// (`pull.comments.update`) — body: `body` (required).
async fn update_pull_comment(
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
        "pull.comments.update",
        input,
        StatusCode::OK,
    )
    .await
}

/// `DELETE /api/v1/repos/{owner}/{repo}/pulls/{number}/comments/{comment_id}`
/// (`pull.comments.delete`).
async fn delete_pull_comment(
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
        "pull.comments.delete",
        input,
        StatusCode::OK,
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
