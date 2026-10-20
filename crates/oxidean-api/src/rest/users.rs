//! `/api/v1/user*` + `/api/v1/users/**` — authenticated self + public profiles.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::Json;
use oxidean_core::{
    CreateRepoRequest, ListStarredRequest, OrgListMineResponse, PublicUserProfile,
    RepoListMineResponse, RepoPublic, RpcResponse, UserPublic,
};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, call_map, ctx_for, dispatch, status_for_error};
use crate::app::AppState;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/user",
        tags: &["users"],
        operation_id: "getAuthenticatedUser",
        summary: "Authenticated user (`auth.me`)",
        procedure: "auth.me",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<UserPublic>,
        )),
        mount: || get(current_user),
    },
    RouteDef {
        method: "PATCH",
        path: "/user",
        tags: &["users"],
        operation_id: "updateAuthenticatedUser",
        summary: "Update profile (`user.update_profile`; `name` → display_name; unspecified fields keep current values)",
        procedure: "user.update_profile",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Json(user_patch_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<UserPublic>,
        )),
        mount: || patch(update_current_user),
    },
    RouteDef {
        method: "GET",
        path: "/user/repos",
        tags: &["users", "repos"],
        operation_id: "listMyRepos",
        summary: "List repos owned by the caller (`repo.listMine`)",
        procedure: "repo.listMine",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoListMineResponse>,
        )),
        mount: || get(list_my_repos),
    },
    RouteDef {
        method: "POST",
        path: "/user/repos",
        tags: &["users", "repos"],
        operation_id: "createMyRepo",
        summary: "Create a repo owned by the caller (`repo.create`)",
        procedure: "repo.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateRepoRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoPublic>,
        )),
        mount: || post(create_my_repo),
    },
    RouteDef {
        method: "GET",
        path: "/user/orgs",
        tags: &["users", "orgs"],
        operation_id: "listMyOrgs",
        summary: "Orgs the caller belongs to (`org.listMine`)",
        procedure: "org.listMine",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<OrgListMineResponse>,
        )),
        mount: || get(list_my_orgs),
    },
    RouteDef {
        method: "GET",
        path: "/user/starred",
        tags: &["users", "repos"],
        operation_id: "listStarred",
        summary: "Repos starred by the caller (`user.listStarred`)",
        procedure: "user.listStarred",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: Some(SchemaGenerator::into_root_schema_for::<ListStarredRequest>),
        body: None,
        response: Some(RespSpec::Json(repo_array_schema)),
        mount: || get(list_starred),
    },
    // ---- notifications (GitHub REST) ----------------------------------------
    RouteDef {
        method: "GET",
        path: "/notifications",
        tags: &["activity"],
        operation_id: "activity/listNotifications",
        summary: "Notification threads (`notification.list`) — `all`, `participating`, `since`, `before`, paging",
        procedure: "notification.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: Some(SchemaGenerator::into_root_schema_for::<NotificationsQuery>),
        body: None,
        response: Some(RespSpec::Json(notification_list_schema)),
        mount: || get(list_notifications_rest),
    },
    RouteDef {
        method: "PUT",
        path: "/notifications",
        tags: &["activity"],
        operation_id: "activity/markNotificationsRead",
        summary: "Mark every notification read (`notification.markAllRead`) — 202",
        procedure: "notification.markAllRead",
        ok: StatusCode::ACCEPTED,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Json(mark_notifications_body_schema)),
        response: Some(RespSpec::Json(mark_notifications_schema)),
        mount: || put(mark_notifications_read),
    },
    RouteDef {
        method: "GET",
        path: "/notifications/new",
        tags: &["notifications"],
        operation_id: "unreadNotificationCount",
        summary: "Unread notification count (`notification.unreadCount`) → `{new: N}`",
        procedure: "notification.unreadCount",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(unread_count_schema)),
        mount: || get(unread_notification_count),
    },
    RouteDef {
        method: "GET",
        path: "/notifications/threads/{thread_id}",
        tags: &["activity"],
        operation_id: "activity/getThread",
        summary: "One notification thread (`notification.list` filtered by id) — 404 when absent",
        procedure: "notification.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(notification_schema)),
        mount: || get(get_notification_thread),
    },
    RouteDef {
        method: "PATCH",
        path: "/notifications/threads/{thread_id}",
        tags: &["activity"],
        operation_id: "activity/markThreadRead",
        summary: "Mark a thread read (`notification.markRead`) — 205",
        procedure: "notification.markRead",
        ok: StatusCode::RESET_CONTENT,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: None,
        mount: || patch(mark_notification_thread_read),
    },
    // ---- follows (GitHub REST) ------------------------------------------------
    RouteDef {
        method: "GET",
        path: "/user/followers",
        tags: &["users"],
        operation_id: "users/listFollowersOfMe",
        summary: "Caller's followers (`user.followers.list`)",
        procedure: "user.followers.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(follow_list_schema)),
        mount: || get(list_my_followers),
    },
    RouteDef {
        method: "GET",
        path: "/user/following",
        tags: &["users"],
        operation_id: "users/listFollowedByMe",
        summary: "Users the caller follows (`user.following.list`)",
        procedure: "user.following.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(follow_list_schema)),
        mount: || get(list_my_following),
    },
    RouteDef {
        method: "GET",
        path: "/user/following/{username}",
        tags: &["users"],
        operation_id: "users/checkFollowedByMe",
        summary: "Caller follows `{username}` — 204 / 404",
        procedure: "user.following.list",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: None,
        mount: || get(check_my_following),
    },
    RouteDef {
        method: "PUT",
        path: "/user/following/{username}",
        tags: &["users"],
        operation_id: "users/follow",
        summary: "Follow `{username}` (`user.follow`) — 204",
        procedure: "user.follow",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: None,
        mount: || put(follow_user),
    },
    RouteDef {
        method: "DELETE",
        path: "/user/following/{username}",
        tags: &["users"],
        operation_id: "users/unfollow",
        summary: "Unfollow `{username}` (`user.unfollow`) — 204",
        procedure: "user.unfollow",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: None,
        mount: || delete(unfollow_user),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}/followers",
        tags: &["users"],
        operation_id: "users/listFollowersOfUser",
        summary: "User's followers (`user.followers.list`)",
        procedure: "user.followers.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["username"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(follow_list_schema)),
        mount: || get(list_user_followers),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}/following",
        tags: &["users"],
        operation_id: "users/listFollowedByUser",
        summary: "Users followed by `{username}` (`user.following.list`)",
        procedure: "user.following.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["username"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(follow_list_schema)),
        mount: || get(list_user_following),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}/following/{target}",
        tags: &["users"],
        operation_id: "users/checkFollowing",
        summary: "`{username}` follows `{target}` — 204 / 404",
        procedure: "user.following.list",
        ok: StatusCode::NO_CONTENT,
        anonymous: true,
        path_fields: &["username", "target"],
        query: None,
        body: None,
        response: None,
        mount: || get(check_user_following),
    },
    // ---- account emails (GitHub REST) ----------------------------------------
    RouteDef {
        method: "GET",
        path: "/user/emails",
        tags: &["users"],
        operation_id: "users/listEmails",
        summary: "Caller's email addresses (`email.list`) — GitHub `{email, primary, verified}` rows",
        procedure: "email.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(email_list_schema)),
        mount: || get(list_emails),
    },
    RouteDef {
        method: "POST",
        path: "/user/emails",
        tags: &["users"],
        operation_id: "users/addEmails",
        summary: "Add addresses (`email.add` per entry) — `{emails: [..]}` → 201 + list",
        procedure: "email.add",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Json(email_body_schema)),
        response: Some(RespSpec::Json(email_list_schema)),
        mount: || post(add_emails),
    },
    RouteDef {
        method: "DELETE",
        path: "/user/emails",
        tags: &["users"],
        operation_id: "users/deleteEmails",
        summary: "Remove addresses (`email.remove` via list→id) — `{emails: [..]}` → 204",
        procedure: "email.remove",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Json(email_body_schema)),
        response: None,
        mount: || delete(delete_emails),
    },
    // ---- SSH / GPG keys (GitHub REST) ----------------------------------------
    RouteDef {
        method: "GET",
        path: "/user/keys",
        tags: &["users"],
        operation_id: "users/listSshKeys",
        summary: "Caller's SSH keys (`sshKey.list`) — GitHub `{id, key, title, verified, read_only}` rows",
        procedure: "sshKey.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(ssh_key_list_schema)),
        mount: || get(list_ssh_keys),
    },
    RouteDef {
        method: "POST",
        path: "/user/keys",
        tags: &["users"],
        operation_id: "users/createSshKey",
        summary: "Add an SSH key (`sshKey.add`) — `{title, key}` → 201",
        procedure: "sshKey.add",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Json(ssh_key_body_schema)),
        response: Some(RespSpec::Json(ssh_key_schema)),
        mount: || post(create_ssh_key),
    },
    RouteDef {
        method: "GET",
        path: "/user/keys/{key_id}",
        tags: &["users"],
        operation_id: "users/getSshKey",
        summary: "One SSH key (`sshKey.list`, id-filtered) — 404 when absent",
        procedure: "sshKey.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(ssh_key_schema)),
        mount: || get(get_ssh_key),
    },
    RouteDef {
        method: "DELETE",
        path: "/user/keys/{key_id}",
        tags: &["users"],
        operation_id: "users/deleteSshKey",
        summary: "Revoke an SSH key (`sshKey.revoke`) — 204",
        procedure: "sshKey.revoke",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_ssh_key),
    },
    RouteDef {
        method: "GET",
        path: "/user/gpg_keys",
        tags: &["users"],
        operation_id: "users/listGpgKeys",
        summary: "Caller's GPG keys (`gpgKey.list`) — GitHub `{id, name, key_id, public_key, emails}` rows",
        procedure: "gpgKey.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(gpg_key_list_schema)),
        mount: || get(list_gpg_keys),
    },
    RouteDef {
        method: "POST",
        path: "/user/gpg_keys",
        tags: &["users"],
        operation_id: "users/createGpgKey",
        summary: "Add a GPG key (`gpgKey.add`) — `{name, armored_public_key}` → 201",
        procedure: "gpgKey.add",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: Some(BodySpec::Json(gpg_key_body_schema)),
        response: Some(RespSpec::Json(gpg_key_schema)),
        mount: || post(create_gpg_key),
    },
    RouteDef {
        method: "GET",
        path: "/user/gpg_keys/{gpg_key_id}",
        tags: &["users"],
        operation_id: "users/getGpgKey",
        summary: "One GPG key (`gpgKey.list`, id-filtered) — 404 when absent",
        procedure: "gpgKey.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: Some(RespSpec::Json(gpg_key_schema)),
        mount: || get(get_gpg_key),
    },
    RouteDef {
        method: "DELETE",
        path: "/user/gpg_keys/{gpg_key_id}",
        tags: &["users"],
        operation_id: "users/deleteGpgKey",
        summary: "Revoke a GPG key (`gpgKey.revoke`) — 204",
        procedure: "gpgKey.revoke",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &[],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_gpg_key),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}",
        tags: &["users"],
        operation_id: "getUser",
        summary: "Public profile (`user.getPublicProfile`)",
        procedure: "user.getPublicProfile",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["username"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<PublicUserProfile>,
        )),
        mount: || get(get_user),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}/repos",
        tags: &["users", "repos"],
        operation_id: "listUserRepos",
        summary: "Repos under a user (`repo.listByOwner`)",
        procedure: "repo.listByOwner",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["username"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<RepoListMineResponse>,
        )),
        mount: || get(list_user_repos),
    },
    RouteDef {
        method: "GET",
        path: "/search/repositories",
        tags: &["repos"],
        operation_id: "searchRepositories",
        summary: "Repository search (`repo.explore`) — GitHub `{total_count, items}` envelope",
        procedure: "repo.explore",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &[],
        query: Some(SchemaGenerator::into_root_schema_for::<RepoSearchQuery>),
        body: None,
        response: Some(RespSpec::Json(search_repos_schema)),
        mount: || get(search_repositories),
    },
    RouteDef {
        method: "GET",
        path: "/topics",
        tags: &["repos"],
        operation_id: "searchTopics",
        summary: "Topic search (`repo.topicsSuggest`) — bare array (Gitea)",
        procedure: "repo.topicsSuggest",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &[],
        query: Some(SchemaGenerator::into_root_schema_for::<TopicsQuery>),
        body: None,
        response: Some(RespSpec::Json(topics_search_schema)),
        mount: || get(search_topics),
    },
];

