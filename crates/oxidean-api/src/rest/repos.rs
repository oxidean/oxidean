//! `/api/v1/repos/**` — repository metadata, git browsing, labels, hooks.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::Json;
use oxidean_core::{
    CreateWebhookRequest, LabelsListResponse, ListLabelsForRepoRequest, RepoCommitResponse,
    RepoCommitsResponse, RepoCompareResponse, RepoExploreRequest, RepoLanguagesResponse,
    RepoListMineResponse, RepoPublic, RepoRefEntry, RepoSoftDeleteResponse,
    RepoStargazersListRequest, RepoStargazersListResponse, RepoTreeResponse,
    RepoUpdateMetadataRequest, RpcResponse, UpdateWebhookRequest, WebhookListResponse,
    WebhookPublic,
};
use oxidean_core::{DeleteWebhookResponse, RepoBlobResponse};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, call_map, ctx_for, dispatch, merge_fields, repo_ref, RepoPath};
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
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/hooks/{id}/deliveries",
        tags: &["repos"],
        operation_id: "listHookDeliveries",
        summary: "List webhook deliveries, GitHub shape (`webhook.deliveries.list`; repo Admin)",
        procedure: "webhook.deliveries.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(deliveries_list_schema)),
        mount: || get(list_hook_deliveries),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/hooks/{id}/deliveries/{delivery_id}",
        tags: &["repos"],
        operation_id: "getHookDelivery",
        summary:
            "Get a webhook delivery incl. request/response (`webhook.deliveries.get`; repo Admin)",
        procedure: "webhook.deliveries.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "id", "delivery_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(delivery_schema)),
        mount: || get(get_hook_delivery),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/hooks/{id}/deliveries/{delivery_id}/attempts",
        tags: &["repos"],
        operation_id: "redeliverHookDelivery",
        summary: "Redeliver a webhook delivery (`webhook.redeliver`; repo Admin)",
        procedure: "webhook.redeliver",
        ok: StatusCode::ACCEPTED,
        anonymous: false,
        path_fields: &["owner", "name", "id", "delivery_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(redeliver_hook_delivery),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/hooks/{id}/pings",
        tags: &["repos"],
        operation_id: "pingHook",
        summary: "Ping a webhook (`webhook.ping`; repo Admin)",
        procedure: "webhook.ping",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: None,
        response: None,
        mount: || post(ping_hook),
    },
    // ---- collaborators / forks / subscription / starring (GitHub REST) ------
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/collaborators",
        tags: &["repos"],
        operation_id: "repos/listCollaborators",
        summary: "List collaborators (`repo.collaborators.list`) — GitHub permission projection",
        procedure: "repo.collaborators.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(collaborator_list_schema)),
        mount: || get(list_collaborators),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/collaborators/{username}",
        tags: &["repos"],
        operation_id: "repos/checkCollaborator",
        summary: "204 when a collaborator, 404 otherwise (`repo.collaborators.list`)",
        procedure: "repo.collaborators.list",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "username"],
        query: None,
        body: None,
        response: None,
        mount: || get(check_collaborator),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/collaborators/{username}",
        tags: &["repos"],
        operation_id: "repos/addCollaborator",
        summary: "Add/update a collaborator (`repo.collaborators.add` / `.update`) — 201 new, 204 existing",
        procedure: "repo.collaborators.add",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "username"],
        query: None,
        body: Some(BodySpec::Json(collaborator_body_schema)),
        response: Some(RespSpec::Json(collaborator_schema)),
        mount: || put(add_collaborator),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/collaborators/{username}",
        tags: &["repos"],
        operation_id: "repos/removeCollaborator",
        summary: "Remove a collaborator (`repo.collaborators.remove`) — 204",
        procedure: "repo.collaborators.remove",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "username"],
        query: None,
        body: None,
        response: None,
        mount: || delete(remove_collaborator),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/collaborators/{username}/permission",
        tags: &["repos"],
        operation_id: "repos/getCollaboratorPermissionLevel",
        summary: "Effective collaborator permission (`repo.collaborators.list`, filtered)",
        procedure: "repo.collaborators.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "username"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(collaborator_permission_schema)),
        mount: || get(collaborator_permission),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/forks",
        tags: &["repos"],
        operation_id: "repos/listForks",
        summary: "List forks (`repo.forks.list`) — `sort`, `q`, paging",
        procedure: "repo.forks.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<ForksListQuery>),
        body: None,
        response: Some(RespSpec::Json(fork_list_schema)),
        mount: || get(list_forks),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/forks",
        tags: &["repos"],
        operation_id: "repos/createFork",
        summary: "Fork the repo (`repo.fork`) — `{name, organization}` → 202",
        procedure: "repo.fork",
        ok: StatusCode::ACCEPTED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(fork_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoPublic>,
        )),
        mount: || post(create_fork),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/subscribers",
        tags: &["repos"],
        operation_id: "activity/listWatchers",
        summary: "List watchers (`repo.watchers.list`) — `q`, `offset`, `limit`",
        procedure: "repo.watchers.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<WatchersListQuery>),
        body: None,
        response: Some(RespSpec::Json(watcher_list_schema)),
        mount: || get(list_watchers),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/subscription",
        tags: &["repos"],
        operation_id: "activity/getRepoSubscription",
        summary: "Caller's subscription (`user.listWatched`, filtered) — 404 when not watching",
        procedure: "user.listWatched",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(subscription_schema)),
        mount: || get(get_subscription),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/subscription",
        tags: &["repos"],
        operation_id: "activity/setRepoSubscription",
        summary: "Set watch level (`repo.watch`) — `{subscribed, ignored}` map to levels",
        procedure: "repo.watch",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(subscription_body_schema)),
        response: Some(RespSpec::Json(subscription_schema)),
        mount: || put(set_subscription),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/subscription",
        tags: &["repos"],
        operation_id: "activity/deleteRepoSubscription",
        summary: "Stop watching (`repo.unwatch`) — 204",
        procedure: "repo.unwatch",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_subscription),
    },
    // ---- /user/starred/{owner}/{repo} (self-scoped; GET /user/starred
    // already lives in users.rs) ------------------------------------------
    RouteDef {
        method: "GET",
        path: "/user/starred/{owner}/{repo}",
        tags: &["users"],
        operation_id: "activity/checkRepoIsStarredByAuthenticatedUser",
        summary: "204 when starred, 404 otherwise (`user.listStarred`, filtered)",
        procedure: "user.listStarred",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: None,
        response: None,
        mount: || get(check_starred),
    },
    RouteDef {
        method: "PUT",
        path: "/user/starred/{owner}/{repo}",
        tags: &["users"],
        operation_id: "activity/starRepoForAuthenticatedUser",
        summary: "Star a repo (`repo.star`) — 204",
        procedure: "repo.star",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || put(star_repo),
    },
    RouteDef {
        method: "DELETE",
        path: "/user/starred/{owner}/{repo}",
        tags: &["users"],
        operation_id: "activity/unstarRepoForAuthenticatedUser",
        summary: "Unstar a repo (`repo.unstar`) — 204",
        procedure: "repo.unstar",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(unstar_repo),
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

#[derive(Deserialize)]
pub struct DeliveryPath {
    owner: String,
    repo: String,
    id: String,
    delivery_id: String,
}

impl DeliveryPath {
    fn input(&self) -> Value {
        json!({
            "owner": self.owner,
            "name": self.repo,
            "webhook_id": self.id,
            "delivery_id": self.delivery_id,
        })
    }
}

/// `WebhookDeliveryPublic` → GitHub's delivery object (`status`/`status_code`
/// split; `redelivery` is false — our redeliveries are their own rows).
fn gh_delivery(d: &Value, detail: bool) -> Value {
    let status = match d["status"].as_str().unwrap_or("pending") {
        "success" => json!("OK"),
        "failed" => d["error_message"].clone(),
        other => json!(match other {
            "pending" => "Pending",
            s => s,
        }),
    };
    let mut out = json!({
        "id": d["id"],
        "guid": d["delivery_guid"],
        "delivered_at": d["created_at"],
        "redelivery": false,
        "duration": d["duration_ms"].as_i64().map(|ms| ms as f64 / 1000.0),
        "status": status,
        "status_code": d["http_status"].as_i64().unwrap_or(0),
        "event": d["event"],
        "action": match d["action"].as_str() {
            Some("") | None => Value::Null,
            v => json!(v),
        },
        "installation_id": Value::Null,
        "repository_id": Value::Null,
        "throttled_at": Value::Null,
        "url": Value::Null,
    });
    if detail {
        // Stored request headers aren't kept; the payload is verbatim.
        let payload = d["payload"]
            .as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(Value::Null);
        out["request"] = json!({"headers": {}, "payload": payload});
        out["response"] = json!({
            "headers": {},
            "payload": d["response_snippet"].as_str().unwrap_or(""),
        });
    }
    out
}

fn deliveries_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "guid": {"type": "string"},
                "delivered_at": {"type": "string"},
                "redelivery": {"type": "boolean"},
                "duration": {"type": ["number", "null"]},
                "status": {"type": "string"},
                "status_code": {"type": "integer"},
                "event": {"type": "string"},
                "action": {"type": ["string", "null"]},
            },
        },
    })
}

