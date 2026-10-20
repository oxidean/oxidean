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
use super::{
    call, call_map, ctx_for, dispatch, merge_fields, repo_ref, status_for_error, RepoPath,
};
use crate::app::AppState;

/// `repo.refs` entries filtered to `refs/tags/*` and renamed to short
/// names — `{tags: [...]}`.
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
        summary: "List branches (`repo.refs` heads + `branchProtection.list`) — GitHub bare array",
        procedure: "repo.refs",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(branch_list_schema)),
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
        method: "PUT",
        path: "/repos/{owner}/{repo}/contents/{*path}",
        tags: &["repos"],
        operation_id: "putContents",
        summary: "Create or update a file (`repo.file.create` / `.update`) — `sha` selects update",
        procedure: "repo.file.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "path"],
        query: None,
        body: Some(BodySpec::Json(put_contents_body_schema)),
        response: Some(RespSpec::Json(contents_write_schema)),
        mount: || put(put_contents),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/contents/{*path}",
        tags: &["repos"],
        operation_id: "deleteContents",
        summary: "Delete a file or directory (`repo.file.delete`)",
        procedure: "repo.file.delete",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "path"],
        query: None,
        body: Some(BodySpec::Json(delete_contents_body_schema)),
        response: Some(RespSpec::Json(contents_write_schema)),
        mount: || delete(delete_contents),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/contributors",
        tags: &["repos"],
        operation_id: "listContributors",
        summary: "Commit contributors (`repo.contributors.list`), GitHub projection",
        procedure: "repo.contributors.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(contributors_schema)),
        mount: || get(list_contributors),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/topics",
        tags: &["repos"],
        operation_id: "listTopics",
        summary: "Repository topics (`repo.get`) — `{names}`",
        procedure: "repo.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(topics_schema)),
        mount: || get(list_topics),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/topics",
        tags: &["repos"],
        operation_id: "replaceTopics",
        summary: "Replace all topics (`repo.updateMetadata`)",
        procedure: "repo.updateMetadata",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(topics_body_schema)),
        response: Some(RespSpec::Json(topics_schema)),
        mount: || put(replace_topics),
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
    // ---- branches / protection / git refs (GitHub REST) ---------------------
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/branches/{branch}",
        tags: &["repos"],
        operation_id: "repos/getBranch",
        summary: "One branch (`repo.refs` + `repo.branchProtection.list`) — 404 when absent",
        procedure: "repo.refs",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "branch"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(branch_schema)),
        mount: || get(get_branch_rest),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/branches/{branch}/rename",
        tags: &["repos"],
        operation_id: "repos/renameBranch",
        summary: "Rename a branch (`repo.branchRename`) — `{new_name}`; 201",
        procedure: "repo.branchRename",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "branch"],
        query: None,
        body: Some(BodySpec::Json(branch_rename_body_schema)),
        response: Some(RespSpec::Json(branch_schema)),
        mount: || post(rename_branch_rest),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/branches/{branch}/protection",
        tags: &["repos"],
        operation_id: "repos/getBranchProtection",
        summary: "Effective protection for the branch (`repo.branchProtection.list`, pattern-matched)",
        procedure: "repo.branchProtection.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "branch"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(branch_protection_schema)),
        mount: || get(get_branch_protection_rest),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/branches/{branch}/protection",
        tags: &["repos"],
        operation_id: "repos/updateBranchProtection",
        summary: "Create/update protection pinned to the branch name (`repo.branchProtection.create|update`)",
        procedure: "repo.branchProtection.create",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "branch"],
        query: None,
        body: Some(BodySpec::Json(branch_protection_body_schema)),
        response: Some(RespSpec::Json(branch_protection_schema)),
        mount: || put(put_branch_protection_rest),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/branches/{branch}/protection",
        tags: &["repos"],
        operation_id: "repos/deleteBranchProtection",
        summary: "Delete the exact-pattern protection rule (`repo.branchProtection.delete`) — 204",
        procedure: "repo.branchProtection.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "branch"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_branch_protection_rest),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/merge-upstream",
        tags: &["repos"],
        operation_id: "repos/mergeUpstream",
        summary: "Sync a fork from upstream (`repo.syncFork`) — `{branch}`",
        procedure: "repo.syncFork",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(merge_upstream_body_schema)),
        response: Some(RespSpec::Json(merge_upstream_schema)),
        mount: || post(merge_upstream_rest),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/git/matching-refs/{*ref}",
        tags: &["git"],
        operation_id: "git/listMatchingRefs",
        summary: "Refs under the given prefix (`repo.refs`, filtered) — GitHub bare array",
        procedure: "repo.refs",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "ref"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(git_ref_list_schema)),
        mount: || get(list_matching_refs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/git/ref/{*ref}",
        tags: &["git"],
        operation_id: "git/getRef",
        summary: "One exact ref (`repo.refs`, filtered) — 404 when absent",
        procedure: "repo.refs",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name", "ref"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(git_ref_schema)),
        mount: || get(get_git_ref),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/git/refs",
        tags: &["git"],
        operation_id: "git/createRef",
        summary: "Create `refs/heads/*` (`repo.branchCreate`) — `{ref, sha}` → 201",
        procedure: "repo.branchCreate",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(git_create_ref_body_schema)),
        response: Some(RespSpec::Json(git_ref_schema)),
        mount: || post(create_git_ref),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/git/refs/{*ref}",
        tags: &["git"],
        operation_id: "git/deleteRef",
        summary: "Delete `refs/heads/*` (`repo.branchDelete`) — 204",
        procedure: "repo.branchDelete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "ref"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_git_ref),
    },
    // ---- repo-scoped notifications (GitHub REST) -----------------------------
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/notifications",
        tags: &["activity"],
        operation_id: "activity/listRepoNotifications",
        summary: "This repo's notification threads (`notification.list`, filtered)",
        procedure: "notification.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<
            super::users::NotificationsQuery,
        >),
        body: None,
        response: Some(RespSpec::Json(repo_notification_list_schema)),
        mount: || get(list_repo_notifications),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/notifications",
        tags: &["activity"],
        operation_id: "activity/markRepoNotificationsRead",
        summary: "Mark this repo's threads read (`notification.markRead`) — 202",
        procedure: "notification.markRead",
        ok: StatusCode::ACCEPTED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || put(mark_repo_notifications_read),
    },
    // ---- deploy keys + transfer (GitHub REST) --------------------------------
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/keys",
        tags: &["repos"],
        operation_id: "repos/listDeployKeys",
        summary: "Deploy keys (`repo.deployKey.list`) — GitHub `{id, key, title, read_only}` rows",
        procedure: "repo.deployKey.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(deploy_key_list_schema)),
        mount: || get(list_deploy_keys),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/keys/{key_id}",
        tags: &["repos"],
        operation_id: "repos/getDeployKey",
        summary: "One deploy key (`repo.deployKey.list`, id-filtered) — 404 when absent",
        procedure: "repo.deployKey.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "key_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(deploy_key_schema)),
        mount: || get(get_deploy_key),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/keys",
        tags: &["repos"],
        operation_id: "repos/createDeployKey",
        summary: "Add a deploy key (`repo.deployKey.create`) — `{title, key, read_only}` → 201",
        procedure: "repo.deployKey.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(deploy_key_body_schema)),
        response: Some(RespSpec::Json(deploy_key_schema)),
        mount: || post(create_deploy_key),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/keys/{key_id}",
        tags: &["repos"],
        operation_id: "repos/deleteDeployKey",
        summary: "Remove a deploy key (`repo.deployKey.delete`) — 204",
        procedure: "repo.deployKey.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "key_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_deploy_key),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/transfer",
        tags: &["repos"],
        operation_id: "repos/transfer",
        summary: "Transfer ownership (`repo.transfer`) — `{new_owner, new_name?}`; confirm auto-filled from the path",
        procedure: "repo.transfer",
        ok: StatusCode::ACCEPTED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(transfer_body_schema)),
        response: Some(RespSpec::Json(transfer_repo_schema)),
        mount: || post(transfer_repo_rest),
    },
    // ---- tag protections / activity / mirror-sync (GitHub+Gitea REST) ---------
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/tag_protections",
        tags: &["repos"],
        operation_id: "repos/listTagProtections",
        summary: "Tag protection rules (`repo.tagProtection.list`) — Gitea `{id, name_pattern, …}` rows",
        procedure: "repo.tagProtection.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(tag_protection_list_schema)),
        mount: || get(list_tag_protections),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/tag_protections",
        tags: &["repos"],
        operation_id: "repos/createTagProtection",
        summary: "Create a rule (`repo.tagProtection.create`) — `{name_pattern}` → 201",
        procedure: "repo.tagProtection.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(tag_protection_body_schema)),
        response: Some(RespSpec::Json(tag_protection_schema)),
        mount: || post(create_tag_protection),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/tag_protections/{id}",
        tags: &["repos"],
        operation_id: "repos/getTagProtection",
        summary: "One rule (`repo.tagProtection.list`, id-filtered) — 404 when absent",
        procedure: "repo.tagProtection.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(tag_protection_schema)),
        mount: || get(get_tag_protection),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/tag_protections/{id}",
        tags: &["repos"],
        operation_id: "repos/updateTagProtection",
        summary: "Update a rule (`repo.tagProtection.update`) — `{name_pattern}`; existing flags retained when omitted",
        procedure: "repo.tagProtection.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: Some(BodySpec::Json(tag_protection_patch_schema)),
        response: Some(RespSpec::Json(tag_protection_schema)),
        mount: || patch(update_tag_protection),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/tag_protections/{id}",
        tags: &["repos"],
        operation_id: "repos/deleteTagProtection",
        summary: "Delete a rule (`repo.tagProtection.delete`) — 204",
        procedure: "repo.tagProtection.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_tag_protection),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/activity",
        tags: &["repos"],
        operation_id: "activity/listRepoActivities",
        summary: "Repo activity feed (`repo.activity.list`) — GitHub `{ref, activity_type, actor, before, after}` rows",
        procedure: "repo.activity.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<RepoActivityQuery>),
        body: None,
        response: Some(RespSpec::Json(activity_list_schema)),
        mount: || get(list_repo_activity),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/mirror-sync",
        tags: &["repos"],
        operation_id: "repos/mirrorSync",
        summary: "Kick a pull-mirror sync (`repo.mirror.syncNow`) — Gitea parity; 200",
        procedure: "repo.mirror.syncNow",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(mirror_sync_schema)),
        mount: || post(mirror_sync_rest),
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

