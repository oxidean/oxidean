//! `/api/v1/repos/{owner}/{repo}/pulls/**` — pull requests, comments, reviews.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::Json;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{
    call, ctx_for, dispatch, merge_fields, repo_ref, respond, CommentPath, RepoNumberPath, RepoPath,
};
use oxidean_api_core::app::AppState;
use oxidean_core::{
    CreatePullCommentRequest, CreatePullRequest, DeletePullCommentResponse, MergePullRequest,
    MergePullResponse, PullCommentPublic, PullCommentsListResponse, PullCommitsResponse,
    PullFilesResponse, PullListRequest, PullListResponse, PullPublic, PullReviewPublic,
    PullReviewsListResponse, RpcResponse, SubmitPullReviewRequest, UpdatePullBranchResponse,
    UpdatePullCommentRequest,
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
        method: "PUT",
        path: "/repos/{owner}/{repo}/pulls/{number}/merge",
        tags: &["pulls"],
        operation_id: "mergePull",
        summary: "Merge pull request (`pull.merge`) — `merge_method` → `method`",
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
        mount: || put(merge_pull),
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
    // ---- requested reviewers / dismissals / update-branch (GitHub REST) ----
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/pulls/{number}/requested_reviewers",
        tags: &["pulls"],
        operation_id: "pulls/listRequestedReviewers",
        summary: "List requested reviewers (`pull.reviewRequests.list`)",
        procedure: "pull.reviewRequests.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(requested_reviewers_schema)),
        mount: || get(list_requested_reviewers),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/pulls/{number}/requested_reviewers",
        tags: &["pulls"],
        operation_id: "pulls/requestReviewers",
        summary: "Request reviewers (`pull.reviewRequests.add` per login)",
        procedure: "pull.reviewRequests.add",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Json(reviewers_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullPublic>,
        )),
        mount: || post(request_reviewers),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/pulls/{number}/requested_reviewers",
        tags: &["pulls"],
        operation_id: "pulls/deleteReviewers",
        summary: "Remove requested reviewers (`pull.reviewRequests.remove` per login)",
        procedure: "pull.reviewRequests.remove",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Json(reviewers_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullPublic>,
        )),
        mount: || delete(delete_requested_reviewers),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/pulls/{number}/reviews/{review_id}/dismissals",
        tags: &["pulls"],
        operation_id: "pulls/dismissReview",
        summary: "Dismiss a review (`pull.reviews.dismiss`) — body `message` → `reason`",
        procedure: "pull.reviews.dismiss",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number", "review_id"],
        query: None,
        body: Some(BodySpec::Json(dismissal_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PullReviewPublic>,
        )),
        mount: || post(dismiss_pull_review),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/pulls/{number}/update-branch",
        tags: &["pulls"],
        operation_id: "pulls/updateBranch",
        summary: "Update the PR branch from base (`pull.updateBranch`)",
        procedure: "pull.updateBranch",
        ok: StatusCode::ACCEPTED,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<UpdatePullBranchResponse>,
        )),
        mount: || put(update_pull_branch),
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

/// GitHub `head`/`base` → `pull.create`'s `head_ref`/`base_ref`; an
/// `owner:branch` head additionally sets `head_owner` (cross-repo).
fn gh_pull_refs(body: Value) -> Value {
    let mut body = body;
    let Some(o) = body.as_object_mut() else {
        return body;
    };
    if let Some(head) = o
        .remove("head")
        .and_then(|v| v.as_str().map(str::to_string))
    {
        if let Some((owner, branch)) = head.split_once(':') {
            o.insert("head_owner".into(), json!(owner));
            o.insert("head_ref".into(), json!(branch));
        } else {
            o.insert("head_ref".into(), json!(head));
        }
    }
    if let Some(v) = o.remove("base") {
        o.insert("base_ref".into(), v);
    }
    body
}

/// `POST /api/v1/repos/{owner}/{repo}/pulls` (`pull.create`) —
/// GitHub body: `title`, `head`, `base` (required); `body`, `draft`
/// (optional). `head`/`base` translate to `head_ref`/`base_ref` via
/// [`gh_pull_refs`]; native `head_ref`/`base_ref` keys pass through.
async fn create_pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(repo_ref(&p.owner, &p.repo), gh_pull_refs(body));
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
    let body = gh_pull_refs(body);
    let has_fields = body.get("title").is_some()
        || body.get("body").is_some()
        || body.get("base_ref").is_some()
        || body.get("draft").is_some();
    let state_change = body.get("state").and_then(|v| v.as_str());
    if !has_fields && state_change.is_none() {
        return bad_input("PATCH requires at least one of: title, body, base, draft, state");
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
        None => bad_input("PATCH requires at least one of: title, body, base, draft, state"),
    }
}