fn delivery_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

/// `GET /api/v1/repos/{owner}/{repo}/hooks/{id}/deliveries`
/// (`webhook.deliveries.list`) → GitHub's delivery array.
async fn list_hook_deliveries(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<HookPath>,
) -> Response {
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "webhook_id": p.id,
    });
    call_map(
        &state,
        &headers,
        "webhook.deliveries.list",
        input,
        StatusCode::OK,
        |data| {
            json!(data["deliveries"]
                .as_array()
                .map(|rows| rows
                    .iter()
                    .map(|d| gh_delivery(d, false))
                    .collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

/// `GET …/deliveries/{delivery_id}` (`webhook.deliveries.get`) — adds
/// `request`/`response` blocks.
async fn get_hook_delivery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<DeliveryPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "webhook.deliveries.get",
        p.input(),
        StatusCode::OK,
        |data| gh_delivery(&data, true),
    )
    .await
}

/// `POST …/deliveries/{delivery_id}/attempts` (`webhook.redeliver`) → 202.
async fn redeliver_hook_delivery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<DeliveryPath>,
) -> Response {
    call(
        &state,
        &headers,
        "webhook.redeliver",
        p.input(),
        StatusCode::ACCEPTED,
    )
    .await
}

/// `POST …/hooks/{id}/pings` (`webhook.ping`) → 201.
async fn ping_hook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<HookPath>,
) -> Response {
    call(
        &state,
        &headers,
        "webhook.ping",
        p.input(),
        StatusCode::CREATED,
    )
    .await
}

