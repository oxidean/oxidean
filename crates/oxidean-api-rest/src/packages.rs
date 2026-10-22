//! `/api/v1/…/packages/**` — GitHub-shaped Packages REST over `packages.*` RPCs.
//!
//! GitHub keys packages by `{package_type, package_name}` under three owner
//! scopes (`/user`, `/users/{username}`, `/orgs/{org}`); our registry stores a
//! `format` column instead — `container` maps to `oci` (and vice versa), the
//! other names pass through, and `generic` is an Oxidean extension GitHub
//! lacks.
//!
//! Auth: anonymous reads follow the Pull ACL (public packages, or the linked
//! repo's Read capability); mutations ride `packages.delete` /
//! `packages.deleteVersion`, which require the package owner's Admin
//! capability — org-package deletes are org-owner only, matching GitHub.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use axum::Json;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{RespSpec, RouteDef};
use super::{ctx_for, dispatch, status_for_error};
use oxidean_api_core::app::AppState;
use oxidean_core::RpcResponse;

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/user/packages",
        tags: &["packages"],
        operation_id: "packages/listPackagesForAuthenticatedUser",
        summary: "List packages for the authenticated user (packages.list)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &[],
        query: Some(SchemaGenerator::into_root_schema_for::<PackagesQuery>),
        body: None,
        response: Some(RespSpec::Json(packages_list_schema)),
        mount: || get(list_user_packages),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}/packages",
        tags: &["packages"],
        operation_id: "packages/listPackagesForUser",
        summary: "List a user's packages (packages.list)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["username"],
        query: Some(SchemaGenerator::into_root_schema_for::<PackagesQuery>),
        body: None,
        response: Some(RespSpec::Json(packages_list_schema)),
        mount: || get(list_username_packages),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/packages",
        tags: &["packages"],
        operation_id: "packages/listPackagesForOrg",
        summary: "List an org's packages (packages.list)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["org"],
        query: Some(SchemaGenerator::into_root_schema_for::<PackagesQuery>),
        body: None,
        response: Some(RespSpec::Json(packages_list_schema)),
        mount: || get(list_org_packages),
    },
    RouteDef {
        method: "GET",
        path: "/user/packages/{package_type}/{package_name}",
        tags: &["packages"],
        operation_id: "packages/getPackageForAuthenticatedUser",
        summary: "Get one of the caller's packages (packages.list, filtered)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["package_type", "package_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(package_schema)),
        mount: || get(get_user_package),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}/packages/{package_type}/{package_name}",
        tags: &["packages"],
        operation_id: "packages/getPackageForUser",
        summary: "Get one user package (packages.list, filtered)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["username", "package_type", "package_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(package_schema)),
        mount: || get(get_username_package),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/packages/{package_type}/{package_name}",
        tags: &["packages"],
        operation_id: "packages/getPackageForOrg",
        summary: "Get one org package (packages.list, filtered)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["org", "package_type", "package_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(package_schema)),
        mount: || get(get_org_package),
    },
    RouteDef {
        method: "GET",
        path: "/user/packages/{package_type}/{package_name}/versions",
        tags: &["packages"],
        operation_id: "packages/getAllVersionsForAuthenticatedUser",
        summary: "List versions of the caller's package (packages.list, filtered)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["package_type", "package_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(versions_list_schema)),
        mount: || get(list_user_package_versions),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}/packages/{package_type}/{package_name}/versions",
        tags: &["packages"],
        operation_id: "packages/getAllVersionsForUser",
        summary: "List versions of a user's package (packages.list, filtered)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["username", "package_type", "package_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(versions_list_schema)),
        mount: || get(list_username_package_versions),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/packages/{package_type}/{package_name}/versions",
        tags: &["packages"],
        operation_id: "packages/getAllVersionsForOrg",
        summary: "List versions of an org's package (packages.list, filtered)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["org", "package_type", "package_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(versions_list_schema)),
        mount: || get(list_org_package_versions),
    },
    RouteDef {
        method: "GET",
        path: "/user/packages/{package_type}/{package_name}/versions/{version_id}",
        tags: &["packages"],
        operation_id: "packages/getVersionForAuthenticatedUser",
        summary: "Get one version of the caller's package (packages.list, filtered)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["package_type", "package_name", "version_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(version_schema)),
        mount: || get(get_user_package_version),
    },
    RouteDef {
        method: "GET",
        path: "/users/{username}/packages/{package_type}/{package_name}/versions/{version_id}",
        tags: &["packages"],
        operation_id: "packages/getVersionForUser",
        summary: "Get one version of a user's package (packages.list, filtered)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["username", "package_type", "package_name", "version_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(version_schema)),
        mount: || get(get_username_package_version),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/packages/{package_type}/{package_name}/versions/{version_id}",
        tags: &["packages"],
        operation_id: "packages/getVersionForOrg",
        summary: "Get one version of an org's package (packages.list, filtered)",
        procedure: "packages.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["org", "package_type", "package_name", "version_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(version_schema)),
        mount: || get(get_org_package_version),
    },
    RouteDef {
        method: "DELETE",
        path: "/user/packages/{package_type}/{package_name}",
        tags: &["packages"],
        operation_id: "packages/deletePackageForAuthenticatedUser",
        summary: "Delete one of the caller's packages (packages.delete)",
        procedure: "packages.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["package_type", "package_name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_user_package),
    },
    RouteDef {
        method: "DELETE",
        path: "/orgs/{org}/packages/{package_type}/{package_name}",
        tags: &["packages"],
        operation_id: "packages/deletePackageForOrg",
        summary: "Delete an org package (packages.delete)",
        procedure: "packages.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["org", "package_type", "package_name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_org_package),
    },
    RouteDef {
        method: "DELETE",
        path: "/user/packages/{package_type}/{package_name}/versions/{version_id}",
        tags: &["packages"],
        operation_id: "packages/deleteVersionForAuthenticatedUser",
        summary: "Delete one version of the caller's package (packages.deleteVersion)",
        procedure: "packages.deleteVersion",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["package_type", "package_name", "version_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_user_package_version),
    },
    RouteDef {
        method: "DELETE",
        path: "/orgs/{org}/packages/{package_type}/{package_name}/versions/{version_id}",
        tags: &["packages"],
        operation_id: "packages/deleteVersionForOrg",
        summary: "Delete one version of an org's package (packages.deleteVersion)",
        procedure: "packages.deleteVersion",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["org", "package_type", "package_name", "version_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_org_package_version),
    },
];

// ---- extractors ---------------------------------------------------------

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PackagesQuery {
    /// GitHub's `package_type` — `container` maps to our `oci` format.
    #[serde(default)]
    package_type: Option<String>,
    /// `public` | `private` | `internal` (`internal` ≈ private here).
    #[serde(default)]
    visibility: Option<String>,
}

#[derive(Deserialize)]
pub struct OwnerPath {
    username: Option<String>,
    org: Option<String>,
}

#[derive(Deserialize)]
pub struct PkgPath {
    username: Option<String>,
    org: Option<String>,
    package_type: String,
    package_name: String,
}

#[derive(Deserialize)]
pub struct PkgVersionPath {
    username: Option<String>,
    org: Option<String>,
    package_type: String,
    package_name: String,
    version_id: String,
}

// ---- shared resolution --------------------------------------------------

/// GitHub `package_type` → stored `format` (`container`/`docker` → `oci`).
fn format_for(package_type: &str) -> &str {
    match package_type {
        "container" | "docker" => "oci",
        other => other,
    }
}

/// Stored `format` → GitHub `package_type`.
fn package_type_for(format: &str) -> &str {
    match format {
        "oci" => "container",
        other => other,
    }
}

/// Run `packages.list` for an owner login and pick the (type, name) row —
/// the proc already applies the Pull ACL, so filtered reads inherit it.
async fn find_pkg(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    package_type: &str,
    package_name: &str,
) -> Result<Value, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    let resp = dispatch(&mut ctx, "packages.list", json!({ "owner": owner })).await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return Err(Box::new(
                (status_for_error(&error), Json(error)).into_response(),
            ))
        }
    };
    let fmt = format_for(package_type);
    data["packages"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|p| p["name"] == package_name && p["format"] == fmt)
                .cloned()
        })
        .ok_or_else(|| {
            Box::new(
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "packages.not_found", "message": "package not found"})),
                )
                    .into_response(),
            )
        })
}