fn bad_input(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "code": "rpc.bad_input", "message": message })),
    )
        .into_response()
}

/// `PUT /api/v1/repos/{owner}/{repo}/pulls/{number}/merge` (`pull.merge`) —
/// GitHub body: `merge_method` (`merge`|`squash`|`rebase`, required);
/// `commit_title`, `commit_message`, `sha` (optional). `merge_method`
/// translates to `pull.merge`'s `method`; native `method` passes through.
async fn merge_pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut body = body;
    if let Some(o) = body.as_object_mut() {
        if let Some(v) = o.remove("merge_method") {
            o.insert("method".into(), v);
        }
    }
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

// ---------------------------------------------------------------------------
// Requested reviewers / review dismissals / update-branch (GitHub REST)
// ---------------------------------------------------------------------------

/// Path params for the review-dismissal route.
#[derive(Deserialize)]
pub struct PullReviewPath {
    owner: String,
    repo: String,
    number: i64,
    review_id: String,
}

fn requested_reviewers_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "users": {"type": "array", "items": {"type": "object"}},
            "teams": {"type": "array", "items": {"type": "object"}},
        },
    })
}

fn reviewers_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["reviewers"],
        "properties": {
            "reviewers": {"type": "array", "items": {"type": "string"}},
            "team_reviewers": {"type": "array", "items": {"type": "string"}},
        },
    })
}

fn dismissal_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "message": {"type": "string"},
        },
    })
}

/// `{usernames: [...]}` → GitHub's `{users: [{login}], teams: []}`.
fn gh_requested_reviewers(d: &Value) -> Value {
    let users: Vec<Value> = d["usernames"]
        .as_array()
        .map(|us| us.iter().map(|u| json!({"login": u})).collect())
        .unwrap_or_default();
    json!({"users": users, "teams": []})
}

async fn list_requested_reviewers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(&mut ctx, "pull.reviewRequests.list", p.ref_input()).await {
        RpcResponse::Ok { data, .. } => {
            (StatusCode::OK, Json(gh_requested_reviewers(&data))).into_response()
        }
        RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

/// Requested `reviewers: [logins]` from the GitHub body (teams unsupported —
/// the domain has no teams yet; an empty/absent array is fine).
fn requested_reviewer_logins(body: &Value) -> Result<Vec<String>, Box<Response>> {
    match body["reviewers"].as_array().map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    }) {
        Some(r) => Ok(r),
        None => Err(Box::new(
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "message": "Validation Failed",
                    "errors": ["`reviewers` must be an array of logins"],
                })),
            )
                .into_response(),
        )),
    }
}

/// Fan the GitHub `reviewers[]` list over the per-username RPC, then answer
/// the updated pull (GitHub's response shape).
async fn mutate_reviewers(
    state: &AppState,
    headers: &HeaderMap,
    p: &RepoNumberPath,
    proc: &'static str,
    logins: &[String],
    ok: StatusCode,
) -> Response {
    let mut ctx = match ctx_for(state, headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    for login in logins {
        let input = merge_fields(p.ref_input(), json!({"username": login}));
        match dispatch(&mut ctx, proc, input).await {
            RpcResponse::Ok { .. } => {}
            RpcResponse::Err { error, .. } => {
                return (super::status_for_error(&error), Json(error)).into_response()
            }
        }
    }
    match dispatch(&mut ctx, "pull.get", p.ref_input()).await {
        RpcResponse::Ok { data, .. } => (ok, Json(data)).into_response(),
        RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

async fn request_reviewers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let logins = match requested_reviewer_logins(&body) {
        Ok(l) => l,
        Err(r) => return *r,
    };
    mutate_reviewers(
        &state,
        &headers,
        &p,
        "pull.reviewRequests.add",
        &logins,
        StatusCode::CREATED,
    )
    .await
}

async fn delete_requested_reviewers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let logins = match requested_reviewer_logins(&body) {
        Ok(l) => l,
        Err(r) => return *r,
    };
    mutate_reviewers(
        &state,
        &headers,
        &p,
        "pull.reviewRequests.remove",
        &logins,
        StatusCode::OK,
    )
    .await
}

/// GitHub's dismissal body uses `message`; the RPC field is `reason`.
async fn dismiss_pull_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PullReviewPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut input = merge_fields(
        json!({"owner": p.owner, "name": p.repo, "number": p.number, "review_id": p.review_id}),
        body,
    );
    if let Some(m) = input.get("message").and_then(Value::as_str) {
        input["reason"] = json!(m);
        input.as_object_mut().map(|o| o.remove("message"));
    }
    call(
        &state,
        &headers,
        "pull.reviews.dismiss",
        input,
        StatusCode::OK,
    )
    .await
}

async fn update_pull_branch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "pull.updateBranch",
        p.ref_input(),
        StatusCode::ACCEPTED,
    )
    .await
}
