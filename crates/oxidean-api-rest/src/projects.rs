//! `/api/v1/repos/{owner}/{repo}/projects/**` — classic project boards.
//!
//! GitHub's classic Projects REST addresses boards by a global id; our boards
//! are repo-scoped and addressed by the per-repo `#N`, so the surface uses
//! Gitea's repo-scoped layout (`/repos/{o}/{r}/projects/{number}`) with
//! GitHub field names (`name` → `title`, `column_name`, `content_type` /
//! `content_id` card links). Column/card ids are opaque row ids; moves take a
//! numeric `position` (GitHub's `"first"`/`"last"`/`"after:<id>"` words are
//! not modeled).

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::Json;
use oxidean_core::{ProjectCardPublic, ProjectColumnPublic, ProjectPublic, RpcResponse};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, call_map, ctx_for, dispatch, status_for_error};
use oxidean_api_core::app::AppState;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/projects",
        tags: &["repos", "projects"],
        operation_id: "listRepoProjects",
        summary: "List project boards (`projects.list`) — bare array, `?state=`",
        procedure: "projects.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<ProjectsQuery>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectPublic>,
        )),
        mount: || get(list_projects),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/projects",
        tags: &["repos", "projects"],
        operation_id: "createRepoProject",
        summary: "Create a project board (`projects.create`; `name` → `title`) → 201",
        procedure: "projects.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(project_write_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectPublic>,
        )),
        mount: || post(create_project),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/projects/{project}",
        tags: &["repos", "projects"],
        operation_id: "getRepoProject",
        summary: "Get one project board (`projects.get`)",
        procedure: "projects.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "project"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectPublic>,
        )),
        mount: || get(get_project),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/projects/{project}",
        tags: &["repos", "projects"],
        operation_id: "updateRepoProject",
        summary: "Update a board (`projects.update`; `name` → `title`, `body`, `state`)",
        procedure: "projects.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "project"],
        query: None,
        body: Some(BodySpec::Json(project_write_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectPublic>,
        )),
        mount: || patch(update_project),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/projects/{project}",
        tags: &["repos", "projects"],
        operation_id: "deleteRepoProject",
        summary: "Delete a board (`projects.delete`) → 204",
        procedure: "projects.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "project"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_project),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/projects/{project}/columns",
        tags: &["repos", "projects"],
        operation_id: "listProjectColumns",
        summary: "List columns (`projects.columns.list`) — bare array",
        procedure: "projects.columns.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "project"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectColumnPublic>,
        )),
        mount: || get(list_project_columns),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/projects/{project}/columns",
        tags: &["repos", "projects"],
        operation_id: "createProjectColumn",
        summary: "Create a column (`projects.columns.create`; `name` → `column_name`) → 201",
        procedure: "projects.columns.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "project"],
        query: None,
        body: Some(BodySpec::Json(column_write_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectColumnPublic>,
        )),
        mount: || post(create_project_column),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/projects/{project}/columns/{column}",
        tags: &["repos", "projects"],
        operation_id: "updateProjectColumn",
        summary: "Rename a column (`projects.columns.update`; `name` → `column_name`)",
        procedure: "projects.columns.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "project", "column"],
        query: None,
        body: Some(BodySpec::Json(column_write_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectColumnPublic>,
        )),
        mount: || patch(update_project_column),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/projects/{project}/columns/{column}",
        tags: &["repos", "projects"],
        operation_id: "deleteProjectColumn",
        summary: "Delete a column (`projects.columns.delete`) → 204",
        procedure: "projects.columns.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "project", "column"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_project_column),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/projects/{project}/columns/{column}/moves",
        tags: &["repos", "projects"],
        operation_id: "moveProjectColumn",
        summary: "Reorder a column (`projects.columns.move`) — `{position: int}`",
        procedure: "projects.columns.move",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "project", "column"],
        query: None,
        body: Some(BodySpec::Json(move_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectColumnPublic>,
        )),
        mount: || post(move_project_column),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/projects/{project}/columns/{column}/cards",
        tags: &["repos", "projects"],
        operation_id: "listProjectCards",
        summary: "List cards (`projects.cards.list`) — bare array, `?archived_state=`",
        procedure: "projects.cards.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "column"],
        query: Some(SchemaGenerator::into_root_schema_for::<CardsQuery>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectCardPublic>,
        )),
        mount: || get(list_project_cards),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/projects/{project}/columns/{column}/cards",
        tags: &["repos", "projects"],
        operation_id: "createProjectCard",
        summary:
            "Create a card (`projects.cards.create`; `note` or `content_type`+`content_id`) → 201",
        procedure: "projects.cards.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "column"],
        query: None,
        body: Some(BodySpec::Json(card_write_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectCardPublic>,
        )),
        mount: || post(create_project_card),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/projects/{project}/cards/{card}",
        tags: &["repos", "projects"],
        operation_id: "getProjectCard",
        summary: "One card (`projects.cards.list` filtered by id) — 404 when absent",
        procedure: "projects.cards.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "project", "card"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectCardPublic>,
        )),
        mount: || get(get_project_card),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/projects/{project}/cards/{card}",
        tags: &["repos", "projects"],
        operation_id: "updateProjectCard",
        summary: "Update a card (`projects.cards.update`; `note`, `archived`)",
        procedure: "projects.cards.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "project", "card"],
        query: None,
        body: Some(BodySpec::Json(card_write_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectCardPublic>,
        )),
        mount: || patch(update_project_card),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/projects/{project}/cards/{card}",
        tags: &["repos", "projects"],
        operation_id: "deleteProjectCard",
        summary: "Delete a card (`projects.cards.delete`) → 204",
        procedure: "projects.cards.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "project", "card"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_project_card),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/projects/{project}/cards/{card}/moves",
        tags: &["repos", "projects"],
        operation_id: "moveProjectCard",
        summary: "Move a card (`projects.cards.move`; `{column_id?, position}`)",
        procedure: "projects.cards.move",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "project", "card"],
        query: None,
        body: Some(BodySpec::Json(move_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectCardPublic>,
        )),
        mount: || post(move_project_card),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/projects/{project}/cards/{card}/convert",
        tags: &["repos", "projects"],
        operation_id: "convertProjectCard",
        summary: "Convert a note card to an issue (`projects.cards.convert`)",
        procedure: "projects.cards.convert",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "project", "card"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<ProjectCardPublic>,
        )),
        mount: || post(convert_project_card),
    },
];

