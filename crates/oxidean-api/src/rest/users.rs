//! `/api/v1/user*` + `/api/v1/users/**` — authenticated self + public profiles.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Json;
use oxidean_core::{
    CreateRepoRequest, ListStarredRequest, OrgListMineResponse, PublicUserProfile,
    RepoListMineResponse, RepoPublic, UserPublic,
};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::call;
use super::spec::{BodySpec, RespSpec, RouteDef};
use crate::app::AppState;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/user",
        tags: &["users"],
        operation_id: "getAuthenticatedUser",
        summary: "Authenticated user (`auth.me`)",
        procedure: "auth.me",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<UserPublic>,
        )),
        mount: || get(current_user),
    },
    RouteDef {
        method: "GET",
        path: "/user/repos",
        tags: &["users", "repos"],
        operation_id: "listMyRepos",
        summary: "List repos owned by the caller (`repo.listMine`)",
        procedure: "repo.listMine",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoListMineResponse>,
        )),
        mount: || get(list_my_repos),
    },
    RouteDef {
        method: "POST",
        path: "/user/repos",
        tags: &["users", "repos"],
        operation_id: "createMyRepo",
        summary: "Create a repo owned by the caller (`repo.create`)",
        procedure: "repo.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateRepoRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoPublic>,
        )),
        mount: || post(create_my_repo),
    },
    RouteDef {
        method: "GET",
        path: "/user/orgs",
        tags: &["users", "orgs"],
        operation_id: "listMyOrgs",
        summary: "Orgs the caller belongs to (`org.listMine`)",
        procedure: "org.listMine",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<OrgListMineResponse>,
        )),
        mount: || get(list_my_orgs),
    },
    RouteDef {
        method: "GET",
        path: "/user/starred",
        tags: &["users", "repos"],
        operation_id: "listStarred",
        summary: "Repos starred by the caller (`user.listStarred`)",
        procedure: "user.listStarred",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: Some(SchemaGenerator::into_root_schema_for::<ListStarredRequest>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoListMineResponse>,
        )),
        mount: || get(list_starred),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}",
        tags: &["users"],
        operation_id: "getUser",
        summary: "Public profile (`user.getPublicProfile`)",
        procedure: "user.getPublicProfile",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["username"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PublicUserProfile>,
        )),
        mount: || get(get_user),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}/repos",
        tags: &["users", "repos"],
        operation_id: "listUserRepos",
        summary: "Repos under a user (`repo.listByOwner`)",
        procedure: "repo.listByOwner",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["username"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoListMineResponse>,
        )),
        mount: || get(list_user_repos),
    },
];

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
