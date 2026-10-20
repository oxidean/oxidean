//! `/api/v1/orgs/**` — organizations.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::Json;
use oxidean_core::{
    CreateOrgRequest, CreateRepoRequest, OrgPublic, RepoListMineResponse, RepoPublic, RpcResponse,
};
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, call_map, ctx_for, dispatch, merge_fields, status_for_error};
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
        method: "PATCH",
        path: "/orgs/{slug}",
        tags: &["orgs"],
        operation_id: "updateOrg",
        summary: "Update org settings (`org.updateSettings`) — `name`/`display_name`/`full_name`, `default_repository_permission`, `two_factor_requirement_enabled`",
        procedure: "org.updateSettings",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: Some(BodySpec::Json(org_patch_body_schema)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<OrgPublic>,
        )),
        mount: || patch(update_org),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{slug}/members",
        tags: &["orgs"],
        operation_id: "listOrgMembers",
        summary: "Org members (`org.members.list`) — GitHub bare-array user projection",
        procedure: "org.members.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(members_schema)),
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
    RouteDef {
        method: "GET",
        path: "/orgs/{slug}/memberships/{username}",
        tags: &["orgs"],
        operation_id: "orgs/getMembership",
        summary:
            "Caller-visible membership for a user (`org.members.list`) — `{state, role, user}`",
        procedure: "org.members.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug", "username"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(membership_schema)),
        mount: || get(get_membership),
    },
    RouteDef {
        method: "PUT",
        path: "/orgs/{slug}/memberships/{username}",
        tags: &["orgs"],
        operation_id: "orgs/setMembership",
        summary: "Add or update a membership (`org.members.add` / `.updateRole`) — `{role}`",
        procedure: "org.members.updateRole",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug", "username"],
        query: None,
        body: Some(BodySpec::Json(membership_body_schema)),
        response: Some(RespSpec::Json(membership_schema)),
        mount: || put(set_membership),
    },
    RouteDef {
        method: "DELETE",
        path: "/orgs/{slug}/memberships/{username}",
        tags: &["orgs"],
        operation_id: "orgs/removeMembership",
        summary: "Remove a member (`org.members.remove`) → 204",
        procedure: "org.members.remove",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "username"],
        query: None,
        body: None,
        response: None,
        mount: || delete(remove_membership),
    },
    RouteDef {
        method: "DELETE",
        path: "/orgs/{slug}/members/{username}",
        tags: &["orgs"],
        operation_id: "orgs/removeMember",
        summary: "Remove a member (`org.members.remove`) → 204 — GitHub/Gitea path alias",
        procedure: "org.members.remove",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "username"],
        query: None,
        body: None,
        response: None,
        mount: || delete(remove_membership),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{slug}/invitations",
        tags: &["orgs"],
        operation_id: "orgs/listInvitations",
        summary: "Pending org invitations (`org.invites.list`) — bare array",
        procedure: "org.invites.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(invites_schema)),
        mount: || get(list_org_invites),
    },
    RouteDef {
        method: "POST",
        path: "/orgs/{slug}/invitations",
        tags: &["orgs"],
        operation_id: "orgs/createInvitation",
        summary: "Invite by email (`org.invites.create`) — `{email}` or `{emails:[]}` + `role`",
        procedure: "org.invites.create",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: Some(BodySpec::Json(invite_body_schema)),
        response: Some(RespSpec::Json(invites_schema)),
        mount: || post(create_org_invite),
    },
    RouteDef {
        method: "DELETE",
        path: "/orgs/{slug}/invitations/{invite_id}",
        tags: &["orgs"],
        operation_id: "orgs/cancelInvitation",
        summary: "Revoke a pending invitation (`org.invites.revoke`) → 204",
        procedure: "org.invites.revoke",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "invite_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(revoke_org_invite),
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

/// `GET /api/v1/orgs/{slug}/members` (`org.members.list`) — GitHub bare-array
/// user projection (`role` is an extension field; GitHub omits it here).
async fn list_org_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "org.members.list",
        json!({ "slug": p.slug }),
        StatusCode::OK,
        |d| {
            json!(d["members"]
                .as_array()
                .map(|ms| ms.iter().map(gh_org_member).collect::<Vec<_>>())
                .unwrap_or_default())
        },
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

#[derive(Deserialize)]
struct OrgUserPath {
    slug: String,
    username: String,
}

#[derive(Deserialize)]
struct OrgInvitePath {
    slug: String,
    invite_id: String,
}

/// GitHub user projection for an `OrgMemberPublic` row.
fn gh_org_member(m: &Value) -> Value {
    json!({
        "login": m["username"],
        "id": m["user_id"],
        "type": "User",
        "role": m["role"],
    })
}

/// `GET /orgs/{slug}/memberships/{username}` → GitHub `{state, role, user}`.
async fn get_membership(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgUserPath>,
) -> Response {
    match member_row(&state, &headers, &p.slug, &p.username).await {
        Ok(Some(m)) => (
            StatusCode::OK,
            Json(json!({
                "state": "active",
                "role": m["role"],
                "user": {"login": m["username"], "id": m["user_id"], "type": "User"},
            })),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "membership.not_found", "message": "user is not a member"})),
        )
            .into_response(),
        Err(r) => *r,
    }
}

/// Shared `org.members.list` → member row for `username` (or `None`).
async fn member_row(
    state: &AppState,
    headers: &HeaderMap,
    slug: &str,
    username: &str,
) -> Result<Option<Value>, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    let data = match dispatch(&mut ctx, "org.members.list", json!({"slug": slug})).await {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return Err(Box::new(
                (status_for_error(&error), Json(error)).into_response(),
            ));
        }
    };
    Ok(data["members"].as_array().and_then(|ms| {
        ms.iter()
            .find(|m| m["username"].as_str() == Some(username))
            .cloned()
    }))
}

