//! `/api/v1/repos/{owner}/{repo}/releases/**` — releases by tag.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::{delete, get, patch, post};
use axum::Json;
use oxidean_core::{
    CreateReleaseRequest, DeleteReleaseResponse, ReleaseListResponse, ReleasePublic, RpcResponse,
    UpdateReleaseRequest,
};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, ctx_for, dispatch, github, merge_fields, repo_ref, status_for_error, RepoPath};
use oxidean_api_core::app::AppState;
use oxidean_core::AppError;

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
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/releases/latest",
        tags: &["releases"],
        operation_id: "getLatestRelease",
        summary: "Latest release (`release.list`, newest non-draft non-prerelease) — 404 when none",
        procedure: "release.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ReleasePublic>,
        )),
        mount: || get(get_latest_release),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/releases/{release_id}",
        tags: &["releases"],
        operation_id: "getReleaseById",
        summary: "Release by id (`release.list` resolved → `release.get`)",
        procedure: "release.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ReleasePublic>,
        )),
        mount: || get(get_release_by_id),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/releases/{release_id}",
        tags: &["releases"],
        operation_id: "updateReleaseById",
        summary: "Update release by id (`release.update` via resolved tag)",
        procedure: "release.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<UpdateReleaseRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ReleasePublic>,
        )),
        mount: || patch(update_release_by_id),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/releases/{release_id}",
        tags: &["releases"],
        operation_id: "deleteReleaseById",
        summary: "Delete release by id (`release.delete` via resolved tag) → 204",
        procedure: "release.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_release_by_id),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/releases/{release_id}/assets",
        tags: &["releases"],
        operation_id: "listReleaseAssets",
        summary: "Release assets (`release.list` resolved) — bare array",
        procedure: "release.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(release_assets_schema)),
        mount: || get(list_release_assets),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/releases/assets/{asset_id}",
        tags: &["releases"],
        operation_id: "deleteReleaseAsset",
        summary: "Delete a release asset (`release.deleteAsset`) → 204",
        procedure: "release.deleteAsset",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_release_asset),
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

// ---------------------------------------------------------------------------
// Id-addressed + latest + assets — GitHub routes resolve `release_id` /
// `latest` through `release.list`, then run the tag-keyed procs.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReleaseIdPath {
    owner: String,
    repo: String,
    release_id: String,
}

#[derive(Deserialize)]
pub struct ReleaseAssetPath {
    owner: String,
    repo: String,
    asset_id: String,
}

async fn releases_for(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
) -> Result<Vec<Value>, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await?;
    match dispatch(&mut ctx, "release.list", repo_ref(owner, repo)).await {
        RpcResponse::Ok { data, .. } => {
            Ok(data["releases"].as_array().cloned().unwrap_or_default())
        }
        RpcResponse::Err { error, .. } => Err(Box::new(if state.rest_shim_enabled {
            github::error_compat(&error, headers)
        } else {
            (status_for_error(&error), Json(error)).into_response()
        })),
    }
}

/// Resolve `release_id` to the release row's `tag_name` (releases are
/// tag-keyed at the RPC layer).
async fn release_tag_for_id(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    id: &str,
) -> Result<Value, Box<Response>> {
    let releases = releases_for(state, headers, owner, repo).await?;
    releases
        .into_iter()
        .find(|r| r["id"].as_str() == Some(id))
        .ok_or_else(|| {
            Box::new(if state.rest_shim_enabled {
                github::error_compat(
                    &AppError::new("release.not_found", "release not found"),
                    headers,
                )
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "release.not_found", "message": "release not found"})),
                )
                    .into_response()
            })
        })
}

/// `GET /repos/{o}/{r}/releases/latest` — newest non-draft, non-prerelease
/// by `created_at` (GitHub "latest" semantics).
async fn get_latest_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    let releases = match releases_for(&state, &headers, &p.owner, &p.repo).await {
        Ok(rs) => rs,
        Err(r) => return *r,
    };
    let latest = releases
        .iter()
        .filter(|r| r["draft"] != true && r["prerelease"] != true)
        .max_by(|a, b| a["created_at"].as_str().cmp(&b["created_at"].as_str()));
    match latest {
        Some(r) if state.rest_shim_enabled => github::respond_data_compat(
            r.clone(),
            StatusCode::OK,
            "release.get",
            &repo_ref(&p.owner, &p.repo),
            &headers,
        ),
        Some(r) => (StatusCode::OK, Json(r.clone())).into_response(),
        None if state.rest_shim_enabled => github::error_compat(
            &AppError::new("release.not_found", "no non-draft release"),
            &headers,
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "release.not_found", "message": "no non-draft release"})),
        )
            .into_response(),
    }
}

/// `GET /repos/{o}/{r}/releases/{release_id}` (`release.get` via resolved tag).
async fn get_release_by_id(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ReleaseIdPath>,
) -> Response {
    match release_tag_for_id(&state, &headers, &p.owner, &p.repo, &p.release_id).await {
        Ok(rel) if state.rest_shim_enabled => github::respond_data_compat(
            rel,
            StatusCode::OK,
            "release.get",
            &repo_ref(&p.owner, &p.repo),
            &headers,
        ),
        Ok(rel) => (StatusCode::OK, Json(rel)).into_response(),
        Err(r) => *r,
    }
}

/// `PATCH /repos/{o}/{r}/releases/{release_id}` (`release.update`).
async fn update_release_by_id(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ReleaseIdPath>,
    Json(body): Json<Value>,
) -> Response {
    let rel = match release_tag_for_id(&state, &headers, &p.owner, &p.repo, &p.release_id).await {
        Ok(r) => r,
        Err(r) => return *r,
    };
    let input = merge_fields(
        json!({"owner": p.owner, "name": p.repo, "tag_name": rel["tag_name"]}),
        body,
    );
    call(&state, &headers, "release.update", input, StatusCode::OK).await
}

/// `DELETE /repos/{o}/{r}/releases/{release_id}` (`release.delete`) → 204.
async fn delete_release_by_id(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ReleaseIdPath>,
) -> Response {
    let rel = match release_tag_for_id(&state, &headers, &p.owner, &p.repo, &p.release_id).await {
        Ok(r) => r,
        Err(r) => return *r,
    };
    call(
        &state,
        &headers,
        "release.delete",
        json!({"owner": p.owner, "name": p.repo, "tag_name": rel["tag_name"]}),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `GET /repos/{o}/{r}/releases/{release_id}/assets` — bare array.
async fn list_release_assets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ReleaseIdPath>,
) -> Response {
    match release_tag_for_id(&state, &headers, &p.owner, &p.repo, &p.release_id).await {
        Ok(rel) => {
            let assets = rel["assets"].as_array().cloned().unwrap_or_default();
            if state.rest_shim_enabled {
                return github::respond_data_compat(
                    json!({"assets": assets}),
                    StatusCode::OK,
                    "release.assets.list",
                    &repo_ref(&p.owner, &p.repo),
                    &headers,
                );
            }
            (StatusCode::OK, Json(assets)).into_response()
        }
        Err(r) => *r,
    }
}

/// `DELETE /repos/{o}/{r}/releases/assets/{asset_id}` (`release.deleteAsset`).
async fn delete_release_asset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ReleaseAssetPath>,
) -> Response {
    call(
        &state,
        &headers,
        "release.deleteAsset",
        json!({"owner": p.owner, "name": p.repo, "asset_id": p.asset_id}),
        StatusCode::NO_CONTENT,
    )
    .await
}

fn release_assets_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"$ref": "#/components/schemas/ReleaseAssetPublic"}})
}
