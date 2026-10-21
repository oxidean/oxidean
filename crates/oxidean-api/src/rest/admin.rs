//! `/api/v1/admin/**` — instance administration (sys-admin only; session
//! cookie or appropriately-scoped credentials — `admin.*` procedures are
//! session-only for PATs per API-02).

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::Json;
use oxidean_core::{
    ActionListRunnersResponse, ActionRegistrationTokenResponse, AdminLfsUsageResponse,
    AdminUsersListRequest, AdminUsersListResponse, RpcResponse,
};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, ctx_for, dispatch, status_for_error};
use crate::app::AppState;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/admin/users",
        tags: &["admin"],
        operation_id: "adminListUsers",
        summary: "List users (`admin.users.list`; sys-admin session only)",
        procedure: "admin.users.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: Some(SchemaGenerator::into_root_schema_for::<AdminUsersListRequest>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<AdminUsersListResponse>,
        )),
        mount: || get(list_users),
    },
    RouteDef {
        method: "GET",
        path: "/admin/lfs/usage",
        tags: &["admin"],
        operation_id: "adminLfsUsage",
        summary: "Instance LFS usage stats (`admin.lfs.getUsage`; sys-admin session only)",
        procedure: "admin.lfs.getUsage",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<AdminLfsUsageResponse>,
        )),
        mount: || get(lfs_usage),
    },
    RouteDef {
        method: "GET",
        path: "/admin/actions/runners",
        tags: &["admin"],
        operation_id: "adminListActionsRunners",
        summary: "List self-hosted runners (`admin.actions.listRunners`; sys-admin session only)",
        procedure: "admin.actions.listRunners",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ActionListRunnersResponse>,
        )),
        mount: || get(admin_runners),
    },
    RouteDef {
        method: "POST",
        path: "/admin/actions/runners/registration-token",
        tags: &["admin"],
        operation_id: "adminCreateActionsRegistrationToken",
        summary: "Mint an instance-scoped runner registration token (`admin.actions.createRegistrationToken`)",
        procedure: "admin.actions.createRegistrationToken",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ActionRegistrationTokenResponse>,
        )),
        mount: || post(admin_runner_token),
    },
    // Gitea admin-user REST (`adminGetUser` / `adminEditUser` /
    // `adminDeleteUser`): the domain procs are `user_id`-keyed, so the
    // handlers resolve `{username}` through `admin.users.list` first.
    RouteDef {
        method: "GET",
        path: "/admin/users/{username}",
        tags: &["admin"],
        operation_id: "adminGetUser",
        summary: "Get one user (resolved via `admin.users.list`; sys-admin session only)",
        procedure: "admin.users.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(admin_user_schema)),
        mount: || get(admin_get_user),
    },
    RouteDef {
        method: "PATCH",
        path: "/admin/users/{username}",
        tags: &["admin"],
        operation_id: "adminEditUser",
        summary: "Edit a user's admin/active state (`admin.users.updateRole` + ban/unban)",
        procedure: "admin.users.updateRole",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Json(admin_edit_user_body_schema)),
        response: Some(RespSpec::Json(admin_user_schema)),
        mount: || patch(admin_edit_user),
    },
    RouteDef {
        method: "DELETE",
        path: "/admin/users/{username}",
        tags: &["admin"],
        operation_id: "adminDeleteUser",
        summary: "Delete a user (`admin.users.delete`; sys-admin session only)",
        procedure: "admin.users.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: None,
        mount: || delete(admin_delete_user),
    },
    // Gitea `adminCronRun` — the one maintenance task we model. Other
    // upstream task names have no analogue and return 404.
    RouteDef {
        method: "POST",
        path: "/admin/cron/{task}",
        tags: &["admin"],
        operation_id: "adminCronRun",
        summary: "Run a maintenance task — `repo_gc_check` only (`admin.repos.gc`)",
        procedure: "admin.repos.gc",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: None,
        mount: || post(admin_cron_run),
    },
];

#[derive(serde::Serialize, Deserialize)]
pub struct AdminUsersQuery {
    /// Username/email substring filter.
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
}

/// `GET /api/v1/admin/users` (`admin.users.list`) — sys-admin only.
async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<AdminUsersQuery>,
) -> Response {
    let input = serde_json::to_value(&q).unwrap_or_else(|_| json!({}));
    call(&state, &headers, "admin.users.list", input, StatusCode::OK).await
}

/// `GET /api/v1/admin/lfs/usage` (`admin.lfs.getUsage`) — instance LFS
/// storage stats, sys-admin only.
async fn lfs_usage(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "admin.lfs.getUsage",
        Value::Object(serde_json::Map::new()),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/admin/actions/runners` (`admin.actions.listRunners`) —
/// every registered runner on the instance, sys-admin only.
async fn admin_runners(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "admin.actions.listRunners",
        Value::Object(serde_json::Map::new()),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/admin/actions/runners/registration-token`
/// (`admin.actions.createRegistrationToken`) — instance-scoped token,
/// sys-admin only.
async fn admin_runner_token(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "admin.actions.createRegistrationToken",
        Value::Object(serde_json::Map::new()),
        StatusCode::CREATED,
    )
    .await
}

#[derive(Deserialize)]
struct AdminUsernamePath {
    username: String,
}