/// `PATCH /api/v1/repos/{owner}/{repo}` — GitHub's update surface:
/// `description`/`homepage`/`topics` → `repo.updateMetadata`; a `name`
/// different from the path runs `repo.rename` first, then metadata under the
/// new name (GitHub treats rename as a PATCH field).
async fn update_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let new_name = body["name"].as_str().unwrap_or_default().to_string();
    let renaming = !new_name.is_empty() && new_name != p.repo;
    let mut meta = body.clone();
    if let Some(m) = meta.as_object_mut() {
        m.remove("name");
    }
    if renaming {
        let mut ctx = match ctx_for(&state, &headers).await {
            Ok(c) => c,
            Err(r) => return r,
        };
        match dispatch(
            &mut ctx,
            "repo.rename",
            merge_fields(repo_ref(&p.owner, &p.repo), json!({"new_name": new_name})),
        )
        .await
        {
            RpcResponse::Ok { .. } => {}
            RpcResponse::Err { error, .. } => {
                return (super::status_for_error(&error), Json(error)).into_response();
            }
        }
        if meta.as_object().map(|m| !m.is_empty()).unwrap_or(false) {
            match dispatch(
                &mut ctx,
                "repo.updateMetadata",
                merge_fields(repo_ref(&p.owner, &new_name), meta),
            )
            .await
            {
                RpcResponse::Ok { data, .. } => {
                    return (StatusCode::OK, Json(data)).into_response();
                }
                RpcResponse::Err { error, .. } => {
                    return (super::status_for_error(&error), Json(error)).into_response();
                }
            }
        }
        // Metadata empty → return the renamed repo (matches PATCH's
        // repo-object response; `repo.get` under the new name).
        return call(
            &state,
            &headers,
            "repo.get",
            repo_ref(&p.owner, &new_name),
            StatusCode::OK,
        )
        .await;
    }
    let input = merge_fields(repo_ref(&p.owner, &p.repo), meta);
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