// ---------------------------------------------------------------------------
// Collaborators / forks / subscription / starring (GitHub REST)
// ---------------------------------------------------------------------------

/// `{username}` collaborator path params.
#[derive(Deserialize)]
pub struct CollaboratorPath {
    owner: String,
    repo: String,
    username: String,
}

/// `{owner}/{repo}` under `/user/starred/**` (self-scoped routes).
#[derive(Deserialize)]
pub struct StarredPath {
    owner: String,
    repo: String,
}

#[derive(serde::Serialize, Deserialize, schemars::JsonSchema)]
pub struct ForksListQuery {
    #[serde(default)]
    q: Option<String>,
    /// `stars` (default) | `updated` | `created`
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(serde::Serialize, Deserialize, schemars::JsonSchema)]
pub struct WatchersListQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

fn collaborator_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn collaborator_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn collaborator_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "permission": {"type": "string", "enum": ["read", "write", "admin"]},
        },
    })
}

fn collaborator_permission_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "permission": {"type": "string"},
            "role_name": {"type": "string"},
            "user": {"type": "object"},
        },
    })
}

fn fork_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn fork_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "organization": {"type": "string"},
            "name": {"type": "string"},
        },
    })
}

fn watcher_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn subscription_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "subscribed": {"type": "boolean"},
            "ignored": {"type": "boolean"},
        },
    })
}

fn subscription_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "subscribed": {"type": "boolean"},
            "ignored": {"type": "boolean"},
        },
    })
}

/// `RepoCollaboratorPublic` → GitHub's collaborator object.
fn gh_collaborator(c: &Value) -> Value {
    let perm = c["permission"].as_str().unwrap_or("read");
    json!({
        "id": c["user_id"],
        "login": c["username"],
        "type": "User",
        "role_name": perm,
        "permissions": {
            "pull": true,
            "push": perm == "write" || perm == "admin",
            "admin": perm == "admin",
            "triage": perm == "write" || perm == "admin",
            "maintain": false,
        },
        "created_at": c["created_at"],
    })
}