/// `/user/…` scope — the session username. Unauthenticated → 401.
async fn user_login(state: &AppState, headers: &HeaderMap) -> Result<String, Box<Response>> {
    let ctx = ctx_for(state, headers).await.map_err(Box::new)?;
    let Some(session) = &ctx.session else {
        return Err(Box::new(
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"code": "auth.unauthenticated", "message": "authentication required"})),
            )
                .into_response(),
        ));
    };
    match ctx.db.find_user_by_id(&session.user_id).await {
        Ok(Some(u)) => Ok(u.username),
        _ => Err(Box::new(
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"code": "auth.unauthenticated", "message": "authentication required"})),
            )
                .into_response(),
        )),
    }
}

/// Owner login for a route scope: `/user` → session user; `/users/{u}` and
/// `/orgs/{org}` → the path owner verbatim.
async fn scope_owner(
    state: &AppState,
    headers: &HeaderMap,
    username: Option<&str>,
    org: Option<&str>,
) -> Result<String, Box<Response>> {
    if let Some(o) = org.or(username) {
        return Ok(o.to_string());
    }
    user_login(state, headers).await
}

// ---- GitHub reshaping ---------------------------------------------------

/// `PackagePublic` → GitHub's package object (`id` stays a string — GitHub's
/// is numeric, but `gh` treats it opaquely).
fn gh_package(origin: &str, p: &Value) -> Value {
    let pt = package_type_for(p["format"].as_str().unwrap_or("generic"));
    let owner = owner_login_guess(p);
    let name = p["name"].as_str().unwrap_or_default();
    let version_count = p["versions"].as_array().map(|v| v.len()).unwrap_or(0);
    json!({
        "id": p["id"],
        "name": name,
        "package_type": pt,
        "version_count": version_count,
        "visibility": p["visibility"],
        "url": format!("{origin}/api/v1/user/packages/{pt}/{name}"),
        "html_url": format!("{origin}/{owner}/-/packages/{pt}/{name}"),
        "created_at": p["created_at"],
        "updated_at": p["updated_at"],
        "description": match p["description"].as_str() {
            Some("") | None => Value::Null,
            v => json!(v),
        },
        "owner": {
            "login": owner,
            "type": if p["owner_type"] == "org" { "Organization" } else { "User" },
        },
    })
}

