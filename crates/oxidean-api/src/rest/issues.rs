//! `/api/v1/repos/{owner}/{repo}/issues/**` — issues + comments.

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
    CreateIssueCommentRequest, CreateIssueRequest, DeleteIssueCommentResponse, IssueCommentPublic,
    IssueCommentsListResponse, IssueListRequest, IssueListResponse, IssuePublic, RpcResponse,
    UpdateIssueCommentRequest,
};

/// `PATCH /issues/{number}` body — `title`/`body` go to `issue.update`;
/// `state` fans out to `issue.close`/`issue.reopen`.
fn issue_patch_body(_: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "body": { "type": "string" },
            "state": {
                "type": "string",
                "enum": ["open", "closed"],
                "description": "`closed` → issue.close; `open` → issue.reopen",
            },
        },
        "description": "At least one of `title`, `body`, `state` is required",
    })
}

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues",
        tags: &["issues"],
        operation_id: "listIssues",
        summary: "List issues (`issue.list`)",
        procedure: "issue.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<IssueListRequest>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssueListResponse>,
        )),
        mount: || get(list_issues),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/issues",
        tags: &["issues"],
        operation_id: "createIssue",
        summary: "Create issue (`issue.create`)",
        procedure: "issue.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateIssueRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssuePublic>,
        )),
        mount: || post(create_issue),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues/{number}",
        tags: &["issues"],
        operation_id: "getIssue",
        summary: "Get issue (`issue.get`)",
        procedure: "issue.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssuePublic>,
        )),
        mount: || get(get_issue),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/issues/{number}",
        tags: &["issues"],
        operation_id: "updateIssue",
        summary:
            "Update issue (`issue.update` + `issue.close`/`issue.reopen` when `state` is given)",
        procedure: "issue.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Json(issue_patch_body)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssuePublic>,
        )),
        mount: || patch(update_issue),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues/{number}/comments",
        tags: &["issues"],
        operation_id: "listIssueComments",
        summary: "Issue comments (`issue.comments.list`)",
        procedure: "issue.comments.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssueCommentsListResponse>,
        )),
        mount: || get(list_issue_comments),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/issues/{number}/comments",
        tags: &["issues"],
        operation_id: "createIssueComment",
        summary: "Comment on issue (`issue.comments.create`)",
        procedure: "issue.comments.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateIssueCommentRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssueCommentPublic>,
        )),
        mount: || post(create_issue_comment),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}",
        tags: &["issues"],
        operation_id: "updateIssueComment",
        summary: "Edit comment (`issue.comments.update`; author only)",
        procedure: "issue.comments.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number", "commentId"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<UpdateIssueCommentRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssueCommentPublic>,
        )),
        mount: || patch(update_issue_comment),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}",
        tags: &["issues"],
        operation_id: "deleteIssueComment",
        summary: "Delete comment (`issue.comments.delete`)",
        procedure: "issue.comments.delete",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number", "commentId"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<DeleteIssueCommentResponse>,
        )),
        mount: || delete(delete_issue_comment),
    },
];

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
        _ => {}
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