/// `GET /api/v1/repos/{owner}/{repo}/branches` — GitHub's bare
/// `[{name, commit:{sha}, protected}]` over `repo.refs` + `branchProtection.list`.
async fn list_branches(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    match refs_and_rules(&state, &headers, &p.owner, &p.repo).await {
        Ok((refs, rules)) => (
            StatusCode::OK,
            Json(json!(branches_with_protection(&refs, &rules))),
        )
            .into_response(),
        Err(r) => *r,
    }
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

// ---------------------------------------------------------------------------
// Branches / protection / git refs (GitHub REST)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BranchPath {
    owner: String,
    repo: String,
    branch: String,
}

#[derive(Deserialize)]
pub struct GitRefPath {
    owner: String,
    repo: String,
    #[serde(rename = "ref")]
    ref_name: String,
}

fn branch_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn branch_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "commit": {"type": "object"},
            "protected": {"type": "boolean"},
        },
    })
}

fn branch_rename_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["new_name"],
        "properties": {"new_name": {"type": "string"}},
    })
}

fn branch_protection_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

/// GitHub's protection body — field names translated to the rule input.
fn branch_protection_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "required_status_checks": {
                "anyOf": [{"type": "null"}, {
                    "type": "object",
                    "properties": {
                        "strict": {"type": "boolean"},
                        "contexts": {"type": "array", "items": {"type": "string"}},
                        "checks": {"type": "array", "items": {"type": "object"}},
                    },
                }],
            },
            "required_pull_request_reviews": {
                "anyOf": [{"type": "null"}, {"type": "object"}],
            },
            "enforce_admins": {"anyOf": [{"type": "null"}, {"type": "boolean"}, {"type": "object"}]},
            "allow_force_pushes": {"anyOf": [{"type": "null"}, {"type": "boolean"}, {"type": "object"}]},
            "allow_deletions": {"anyOf": [{"type": "null"}, {"type": "boolean"}, {"type": "object"}]},
            "required_linear_history": {"anyOf": [{"type": "null"}, {"type": "boolean"}, {"type": "object"}]},
            "required_conversation_resolution": {"anyOf": [{"type": "null"}, {"type": "boolean"}, {"type": "object"}]},
            "required_signatures": {"anyOf": [{"type": "null"}, {"type": "boolean"}, {"type": "object"}]},
            "lock_branch": {"anyOf": [{"type": "null"}, {"type": "boolean"}, {"type": "object"}]},
            "required_merge_queue": {"anyOf": [{"type": "null"}, {"type": "boolean"}, {"type": "object"}]},
        },
    })
}

fn merge_upstream_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {"branch": {"type": "string"}},
    })
}

fn merge_upstream_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn git_ref_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn git_ref_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": {"type": "string"},
            "object": {"type": "object"},
        },
    })
}

fn git_create_ref_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["ref", "sha"],
        "properties": {
            "ref": {"type": "string"},
            "sha": {"type": "string"},
        },
    })
}

/// `RepoRefEntry` (a `refs/heads/*` row) → GitHub's branch object.
fn gh_branch(r: &Value, protected: bool) -> Value {
    json!({
        "name": r["name"],
        "commit": {"sha": r["oid"], "url": null},
        "protected": protected,
    })
}

/// `RepoRefEntry` → GitHub's git-ref object.
fn gh_git_ref(r: &Value, owner: &str, repo: &str) -> Value {
    json!({
        "ref": r["name"],
        "object": {"sha": r["oid"], "type": "commit"},
        "url": format!(
            "/api/v1/repos/{}/{}/git/ref/{}",
            owner,
            repo,
            r["name"].as_str().unwrap_or_default().trim_start_matches("refs/")
        ),
    })
}

/// Fetch repo refs + protection rules once for the branch handlers.
async fn refs_and_rules(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
) -> Result<(Vec<Value>, Vec<Value>), Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    let refs = match dispatch(&mut ctx, "repo.refs", repo_ref(owner, repo)).await {
        RpcResponse::Ok { data, .. } => data["refs"].as_array().cloned().unwrap_or_default(),
        RpcResponse::Err { error, .. } => {
            return Err(Box::new(
                (super::status_for_error(&error), Json(error)).into_response(),
            ))
        }
    };
    let rules = match dispatch(
        &mut ctx,
        "repo.branchProtection.list",
        repo_ref(owner, repo),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => data["rules"].as_array().cloned().unwrap_or_default(),
        RpcResponse::Err { .. } => vec![], // Read-only callers without Admin see no rules
    };
    Ok((refs, rules))
}

/// Heads-only entries with the `protected` flag folded in.
fn branches_with_protection(refs: &[Value], rules: &[Value]) -> Vec<Value> {
    refs.iter()
        .filter(|r| {
            r["name"]
                .as_str()
                .map(|n| n.starts_with("refs/heads/"))
                .unwrap_or(false)
        })
        .map(|r| {
            let branch = r["name"]
                .as_str()
                .unwrap_or_default()
                .trim_start_matches("refs/heads/");
            let protected = rules.iter().any(|rule| {
                rule["pattern"]
                    .as_str()
                    .map(|p| crate::protection::pattern_matches(p, branch))
                    .unwrap_or(false)
            });
            gh_branch(&json!({"name": branch, "oid": r["oid"]}), protected)
        })
        .collect()
}