#[derive(Deserialize)]
pub struct ProjectListPath {
    owner: String,
    repo: String,
}

#[derive(Deserialize)]
pub struct ProjectPath {
    owner: String,
    repo: String,
    project: i64,
}

#[derive(Deserialize)]
pub struct ProjectColumnPath {
    owner: String,
    repo: String,
    project: i64,
    column: String,
}

#[derive(Deserialize)]
pub struct ProjectCardPath {
    owner: String,
    repo: String,
    project: i64,
    card: String,
}

/// `{owner, repo}` + `{column}` — card-list/create paths carry no `project`.
#[derive(Deserialize)]
pub struct ColumnCardsPath {
    owner: String,
    repo: String,
    column: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ProjectsQuery {
    /// `open` (default) | `closed` | `all`.
    pub state: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CardsQuery {
    /// `all` | `archived` | `not_archived` (GitHub vocabulary; only
    /// `archived`/`all` surface archived cards).
    pub archived_state: Option<String>,
}

fn repo_base(p_owner: &str, p_repo: &str) -> Value {
    json!({"owner": p_owner, "name": p_repo})
}

/// `GET /repos/{o}/{r}/projects` (`projects.list`) — bare array, `?state=`.
async fn list_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectListPath>,
    Query(q): Query<ProjectsQuery>,
) -> Response {
    let mut input = repo_base(&p.owner, &p.repo);
    if let Some(s) = q.state {
        input["state"] = json!(s);
    }
    call_map(
        &state,
        &headers,
        "projects.list",
        input,
        StatusCode::OK,
        |d| d["projects"].clone(),
    )
    .await
}

/// `POST /repos/{o}/{r}/projects` (`projects.create`) — GitHub `name` maps
/// to our `title`.
async fn create_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectListPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut input = repo_base(&p.owner, &p.repo);
    input["title"] = body
        .get("name")
        .or_else(|| body.get("title"))
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(b) = body.get("body") {
        input["body"] = b.clone();
    }
    call(
        &state,
        &headers,
        "projects.create",
        input,
        StatusCode::CREATED,
    )
    .await
}

/// `GET /repos/{o}/{r}/projects/{project}` (`projects.get`).
async fn get_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectPath>,
) -> Response {
    call(
        &state,
        &headers,
        "projects.get",
        json!({"owner": p.owner, "name": p.repo, "project": p.project}),
        StatusCode::OK,
    )
    .await
}

