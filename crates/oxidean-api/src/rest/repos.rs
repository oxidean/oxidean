//! `/api/v1/repos/**` — repository metadata, git browsing, labels, hooks.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::Json;
use oxidean_core::{
    CreateWebhookRequest, LabelsListResponse, ListLabelsForRepoRequest, RepoCommitResponse,
    RepoCommitsResponse, RepoCompareResponse, RepoExploreRequest, RepoLanguagesResponse,
    RepoListMineResponse, RepoPublic, RepoRefEntry, RepoSoftDeleteResponse,
    RepoStargazersListRequest, RepoStargazersListResponse, RepoTreeResponse,
    RepoUpdateMetadataRequest, UpdateWebhookRequest, WebhookListResponse, WebhookPublic,
};
use oxidean_core::{DeleteWebhookResponse, RepoBlobResponse};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, call_map, merge_fields, repo_ref, RepoPath};
use crate::app::AppState;

/// `repo.refs` entries filtered to `refs/heads/*` and renamed to short
/// names — `{branches: [...]}`.
fn branches_schema(gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["branches"],
        "properties": {
            "branches": {
                "type": "array",
                "items": gen.subschema_for::<RepoRefEntry>(),
            },
        },
    })
}

/// Same for `refs/tags/*` — `{tags: [...]}`.
fn tags_schema(gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["tags"],
        "properties": {
            "tags": {
                "type": "array",
                "items": gen.subschema_for::<RepoRefEntry>(),
            },
        },
    })
}

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos",
        tags: &["repos"],
        operation_id: "exploreRepos",
        summary: "Explore public repositories (`repo.explore`)",
        procedure: "repo.explore",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &[],
        query: Some(SchemaGenerator::into_root_schema_for::<RepoExploreRequest>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoListMineResponse>,
        )),
        mount: || get(list_repositories),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}",
        tags: &["repos"],
        operation_id: "getRepo",
        summary: "Get a repository (`repo.get`) — anonymous OK for public",
        procedure: "repo.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoPublic>,
        )),
        mount: || get(get_repo),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}",
        tags: &["repos"],
        operation_id: "updateRepo",
        summary: "Update repo metadata (`repo.updateMetadata`)",
        procedure: "repo.updateMetadata",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<RepoUpdateMetadataRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoPublic>,
        )),
        mount: || patch(update_repo),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}",
        tags: &["repos"],
        operation_id: "deleteRepo",
        summary: "Soft-delete a repository (`repo.softDelete`; the URL is the typed confirmation)",
        procedure: "repo.softDelete",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoSoftDeleteResponse>,
        )),
        mount: || delete(delete_repo),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/branches",
        tags: &["repos"],
        operation_id: "listBranches",
        summary: "List branches (`repo.refs` filtered to `refs/heads/*`)",
        procedure: "repo.refs",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(branches_schema)),
        mount: || get(list_branches),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/tags",
        tags: &["repos"],
        operation_id: "listTags",
        summary: "List tags (`repo.refs` filtered to `refs/tags/*`)",
        procedure: "repo.refs",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(tags_schema)),
        mount: || get(list_tags),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/commits",
        tags: &["repos"],
        operation_id: "listCommits",
        summary: "Commit log (`repo.commits`)",
        procedure: "repo.commits",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<CommitsQuery>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoCommitsResponse>,
        )),
        mount: || get(list_commits),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/commits/{sha}",
        tags: &["repos"],
        operation_id: "getCommit",
        summary: "Single commit with diff (`repo.commit`)",
        procedure: "repo.commit",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "sha"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoCommitResponse>,
        )),
        mount: || get(get_commit),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/compare/{basehead}",
        tags: &["repos"],
        operation_id: "compareRefs",
        summary: "Compare two refs (`repo.compare`); `{basehead}` = `base...head` (`..` accepted)",
        procedure: "repo.compare",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "base", "head"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoCompareResponse>,
        )),
        mount: || get(compare_refs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/tree",
        tags: &["repos"],
        operation_id: "getTreeRoot",
        summary: "Root tree listing (`repo.tree`)",
        procedure: "repo.tree",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<TreeQuery>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoTreeResponse>,
        )),
        mount: || get(get_tree_root),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/tree/{*path}",
        tags: &["repos"],
        operation_id: "getTree",
        summary: "Directory listing (`repo.tree`); `{path}` may contain slashes",
        procedure: "repo.tree",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "path"],
        query: Some(SchemaGenerator::into_root_schema_for::<TreeQuery>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoTreeResponse>,
        )),
        mount: || get(get_tree),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/contents/{*path}",
        tags: &["repos"],
        operation_id: "getContents",
        summary: "File content + metadata (`repo.blob`); `{path}` may contain slashes",
        procedure: "repo.blob",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "path"],
        query: Some(SchemaGenerator::into_root_schema_for::<TreeQuery>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoBlobResponse>,
        )),
        mount: || get(get_contents),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/languages",
        tags: &["repos"],
        operation_id: "getLanguages",
        summary: "Language breakdown (`repo.languages`)",
        procedure: "repo.languages",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoLanguagesResponse>,
        )),
        mount: || get(get_languages),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/labels",
        tags: &["repos"],
        operation_id: "listLabels",
        summary: "Effective labels (`label.listForRepo`, includes org labels)",
        procedure: "label.listForRepo",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<ListLabelsForRepoRequest>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<LabelsListResponse>,
        )),
        mount: || get(list_labels),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/stargazers",
        tags: &["repos"],
        operation_id: "listStargazers",
        summary: "Stargazers (`repo.stargazers.list`; Write+ only)",
        procedure: "repo.stargazers.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<RepoStargazersListRequest>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoStargazersListResponse>,
        )),
        mount: || get(list_stargazers),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/hooks",
        tags: &["repos"],
        operation_id: "listHooks",
        summary: "List webhooks (`webhook.list`; repo Admin)",
        procedure: "webhook.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<WebhookListResponse>,
        )),
        mount: || get(list_hooks),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/hooks",
        tags: &["repos"],
        operation_id: "createHook",
        summary: "Create webhook (`webhook.create`) — returns one-time `secret`",
        procedure: "webhook.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateWebhookRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<WebhookPublic>,
        )),
        mount: || post(create_hook),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/hooks/{id}",
        tags: &["repos"],
        operation_id: "getHook",
        summary: "Get webhook (`webhook.get`)",
        procedure: "webhook.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<WebhookPublic>,
        )),
        mount: || get(get_hook),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/hooks/{id}",
        tags: &["repos"],
        operation_id: "updateHook",
        summary: "Update webhook (`webhook.update`)",
        procedure: "webhook.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<UpdateWebhookRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<WebhookPublic>,
        )),
        mount: || patch(update_hook),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/hooks/{id}",
        tags: &["repos"],
        operation_id: "deleteHook",
        summary: "Delete webhook (`webhook.delete`)",
        procedure: "webhook.delete",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<DeleteWebhookResponse>,
        )),
        mount: || delete(delete_hook),
    },
];