async fn get_branch_rest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<BranchPath>,
) -> Response {
    match refs_and_rules(&state, &headers, &p.owner, &p.repo).await {
        Ok((refs, rules)) => {
            let branches = branches_with_protection(&refs, &rules);
            match branches
                .iter()
                .find(|b| b["name"].as_str() == Some(p.branch.as_str()))
            {
                Some(b) => (StatusCode::OK, Json(b.clone())).into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "branch.not_found", "message": "branch not found"})),
                )
                    .into_response(),
            }
        }
        Err(r) => *r,
    }
}

/// `POST /branches/{b}/rename {new_name}` → `repo.branchRename {from,to}` → 201.
async fn rename_branch_rest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<BranchPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({"from": p.branch, "to": body["new_name"]}),
    );
    call(
        &state,
        &headers,
        "repo.branchRename",
        input,
        StatusCode::CREATED,
    )
    .await
}

/// `BranchProtectionRulePublic` → GitHub's protection object.
fn gh_protection(rule: &Value) -> Value {
    json!({
        "required_status_checks": if rule["required_status_contexts"].as_array().map(|c| !c.is_empty()).unwrap_or(false) {
            json!({
                "strict": rule["strict_status_checks"],
                "contexts": rule["required_status_contexts"],
            })
        } else { Value::Null },
        "required_pull_request_reviews": if rule["require_reviews"].as_bool().unwrap_or(false) {
            json!({
                "required_approving_review_count": rule["required_approving_review_count"],
                "dismiss_stale_reviews": rule["dismiss_stale_reviews"],
                "require_last_push_approval": rule["require_last_push_approval"],
            })
        } else { Value::Null },
        "enforce_admins": {"enabled": rule["enforce_admins"]},
        "required_linear_history": {"enabled": rule["required_linear_history"]},
        "allow_force_pushes": {"enabled": rule["allow_force_pushes"]},
        "allow_deletions": {"enabled": rule["allow_deletions"]},
        "required_conversation_resolution": {"enabled": rule["require_conversation_resolution"]},
        "required_signatures": {"enabled": rule["require_signed_commits"]},
        "lock_branch": {"enabled": rule["lock_branch"]},
        "required_merge_queue": {"enabled": rule["required_merge_queue"]},
    })
}

/// Merge the matching rules into one GitHub projection (bool = OR, contexts
/// = union, review count = max) — mirrors `protection::union_rules`.
fn merge_rules_for_branch(rules: &[Value], branch: &str) -> Option<Value> {
    let matching: Vec<&Value> = rules
        .iter()
        .filter(|r| {
            r["pattern"]
                .as_str()
                .map(|p| crate::protection::pattern_matches(p, branch))
                .unwrap_or(false)
        })
        .collect();
    if matching.is_empty() {
        return None;
    }
    let any = |k: &str| matching.iter().any(|r| r[k].as_bool().unwrap_or(false));
    let mut contexts: Vec<Value> = vec![];
    for r in &matching {
        for c in r["required_status_contexts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            if !contexts.contains(&c) {
                contexts.push(c);
            }
        }
    }
    let max_reviews = matching
        .iter()
        .filter_map(|r| r["required_approving_review_count"].as_i64())
        .max()
        .unwrap_or(1);
    Some(gh_protection(&json!({
        "required_status_contexts": contexts,
        "strict_status_checks": any("strict_status_checks"),
        "require_reviews": any("require_reviews"),
        "required_approving_review_count": max_reviews,
        "dismiss_stale_reviews": any("dismiss_stale_reviews"),
        "require_last_push_approval": any("require_last_push_approval"),
        "enforce_admins": any("enforce_admins"),
        "required_linear_history": any("required_linear_history"),
        "allow_force_pushes": any("allow_force_pushes"),
        "allow_deletions": any("allow_deletions"),
        "require_conversation_resolution": any("require_conversation_resolution"),
        "require_signed_commits": any("require_signed_commits"),
        "lock_branch": any("lock_branch"),
        "required_merge_queue": any("required_merge_queue"),
    })))
}

async fn get_branch_protection_rest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<BranchPath>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(
        &mut ctx,
        "repo.branchProtection.list",
        repo_ref(&p.owner, &p.repo),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => {
            let rules = data["rules"].as_array().cloned().unwrap_or_default();
            match merge_rules_for_branch(&rules, &p.branch) {
                Some(pj) => (StatusCode::OK, Json(pj)).into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "protection.not_found", "message": "branch is not protected"})),
                )
                    .into_response(),
            }
        }
        RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

/// Translate GitHub's protection body to `BranchProtectionRuleInput` fields.
fn gh_protection_to_rule(body: &Value, pattern: &str) -> Value {
    let on = |k: &str| -> bool {
        let v = &body[k];
        v.as_bool()
            .or_else(|| v["enabled"].as_bool())
            .or_else(|| if v.is_object() { Some(true) } else { None })
            .unwrap_or(false)
    };
    let contexts = body["required_status_checks"]["contexts"]
        .as_array()
        .cloned()
        .or_else(|| {
            // `checks: [{context, app_id}]` is the newer form.
            body["required_status_checks"]["checks"]
                .as_array()
                .map(|cs| {
                    cs.iter()
                        .filter_map(|c| c["context"].as_str().map(|s| json!(s)))
                        .collect()
                })
        })
        .unwrap_or_default();
    let reviews = &body["required_pull_request_reviews"];
    json!({
        "pattern": pattern,
        "require_reviews": !reviews.is_null() && (reviews.is_object() || reviews.as_bool() == Some(true)),
        "required_approving_review_count": reviews["required_approving_review_count"].as_i64().unwrap_or(1),
        "dismiss_stale_reviews": reviews["dismiss_stale_reviews"].as_bool().unwrap_or(false),
        "require_last_push_approval": reviews["require_last_push_approval"].as_bool().unwrap_or(false),
        "require_conversation_resolution": on("required_conversation_resolution"),
        "required_status_contexts": contexts,
        "strict_status_checks": body["required_status_checks"]["strict"].as_bool().unwrap_or(false),
        "allow_force_pushes": on("allow_force_pushes"),
        "allow_deletions": on("allow_deletions"),
        "enforce_admins": on("enforce_admins"),
        "required_linear_history": on("required_linear_history"),
        "lock_branch": on("lock_branch"),
        "require_signed_commits": on("required_signatures"),
        "required_merge_queue": on("required_merge_queue"),
    })
}

