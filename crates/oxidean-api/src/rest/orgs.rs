//! `/api/v1/orgs/**` — organizations.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Json;
use oxidean_core::{
    CreateOrgRequest, CreateRepoRequest, OrgMembersListResponse, OrgPublic, RepoListMineResponse,
    RepoPublic,
};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, merge_fields};
use crate::app::AppState;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "POST",
        path: "/orgs",
        tags: &["orgs"],
        operation_id: "createOrg",
        summary: "Create an org (`org.create`) — session cookie only",
        procedure: "org.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateOrgRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<OrgPublic>,
        )),
        mount: || post(create_org),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{slug}",
        tags: &["orgs"],
        operation_id: "getOrg",
        summary: "Get an org (`org.get`) — anonymous OK for public org data",
        procedure: "org.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["slug"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<OrgPublic>,
        )),
        mount: || get(get_org),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{slug}/members",
        tags: &["orgs"],
        operation_id: "listOrgMembers",
        summary: "Org members (`org.members.list`)",
        procedure: "org.members.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<OrgMembersListResponse>,
        )),
        mount: || get(list_org_members),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{slug}/repos",
        tags: &["orgs", "repos"],
        operation_id: "listOrgRepos",
        summary: "Repos under an org (`repo.listByOwner`)",
        procedure: "repo.listByOwner",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoListMineResponse>,
        )),
        mount: || get(list_org_repos),
    },
    RouteDef {
        method: "POST",
        path: "/orgs/{slug}/repos",
        tags: &["orgs", "repos"],
        operation_id: "createOrgRepo",
        summary: "Create an org repo (`repo.create` with `owner` = slug)",
        procedure: "repo.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateRepoRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoPublic>,
        )),
        mount: || post(create_org_repo),
    },
];

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