#[derive(serde::Serialize, Deserialize)]
pub struct ExploreQuery {
    q: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
}

/// `GET /api/v1/repos` — public repository discovery (`repo.explore`).
async fn list_repositories(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ExploreQuery>,
) -> Response {
    let input = serde_json::to_value(&q).unwrap_or_else(|_| json!({}));
    call(&state, &headers, "repo.explore", input, StatusCode::OK).await
}

/// `GET /api/v1/repos/{owner}/{repo}` (`repo.get`).
async fn get_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.get",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
    )
    .await
}

/// `PATCH /api/v1/repos/{owner}/{repo}` (`repo.updateMetadata`) —
/// body: `description`, `homepage`, `topics` (all optional).
async fn update_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(repo_ref(&p.owner, &p.repo), body);
    call(
        &state,
        &headers,
        "repo.updateMetadata",
        input,
        StatusCode::OK,
    )
    .await
}

/// `DELETE /api/v1/repos/{owner}/{repo}` (`repo.softDelete`) — the URL is the
/// typed confirmation (Admin capability still enforced by the handler).
async fn delete_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "confirmName": p.repo,
    });
    call(&state, &headers, "repo.softDelete", input, StatusCode::OK).await
}

fn ref_list(data: Value, prefix: &str, key: &str) -> Value {
    let items: Vec<Value> = data
        .get("refs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|mut r| {
            let name = r.get("name")?.as_str()?;
            let short = name.strip_prefix(prefix)?.to_string();
            r.as_object_mut()?.insert("name".into(), json!(short));
            Some(r)
        })
        .collect();
    json!({ key: items })
}