/// Find the rule whose pattern is exactly this branch name (PUT/DELETE key).
async fn exact_rule_id(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<Option<String>, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    match dispatch(
        &mut ctx,
        "repo.branchProtection.list",
        repo_ref(owner, repo),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => Ok(data["rules"].as_array().and_then(|rs| {
            rs.iter()
                .find(|r| r["pattern"].as_str() == Some(branch))
                .and_then(|r| r["id"].as_str().map(str::to_string))
        })),
        RpcResponse::Err { error, .. } => Err(Box::new(
            (super::status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

async fn put_branch_protection_rest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<BranchPath>,
    Json(body): Json<Value>,
) -> Response {
    let rule = gh_protection_to_rule(&body, &p.branch);
    let input = merge_fields(repo_ref(&p.owner, &p.repo), rule);
    let existing = match exact_rule_id(&state, &headers, &p.owner, &p.repo, &p.branch).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let (proc, input) = match existing {
        Some(id) => (
            "repo.branchProtection.update",
            merge_fields(input, json!({"id": id})),
        ),
        None => ("repo.branchProtection.create", input),
    };
    call(&state, &headers, proc, input, StatusCode::OK).await
}

async fn delete_branch_protection_rest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<BranchPath>,
) -> Response {
    match exact_rule_id(&state, &headers, &p.owner, &p.repo, &p.branch).await {
        Ok(Some(id)) => {
            call(
                &state,
                &headers,
                "repo.branchProtection.delete",
                merge_fields(repo_ref(&p.owner, &p.repo), json!({"id": id})),
                StatusCode::NO_CONTENT,
            )
            .await
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "protection.not_found", "message": "branch is not protected"})),
        )
            .into_response(),
        Err(r) => *r,
    }
}

async fn merge_upstream_rest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(repo_ref(&p.owner, &p.repo), body);
    call_map(
        &state,
        &headers,
        "repo.syncFork",
        input,
        StatusCode::OK,
        |d| {
            let merge_type = match d["status"].as_str().unwrap_or("") {
                "merged" => "merge",
                "fast_forwarded" => "fast-forward",
                _ => "none",
            };
            json!({
                "message": format!("Successfully fetched and fast-forwarded from upstream {}", d["upstream_branch"].as_str().unwrap_or("")),
                "merge_type": merge_type,
                "base_branch": d["upstream_branch"],
            })
        },
    )
    .await
}

async fn list_matching_refs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<GitRefPath>,
) -> Response {
    let prefix = format!("refs/{}", p.ref_name.trim_start_matches("refs/"));
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(&mut ctx, "repo.refs", repo_ref(&p.owner, &p.repo)).await {
        RpcResponse::Ok { data, .. } => {
            let out: Vec<Value> = data["refs"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|r| {
                    r["name"]
                        .as_str()
                        .map(|n| n.starts_with(&prefix))
                        .unwrap_or(false)
                })
                .map(|r| gh_git_ref(r, &p.owner, &p.repo))
                .collect();
            (StatusCode::OK, Json(json!(out))).into_response()
        }
        RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

async fn get_git_ref(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<GitRefPath>,
) -> Response {
    let full = format!("refs/{}", p.ref_name.trim_start_matches("refs/"));
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(&mut ctx, "repo.refs", repo_ref(&p.owner, &p.repo)).await {
        RpcResponse::Ok { data, .. } => {
            let hit = data["refs"].as_array().and_then(|rs| {
                rs.iter()
                    .find(|r| r["name"].as_str() == Some(full.as_str()))
            });
            match hit {
                Some(r) => (StatusCode::OK, Json(gh_git_ref(r, &p.owner, &p.repo))).into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "ref.not_found", "message": "ref not found"})),
                )
                    .into_response(),
            }
        }
        RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

/// Only `refs/heads/*` can be created via `repo.branchCreate` — other
/// namespaces 422 (GitHub creates tags too; tag refs ride the tag pipeline).
async fn create_git_ref(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let Some(branch) = body["ref"]
        .as_str()
        .and_then(|r| r.strip_prefix("refs/heads/"))
    else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "message": "Validation Failed",
                "errors": ["only refs/heads/* can be created via this endpoint"],
            })),
        )
            .into_response();
    };
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({"branch": branch, "start": body["sha"]}),
    );
    call_map(
        &state,
        &headers,
        "repo.branchCreate",
        input,
        StatusCode::CREATED,
        |d| {
            json!({
                "ref": format!("refs/heads/{}", d["branch"].as_str().unwrap_or_default()),
                "object": {"sha": body["sha"], "type": "commit"},
            })
        },
    )
    .await
}

async fn delete_git_ref(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<GitRefPath>,
) -> Response {
    let Some(branch) = p
        .ref_name
        .trim_start_matches("refs/")
        .strip_prefix("heads/")
    else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "message": "Validation Failed",
                "errors": ["only refs/heads/* can be deleted via this endpoint"],
            })),
        )
            .into_response();
    };
    call(
        &state,
        &headers,
        "repo.branchDelete",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"branch": branch})),
        StatusCode::NO_CONTENT,
    )
    .await
}

fn repo_notification_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

// ---------------------------------------------------------------------------
// Repo-scoped notifications (GitHub REST)
// ---------------------------------------------------------------------------