/// `PackageVersionPublic` → GitHub's version object.
fn gh_version(origin: &str, p: &Value, v: &Value) -> Value {
    let pt = package_type_for(p["format"].as_str().unwrap_or("generic"));
    let name = p["name"].as_str().unwrap_or_default();
    json!({
        "id": v["id"],
        "name": v["version"],
        "url": format!("{origin}/api/v1/user/packages/{pt}/{name}/versions/{}", v["id"]),
        "package_html_url": format!("{origin}/{}/-/packages/{pt}/{name}", owner_login_guess(p)),
        "created_at": v["created_at"],
        "updated_at": v["created_at"],
        "html_url": Value::Null,
        "metadata": {"package_type": pt, "container": Value::Null},
    })
}

/// The public row doesn't echo the owner login — the URL path owner is the
/// closest honest value, so `gh_package`/`gh_version` callers re-anchor.
fn owner_login_guess(p: &Value) -> &str {
    p["owner_login"].as_str().unwrap_or("")
}

fn packages_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn package_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn versions_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn version_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

/// `GET` list shared by the three owner scopes: `packages.list` → filter →
/// GitHub package objects.
async fn list_packages(
    state: &AppState,
    headers: &HeaderMap,
    username: Option<&str>,
    org: Option<&str>,
    q: &PackagesQuery,
) -> Response {
    let owner = match scope_owner(state, headers, username, org).await {
        Ok(o) => o,
        Err(r) => return *r,
    };
    let mut ctx = match ctx_for(state, headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let resp = dispatch(&mut ctx, "packages.list", json!({ "owner": owner })).await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response()
        }
    };
    let origin = oxidean_api_core::public_origin::resolve_public_origin();
    let type_filter = q.package_type.as_deref().map(format_for);
    let vis_filter = q.visibility.as_deref().map(|v| {
        // GitHub `internal` ≈ our `private` (org-only visibility class).
        match v {
            "internal" => "private",
            other => other,
        }
    });
    let out: Vec<Value> = data["packages"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|p| {
                    type_filter.is_none_or(|t| p["format"] == t)
                        && vis_filter.is_none_or(|v| p["visibility"] == v)
                })
                .map(|p| {
                    let mut p = p.clone();
                    p["owner_login"] = json!(owner);
                    gh_package(&origin, &p)
                })
                .collect()
        })
        .unwrap_or_default();
    (StatusCode::OK, Json(json!(out))).into_response()
}