/// `GET /api/v1/repos/{owner}/{repo}/branches` — `repo.refs` filtered to
/// `refs/heads/*` with short branch names.
async fn list_branches(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "repo.refs",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |data| ref_list(data, "refs/heads/", "branches"),
    )
    .await
}

/// `GET /api/v1/repos/{owner}/{repo}/tags` — `repo.refs` filtered to
/// `refs/tags/*` with short tag names.
async fn list_tags(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "repo.refs",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |data| ref_list(data, "refs/tags/", "tags"),
    )
    .await
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CommitsQuery {
    /// Branch, tag, or SHA to start listing from (default: default branch).
    /// `sha` mirrors GitHub's parameter name; `ref` is accepted as an alias.
    #[serde(default, alias = "ref")]
    sha: Option<String>,
    #[serde(default)]
    skip: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
}

/// `GET /api/v1/repos/{owner}/{repo}/commits` (`repo.commits`).
async fn list_commits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<CommitsQuery>,
) -> Response {
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "ref": q.sha.unwrap_or_default(),
        "skip": q.skip.unwrap_or(0),
        "limit": q.limit.unwrap_or(0),
    });
    call(&state, &headers, "repo.commits", input, StatusCode::OK).await
}

#[derive(Deserialize)]
pub struct ShaPath {
    pub owner: String,
    pub repo: String,
    pub sha: String,
}

/// `GET /api/v1/repos/{owner}/{repo}/commits/{sha}` (`repo.commit`).
async fn get_commit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ShaPath>,
) -> Response {
    let input = json!({ "owner": p.owner, "name": p.repo, "sha": p.sha });
    call(&state, &headers, "repo.commit", input, StatusCode::OK).await
}

#[derive(Deserialize)]
pub struct ComparePath {
    owner: String,
    repo: String,
    /// `{base}...{head}` (two-dot `..` also accepted), URL-encoded.
    basehead: String,
}

/// `GET /api/v1/repos/{owner}/{repo}/compare/{basehead}` (`repo.compare`).
async fn compare_refs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<ComparePath>,
) -> Response {
    let (base, head) = match p
        .basehead
        .split_once("...")
        .or_else(|| p.basehead.split_once(".."))
    {
        Some(pair) => pair,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": "rpc.bad_input",
                    "message": "compare path must be {base}...{head}",
                })),
            )
                .into_response();
        }
    };
    let input = json!({ "owner": p.owner, "name": p.repo, "base": base, "head": head });
    call(&state, &headers, "repo.compare", input, StatusCode::OK).await
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TreeQuery {
    #[serde(default, rename = "ref")]
    ref_name: Option<String>,
}

#[derive(Deserialize)]
pub struct TreePath {
    owner: String,
    repo: String,
    path: Option<String>,
}

async fn tree_inner(
    state: &AppState,
    headers: &HeaderMap,
    p: TreePath,
    ref_name: Option<String>,
) -> Response {
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "ref": ref_name.unwrap_or_default(),
        "path": p.path.unwrap_or_default(),
    });
    call(state, headers, "repo.tree", input, StatusCode::OK).await
}