/// `GET /api/v1/repos/{o}/{r}/notifications` — the global list filtered to
/// this repo (`owner`/`repo` on `NotificationPublic`).
async fn list_repo_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<super::users::NotificationsQuery>,
) -> Response {
    match super::users::fetch_notifications(&state, &headers, &q).await {
        Ok(rows) => {
            let page = q.page.unwrap_or(1).max(1);
            let per = q.per_page.unwrap_or(30).clamp(1, 100) as usize;
            let start = ((page - 1) * per as i64).max(0) as usize;
            let out: Vec<Value> = rows
                .iter()
                .filter(|n| {
                    n["owner"].as_str() == Some(p.owner.as_str())
                        && n["repo"].as_str() == Some(p.repo.as_str())
                })
                .skip(start)
                .take(per)
                .map(super::users::gh_notification)
                .collect();
            (StatusCode::OK, Json(json!(out))).into_response()
        }
        Err(r) => *r,
    }
}

/// `PUT /api/v1/repos/{o}/{r}/notifications` — mark this repo's threads
/// read: list unread → `markRead` the matching ids → 202.
async fn mark_repo_notifications_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    let q = super::users::NotificationsQuery {
        all: None,
        participating: None,
        since: None,
        before: None,
        page: Some(1),
        per_page: Some(100),
    };
    match super::users::fetch_notifications(&state, &headers, &q).await {
        Ok(rows) => {
            let ids: Vec<&str> = rows
                .iter()
                .filter(|n| {
                    n["owner"].as_str() == Some(p.owner.as_str())
                        && n["repo"].as_str() == Some(p.repo.as_str())
                })
                .filter_map(|n| n["id"].as_str())
                .collect();
            call(
                &state,
                &headers,
                "notification.markRead",
                json!({"ids": ids}),
                StatusCode::ACCEPTED,
            )
            .await
        }
        Err(r) => *r,
    }
}

// ---------------------------------------------------------------------------
// Deploy keys + transfer (GitHub REST)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DeployKeyPath {
    owner: String,
    repo: String,
    key_id: String,
}

fn deploy_key_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn deploy_key_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "key": {"type": "string"},
            "title": {"type": "string"},
            "read_only": {"type": "boolean"},
            "verified": {"type": "boolean"},
        },
    })
}

fn deploy_key_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["title", "key"],
        "properties": {
            "title": {"type": "string"},
            "key": {"type": "string"},
            "read_only": {"type": "boolean"},
        },
    })
}

fn transfer_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["new_owner"],
        "properties": {
            "new_owner": {"type": "string"},
            "new_name": {"type": "string"},
            "team_ids": {"type": "array", "items": {"type": "integer"}},
        },
    })
}

fn transfer_repo_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

/// `DeployKeyPublic` → GitHub's deploy-key object.
fn gh_deploy_key(k: &Value) -> Value {
    json!({
        "id": k["id"],
        "key": k["public_key"],
        "title": k["title"],
        "verified": true,
        "read_only": !k["can_write"].as_bool().unwrap_or(false),
        "created_at": k["created_at"],
        "last_used": k["last_used_at"],
    })
}

async fn list_deploy_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "repo.deployKey.list",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |d| {
            json!(d["keys"]
                .as_array()
                .map(|ks| ks.iter().map(gh_deploy_key).collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

async fn get_deploy_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<DeployKeyPath>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(&mut ctx, "repo.deployKey.list", repo_ref(&p.owner, &p.repo)).await {
        RpcResponse::Ok { data, .. } => {
            let hit = data["keys"].as_array().and_then(|ks| {
                ks.iter()
                    .find(|k| k["id"].as_str() == Some(p.key_id.as_str()))
            });
            match hit {
                Some(k) => (StatusCode::OK, Json(gh_deploy_key(k))).into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "deploy_key.not_found", "message": "key not found"})),
                )
                    .into_response(),
            }
        }
        RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

/// `POST /repos/{o}/{r}/keys {title, key, read_only}` — `read_only` inverts
/// `can_write` on `repo.deployKey.create`.
async fn create_deploy_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let read_only = body["read_only"].as_bool().unwrap_or(false);
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({
            "title": body["title"],
            "public_key": body["key"],
            "can_write": !read_only,
        }),
    );
    call_map(
        &state,
        &headers,
        "repo.deployKey.create",
        input,
        StatusCode::CREATED,
        |d| gh_deploy_key(&d),
    )
    .await
}

async fn delete_deploy_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<DeployKeyPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.deployKey.delete",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"id": p.key_id})),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `POST /repos/{o}/{r}/transfer {new_owner, new_name?}` — resolve the login
/// to user/org (`org.get` then `user.getPublicProfile`), `confirm_name`
/// auto-fills from the path (the URL already names the repo), then rename
/// when `new_name` is given. GitHub returns 202.
async fn transfer_repo_rest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let Some(new_owner) = body["new_owner"].as_str() else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"message": "Validation Failed", "errors": ["new_owner is required"]})),
        )
            .into_response();
    };
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    // Resolve dest_owner_type — org first, then user profile.
    let dest_type =
        match dispatch(&mut ctx, "org.get", json!({"slug": new_owner})).await {
            RpcResponse::Ok { .. } => "org",
            _ => match dispatch(
                &mut ctx,
                "user.getPublicProfile",
                json!({"username": new_owner}),
            )
            .await
            {
                RpcResponse::Ok { .. } => "user",
                _ => return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(
                        json!({"message": "Validation Failed", "errors": ["new_owner not found"]}),
                    ),
                )
                    .into_response(),
            },
        };
    match dispatch(
        &mut ctx,
        "repo.transfer",
        merge_fields(
            repo_ref(&p.owner, &p.repo),
            json!({
                "dest_owner": new_owner,
                "dest_owner_type": dest_type,
                "confirm_name": p.repo,
            }),
        ),
    )
    .await
    {
        RpcResponse::Err { error, .. } => {
            return (super::status_for_error(&error), Json(error)).into_response();
        }
        RpcResponse::Ok { .. } => {}
    }
    let final_name = body["new_name"].as_str().unwrap_or(&p.repo).to_string();
    if final_name != p.repo {
        match dispatch(
            &mut ctx,
            "repo.rename",
            merge_fields(
                repo_ref(new_owner, &p.repo),
                json!({"new_name": final_name}),
            ),
        )
        .await
        {
            RpcResponse::Err { error, .. } => {
                return (super::status_for_error(&error), Json(error)).into_response();
            }
            RpcResponse::Ok { .. } => {}
        }
    }
    match dispatch(&mut ctx, "repo.get", repo_ref(new_owner, &final_name)).await {
        RpcResponse::Ok { data, .. } => (StatusCode::ACCEPTED, Json(data)).into_response(),
        RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Tag protections / activity / mirror-sync (GitHub+Gitea REST)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TagProtectionPath {
    owner: String,
    repo: String,
    id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RepoActivityQuery {
    /// GitHub's name for our `push_type` filter.
    #[serde(default)]
    activity_type: Option<String>,
    /// `week` | `month` | `year` | `all`.
    #[serde(default)]
    time_period: Option<String>,
    /// GitHub `actor`/`direction` params are accepted but unmapped (the proc
    /// has no actor filter and only emits newest-first).
    #[serde(default)]
    per_page: Option<i64>,
    #[serde(default)]
    page: Option<i64>,
}

fn tag_protection_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn tag_protection_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "name_pattern": {"type": "string"},
            "whitelist_usernames": {"type": "array", "items": {"type": "string"}},
            "whitelist_teams": {"type": "array", "items": {"type": "string"}},
        },
    })
}