async fn list_user_packages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PackagesQuery>,
) -> Response {
    list_packages(&state, &headers, None, None, &q).await
}

async fn list_username_packages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OwnerPath>,
    Query(q): Query<PackagesQuery>,
) -> Response {
    list_packages(&state, &headers, p.username.as_deref(), None, &q).await
}

async fn list_org_packages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<OwnerPath>,
    Query(q): Query<PackagesQuery>,
) -> Response {
    list_packages(&state, &headers, None, p.org.as_deref(), &q).await
}

// ---- single-package get --------------------------------------------------

async fn get_package(
    state: &AppState,
    headers: &HeaderMap,
    username: Option<&str>,
    org: Option<&str>,
    package_type: &str,
    package_name: &str,
) -> Response {
    let owner = match scope_owner(state, headers, username, org).await {
        Ok(o) => o,
        Err(r) => return *r,
    };
    match find_pkg(state, headers, &owner, package_type, package_name).await {
        Ok(mut p) => {
            p["owner_login"] = json!(owner);
            (
                StatusCode::OK,
                Json(gh_package(
                    &oxidean_api_core::public_origin::resolve_public_origin(),
                    &p,
                )),
            )
                .into_response()
        }
        Err(r) => *r,
    }
}

async fn get_user_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgPath>,
) -> Response {
    get_package(
        &state,
        &headers,
        None,
        None,
        &p.package_type,
        &p.package_name,
    )
    .await
}

async fn get_username_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgPath>,
) -> Response {
    get_package(
        &state,
        &headers,
        p.username.as_deref(),
        None,
        &p.package_type,
        &p.package_name,
    )
    .await
}

async fn get_org_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgPath>,
) -> Response {
    get_package(
        &state,
        &headers,
        None,
        p.org.as_deref(),
        &p.package_type,
        &p.package_name,
    )
    .await
}

// ---- versions ------------------------------------------------------------

async fn list_versions(
    state: &AppState,
    headers: &HeaderMap,
    username: Option<&str>,
    org: Option<&str>,
    package_type: &str,
    package_name: &str,
) -> Response {
    let owner = match scope_owner(state, headers, username, org).await {
        Ok(o) => o,
        Err(r) => return *r,
    };
    match find_pkg(state, headers, &owner, package_type, package_name).await {
        Ok(mut p) => {
            p["owner_login"] = json!(owner);
            let origin = oxidean_api_core::public_origin::resolve_public_origin();
            let out: Vec<Value> = p["versions"]
                .as_array()
                .map(|vs| vs.iter().map(|v| gh_version(&origin, &p, v)).collect())
                .unwrap_or_default();
            (StatusCode::OK, Json(json!(out))).into_response()
        }
        Err(r) => *r,
    }
}

async fn list_user_package_versions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgPath>,
) -> Response {
    list_versions(
        &state,
        &headers,
        None,
        None,
        &p.package_type,
        &p.package_name,
    )
    .await
}

async fn list_username_package_versions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgPath>,
) -> Response {
    list_versions(
        &state,
        &headers,
        p.username.as_deref(),
        None,
        &p.package_type,
        &p.package_name,
    )
    .await
}

async fn list_org_package_versions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgPath>,
) -> Response {
    list_versions(
        &state,
        &headers,
        None,
        p.org.as_deref(),
        &p.package_type,
        &p.package_name,
    )
    .await
}