/// Resolve a username to its `AdminUserPublic` row (`admin.users.list` is
/// substring-filtered; we exact-match the `username` field).
async fn admin_user_row(
    state: &AppState,
    headers: &HeaderMap,
    username: &str,
) -> Result<Value, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    match dispatch(
        &mut ctx,
        "admin.users.list",
        json!({ "query": username, "limit": 200 }),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => data["users"]
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|u| u["username"].as_str() == Some(username))
                    .cloned()
            })
            .ok_or_else(|| {
                Box::new(
                    (
                        StatusCode::NOT_FOUND,
                        Json(json!({ "message": "user not found" })),
                    )
                        .into_response(),
                )
            }),
        RpcResponse::Err { error, .. } => Err(Box::new(
            (status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

/// Gitea `User` projection of an `AdminUserPublic` row.
fn admin_user_json(row: &Value) -> Value {
    let username = row["username"].as_str().unwrap_or_default();
    let banned = row["banned_at"].is_string();
    json!({
        "id": row["id"],
        "login": username,
        "username": username,
        "full_name": row["display_name"],
        "email": row["email"],
        "is_admin": row["role"].as_str() == Some("sys-admin"),
        "active": !banned,
        "prohibit_login": banned,
        "restricted": false,
        "visibility": "public",
        "created": row["created_at"],
        "email_verified": row["email_verified"],
    })
}

/// `GET /api/v1/admin/users/{username}` — Gitea `adminGetUser`.
async fn admin_get_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<AdminUsernamePath>,
) -> Response {
    match admin_user_row(&state, &headers, &p.username).await {
        Ok(row) => (StatusCode::OK, Json(admin_user_json(&row))).into_response(),
        Err(res) => *res,
    }
}

/// Gitea `EditUserOption` — `admin` and `active` are the fields our domain
/// model maps (`admin.users.updateRole`, `ban`, `unban`); the remaining
/// upstream fields are accepted for compatibility.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct AdminEditUserBody {
    /// Grant (`true`) or revoke (`false`) the sys-admin role.
    #[serde(default)]
    admin: Option<bool>,
    /// `false` bans the account, `true` lifts the ban.
    #[serde(default)]
    active: Option<bool>,
}

/// `PATCH /api/v1/admin/users/{username}` — Gitea `adminEditUser`.
async fn admin_edit_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<AdminUsernamePath>,
    Json(body): Json<AdminEditUserBody>,
) -> Response {
    let row = match admin_user_row(&state, &headers, &p.username).await {
        Ok(row) => row,
        Err(res) => return *res,
    };
    let user_id = row["id"].as_str().unwrap_or_default().to_string();
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    if let Some(admin) = body.admin {
        let role = if admin { "sys-admin" } else { "user" };
        if let RpcResponse::Err { error, .. } = dispatch(
            &mut ctx,
            "admin.users.updateRole",
            json!({ "user_id": user_id, "role": role }),
        )
        .await
        {
            return (status_for_error(&error), Json(error)).into_response();
        }
    }
    if let Some(active) = body.active {
        let procedure = if active {
            "admin.users.unban"
        } else {
            "admin.users.ban"
        };
        if let RpcResponse::Err { error, .. } =
            dispatch(&mut ctx, procedure, json!({ "user_id": user_id })).await
        {
            return (status_for_error(&error), Json(error)).into_response();
        }
    }
    match admin_user_row(&state, &headers, &p.username).await {
        Ok(row) => (StatusCode::OK, Json(admin_user_json(&row))).into_response(),
        Err(res) => *res,
    }
}

#[derive(Deserialize)]
struct AdminDeleteQuery {
    /// Gitea `purge` — also delete sole-owner orgs the user still shares
    /// with others (maps to `admin.users.delete`'s `delete_orgs` opt-in).
    #[serde(default)]
    purge: Option<bool>,
}

/// `DELETE /api/v1/admin/users/{username}` — Gitea `adminDeleteUser`. The
/// path username doubles as the proc's typed confirmation.
async fn admin_delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<AdminUsernamePath>,
    Query(q): Query<AdminDeleteQuery>,
) -> Response {
    let row = match admin_user_row(&state, &headers, &p.username).await {
        Ok(row) => row,
        Err(res) => return *res,
    };
    call(
        &state,
        &headers,
        "admin.users.delete",
        json!({
            "user_id": row["id"],
            "confirmation": p.username,
            "delete_orgs": q.purge,
        }),
        StatusCode::NO_CONTENT,
    )
    .await
}

#[derive(Deserialize)]
struct AdminTaskPath {
    task: String,
}

/// `POST /api/v1/admin/cron/{task}` — Gitea `adminCronRun`. Only
/// `repo_gc_check` maps to a domain proc (`admin.repos.gc`).
async fn admin_cron_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<AdminTaskPath>,
) -> Response {
    if p.task != "repo_gc_check" {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": format!("task '{}' not found", p.task) })),
        )
            .into_response();
    }
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    match dispatch(&mut ctx, "admin.repos.gc", json!({})).await {
        RpcResponse::Ok { .. } => StatusCode::NO_CONTENT.into_response(),
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

fn admin_user_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "login": { "type": "string" },
            "username": { "type": "string" },
            "full_name": { "type": "string" },
            "email": { "type": "string" },
            "is_admin": { "type": "boolean" },
            "active": { "type": "boolean" },
            "prohibit_login": { "type": "boolean" },
            "restricted": { "type": "boolean" },
            "visibility": { "type": "string" },
            "created": { "type": "string" },
            "email_verified": { "type": "boolean" }
        }
    })
}

fn admin_edit_user_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "admin": {
                "type": "boolean",
                "description": "Grant or revoke the sys-admin role (`admin.users.updateRole`)"
            },
            "active": {
                "type": "boolean",
                "description": "false bans the account, true lifts the ban (`admin.users.ban`/`unban`)"
            }
        }
    })
}