fn tag_protection_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["name_pattern"],
        "properties": {"name_pattern": {"type": "string"}},
    })
}

fn tag_protection_patch_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "name_pattern": {"type": "string"},
            "allow_create": {"type": "boolean"},
            "allow_update": {"type": "boolean"},
            "allow_delete": {"type": "boolean"},
            "enforce_admins": {"type": "boolean"},
        },
    })
}

fn activity_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn mirror_sync_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

/// `TagProtectionRulePublic` → Gitea's `TagProtection` shape.
fn gitea_tag_protection(r: &Value) -> Value {
    json!({
        "id": r["id"],
        "name_pattern": r["pattern"],
        "whitelist_usernames": [],
        "whitelist_teams": [],
        "allow_create": r["allow_create"],
        "allow_update": r["allow_update"],
        "allow_delete": r["allow_delete"],
        "enforce_admins": r["enforce_admins"],
        "created_at": r["created_at"],
        "updated_at": r["updated_at"],
    })
}

async fn list_tag_protections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "repo.tagProtection.list",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |d| {
            json!(d["rules"]
                .as_array()
                .map(|rs| rs.iter().map(gitea_tag_protection).collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

/// Fetch a single tag-protection rule by id for get/patch guards.
async fn fetch_tag_rule(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    id: &str,
) -> Result<Option<Value>, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    match dispatch(&mut ctx, "repo.tagProtection.list", repo_ref(owner, repo)).await {
        RpcResponse::Ok { data, .. } => Ok(data["rules"]
            .as_array()
            .and_then(|rs| rs.iter().find(|r| r["id"].as_str() == Some(id)).cloned())),
        RpcResponse::Err { error, .. } => Err(Box::new(
            (super::status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

async fn create_tag_protection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    // Gitea creates a fully-deny rule (empty whitelist); our input takes the
    // allow_* flags explicitly — default all false = nobody may write the tag.
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({"pattern": body["name_pattern"]}),
    );
    call_map(
        &state,
        &headers,
        "repo.tagProtection.create",
        input,
        StatusCode::CREATED,
        |d| gitea_tag_protection(&d),
    )
    .await
}

async fn get_tag_protection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TagProtectionPath>,
) -> Response {
    match fetch_tag_rule(&state, &headers, &p.owner, &p.repo, &p.id).await {
        Ok(Some(r)) => (StatusCode::OK, Json(gitea_tag_protection(&r))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "tag_protection.not_found", "message": "rule not found"})),
        )
            .into_response(),
        Err(r) => *r,
    }
}

/// Gitea PATCH merges — omitted fields keep the stored values.
async fn update_tag_protection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TagProtectionPath>,
    Json(body): Json<Value>,
) -> Response {
    let existing = match fetch_tag_rule(&state, &headers, &p.owner, &p.repo, &p.id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"code": "tag_protection.not_found", "message": "rule not found"})),
            )
                .into_response()
        }
        Err(r) => return *r,
    };
    let pick = |key: &str| -> Value {
        if body.get(key).is_some() {
            body[key].clone()
        } else {
            existing[key].clone()
        }
    };
    let pattern = body["name_pattern"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| existing["pattern"].as_str().unwrap_or_default().to_string());
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({
            "id": p.id,
            "pattern": pattern,
            "allow_create": pick("allow_create"),
            "allow_update": pick("allow_update"),
            "allow_delete": pick("allow_delete"),
            "enforce_admins": pick("enforce_admins"),
        }),
    );
    call_map(
        &state,
        &headers,
        "repo.tagProtection.update",
        input,
        StatusCode::OK,
        |d| gitea_tag_protection(&d),
    )
    .await
}

async fn delete_tag_protection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TagProtectionPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.tagProtection.delete",
        merge_fields(repo_ref(&p.owner, &p.repo), json!({"id": p.id})),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `RepoActivityItem` → GitHub's activity row.
fn gh_activity(a: &Value) -> Value {
    json!({
        "id": a["id"],
        "ref": a["ref_name"],
        "activity_type": a["push_type"],
        "before": a["before"],
        "after": a["after"],
        "timestamp": a["pushed_at"],
        "actor": {
            "login": a["pusher"]["login"],
            "name": a["pusher"]["name"],
            "avatar_url": a["pusher"]["avatar_url"],
        },
    })
}