/// `PUT /orgs/{slug}/memberships/{username}` `{role}` — add when absent,
/// `updateRole` when present.
async fn set_membership(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgUserPath>,
    Json(body): Json<Value>,
) -> Response {
    let role = body["role"].as_str().unwrap_or("member").to_string();
    let existing = match member_row(&state, &headers, &p.slug, &p.username).await {
        Ok(m) => m,
        Err(r) => return *r,
    };
    let input = match &existing {
        Some(m) => json!({
            "slug": p.slug,
            "user_id": m["user_id"],
            "role": role,
        }),
        None => json!({
            "slug": p.slug,
            "username": p.username,
            "role": role,
        }),
    };
    let proc = if existing.is_some() {
        "org.members.updateRole"
    } else {
        "org.members.add"
    };
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(&mut ctx, proc, input).await {
        RpcResponse::Ok { data, .. } => (
            StatusCode::OK,
            Json(json!({
                "state": "active",
                "role": data["role"],
                "user": {"login": data["username"], "id": data["user_id"], "type": "User"},
            })),
        )
            .into_response(),
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

/// `DELETE /orgs/{slug}/memberships/{username}` and `…/members/{username}` → 204.
async fn remove_membership(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgUserPath>,
) -> Response {
    let existing = match member_row(&state, &headers, &p.slug, &p.username).await {
        Ok(m) => m,
        Err(r) => return *r,
    };
    match existing {
        Some(m) => {
            call(
                &state,
                &headers,
                "org.members.remove",
                json!({"slug": p.slug, "user_id": m["user_id"]}),
                StatusCode::NO_CONTENT,
            )
            .await
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "membership.not_found", "message": "user is not a member"})),
        )
            .into_response(),
    }
}

/// `GET /orgs/{slug}/invitations` — bare array.
async fn list_org_invites(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "org.invites.list",
        json!({"slug": p.slug}),
        StatusCode::OK,
        |d| d["invites"].clone(),
    )
    .await
}

/// `POST /orgs/{slug}/invitations` — `{email}` or `{emails:[]}` + `role`
/// (GitHub `role`: `admin`|`member`; `owner` also accepted). Response is the
/// per-recipient results array.
async fn create_org_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgPath>,
    Json(body): Json<Value>,
) -> Response {
    let emails = match body.get("emails").and_then(Value::as_array) {
        Some(e) => e.clone(),
        None => match body.get("email").and_then(Value::as_str) {
            Some(e) => vec![json!(e)],
            None => vec![],
        },
    };
    let role = body["role"].as_str().unwrap_or("member");
    call_map(
        &state,
        &headers,
        "org.invites.create",
        json!({"slug": p.slug, "emails": emails, "role": role}),
        StatusCode::OK,
        |d| d["results"].clone(),
    )
    .await
}

/// `DELETE /orgs/{slug}/invitations/{invite_id}` → 204.
async fn revoke_org_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgInvitePath>,
) -> Response {
    call(
        &state,
        &headers,
        "org.invites.revoke",
        json!({"slug": p.slug, "invite_id": p.invite_id}),
        StatusCode::NO_CONTENT,
    )
    .await
}

fn members_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "array",
        "items": {"type": "object", "properties": {
            "login": {"type": "string"},
            "id": {"type": "string"},
            "type": {"type": "string"},
            "role": {"type": "string"}
        }}
    })
}

fn membership_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "state": {"type": "string"},
            "role": {"type": "string"},
            "user": {"type": "object"}
        }
    })
}

fn membership_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {"role": {"type": "string", "enum": ["owner", "admin", "member"]}}
    })
}

fn invites_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "array",
        "items": {"type": "object", "properties": {
            "id": {"type": "string"},
            "email": {"type": ["string", "null"]},
            "role": {"type": "string"},
            "expires_at": {"type": ["string", "null"]},
            "invited_by": {"type": "string"},
            "created_at": {"type": "string"}
        }}
    })
}

fn invite_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "email": {"type": "string"},
            "emails": {"type": "array", "items": {"type": "string"}},
            "role": {"type": "string", "enum": ["owner", "admin", "member"]}
        }
    })
}

/// `PATCH /orgs/{slug}` — GitHub/Gitea update aliases onto
/// `org.updateSettings` (`name`/`display_name`/`full_name` → `display_name`;
/// `default_repository_permission` → `member_base_permission`;
/// `two_factor_requirement_enabled`/`require_two_factor` → `require_two_factor`).
async fn update_org(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OrgPath>,
    Json(body): Json<Value>,
) -> Response {
    let mut input = json!({"slug": p.slug});
    let display_name = body
        .get("display_name")
        .or_else(|| body.get("name"))
        .or_else(|| body.get("full_name"));
    if let Some(d) = display_name {
        input["display_name"] = d.clone();
    }
    if let Some(perm) = body
        .get("member_base_permission")
        .or_else(|| body.get("default_repository_permission"))
    {
        input["member_base_permission"] = perm.clone();
    }
    if let Some(tfa) = body
        .get("require_two_factor")
        .or_else(|| body.get("two_factor_requirement_enabled"))
    {
        input["require_two_factor"] = tfa.clone();
    }
    call(
        &state,
        &headers,
        "org.updateSettings",
        input,
        StatusCode::OK,
    )
    .await
}

fn org_patch_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string", "description": "GitHub alias for display_name"},
            "display_name": {"type": "string"},
            "full_name": {"type": "string", "description": "Gitea alias for display_name"},
            "description": {"type": "string", "description": "Accepted for compat; not modeled"},
            "default_repository_permission": {"type": "string", "enum": ["none", "read", "write"]},
            "two_factor_requirement_enabled": {"type": "boolean"}
        }
    })
}
