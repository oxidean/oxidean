//! `/api/v1/repos/{owner}/{repo}/wiki/**` — Gitea-shaped wiki pages surface
//! over the git-backed `wiki.pages.*` / `wiki.revisions.list` procedures.
//!
//! Page write bodies accept `content` (plain Markdown) or `content_base64`
//! (Gitea convention); `page_name` path params are `.md` file stems
//! (`Home`, `_Sidebar`, …).

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::Json;
use base64::Engine;
use oxidean_core::{WikiPagePublic, WikiPagesListResponse, WikiRevisionsListResponse};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, call_map};
use crate::app::AppState;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/wiki/pages",
        tags: &["repos", "wiki"],
        operation_id: "listWikiPages",
        summary: "List wiki pages (`wiki.pages.list`) — bare array",
        procedure: "wiki.pages.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<WikiPagesListResponse>,
        )),
        mount: || get(list_wiki_pages),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/wiki/new",
        tags: &["repos", "wiki"],
        operation_id: "createWikiPage",
        summary: "Create a wiki page (`wiki.pages.create`; `content` or `content_base64`) → 201",
        procedure: "wiki.pages.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(wiki_write_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<WikiPagePublic>,
        )),
        mount: || post(create_wiki_page),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/wiki/revisions/{page_name}",
        tags: &["repos", "wiki"],
        operation_id: "listWikiRevisions",
        summary: "Page revision history (`wiki.revisions.list`) — bare array",
        procedure: "wiki.revisions.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "page"],
        query: Some(SchemaGenerator::into_root_schema_for::<WikiRevisionsQuery>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<WikiRevisionsListResponse>,
        )),
        mount: || get(list_wiki_revisions),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/wiki/page/{page_name}",
        tags: &["repos", "wiki"],
        operation_id: "getWikiPage",
        summary: "Get a wiki page with content (`wiki.pages.get`)",
        procedure: "wiki.pages.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "page"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<WikiPagePublic>,
        )),
        mount: || get(get_wiki_page),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/wiki/page/{page_name}",
        tags: &["repos", "wiki"],
        operation_id: "updateWikiPage",
        summary: "Update a wiki page (`wiki.pages.update`; `content` or `content_base64`)",
        procedure: "wiki.pages.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "page"],
        query: None,
        body: Some(BodySpec::Json(wiki_write_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<WikiPagePublic>,
        )),
        mount: || patch(update_wiki_page),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/wiki/page/{page_name}",
        tags: &["repos", "wiki"],
        operation_id: "deleteWikiPage",
        summary: "Delete a wiki page (`wiki.pages.delete`) → 204",
        procedure: "wiki.pages.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "page"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_wiki_page),
    },
];

#[derive(Deserialize)]
pub struct WikiPagePath {
    owner: String,
    repo: String,
    page_name: String,
}

#[derive(Deserialize)]
pub struct WikiRepoPath {
    owner: String,
    repo: String,
}

/// `?limit=` on the revisions route.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct WikiRevisionsQuery {
    pub limit: Option<u32>,
}

/// `GET /repos/{o}/{r}/wiki/pages` (`wiki.pages.list`) — bare array.
async fn list_wiki_pages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<WikiRepoPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "wiki.pages.list",
        json!({"owner": p.owner, "name": p.repo}),
        StatusCode::OK,
        |d| d["pages"].clone(),
    )
    .await
}

/// `GET /repos/{o}/{r}/wiki/page/{page_name}` (`wiki.pages.get`) — page +
/// Markdown content.
async fn get_wiki_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<WikiPagePath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "wiki.pages.get",
        json!({"owner": p.owner, "name": p.repo, "page": p.page_name}),
        StatusCode::OK,
        |d| d["page"].clone(),
    )
    .await
}

/// `POST /repos/{o}/{r}/wiki/new` (`wiki.pages.create`) — unwraps `{page}` →
/// 201. Accepts `content` (plain) or `content_base64` (Gitea).
async fn create_wiki_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<WikiRepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let content = match wiki_content(&body) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let mut input = json!({
        "owner": p.owner,
        "name": p.repo,
        "title": body["title"],
        "content": content,
    });
    if let Some(m) = body.get("message") {
        input["message"] = m.clone();
    }
    call_map(
        &state,
        &headers,
        "wiki.pages.create",
        input,
        StatusCode::CREATED,
        |d| d["page"].clone(),
    )
    .await
}

/// `PATCH /repos/{o}/{r}/wiki/page/{page_name}` (`wiki.pages.update`) —
/// unwraps `{page}`. `new_title`/`title` rename is not modeled (wiki file
/// stems are immutable via this surface).
async fn update_wiki_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<WikiPagePath>,
    Json(body): Json<Value>,
) -> Response {
    let content = match wiki_content(&body) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let mut input = json!({
        "owner": p.owner,
        "name": p.repo,
        "page": p.page_name,
        "content": content,
    });
    if let Some(m) = body.get("message") {
        input["message"] = m.clone();
    }
    call_map(
        &state,
        &headers,
        "wiki.pages.update",
        input,
        StatusCode::OK,
        |d| d["page"].clone(),
    )
    .await
}

/// `DELETE /repos/{o}/{r}/wiki/page/{page_name}` (`wiki.pages.delete`) → 204.
async fn delete_wiki_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<WikiPagePath>,
) -> Response {
    call(
        &state,
        &headers,
        "wiki.pages.delete",
        json!({"owner": p.owner, "name": p.repo, "page": p.page_name}),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `GET /repos/{o}/{r}/wiki/revisions/{page_name}` (`wiki.revisions.list`) —
/// bare array.
async fn list_wiki_revisions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<WikiPagePath>,
    Query(q): Query<WikiRevisionsQuery>,
) -> Response {
    call_map(
        &state,
        &headers,
        "wiki.revisions.list",
        json!({
            "owner": p.owner,
            "name": p.repo,
            "page": p.page_name,
            "limit": q.limit,
        }),
        StatusCode::OK,
        |d| d["revisions"].clone(),
    )
    .await
}

/// `content` (plain Markdown) or `content_base64` (Gitea wire shape).
fn wiki_content(body: &Value) -> Result<String, Box<Response>> {
    if let Some(b64) = body.get("content_base64").and_then(Value::as_str) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| {
                Box::new(
                    (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({"code": "rpc.bad_input", "message": "content_base64 is not valid base64"})),
                    )
                        .into_response(),
                )
            })?;
        return String::from_utf8(bytes).map_err(|_| {
            Box::new(
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(
                        json!({"code": "rpc.bad_input", "message": "content_base64 is not UTF-8"}),
                    ),
                )
                    .into_response(),
            )
        });
    }
    match body.get("content").and_then(Value::as_str) {
        Some(c) => Ok(c.to_string()),
        None => Err(Box::new(
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"code": "rpc.bad_input", "message": "content is required"})),
            )
                .into_response(),
        )),
    }
}

fn wiki_write_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": {"type": "string", "description": "Page title (create)"},
            "content": {"type": "string", "description": "Markdown body"},
            "content_base64": {"type": "string", "description": "Base64 Markdown body (Gitea shape)"},
            "message": {"type": "string", "description": "Commit message"}
        }
    })
}
