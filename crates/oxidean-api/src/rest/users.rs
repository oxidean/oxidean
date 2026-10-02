//! `/api/v1/user*` + `/api/v1/users/**` — authenticated self + public profiles.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use super::call;
use crate::app::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/user", get(current_user))
        .route("/user/repos", get(list_my_repos).post(create_my_repo))
        .route("/user/orgs", get(list_my_orgs))
        .route("/user/starred", get(list_starred))
        .route("/users/{username}", get(get_user))
        .route("/users/{username}/repos", get(list_user_repos))
}

/// `GET /api/v1/user` (`auth.me`) — the authenticated account.
async fn current_user(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "auth.me",
        Value::Object(serde_json::Map::new()),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/user/repos` (`repo.listMine`) — caller's own repos.
async fn list_my_repos(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "repo.listMine",
        Value::Object(serde_json::Map::new()),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/user/repos` (`repo.create` owned by the caller) —
/// body: `name` (required); `description`, `visibility`, `stack_id`,
/// `instance_pack_id`, `template_repo_id`, `license_id`, `gitignore_id`.
async fn create_my_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    call(&state, &headers, "repo.create", body, StatusCode::CREATED).await
}

/// `GET /api/v1/user/orgs` (`org.listMine`).
async fn list_my_orgs(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "org.listMine",
        Value::Object(serde_json::Map::new()),
        StatusCode::OK,
    )
    .await
}

#[derive(serde::Serialize, Deserialize)]
pub struct StarredQuery {
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

/// `GET /api/v1/user/starred` (`user.listStarred`).
async fn list_starred(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<StarredQuery>,
) -> Response {
    let input = serde_json::to_value(&q).unwrap_or_else(|_| json!({}));
    call(&state, &headers, "user.listStarred", input, StatusCode::OK).await
}

#[derive(Deserialize)]
pub struct UserPath {
    username: String,
}

/// `GET /api/v1/users/{username}` (`user.getPublicProfile`) — public profile,
/// anonymous OK.
async fn get_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<UserPath>,
) -> Response {
    call(
        &state,
        &headers,
        "user.getPublicProfile",
        json!({ "username": p.username }),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/users/{username}/repos` (`repo.listByOwner`).
async fn list_user_repos(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<UserPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.listByOwner",
        json!({ "owner": p.username }),
        StatusCode::OK,
    )
    .await
}
