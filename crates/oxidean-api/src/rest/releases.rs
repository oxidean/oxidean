//! `/api/v1/repos/{owner}/{repo}/releases/**` — releases by tag.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{call, merge_fields, repo_ref, RepoPath};
use crate::app::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repos/{owner}/{repo}/releases",
            get(list_releases).post(create_release),
        )
        .route(
            "/repos/{owner}/{repo}/releases/tags/{*tag}",
            get(get_release)
                .patch(update_release)
                .delete(delete_release),
        )
}

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