/// Find one version by row id (GitHub's `{version_id}`) or — as a fallback
/// for human URLs — by version string.
fn find_version<'v>(pkg: &'v Value, version_id: &str) -> Option<&'v Value> {
    pkg["versions"].as_array().and_then(|vs| {
        vs.iter()
            .find(|v| v["id"] == version_id || v["version"] == version_id)
    })
}

async fn get_version(
    state: &AppState,
    headers: &HeaderMap,
    username: Option<&str>,
    org: Option<&str>,
    package_type: &str,
    package_name: &str,
    version_id: &str,
) -> Response {
    let owner = match scope_owner(state, headers, username, org).await {
        Ok(o) => o,
        Err(r) => return *r,
    };
    match find_pkg(state, headers, &owner, package_type, package_name).await {
        Ok(mut p) => {
            p["owner_login"] = json!(owner);
            match find_version(&p, version_id) {
                Some(v) => (
                    StatusCode::OK,
                    Json(gh_version(
                        &oxidean_api_core::public_origin::resolve_public_origin(),
                        &p,
                        v,
                    )),
                )
                    .into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "packages.not_found", "message": "version not found"})),
                )
                    .into_response(),
            }
        }
        Err(r) => *r,
    }
}

async fn get_user_package_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgVersionPath>,
) -> Response {
    get_version(
        &state,
        &headers,
        None,
        None,
        &p.package_type,
        &p.package_name,
        &p.version_id,
    )
    .await
}

async fn get_username_package_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgVersionPath>,
) -> Response {
    get_version(
        &state,
        &headers,
        p.username.as_deref(),
        None,
        &p.package_type,
        &p.package_name,
        &p.version_id,
    )
    .await
}

async fn get_org_package_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgVersionPath>,
) -> Response {
    get_version(
        &state,
        &headers,
        None,
        p.org.as_deref(),
        &p.package_type,
        &p.package_name,
        &p.version_id,
    )
    .await
}

// ---- deletes -------------------------------------------------------------

/// Shared delete — resolve the package, then dispatch the delete proc with
/// `confirm` auto-filled (GitHub's REST contract carries no confirmation).
async fn delete_pkg(
    state: &AppState,
    headers: &HeaderMap,
    username: Option<&str>,
    org: Option<&str>,
    package_type: &str,
    package_name: &str,
    version_id: Option<&str>,
) -> Response {
    let owner = match scope_owner(state, headers, username, org).await {
        Ok(o) => o,
        Err(r) => return *r,
    };
    let pkg = match find_pkg(state, headers, &owner, package_type, package_name).await {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let package_id = pkg["id"].as_str().unwrap_or_default();
    let (procedure, input) = match version_id {
        None => (
            "packages.delete",
            json!({"package_id": package_id, "confirm": package_name}),
        ),
        Some(vid) => match find_version(&pkg, vid) {
            Some(v) => (
                "packages.deleteVersion",
                json!({
                    "package_id": package_id,
                    "version": v["version"],
                    "confirm": format!("{package_name}@{}", v["version"].as_str().unwrap_or_default()),
                }),
            ),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "packages.not_found", "message": "version not found"})),
                )
                    .into_response()
            }
        },
    };
    let mut ctx = match ctx_for(state, headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(&mut ctx, procedure, input).await {
        RpcResponse::Ok { .. } => StatusCode::NO_CONTENT.into_response(),
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

async fn delete_user_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgPath>,
) -> Response {
    delete_pkg(
        &state,
        &headers,
        None,
        None,
        &p.package_type,
        &p.package_name,
        None,
    )
    .await
}

async fn delete_org_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgPath>,
) -> Response {
    delete_pkg(
        &state,
        &headers,
        None,
        p.org.as_deref(),
        &p.package_type,
        &p.package_name,
        None,
    )
    .await
}

async fn delete_user_package_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgVersionPath>,
) -> Response {
    delete_pkg(
        &state,
        &headers,
        None,
        None,
        &p.package_type,
        &p.package_name,
        Some(&p.version_id),
    )
    .await
}

async fn delete_org_package_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<PkgVersionPath>,
) -> Response {
    delete_pkg(
        &state,
        &headers,
        None,
        p.org.as_deref(),
        &p.package_type,
        &p.package_name,
        Some(&p.version_id),
    )
    .await
}
