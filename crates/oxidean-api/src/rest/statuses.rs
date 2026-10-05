//! `/api/v1/repos/**` commit statuses — GitHub-compatible shapes (API-06).
//!
//! Thin facade over `repo.commitStatus.*`: responses carry the GitHub field
//! names (`state`, `context`, `target_url`, `description`, `created_at`,
//! `creator`) so CI and deploy bots that post or poll commit statuses work
//! unchanged.

use std::collections::{BTreeSet, HashMap};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use oxidean_core::{CommitStatusState, RpcResponse};
use oxidean_db::Database;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::repos::ShaPath;
use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{ctx_for, dispatch, status_for_error};
use crate::app::AppState;

/// One GitHub-shaped status row (what [`github_status`] renders).
fn status_obj() -> Value {
    json!({
        "type": "object",
        "description": "GitHub-shaped commit status row (API-06)",
        "properties": {
            "id": { "type": "string" },
            "sha": { "type": "string" },
            "state": { "type": "string", "enum": ["pending", "success", "failure", "error"] },
            "context": { "type": "string" },
            "description": { "type": "string" },
            "target_url": { "type": "string", "nullable": true },
            "url": { "type": "string" },
            "created_at": { "type": "string" },
            "updated_at": { "type": "string" },
            "creator": {
                "type": "object",
                "nullable": true,
                "properties": {
                    "login": { "type": "string" },
                    "id": { "type": "string" },
                },
            },
        },
    })
}

fn status_list_schema(_: &mut SchemaGenerator) -> Value {
    json!({ "type": "array", "items": status_obj() })
}

fn status_schema(_: &mut SchemaGenerator) -> Value {
    status_obj()
}

fn combined_status_schema(_: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "description": "GitHub combined-status rollup (API-06)",
        "properties": {
            "state": { "type": "string", "enum": ["pending", "success", "failure"] },
            "sha": { "type": "string" },
            "total_count": { "type": "integer" },
            "statuses": { "type": "array", "items": status_obj() },
            "repository": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "full_name": { "type": "string" },
                },
            },
        },
    })
}

/// `POST …/statuses/{sha}` body — GitHub field names (`state`, `context`,
/// `target_url`, `description`); `context` defaults to `default`.
fn status_create_body(_: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["state"],
        "properties": {
            "state": {
                "type": "string",
                "enum": ["pending", "success", "failure", "error"],
            },
            "context": { "type": "string", "default": "default" },
            "target_url": { "type": "string" },
            "description": { "type": "string" },
        },
    })
}

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/statuses/{sha}",
        tags: &["repos"],
        operation_id: "listCommitStatuses",
        summary: "List commit statuses (`repo.commitStatus.list`) — GitHub shape",
        procedure: "repo.commitStatus.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "sha"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(status_list_schema)),
        mount: || get(list_statuses),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/statuses/{sha}",
        tags: &["repos"],
        operation_id: "createCommitStatus",
        summary: "Create a commit status (`repo.commitStatus.create`) — GitHub shape",
        procedure: "repo.commitStatus.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "sha"],
        query: None,
        body: Some(BodySpec::Json(status_create_body)),
        response: Some(RespSpec::Json(status_schema)),
        mount: || post(create_status),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/commits/{sha}/statuses",
        tags: &["repos"],
        operation_id: "listCommitStatusesAlias",
        summary: "List commit statuses (`repo.commitStatus.list`) — alias of `/statuses/{sha}`",
        procedure: "repo.commitStatus.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "sha"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(status_list_schema)),
        mount: || get(list_statuses),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/commits/{sha}/status",
        tags: &["repos"],
        operation_id: "combinedCommitStatus",
        summary: "Combined commit status rollup (`repo.commitStatus.list`)",
        procedure: "repo.commitStatus.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "sha"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(combined_status_schema)),
        mount: || get(combined_status),
    },
];