/// `GET /repos/{o}/{r}/activity` — GitHub query names onto `repo.activity.list`.
async fn list_repo_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<RepoActivityQuery>,
) -> Response {
    let per = q.per_page.unwrap_or(30).clamp(1, 100);
    let page = q.page.unwrap_or(1).max(1);
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({
            "push_type": q.activity_type,
            "period": q.time_period,
            "offset": (page - 1) * per,
            "limit": per,
        }),
    );
    call_map(
        &state,
        &headers,
        "repo.activity.list",
        input,
        StatusCode::OK,
        |d| {
            json!(d["items"]
                .as_array()
                .map(|is| is.iter().map(gh_activity).collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

/// `POST /repos/{o}/{r}/mirror-sync` — Gitea returns 200 + `{ok: true}`.
async fn mirror_sync_rest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "repo.mirror.syncNow",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |d| json!({"ok": d["enqueued"], "mirror": d["mirror"]}),
    )
    .await
}

/// `PUT /repos/{o}/{r}/contents/{path}` — GitHub create-or-update. `sha`
/// in the body selects `repo.file.update`; absent → `repo.file.create`.
/// `content` is base64 (GitHub convention) → `content_base64`.
async fn put_contents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TreePath>,
    Json(body): Json<Value>,
) -> Response {
    let path = p.path.clone().unwrap_or_default();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["path"] = json!(path);
    for k in [
        "message",
        "branch",
        "new_branch",
        "open_pr",
        "pr_title",
        "pr_body",
    ] {
        if let Some(v) = body.get(k) {
            input[k] = v.clone();
        }
    }
    if let Some(c) = body.get("content") {
        input["content_base64"] = c.clone();
    }
    let update = body.get("sha").and_then(Value::as_str).is_some();
    let (proc, status) = if update {
        ("repo.file.update", StatusCode::OK)
    } else {
        ("repo.file.create", StatusCode::CREATED)
    };
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let commit = match dispatch(&mut ctx, proc, input).await {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    // Follow-up read to project GitHub's `content` member.
    let blob_input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({"ref": commit["branch"], "path": path}),
    );
    let content = match dispatch(&mut ctx, "repo.blob", blob_input).await {
        RpcResponse::Ok { data, .. } => json!({
            "name": path.rsplit('/').next().unwrap_or(&path),
            "path": data["path"],
            "size": data["size"],
            "encoding": "base64",
            "content": data["content"],
            "type": "file",
        }),
        RpcResponse::Err { .. } => Value::Null,
    };
    (
        status,
        Json(json!({
            "content": content,
            "commit": {
                "sha": commit["commit_sha"],
                "message": body["message"],
            },
        })),
    )
        .into_response()
}

/// `DELETE /repos/{o}/{r}/contents/{path}` — `{message, sha?, branch?}` →
/// `repo.file.delete`; response `content` is null per GitHub.
async fn delete_contents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<TreePath>,
    Json(body): Json<Value>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    input["path"] = json!(p.path.unwrap_or_default());
    for k in ["message", "branch", "new_branch"] {
        if let Some(v) = body.get(k) {
            input[k] = v.clone();
        }
    }
    call_map(
        &state,
        &headers,
        "repo.file.delete",
        input,
        StatusCode::OK,
        |d| {
            json!({
                "content": Value::Null,
                "commit": {
                    "sha": d["commit_sha"],
                    "message": body["message"],
                },
            })
        },
    )
    .await
}

/// `GET /repos/{o}/{r}/contributors` — GitHub bare-array projection; rows
/// without a linked account report as `Anonymous`.
async fn list_contributors(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "repo.contributors.list",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |d| {
            json!(d["contributors"]
                .as_array()
                .map(|cs| cs
                    .iter()
                    .map(|c| {
                        if let Some(login) = c["username"].as_str() {
                            json!({
                                "login": login,
                                "avatar_url": c["avatar_url"],
                                "contributions": c["commit_count"],
                                "type": "User",
                            })
                        } else {
                            json!({
                                "name": c["display_name"],
                                "contributions": c["commit_count"],
                                "type": "Anonymous",
                            })
                        }
                    })
                    .collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

/// `GET /repos/{o}/{r}/topics` — `{names: [...]}`.
async fn list_topics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "repo.get",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |d| json!({"names": d["topics"]}),
    )
    .await
}

/// `PUT /repos/{o}/{r}/topics` — GitHub `{names}` replaces the topic set.
async fn replace_topics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({"topics": body["names"]}),
    );
    call_map(
        &state,
        &headers,
        "repo.updateMetadata",
        input,
        StatusCode::OK,
        |d| json!({"names": d["topics"]}),
    )
    .await
}

fn put_contents_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["message", "content"],
        "properties": {
            "message": {"type": "string"},
            "content": {"type": "string", "description": "Base64-encoded file bytes"},
            "sha": {"type": "string", "description": "Present → update; absent → create"},
            "branch": {"type": "string"},
            "new_branch": {"type": "string"},
            "committer": {"type": "object", "description": "Accepted for compat; not persisted"},
            "author": {"type": "object", "description": "Accepted for compat; not persisted"}
        }
    })
}

fn delete_contents_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["message"],
        "properties": {
            "message": {"type": "string"},
            "sha": {"type": "string", "description": "Accepted for compat; not enforced"},
            "branch": {"type": "string"}
        }
    })
}

fn contents_write_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "content": {"type": ["object", "null"]},
            "commit": {"type": "object", "properties": {
                "sha": {"type": "string"},
                "message": {"type": "string"}
            }}
        }
    })
}

fn contributors_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "array",
        "items": {"type": "object", "properties": {
            "login": {"type": "string"},
            "name": {"type": "string"},
            "avatar_url": {"type": ["string", "null"]},
            "contributions": {"type": "integer"},
            "type": {"type": "string", "enum": ["User", "Anonymous"]}
        }}
    })
}

fn topics_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["names"],
        "properties": {"names": {"type": "array", "items": {"type": "string"}}}
    })
}

fn topics_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["names"],
        "properties": {"names": {"type": "array", "items": {"type": "string"}}}
    })
}
