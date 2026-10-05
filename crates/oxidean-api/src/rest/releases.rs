//! `/api/v1/repos/{owner}/{repo}/releases/**` — releases by tag.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{delete, get, patch, post};
use axum::Json;
use oxidean_core::{
    CreateReleaseRequest, DeleteReleaseResponse, ReleaseListResponse, ReleasePublic,
    UpdateReleaseRequest,
};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, merge_fields, repo_ref, RepoPath};
use crate::app::AppState;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/releases",
        tags: &["releases"],
        operation_id: "listReleases",
        summary: "List releases (`release.list`)",
        procedure: "release.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ReleaseListResponse>,
        )),
        mount: || get(list_releases),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/releases",
        tags: &["releases"],
        operation_id: "createRelease",
        summary: "Create release (`release.create`; tag must exist)",
        procedure: "release.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateReleaseRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ReleasePublic>,
        )),
        mount: || post(create_release),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/releases/tags/{*tag}",
        tags: &["releases"],
        operation_id: "getReleaseByTag",
        summary: "Release by tag (`release.get`); slashed tags via %-encoding",
        procedure: "release.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "tag_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ReleasePublic>,
        )),
        mount: || get(get_release),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/releases/tags/{*tag}",
        tags: &["releases"],
        operation_id: "updateRelease",
        summary: "Update release (`release.update`)",
        procedure: "release.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "tag_name"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<UpdateReleaseRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ReleasePublic>,
        )),
        mount: || patch(update_release),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/releases/tags/{*tag}",
        tags: &["releases"],
        operation_id: "deleteRelease",
        summary: "Delete release (`release.delete`)",
        procedure: "release.delete",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "tag_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<DeleteReleaseResponse>,
        )),
        mount: || delete(delete_release),
    },
];

/// `GET /api/v1/repos/{owner}/{repo}/releases` (`release.list`).
async fn list_releases(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call(
        &state,
        &headers,
        "release.list",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/repos/{owner}/{repo}/releases` (`release.create`) —
/// body: `tag_name` (required); `title`, `body`, `draft`, `prerelease`.
async fn create_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(repo_ref(&p.owner, &p.repo), body);
    call(
        &state,
        &headers,
        "release.create",
        input,
        StatusCode::CREATED,
    )
    .await
}

/// `{*tag}` — wildcard so slashed tags (`release/1.0`, %-encoded) resolve.
#[derive(Deserialize)]
pub struct TagPath {
    owner: String,
    repo: String,
    tag: String,
}

impl TagPath {
    fn input(&self) -> Value {
        json!({ "owner": self.owner, "name": self.repo, "tag_name": self.tag })
    }
}

/// `GET /api/v1/repos/{owner}/{repo}/releases/tags/{tag}` (`release.get`).
async fn get_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TagPath>,
) -> Response {
    call(&state, &headers, "release.get", p.input(), StatusCode::OK).await
}

/// `PATCH /api/v1/repos/{owner}/{repo}/releases/tags/{tag}` (`release.update`) —
/// body: `title`, `body`, `draft`, `prerelease` (all optional).
async fn update_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TagPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(p.input(), body);
    call(&state, &headers, "release.update", input, StatusCode::OK).await
}

/// `DELETE /api/v1/repos/{owner}/{repo}/releases/tags/{tag}` (`release.delete`).
async fn delete_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TagPath>,
) -> Response {
    call(
        &state,
        &headers,
        "release.delete",
        p.input(),
        StatusCode::OK,
    )
    .await
}