#[derive(Debug, Deserialize)]
pub struct StatusCreateBody {
    state: String,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    target_url: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// `POST /api/v1/repos/{owner}/{repo}/statuses/{sha}` — GitHub-compatible body
/// (`state`, `context`, `target_url`, `description`); translates to
/// `repo.commitStatus.create` and echoes the created status back in the
/// GitHub shape. `state` must be `pending`, `success`, `failure`, or `error`;
/// `context` defaults to `default` like GitHub.
async fn create_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ShaPath>,
    Json(body): Json<StatusCreateBody>,
) -> Response {
    let state_name = body.state.trim();
    if let Err(message) = CommitStatusState::parse(state_name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"code": "rpc.bad_input", "message": message})),
        )
            .into_response();
    }
    let context = body
        .context
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or("default")
        .to_string();
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "sha": p.sha,
        "state": state_name,
        "context": context,
        "description": body.description.unwrap_or_default(),
        "target_url": body.target_url,
    });
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    match dispatch(&mut ctx, "repo.commitStatus.create", input).await {
        RpcResponse::Ok { data, .. } => {
            let logins = creator_logins(&ctx.db, std::slice::from_ref(&data)).await;
            (
                StatusCode::CREATED,
                Json(github_status(&data, &p.owner, &p.repo, &p.sha, &logins)),
            )
                .into_response()
        }
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

/// `GET /api/v1/repos/{owner}/{repo}/statuses/{sha}` — list statuses for a
/// commit via `repo.commitStatus.list`, returned as a GitHub-shaped bare
/// array ordered newest-first.
async fn list_statuses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ShaPath>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let input = json!({"owner": p.owner, "name": p.repo, "sha": p.sha});
    match dispatch(&mut ctx, "repo.commitStatus.list", input).await {
        RpcResponse::Ok { data, .. } => {
            let statuses = status_rows(&data);
            let logins = creator_logins(&ctx.db, &statuses).await;
            let out: Vec<Value> = statuses
                .iter()
                .map(|s| github_status(s, &p.owner, &p.repo, &p.sha, &logins))
                .collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

/// `GET /api/v1/repos/{owner}/{repo}/commits/{sha}/status` — GitHub combined
/// status: `state` is `failure`/`error` if any context reports it, else
/// `success`/`pending` when all/any rows do, else `pending` with no rows.
async fn combined_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ShaPath>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let input = json!({"owner": p.owner, "name": p.repo, "sha": p.sha});
    match dispatch(&mut ctx, "repo.commitStatus.list", input).await {
        RpcResponse::Ok { data, .. } => {
            let statuses = status_rows(&data);
            let logins = creator_logins(&ctx.db, &statuses).await;
            let combined = combined_state(&statuses);
            let out: Vec<Value> = statuses
                .iter()
                .map(|s| github_status(s, &p.owner, &p.repo, &p.sha, &logins))
                .collect();
            (
                StatusCode::OK,
                Json(json!({
                    "state": combined,
                    "sha": p.sha,
                    "total_count": out.len(),
                    "statuses": out,
                    "repository": {
                        "name": p.repo,
                        "full_name": format!("{}/{}", p.owner, p.repo),
                    },
                })),
            )
                .into_response()
        }
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

// --- helpers ----------------------------------------------------------------

fn status_rows(data: &Value) -> Vec<Value> {
    data.get("statuses")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Render one `repo.commitStatus.*` row using GitHub's field names. Internal
/// `repo_id`/`creator_id` are not exposed; `creator` carries the public user
/// shape (`login`, `id`) when resolvable.
fn github_status(
    status: &Value,
    owner: &str,
    repo: &str,
    sha: &str,
    logins: &HashMap<String, String>,
) -> Value {
    let get = |k: &str| status.get(k).cloned().unwrap_or(Value::Null);
    let creator = match status.get("creator_id").and_then(|v| v.as_str()) {
        Some(id) => json!({
            "login": logins.get(id).cloned().unwrap_or_default(),
            "id": id,
        }),
        None => Value::Null,
    };
    json!({
        "id": get("id"),
        "sha": sha,
        "state": get("state"),
        "context": get("context"),
        "description": get("description"),
        "target_url": get("target_url"),
        "url": format!("/api/v1/repos/{owner}/{repo}/statuses/{sha}"),
        "created_at": get("created_at"),
        "updated_at": get("updated_at"),
        "creator": creator,
    })
}

/// Combined status rollup, mirroring GitHub semantics: any `failure` or
/// `error` wins; otherwise `success` requires all rows passing.
fn combined_state(statuses: &[Value]) -> &'static str {
    let mut combined = "success";
    for status in statuses {
        let s = status
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        match s {
            "failure" | "error" => return "failure",
            "pending" => combined = "pending",
            _ => {}
        }
    }
    if statuses.is_empty() {
        "pending"
    } else {
        combined
    }
}

/// Resolve `creator_id` → login for embedding the `creator` object.
async fn creator_logins(db: &Database, statuses: &[Value]) -> HashMap<String, String> {
    let ids: BTreeSet<String> = statuses
        .iter()
        .filter_map(|s| {
            s.get("creator_id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    let ids: Vec<String> = ids.into_iter().collect();
    if ids.is_empty() {
        return HashMap::new();
    }
    db.find_users_by_ids(&ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|u| (u.id, u.username))
        .collect()
}