/// `GET /api/v1/repos/{owner}/{repo}/tree` — root listing (`repo.tree`).
async fn get_tree_root(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<TreeQuery>,
) -> Response {
    tree_inner(
        &state,
        &headers,
        TreePath {
            owner: p.owner,
            repo: p.repo,
            path: None,
        },
        q.ref_name,
    )
    .await
}

/// `GET /api/v1/repos/{owner}/{repo}/tree/{*path}` (`repo.tree`).
async fn get_tree(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TreePath>,
    Query(q): Query<TreeQuery>,
) -> Response {
    tree_inner(&state, &headers, p, q.ref_name).await
}

/// `GET /api/v1/repos/{owner}/{repo}/contents/{*path}?ref=` (`repo.blob`) —
/// file content (base64) + metadata. For raw bytes use
/// `/api/repos/{owner}/{repo}/raw/{ref}/{path}`.
async fn get_contents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TreePath>,
    Query(q): Query<TreeQuery>,
) -> Response {
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "ref": q.ref_name.unwrap_or_default(),
        "path": p.path.unwrap_or_default(),
    });
    call(&state, &headers, "repo.blob", input, StatusCode::OK).await
}

/// `GET /api/v1/repos/{owner}/{repo}/languages` (`repo.languages`).
async fn get_languages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.languages",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
    )
    .await
}

#[derive(Deserialize)]
pub struct LabelsQuery {
    #[serde(default, rename = "includeHidden", alias = "include_hidden")]
    include_hidden: Option<bool>,
}

/// `GET /api/v1/repos/{owner}/{repo}/labels` (`label.listForRepo`).
async fn list_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<LabelsQuery>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    if let Some(v) = q.include_hidden {
        input
            .as_object_mut()
            .map(|m| m.insert("includeHidden".into(), json!(v)));
    }
    call(&state, &headers, "label.listForRepo", input, StatusCode::OK).await
}

#[derive(serde::Serialize, Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

/// `GET /api/v1/repos/{owner}/{repo}/stargazers` (`repo.stargazers.list`).
async fn list_stargazers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<PageQuery>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        serde_json::to_value(&q).unwrap_or_else(|_| json!({})),
    );
    call(
        &state,
        &headers,
        "repo.stargazers.list",
        input,
        StatusCode::OK,
    )
    .await
}

// --- Webhooks (`/hooks`) ---------------------------------------------------

/// `GET /api/v1/repos/{owner}/{repo}/hooks` (`webhook.list`).
async fn list_hooks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call(
        &state,
        &headers,
        "webhook.list",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/repos/{owner}/{repo}/hooks` (`webhook.create`) —
/// body: `url`, `secret`, `events`, optional `active`, `description`.
async fn create_hook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(repo_ref(&p.owner, &p.repo), body);
    call(
        &state,
        &headers,
        "webhook.create",
        input,
        StatusCode::CREATED,
    )
    .await
}

#[derive(Deserialize)]
pub struct HookPath {
    owner: String,
    repo: String,
    id: String,
}

impl HookPath {
    fn input(&self) -> Value {
        json!({ "owner": self.owner, "name": self.repo, "id": self.id })
    }
}

/// `GET /api/v1/repos/{owner}/{repo}/hooks/{id}` (`webhook.get`).
async fn get_hook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<HookPath>,
) -> Response {
    call(&state, &headers, "webhook.get", p.input(), StatusCode::OK).await
}

/// `PATCH /api/v1/repos/{owner}/{repo}/hooks/{id}` (`webhook.update`).
async fn update_hook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<HookPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(p.input(), body);
    call(&state, &headers, "webhook.update", input, StatusCode::OK).await
}

/// `DELETE /api/v1/repos/{owner}/{repo}/hooks/{id}` (`webhook.delete`).
async fn delete_hook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<HookPath>,
) -> Response {
    call(
        &state,
        &headers,
        "webhook.delete",
        p.input(),
        StatusCode::OK,
    )
    .await
}
