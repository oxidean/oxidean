//! `/api/v1/admin/**` — instance administration (sys-admin only; session
//! cookie or appropriately-scoped credentials — `admin.*` procedures are
//! session-only for PATs per API-02).

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use oxidean_core::{
    ActionListRunnersResponse, ActionRegistrationTokenResponse, AdminLfsUsageResponse,
    AdminUsersListRequest, AdminUsersListResponse,
};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::call;
use super::spec::{RespSpec, RouteDef};
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