/// List the repo's collaborators — Err(boxed response) on RPC failure.
async fn collaborators(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
) -> Result<Vec<Value>, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    match dispatch(&mut ctx, "repo.collaborators.list", repo_ref(owner, repo)).await {
        RpcResponse::Ok { data, .. } => Ok(data["collaborators"]
            .as_array()
            .cloned()
            .unwrap_or_default()),
        RpcResponse::Err { error, .. } => Err(Box::new(
            (super::status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

async fn list_collaborators(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    match collaborators(&state, &headers, &p.owner, &p.repo).await {
        Ok(cs) => {
            let out: Vec<Value> = cs.iter().map(gh_collaborator).collect();
            (StatusCode::OK, Json(json!(out))).into_response()
        }
        Err(r) => *r,
    }
}

/// The owner counts as `admin` even though the collaborators table only
/// holds non-owner rows (matches GitHub's effective-permission semantics).
async fn collaborator_entry(
    state: &AppState,
    headers: &HeaderMap,
    p: &CollaboratorPath,
) -> Result<Option<Value>, Box<Response>> {
    if p.username == p.owner {
        return Ok(Some(json!({
            "user_id": "", "username": p.owner, "permission": "admin",
            "created_at": "",
        })));
    }
    let cs = collaborators(state, headers, &p.owner, &p.repo).await?;
    Ok(cs
        .into_iter()
        .find(|c| c["username"].as_str() == Some(p.username.as_str())))
}

fn collaborator_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"code": "collaborator.not_found", "message": "not a collaborator"})),
    )
        .into_response()
}

async fn check_collaborator(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CollaboratorPath>,
) -> Response {
    match collaborator_entry(&state, &headers, &p).await {
        Ok(Some(_)) => StatusCode::NO_CONTENT.into_response(),
        Ok(None) => collaborator_not_found(),
        Err(r) => *r,
    }
}

/// GitHub PUT: 201 when newly added, 204 when the row already existed (the
/// permission is updated when it differs).
async fn add_collaborator(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CollaboratorPath>,
    Json(body): Json<Value>,
) -> Response {
    let permission = body["permission"].as_str().unwrap_or("write");
    let existing = match collaborator_entry(&state, &headers, &p).await {
        Ok(e) => e,
        Err(r) => return *r,
    };
    if let Some(c) = existing {
        if c["permission"].as_str() == Some(permission) {
            return StatusCode::NO_CONTENT.into_response();
        }
        // Update the permission on the existing row.
        return call(
            &state,
            &headers,
            "repo.collaborators.update",
            json!({
                "owner": p.owner, "name": p.repo,
                "user_id": c["user_id"], "permission": permission,
            }),
            StatusCode::NO_CONTENT,
        )
        .await;
    }
    let resp = call(
        &state,
        &headers,
        "repo.collaborators.add",
        json!({
            "owner": p.owner, "name": p.repo,
            "username": p.username, "permission": permission,
        }),
        StatusCode::CREATED,
    )
    .await;
    // Reshape the added row to the GitHub collaborator object.
    let (parts, resp_body) = resp.into_parts();
    if parts.status != StatusCode::CREATED {
        return Response::from_parts(parts, resp_body);
    }
    let bytes = axum::body::to_bytes(resp_body, 1 << 20)
        .await
        .unwrap_or_default();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (StatusCode::CREATED, Json(gh_collaborator(&v))).into_response()
}

