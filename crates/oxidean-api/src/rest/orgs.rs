//! `/api/v1/orgs/**` — organizations.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{call, merge_fields};
use crate::app::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orgs", axum::routing::post(create_org))
        .route("/orgs/{slug}", get(get_org))
        .route("/orgs/{slug}/members", get(list_org_members))
        .route(
            "/orgs/{slug}/repos",
            get(list_org_repos).post(create_org_repo),
        )
}

/// `POST /api/v1/orgs` (`org.create`) — body: `slug`, `display_name`.
/// Session-cookie only; PATs cannot create orgs (API-02 fail-closed).
async fn create_org(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    call(&state, &headers, "org.create", body, StatusCode::CREATED).await
}

#[derive(Deserialize)]
pub struct OrgPath {
    slug: String,
}

/// `GET /api/v1/orgs/{slug}` (`org.get`) — anonymous OK for public org data.
async fn get_org(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgPath>,
) -> Response {
    call(
        &state,
        &headers,
        "org.get",
        json!({ "slug": p.slug }),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/orgs/{slug}/members` (`org.members.list`).
async fn list_org_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgPath>,
) -> Response {
    call(
        &state,
        &headers,
        "org.members.list",
        json!({ "slug": p.slug }),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/orgs/{slug}/repos` (`repo.listByOwner` — owner slug works for
/// orgs and users alike).
async fn list_org_repos(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.listByOwner",
        json!({ "owner": p.slug }),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/orgs/{slug}/repos` (`repo.create` with `owner` = org slug).
async fn create_org_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(json!({ "owner": p.slug }), body);
    call(&state, &headers, "repo.create", input, StatusCode::CREATED).await
}
