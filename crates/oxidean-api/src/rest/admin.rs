//! `/api/v1/admin/**` — instance administration (sys-admin only; session
//! cookie or appropriately-scoped credentials — `admin.*` procedures are
//! session-only for PATs per API-02).

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Value};

use super::call;
use crate::app::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list_users))
        .route("/admin/lfs/usage", get(lfs_usage))
}

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