async fn remove_collaborator(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CollaboratorPath>,
) -> Response {
    let existing = match collaborator_entry(&state, &headers, &p).await {
        Ok(e) => e,
        Err(r) => return *r,
    };
    let Some(c) = existing else {
        return collaborator_not_found();
    };
    call(
        &state,
        &headers,
        "repo.collaborators.remove",
        json!({"owner": p.owner, "name": p.repo, "user_id": c["user_id"]}),
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn collaborator_permission(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CollaboratorPath>,
) -> Response {
    match collaborator_entry(&state, &headers, &p).await {
        Ok(Some(c)) => {
            let perm = c["permission"].as_str().unwrap_or("read");
            (
                StatusCode::OK,
                Json(json!({
                    "permission": perm,
                    "role_name": perm,
                    "user": {"login": c["username"], "type": "User"},
                })),
            )
                .into_response()
        }
        Ok(None) => collaborator_not_found(),
        Err(r) => *r,
    }
}

/// `RepoForkPublic` → GitHub's fork repo object (compact).
fn gh_fork(f: &Value) -> Value {
    json!({
        "id": f["id"],
        "name": f["name"],
        "full_name": format!("{}/{}", f["owner_username"].as_str().unwrap_or(""), f["name"].as_str().unwrap_or("")),
        "owner": {"login": f["owner_username"], "type": "User"},
        "description": f["description"],
        "fork": true,
        "stargazers_count": f["star_count"],
        "forks_count": f["fork_count"],
        "created_at": f["created_at"],
        "updated_at": f["updated_at"],
    })
}

async fn list_forks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<ForksListQuery>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        serde_json::to_value(&q).unwrap_or_else(|_| json!({})),
    );
    call_map(
        &state,
        &headers,
        "repo.forks.list",
        input,
        StatusCode::OK,
        |d| {
            json!(d["forks"]
                .as_array()
                .map(|fs| fs.iter().map(gh_fork).collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

/// `{organization, name}` → `into_owner`/`into_name` (org targets are still
/// rejected by the RPC layer — Phase 21 lands under self only).
async fn create_fork(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    if let Some(org) = body["organization"].as_str() {
        input["into_owner"] = json!(org);
    }
    if let Some(n) = body["name"].as_str() {
        input["into_name"] = json!(n);
    }
    call(&state, &headers, "repo.fork", input, StatusCode::ACCEPTED).await
}

/// `RepoWatcherPublic` → GitHub watcher object.
fn gh_watcher(w: &Value) -> Value {
    json!({
        "id": w["user_id"],
        "login": w["username"],
        "type": "User",
        "avatar_url": w["avatar_url"],
    })
}

async fn list_watchers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<WatchersListQuery>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        serde_json::to_value(&q).unwrap_or_else(|_| json!({})),
    );
    call_map(
        &state,
        &headers,
        "repo.watchers.list",
        input,
        StatusCode::OK,
        |d| {
            json!(d["watchers"]
                .as_array()
                .map(|ws| ws.iter().map(gh_watcher).collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

/// Does `owner`/`name` appear in the caller's `user.listWatched` page?
/// (200-wide page — subscriptions past it still count via PUT/DELETE.)
async fn is_watched(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
) -> Result<bool, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    let resp = dispatch(
        &mut ctx,
        "user.listWatched",
        json!({"offset": 0, "limit": 50}),
    )
    .await;
    match resp {
        RpcResponse::Ok { data, .. } => Ok(data["repos"]
            .as_array()
            .map(|rs| {
                rs.iter().any(|r| {
                    r["owner_username"].as_str() == Some(owner) && r["name"].as_str() == Some(repo)
                })
            })
            .unwrap_or(false)),
        RpcResponse::Err { error, .. } => Err(Box::new(
            (super::status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

async fn get_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<StarredPath>,
) -> Response {
    match is_watched(&state, &headers, &p.owner, &p.repo).await {
        Ok(true) => (
            StatusCode::OK,
            Json(json!({"subscribed": true, "ignored": false, "reason": null})),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "subscription.not_found", "message": "not watching"})),
        )
            .into_response(),
        Err(r) => *r,
    }
}

/// GitHub body `{subscribed, ignored}`: `ignored:true` → level `ignore`;
/// `subscribed:false`/absent → level `participating`; else `all`.
async fn set_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let level = if body["ignored"].as_bool().unwrap_or(false) {
        "ignore"
    } else if body["subscribed"].as_bool().unwrap_or(false) {
        "all"
    } else {
        "participating"
    };
    call_map(
        &state,
        &headers,
        "repo.watch",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"level": level})),
        StatusCode::OK,
        |_| {
            json!({
                "subscribed": level != "ignore",
                "ignored": level == "ignore",
                "reason": null,
            })
        },
    )
    .await
}

async fn delete_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.unwatch",
        repo_ref(&p.owner, &p.repo),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// The caller's starred repos — `RepoListMineResponse.repos` reshaped.
async fn starred_ids(state: &AppState, headers: &HeaderMap) -> Result<Vec<Value>, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    match dispatch(
        &mut ctx,
        "user.listStarred",
        json!({"offset": 0, "limit": 50}),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => Ok(data["repos"].as_array().cloned().unwrap_or_default()),
        RpcResponse::Err { error, .. } => Err(Box::new(
            (super::status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

async fn check_starred(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<StarredPath>,
) -> Response {
    match starred_ids(&state, &headers).await {
        Ok(rs) => {
            let starred = rs.iter().any(|r| {
                r["owner_username"].as_str() == Some(p.owner.as_str())
                    && r["name"].as_str() == Some(p.repo.as_str())
            });
            if starred {
                StatusCode::NO_CONTENT.into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "star.not_found", "message": "repo not starred"})),
                )
                    .into_response()
            }
        }
        Err(r) => *r,
    }
}

async fn star_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<StarredPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.star",
        repo_ref(&p.owner, &p.repo),
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn unstar_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<StarredPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.unstar",
        repo_ref(&p.owner, &p.repo),
        StatusCode::NO_CONTENT,
    )
    .await
}