/// `GET /api/v1/user` (`auth.me`) — the authenticated account.
async fn current_user(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "auth.me",
        Value::Object(serde_json::Map::new()),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/user/repos` (`repo.listMine`) — caller's own repos.
async fn list_my_repos(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "repo.listMine",
        Value::Object(serde_json::Map::new()),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/user/repos` (`repo.create` owned by the caller) —
/// body: `name` (required); `description`, `visibility`, `stack_id`,
/// `instance_pack_id`, `template_repo_id`, `license_id`, `gitignore_id`.
async fn create_my_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    call(&state, &headers, "repo.create", body, StatusCode::CREATED).await
}

/// `GET /api/v1/user/orgs` (`org.listMine`).
async fn list_my_orgs(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call(
        &state,
        &headers,
        "org.listMine",
        Value::Object(serde_json::Map::new()),
        StatusCode::OK,
    )
    .await
}

#[derive(serde::Serialize, Deserialize)]
pub struct StarredQuery {
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

/// `GET /api/v1/user/starred` (`user.listStarred`) — GitHub answers a bare
/// array of repo objects, not the `{repos: [...]}` RPC envelope.
async fn list_starred(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<StarredQuery>,
) -> Response {
    let input = serde_json::to_value(&q).unwrap_or_else(|_| json!({}));
    call_map(
        &state,
        &headers,
        "user.listStarred",
        input,
        StatusCode::OK,
        |d| d["repos"].clone(),
    )
    .await
}

#[derive(Deserialize)]
pub struct UserPath {
    username: String,
}

/// `GET /api/v1/users/{username}` (`user.getPublicProfile`) — public profile,
/// anonymous OK.
async fn get_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<UserPath>,
) -> Response {
    call(
        &state,
        &headers,
        "user.getPublicProfile",
        json!({ "username": p.username }),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/users/{username}/repos` (`repo.listByOwner`).
async fn list_user_repos(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<UserPath>,
) -> Response {
    call(
        &state,
        &headers,
        "repo.listByOwner",
        json!({ "owner": p.username }),
        StatusCode::OK,
    )
    .await
}

// ---------------------------------------------------------------------------
// Notifications (GitHub REST)
// ---------------------------------------------------------------------------

#[derive(Deserialize, schemars::JsonSchema)]
pub struct NotificationsQuery {
    /// `true` → read + unread (`filter=all`); default unread only.
    #[serde(default)]
    pub all: Option<bool>,
    /// `true` → only threads where the caller is directly involved
    /// (mention / review / assign reasons).
    #[serde(default)]
    pub participating: Option<bool>,
    /// ISO-8601 — only threads updated at or after.
    #[serde(default)]
    pub since: Option<String>,
    /// ISO-8601 — only threads updated at or before.
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub per_page: Option<i64>,
}

fn repo_array_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn notification_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn notification_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "repository": {"type": "object"},
            "subject": {"type": "object"},
            "reason": {"type": "string"},
            "unread": {"type": "boolean"},
        },
    })
}

fn mark_notifications_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "last_read_at": {"type": "string", "format": "date-time"},
            "read": {"type": "boolean"},
        },
    })
}

fn mark_notifications_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object", "properties": {"message": {"type": "string"}}})
}

/// `subject_kind` → GitHub's `subject.type` vocabulary.
fn gh_subject_type(kind: &str) -> &'static str {
    match kind {
        "issue" => "Issue",
        "pull_request" => "PullRequest",
        "release" => "Release",
        "workflow_run" => "CheckSuite",
        _ => "Commit",
    }
}

/// Subject deep-link → the matching REST URL (None → `null`).
fn gh_subject_url(n: &Value) -> Value {
    let owner = n["owner"].as_str().unwrap_or_default();
    let repo = n["repo"].as_str().unwrap_or_default();
    let num = n["subject_number"].as_i64();
    let refr = n["subject_ref"].as_str();
    let path = match n["subject_kind"].as_str().unwrap_or("") {
        "issue" => num.map(|i| format!("issues/{i}")),
        "pull_request" => num.map(|i| format!("pulls/{i}")),
        "release" => refr.map(|t| format!("releases/tags/{t}")),
        "workflow_run" => refr.map(|r| format!("actions/runs/{r}")),
        _ => None,
    };
    match path {
        Some(p) => json!(format!("/api/v1/repos/{owner}/{repo}/{p}")),
        None => Value::Null,
    }
}

/// `NotificationPublic` → GitHub's thread shape (`/notifications` rows).
pub(crate) fn gh_notification(n: &Value) -> Value {
    let owner = n["owner"].as_str().unwrap_or_default();
    let repo = n["repo"].as_str().unwrap_or_default();
    let id = n["id"].as_str().unwrap_or_default();
    json!({
        "id": id,
        "unread": n["read_at"].is_null(),
        "reason": n["reason"],
        "updated_at": n["created_at"],
        "last_read_at": n["read_at"],
        "url": format!("/api/v1/notifications/threads/{id}"),
        "repository": {
            "id": n["subject_repo_id"],
            "name": repo,
            "full_name": format!("{owner}/{repo}"),
            "owner": {"login": owner},
        },
        "subject": {
            "title": n["subject_title"],
            "url": gh_subject_url(n),
            "latest_comment_url": Value::Null,
            "type": gh_subject_type(n["subject_kind"].as_str().unwrap_or_default()),
        },
    })
}

/// Reasons that count as direct participation for `participating=true`.
/// Our reason vocabulary is activity-style (`pr_mention`, `review_requested`,
/// `assigned`, …) — substring-matched rather than enumerated so new kinds
/// join automatically.
fn is_participating(n: &Value) -> bool {
    n["reason"]
        .as_str()
        .map(|r| {
            r.contains("mention")
                || r.contains("review")
                || r.contains("assign")
                || r.contains("author")
                || r.contains("comment")
        })
        .unwrap_or(false)
}

/// Fetch the caller's notifications via `notification.list`, honoring
/// `all`/`participating`/`since`/`before`/paging. Returns the raw
/// `notifications` array for callers to filter further (repo scope).
pub(crate) async fn fetch_notifications(
    state: &AppState,
    headers: &HeaderMap,
    q: &NotificationsQuery,
) -> Result<Vec<Value>, Box<Response>> {
    let page = q.page.unwrap_or(1).max(1);
    let per = q.per_page.unwrap_or(30).clamp(1, 100);
    let filter = if q.all == Some(true) { "all" } else { "unread" };
    // `since`/`before`/`participating` filter post-list, so over-fetch the
    // window to keep `per_page` honest after filtering.
    let input = json!({
        "filter": filter,
        "offset": 0,
        "limit": (page * per) + 250,
    });
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    let rows = match super::dispatch(&mut ctx, "notification.list", input).await {
        super::RpcResponse::Ok { data, .. } => data["notifications"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        super::RpcResponse::Err { error, .. } => {
            return Err(Box::new(
                (super::status_for_error(&error), Json(error)).into_response(),
            ))
        }
    };
    Ok(rows
        .into_iter()
        .filter(|n| {
            let created = n["created_at"].as_str().unwrap_or_default();
            q.since.as_deref().map(|s| created >= s).unwrap_or(true)
                && q.before.as_deref().map(|b| created <= b).unwrap_or(true)
                && (!q.participating.unwrap_or(false) || is_participating(n))
        })
        .collect())
}

async fn list_notifications_rest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<NotificationsQuery>,
) -> Response {
    match fetch_notifications(&state, &headers, &q).await {
        Ok(rows) => {
            let page = q.page.unwrap_or(1).max(1);
            let per = q.per_page.unwrap_or(30).clamp(1, 100) as usize;
            let start = ((page - 1) * per as i64).max(0) as usize;
            let out: Vec<Value> = rows
                .iter()
                .skip(start)
                .take(per)
                .map(gh_notification)
                .collect();
            (StatusCode::OK, Json(json!(out))).into_response()
        }
        Err(r) => *r,
    }
}

/// `PUT /api/v1/notifications` — mark all read; 202 like GitHub.
async fn mark_notifications_read(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call_map(
        &state,
        &headers,
        "notification.markAllRead",
        json!({}),
        StatusCode::ACCEPTED,
        |d| json!({"message": format!("Marked {} notifications as read", d["marked"].as_i64().unwrap_or(0))}),
    )
    .await
}

/// `GET /api/v1/notifications/threads/{id}` — the list is the only read
/// surface (notifications are subject-level, not thread-level).
async fn get_notification_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let q = NotificationsQuery {
        all: Some(true),
        participating: None,
        since: None,
        before: None,
        page: Some(1),
        per_page: Some(100),
    };
    match fetch_notifications(&state, &headers, &q).await {
        Ok(rows) => match rows.iter().find(|n| n["id"].as_str() == Some(id.as_str())) {
            Some(n) => (StatusCode::OK, Json(gh_notification(n))).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(json!({"code": "notification.not_found", "message": "thread not found"})),
            )
                .into_response(),
        },
        Err(r) => *r,
    }
}

/// `PATCH /api/v1/notifications/threads/{id}` — mark read; 205 like GitHub.
async fn mark_notification_thread_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    call(
        &state,
        &headers,
        "notification.markRead",
        json!({"ids": [id]}),
        StatusCode::RESET_CONTENT,
    )
    .await
}

// ---------------------------------------------------------------------------
// Follows + emails (GitHub REST)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FollowPath {
    username: String,
}

#[derive(Deserialize)]
pub struct FollowPairPath {
    username: String,
    target: String,
}

fn follow_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn email_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn email_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["emails"],
        "properties": {"emails": {"type": "array", "items": {"type": "string"}}},
    })
}

/// `UserFollowPublic` → GitHub's user-list row.
fn gh_follow_row(u: &Value) -> Value {
    let login = u["username"].as_str().unwrap_or_default();
    json!({
        "login": login,
        "id": u["user_id"],
        "avatar_url": u["avatar_url"],
        "type": "User",
        "url": format!("/api/v1/users/{login}"),
        "html_url": format!("/{login}"),
    })
}

/// `user.followers.list`/`following.list` → GitHub bare user array.
async fn follow_list(
    state: &AppState,
    headers: &HeaderMap,
    proc: &'static str,
    username: String,
) -> Response {
    call_map(
        state,
        headers,
        proc,
        json!({"username": username, "limit": 100}),
        StatusCode::OK,
        |d| {
            json!(d["users"]
                .as_array()
                .map(|us| us.iter().map(gh_follow_row).collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

/// Caller username via `auth.me` for the `/user/following*` routes.
async fn me_username(state: &AppState, headers: &HeaderMap) -> Result<String, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    match super::dispatch(&mut ctx, "auth.me", json!({})).await {
        super::RpcResponse::Ok { data, .. } => data["username"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| {
                Box::new(
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"code": "auth.me.no_username", "message": "session has no username"})),
                    )
                        .into_response(),
                )
            }),
        super::RpcResponse::Err { error, .. } => Err(Box::new(
            (super::status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

async fn list_user_followers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<FollowPath>,
) -> Response {
    follow_list(&state, &headers, "user.followers.list", p.username).await
}

async fn list_user_following(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<FollowPath>,
) -> Response {
    follow_list(&state, &headers, "user.following.list", p.username).await
}

async fn list_my_followers(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match me_username(&state, &headers).await {
        Ok(me) => follow_list(&state, &headers, "user.followers.list", me).await,
        Err(r) => *r,
    }
}

async fn list_my_following(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match me_username(&state, &headers).await {
        Ok(me) => follow_list(&state, &headers, "user.following.list", me).await,
        Err(r) => *r,
    }
}

/// `GET /users/{u}/following/{target}` — 204 when the edge exists, 404 else.
async fn check_user_following(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<FollowPairPath>,
) -> Response {
    check_user_following_inner(&state, &headers, p.username, p.target).await
}

/// `GET /user/following/{u}` — same check against the caller's own list.
async fn check_my_following(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<FollowPath>,
) -> Response {
    match me_username(&state, &headers).await {
        Ok(me) => check_user_following_inner(&state, &headers, me, p.username).await,
        Err(r) => *r,
    }
}

async fn check_user_following_inner(
    state: &AppState,
    headers: &HeaderMap,
    username: String,
    target: String,
) -> Response {
    let mut ctx = match ctx_for(state, headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match super::dispatch(
        &mut ctx,
        "user.following.list",
        json!({"username": username, "limit": 100}),
    )
    .await
    {
        super::RpcResponse::Ok { data, .. } => {
            let found = data["users"]
                .as_array()
                .map(|us| {
                    us.iter()
                        .any(|u| u["username"].as_str() == Some(target.as_str()))
                })
                .unwrap_or(false);
            if found {
                StatusCode::NO_CONTENT.into_response()
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
        super::RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

async fn follow_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<FollowPath>,
) -> Response {
    call(
        &state,
        &headers,
        "user.follow",
        json!({"username": p.username}),
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn unfollow_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<FollowPath>,
) -> Response {
    call(
        &state,
        &headers,
        "user.unfollow",
        json!({"username": p.username}),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `EmailListItem` → GitHub's `{email, primary, verified, visibility}` row.
fn gh_email(e: &Value) -> Value {
    json!({
        "email": e["email"],
        "primary": e["is_primary"],
        "verified": e["verified"],
        "visibility": "public",
    })
}

async fn list_emails(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call_map(
        &state,
        &headers,
        "email.list",
        json!({}),
        StatusCode::OK,
        |d| {
            json!(d
                .as_array()
                .map(|es| es.iter().map(gh_email).collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

/// `POST /user/emails {emails: [..]}` — fan out `email.add` per address.
async fn add_emails(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let emails: Vec<String> = body["emails"]
        .as_array()
        .map(|es| {
            es.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if emails.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"message": "Validation Failed", "errors": ["emails is required"]})),
        )
            .into_response();
    }
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    for email in &emails {
        match super::dispatch(&mut ctx, "email.add", json!({"email": email})).await {
            super::RpcResponse::Ok { .. } => {}
            super::RpcResponse::Err { error, .. } => {
                return (super::status_for_error(&error), Json(error)).into_response();
            }
        }
    }
    // GitHub returns the full list after add.
    match super::dispatch(&mut ctx, "email.list", json!({})).await {
        super::RpcResponse::Ok { data, .. } => {
            let out: Vec<Value> = data
                .as_array()
                .map(|es| es.iter().map(gh_email).collect())
                .unwrap_or_default();
            (StatusCode::CREATED, Json(json!(out))).into_response()
        }
        super::RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

/// `DELETE /user/emails {emails: [..]}` — resolve each address to its row id
/// (`email.remove` takes ids), then remove; 204.
async fn delete_emails(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let want: Vec<String> = body["emails"]
        .as_array()
        .map(|es| {
            es.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let rows = match super::dispatch(&mut ctx, "email.list", json!({})).await {
        super::RpcResponse::Ok { data, .. } => data.as_array().cloned().unwrap_or_default(),
        super::RpcResponse::Err { error, .. } => {
            return (super::status_for_error(&error), Json(error)).into_response();
        }
    };
    for addr in &want {
        let Some(id) = rows
            .iter()
            .find(|e| e["email"].as_str() == Some(addr.as_str()))
            .and_then(|e| e["id"].as_str())
        else {
            continue; // unknown address → GitHub still 204s
        };
        match super::dispatch(&mut ctx, "email.remove", json!({"id": id})).await {
            super::RpcResponse::Ok { .. } => {}
            super::RpcResponse::Err { error, .. } => {
                return (super::status_for_error(&error), Json(error)).into_response();
            }
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

// ---------------------------------------------------------------------------
// SSH / GPG keys (GitHub REST)
// ---------------------------------------------------------------------------

fn ssh_key_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn ssh_key_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "key": {"type": "string"},
            "title": {"type": "string"},
            "verified": {"type": "boolean"},
            "read_only": {"type": "boolean"},
            "created_at": {"type": "string"},
        },
    })
}

fn ssh_key_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["title", "key"],
        "properties": {
            "title": {"type": "string"},
            "key": {"type": "string"},
        },
    })
}

fn gpg_key_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn gpg_key_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "name": {"type": "string"},
            "key_id": {"type": "string"},
            "public_key": {"type": "string"},
            "emails": {"type": "array", "items": {"type": "object"}},
            "can_sign": {"type": "boolean"},
        },
    })
}

fn gpg_key_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["armored_public_key"],
        "properties": {
            "name": {"type": "string"},
            "armored_public_key": {"type": "string"},
        },
    })
}

/// `SshKeyListItem` → GitHub's ssh-key object.
fn gh_ssh_key(k: &Value) -> Value {
    json!({
        "id": k["id"],
        "key": k["public_key"],
        "title": k["title"],
        "verified": true,
        // Sign-only keys can't authenticate git — GitHub's closest field.
        "read_only": !k["can_authenticate"].as_bool().unwrap_or(true),
        "created_at": k["created_at"],
        "last_used": k["last_used_at"],
    })
}

/// `GpgKeyListItem` → GitHub's gpg-key object.
fn gh_gpg_key(k: &Value) -> Value {
    let emails: Vec<Value> = k["uid_emails"]
        .as_array()
        .map(|es| {
            es.iter()
                .map(|e| json!({"email": e, "verified": true}))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": k["id"],
        "name": k["title"],
        "key_id": k["key_id"],
        "public_key": k["armored_public_key"],
        "emails": emails,
        "subkeys": [],
        "can_sign": true,
        "can_encrypt_comms": false,
        "can_encrypt_storage": false,
        "can_certify": true,
        "created_at": k["created_at"],
        "expires_at": Value::Null,
    })
}

async fn list_ssh_keys(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call_map(
        &state,
        &headers,
        "sshKey.list",
        json!({}),
        StatusCode::OK,
        |d| {
            json!(d
                .as_array()
                .map(|ks| ks.iter().map(gh_ssh_key).collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

/// `GET /user/keys/{id}` — no per-key proc; list + id filter → 404.
async fn get_ssh_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match super::dispatch(&mut ctx, "sshKey.list", json!({})).await {
        super::RpcResponse::Ok { data, .. } => {
            let hit = data
                .as_array()
                .and_then(|ks| ks.iter().find(|k| k["id"].as_str() == Some(id.as_str())));
            match hit {
                Some(k) => (StatusCode::OK, Json(gh_ssh_key(k))).into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
        super::RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

/// `POST /user/keys {title, key}` — `key` → `public_key`; GitHub returns 201 + key.
async fn create_ssh_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    call_map(
        &state,
        &headers,
        "sshKey.add",
        json!({"title": body["title"], "public_key": body["key"]}),
        StatusCode::CREATED,
        |d| gh_ssh_key(&d),
    )
    .await
}

async fn delete_ssh_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    call(
        &state,
        &headers,
        "sshKey.revoke",
        json!({"id": id}),
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn list_gpg_keys(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call_map(
        &state,
        &headers,
        "gpgKey.list",
        json!({}),
        StatusCode::OK,
        |d| {
            json!(d
                .as_array()
                .map(|ks| ks.iter().map(gh_gpg_key).collect::<Vec<_>>())
                .unwrap_or_default())
        },
    )
    .await
}

async fn get_gpg_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match super::dispatch(&mut ctx, "gpgKey.list", json!({})).await {
        super::RpcResponse::Ok { data, .. } => {
            let hit = data
                .as_array()
                .and_then(|ks| ks.iter().find(|k| k["id"].as_str() == Some(id.as_str())));
            match hit {
                Some(k) => (StatusCode::OK, Json(gh_gpg_key(k))).into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
        super::RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

/// `POST /user/gpg_keys {name, armored_public_key}` — `name` → `title`.
async fn create_gpg_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    call_map(
        &state,
        &headers,
        "gpgKey.add",
        json!({"title": body["name"], "armored_public_key": body["armored_public_key"]}),
        StatusCode::CREATED,
        |d| gh_gpg_key(&d),
    )
    .await
}

async fn delete_gpg_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    call(
        &state,
        &headers,
        "gpgKey.revoke",
        json!({"id": id}),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `GET /topics` query — Gitea `q` + `limit`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct TopicsQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `GET /topics?q=` — Gitea topic search, bare array of `{name, repo_count}`.
async fn search_topics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<TopicsQuery>,
) -> Response {
    call_map(
        &state,
        &headers,
        "repo.topicsSuggest",
        json!({"q": q.q, "limit": q.limit}),
        StatusCode::OK,
        |d| d["topics"].clone(),
    )
    .await
}

fn topics_search_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "array",
        "items": {"type": "object", "properties": {
            "name": {"type": "string"},
            "repo_count": {"type": "integer"}
        }}
    })
}

/// `GET /search/repositories` query — GitHub `q`, `page`, `per_page`,
/// `sort`, `order` (sort/order accepted; explore ordering is fixed).
#[derive(Deserialize, schemars::JsonSchema)]
pub struct RepoSearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub per_page: Option<i64>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub order: Option<String>,
}

/// `GET /search/repositories` — `repo.explore` under GitHub's search envelope.
/// `total_count` is the page's item count (explore has no global count);
/// `sort`/`order` are accepted but not applied.
async fn search_repositories(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RepoSearchQuery>,
) -> Response {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(30).clamp(1, 100);
    let input = json!({
        "q": q.q,
        "offset": (page - 1) * per_page,
        "limit": per_page,
    });
    let _ = (q.sort, q.order);
    call_map(
        &state,
        &headers,
        "repo.explore",
        input,
        StatusCode::OK,
        |d| {
            let items = d["repos"].clone();
            json!({
                "total_count": items.as_array().map(|a| a.len()).unwrap_or(0),
                "incomplete_results": false,
                "items": items,
            })
        },
    )
    .await
}

fn search_repos_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["total_count", "incomplete_results", "items"],
        "properties": {
            "total_count": {"type": "integer"},
            "incomplete_results": {"type": "boolean"},
            "items": {"type": "array", "items": {"type": "object"}}
        }
    })
}

/// `PATCH /api/v1/user` (`user.update_profile`) — GitHub's update-the-
/// authenticated-user endpoint. `name` maps to `display_name`; fields absent
/// from the body keep their current values (the proc requires all fields).
async fn update_current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let me = match dispatch(&mut ctx, "auth.me", json!({})).await {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let input = json!({
        "display_name": body
            .get("name")
            .or_else(|| body.get("display_name"))
            .cloned()
            .unwrap_or_else(|| me["display_name"].clone()),
        "username": body
            .get("username")
            .cloned()
            .unwrap_or_else(|| me["username"].clone()),
        "bio": body
            .get("bio")
            .cloned()
            .unwrap_or_else(|| me["bio"].clone()),
        "default_branch": body
            .get("default_branch")
            .cloned()
            .unwrap_or_else(|| me["default_branch"].clone()),
    });
    call(
        &state,
        &headers,
        "user.update_profile",
        input,
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/notifications/new` (`notification.unreadCount`) — Gitea
/// `{new: N}` shape.
async fn unread_notification_count(State(state): State<AppState>, headers: HeaderMap) -> Response {
    call_map(
        &state,
        &headers,
        "notification.unreadCount",
        json!({}),
        StatusCode::OK,
        |d| json!({"new": d["count"]}),
    )
    .await
}

fn unread_count_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["new"],
        "properties": {"new": {"type": "integer", "description": "Unread notification count"}}
    })
}

fn user_patch_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string", "description": "GitHub display-name field"},
            "display_name": {"type": "string"},
            "username": {"type": "string"},
            "bio": {"type": "string"},
            "default_branch": {"type": "string"},
            "company": {"type": "string", "description": "Accepted for compat; not modeled"},
            "blog": {"type": "string", "description": "Accepted for compat; not modeled"},
            "location": {"type": "string", "description": "Accepted for compat; not modeled"},
            "hireable": {"type": "boolean", "description": "Accepted for compat; not modeled"}
        }
    })
}