/// `PATCH /repos/{o}/{r}/projects/{project}` (`projects.update`).
async fn update_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut input = json!({"owner": p.owner, "name": p.repo, "project": p.project});
    for (rest_key, rpc_key) in [
        ("name", "title"),
        ("title", "title"),
        ("body", "body"),
        ("state", "state"),
    ] {
        if let Some(v) = body.get(rest_key) {
            input[rpc_key] = v.clone();
        }
    }
    call(&state, &headers, "projects.update", input, StatusCode::OK).await
}

/// `DELETE /repos/{o}/{r}/projects/{project}` (`projects.delete`) → 204.
async fn delete_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectPath>,
) -> Response {
    call(
        &state,
        &headers,
        "projects.delete",
        json!({"owner": p.owner, "name": p.repo, "project": p.project}),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `GET /repos/{o}/{r}/projects/{project}/columns` (`projects.columns.list`).
async fn list_project_columns(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "projects.columns.list",
        json!({"owner": p.owner, "name": p.repo, "project": p.project}),
        StatusCode::OK,
        |d| d["columns"].clone(),
    )
    .await
}

/// `POST /repos/{o}/{r}/projects/{project}/columns` (`projects.columns.create`).
async fn create_project_column(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "project": p.project,
        "column_name": body["name"],
    });
    call(
        &state,
        &headers,
        "projects.columns.create",
        input,
        StatusCode::CREATED,
    )
    .await
}

/// `PATCH .../columns/{column}` (`projects.columns.update`).
async fn update_project_column(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectColumnPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "project": p.project,
        "column": p.column,
        "column_name": body["name"],
    });
    call(
        &state,
        &headers,
        "projects.columns.update",
        input,
        StatusCode::OK,
    )
    .await
}

/// `DELETE .../columns/{column}` (`projects.columns.delete`) → 204.
async fn delete_project_column(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectColumnPath>,
) -> Response {
    call(
        &state,
        &headers,
        "projects.columns.delete",
        json!({"owner": p.owner, "name": p.repo, "project": p.project, "column": p.column}),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `POST .../columns/{column}/moves` (`projects.columns.move`) — GitHub's
/// word vocabulary isn't modeled; `position` is a 0-based int.
async fn move_project_column(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectColumnPath>,
    Json(body): Json<Value>,
) -> Response {
    let Some(position) = body.get("position").and_then(Value::as_i64) else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"code": "rpc.bad_input", "message": "position must be a 0-based index"})),
        )
            .into_response();
    };
    call(
        &state,
        &headers,
        "projects.columns.move",
        json!({"owner": p.owner, "name": p.repo, "project": p.project, "column": p.column, "position": position}),
        StatusCode::OK,
    )
    .await
}

/// `GET .../columns/{column}/cards` (`projects.cards.list`) — bare array.
/// GitHub `archived_state` maps to our `archived` flag.
async fn list_project_cards(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ColumnCardsPath>,
    Query(q): Query<CardsQuery>,
) -> Response {
    let mut input = repo_base(&p.owner, &p.repo);
    input["column"] = json!(p.column);
    match q.archived_state.as_deref() {
        Some("all") | Some("archived") => input["archived"] = json!(true),
        _ => {}
    }
    call_map(
        &state,
        &headers,
        "projects.cards.list",
        input,
        StatusCode::OK,
        |d| {
            let mut cards = d["cards"].clone();
            if q.archived_state.as_deref() == Some("not_archived") || q.archived_state.is_none() {
                if let Some(arr) = cards.as_array_mut() {
                    arr.retain(|c| c["archived"] != true);
                }
            }
            cards
        },
    )
    .await
}

/// `POST .../columns/{column}/cards` (`projects.cards.create`) — `{note}` or
/// `{content_type, content_id}` (issue/pull link).
async fn create_project_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ColumnCardsPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut input = repo_base(&p.owner, &p.repo);
    input["column"] = json!(p.column);
    for k in ["note", "content_type", "content_id"] {
        if let Some(v) = body.get(k) {
            input[k] = v.clone();
        }
    }
    call(
        &state,
        &headers,
        "projects.cards.create",
        input,
        StatusCode::CREATED,
    )
    .await
}

/// Column-context card lookup (no dedicated `cards.get` proc — the list is
/// scoped by column id, so find the column containing the card via the
/// project + id).
async fn find_project_card(
    state: &AppState,
    headers: &HeaderMap,
    p: &ProjectCardPath,
) -> Result<Value, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await?;
    let cols = match dispatch(
        &mut ctx,
        "projects.columns.list",
        json!({"owner": p.owner, "name": p.repo, "project": p.project}),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return Err(Box::new(
                (status_for_error(&error), Json(error)).into_response(),
            ))
        }
    };
    for col in cols["columns"].as_array().cloned().unwrap_or_default() {
        let cards = match dispatch(
            &mut ctx,
            "projects.cards.list",
            json!({
                "owner": p.owner,
                "name": p.repo,
                "column": col["id"],
                "archived": true,
            }),
        )
        .await
        {
            RpcResponse::Ok { data, .. } => data,
            RpcResponse::Err { error, .. } => {
                return Err(Box::new(
                    (status_for_error(&error), Json(error)).into_response(),
                ))
            }
        };
        if let Some(card) = cards["cards"].as_array().and_then(|cs| {
            cs.iter()
                .find(|c| c["id"].as_str() == Some(p.card.as_str()))
        }) {
            return Ok(card.clone());
        }
    }
    Err(Box::new(
        (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "project.card_not_found", "message": "card not found"})),
        )
            .into_response(),
    ))
}

/// `GET .../cards/{card}` (`projects.cards.list` filtered).
async fn get_project_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectCardPath>,
) -> Response {
    match find_project_card(&state, &headers, &p).await {
        Ok(card) => (StatusCode::OK, Json(card)).into_response(),
        Err(r) => *r,
    }
}

/// `PATCH .../cards/{card}` (`projects.cards.update`).
async fn update_project_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectCardPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut input = json!({
        "owner": p.owner,
        "name": p.repo,
        "project": p.project,
        "card": p.card,
    });
    for k in ["note", "archived"] {
        if let Some(v) = body.get(k) {
            input[k] = v.clone();
        }
    }
    call(
        &state,
        &headers,
        "projects.cards.update",
        input,
        StatusCode::OK,
    )
    .await
}

/// `DELETE .../cards/{card}` (`projects.cards.delete`) → 204.
async fn delete_project_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectCardPath>,
) -> Response {
    call(
        &state,
        &headers,
        "projects.cards.delete",
        json!({"owner": p.owner, "name": p.repo, "project": p.project, "card": p.card}),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `POST .../cards/{card}/moves` (`projects.cards.move`) —
/// `{column_id?→column, position}`.
async fn move_project_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectCardPath>,
    Json(body): Json<Value>,
) -> Response {
    let Some(position) = body.get("position").and_then(Value::as_i64) else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"code": "rpc.bad_input", "message": "position must be a 0-based index"})),
        )
            .into_response();
    };
    let mut input = json!({
        "owner": p.owner,
        "name": p.repo,
        "project": p.project,
        "card": p.card,
        "position": position,
    });
    if let Some(col) = body.get("column_id").or_else(|| body.get("column")) {
        input["column"] = col.clone();
    }
    call(
        &state,
        &headers,
        "projects.cards.move",
        input,
        StatusCode::OK,
    )
    .await
}

/// `POST .../cards/{card}/convert` (`projects.cards.convert`) — note card →
/// real issue.
async fn convert_project_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ProjectCardPath>,
) -> Response {
    call(
        &state,
        &headers,
        "projects.cards.convert",
        json!({"owner": p.owner, "name": p.repo, "project": p.project, "card": p.card}),
        StatusCode::OK,
    )
    .await
}

fn project_write_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string", "description": "Board title (GitHub `name`)"},
            "title": {"type": "string"},
            "body": {"type": "string"},
            "state": {"type": "string", "enum": ["open", "closed"], "description": "PATCH only"}
        }
    })
}

fn column_write_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["name"],
        "properties": {"name": {"type": "string"}}
    })
}

fn card_write_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "note": {"type": "string"},
            "content_type": {"type": "string", "enum": ["issue", "pull_request"]},
            "content_id": {"type": "string", "description": "Linked entity row id"},
            "archived": {"type": "boolean", "description": "PATCH only"}
        }
    })
}

fn move_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["position"],
        "properties": {
            "position": {"type": "integer", "description": "0-based destination index"},
            "column_id": {"type": "string", "description": "Card moves only — destination column id"}
        }
    })
}
