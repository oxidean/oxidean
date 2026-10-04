//! Generic/raw package registry (PKG-03 / D-PKG-16).
//!
//! Paths: `/generic/{owner}/{name}/{version}/{filename}` PUT/GET;
//! `DELETE /generic/{owner}/{name}/{version}`; `GET /generic/{owner}/{name}` list.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use oxidean_db::{Database, PackageRow, RepositoryRow};

use crate::app::AppState;
use crate::packages::acl::{self, PackageAction};
use crate::packages::auth::{self, RegistryIdentity};
use crate::packages::store::{self, max_blob_bytes_from_env};
use crate::repo::{coalesce, Capability, MemberBasePermission, OrgRole};

const FORMAT: &str = "generic";

#[derive(Debug, Deserialize)]
pub struct PutQuery {
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    repository_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileMeta {
    name: String,
    digest: String,
    size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct VersionMeta {
    #[serde(default)]
    files: Vec<FileMeta>,
}

fn valid_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

fn valid_filename(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 512
        && !s.contains('/')
        && !s.contains('\\')
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

async fn resolve_owner(
    db: &Database,
    login: &str,
) -> Result<Option<(String, String)>, String> {
    if let Some(u) = db.find_user_by_username(login).await? {
        return Ok(Some(("user".into(), u.id)));
    }
    if let Some(o) = db.find_organization_by_slug(login).await? {
        return Ok(Some(("org".into(), o.id)));
    }
    Ok(None)
}

async fn capability_for_owner(
    db: &Database,
    user_id: Option<&str>,
    owner_type: &str,
    owner_id: &str,
    visibility: &str,
) -> Result<Option<Capability>, String> {
    let public = acl::is_public(visibility);
    let Some(uid) = user_id else {
        return Ok(coalesce(
            false,
            None,
            MemberBasePermission::None,
            None,
            public,
        ));
    };
    if owner_type == "user" {
        return Ok(coalesce(
            owner_id == uid,
            None,
            MemberBasePermission::None,
            None,
            public,
        ));
    }
    let mut org_role = None;
    if let Some(role) = db.find_org_member_role(owner_id, uid).await? {
        org_role = match role.as_str() {
            "owner" => Some(OrgRole::Owner),
            "admin" => Some(OrgRole::Admin),
            "member" => Some(OrgRole::Member),
            _ => None,
        };
    }
    let member_base = match db
        .find_org_member_base_permission(owner_id)
        .await?
        .as_deref()
    {
        Some("read") => MemberBasePermission::Read,
        Some("write") => MemberBasePermission::Write,
        _ => MemberBasePermission::None,
    };
    Ok(coalesce(false, org_role, member_base, None, public))
}

async fn load_linked_repo(
    db: &Database,
    package: &PackageRow,
) -> Result<Option<RepositoryRow>, String> {
    match &package.repository_id {
        Some(id) => db.find_repository_by_id(id).await,
        None => Ok(None),
    }
}

async fn authorize_action(
    db: &Database,
    identity: Option<&RegistryIdentity>,
    package: &PackageRow,
    action: PackageAction,
) -> Result<bool, String> {
    let linked = load_linked_repo(db, package).await?;
    let visibility = acl::effective_visibility(package, linked.as_ref());
    let have = acl::owner_capability(
        db,
        identity.map(|i| i.user_id.as_str()),
        package,
        linked.as_ref(),
    )
    .await?;
    let pat_ok = identity.map(|i| i.allows(action)).unwrap_or(false);
    Ok(acl::authorize(&visibility, have, pat_ok, action))
}

fn parse_meta(raw: &str) -> VersionMeta {
    serde_json::from_str(raw).unwrap_or_default()
}

async fn put_file(
    State(state): State<AppState>,
    Path((owner, name, version, filename)): Path<(String, String, String, String)>,
    Query(q): Query<PutQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !valid_segment(&owner) || !valid_segment(&name) || !valid_segment(&version) || !valid_filename(&filename)
    {
        return (StatusCode::BAD_REQUEST, "invalid path segment").into_response();
    }

    let identity = match auth::authenticate_registry(&state.db, &headers).await {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(error = %e, "registry auth error");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let Some(identity) = identity else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"Oxidean Packages\"")],
        )
            .into_response();
    };
    if !identity.allows(PackageAction::Publish) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some((owner_type, owner_id)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "resolve owner");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let visibility = q
        .visibility
        .as_deref()
        .filter(|v| *v == "public" || *v == "private")
        .unwrap_or("private");

    let existing = match state
        .db
        .find_package(&owner_type, &owner_id, &name, FORMAT)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "find package");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let package = if let Some(pkg) = existing {
        let ok = match authorize_action(&state.db, Some(&identity), &pkg, PackageAction::Publish)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "authorize");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        if !ok {
            return StatusCode::FORBIDDEN.into_response();
        }
        pkg
    } else {
        let have = match capability_for_owner(
            &state.db,
            Some(&identity.user_id),
            &owner_type,
            &owner_id,
            visibility,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "capability");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        if !acl::authorize(visibility, have, true, PackageAction::Publish) {
            return StatusCode::FORBIDDEN.into_response();
        }
        let id = Uuid::new_v4().to_string();
        if let Err(e) = state
            .db
            .insert_package(
                &id,
                &owner_type,
                &owner_id,
                &name,
                FORMAT,
                visibility,
                q.repository_id.as_deref(),
                "",
            )
            .await
        {
            tracing::warn!(error = %e, "insert package");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        match state
            .db
            .find_package(&owner_type, &owner_id, &name, FORMAT)
            .await
        {
            Ok(Some(p)) => p,
            Ok(None) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            Err(e) => {
                tracing::warn!(error = %e, "reload package");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };

    if let Err(msg) = crate::packages::quota::check_can_store(
        &state.db,
        &owner_type,
        &owner_id,
        body.len() as u64,
    )
    .await
    {
        tracing::warn!(error = %msg, "quota");
        return (StatusCode::INSUFFICIENT_STORAGE, msg).into_response();
    }

    let max = max_blob_bytes_from_env();
    let (digest, size) = match store::put_blob(&state.packages_dir, &body, max) {
        Ok(v) => v,
        Err(store::StoreError::TooLarge { .. }) => {
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
        Err(e) => {
            tracing::warn!(error = %e, "put blob");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let existing_ver = match state.db.find_package_version(&package.id, &version).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "find version");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let is_new_version = existing_ver.is_none();

    if let Some(ver) = existing_ver {
        let mut meta = parse_meta(&ver.metadata_json);
        if meta.files.iter().any(|f| f.name == filename) {
            return StatusCode::CONFLICT.into_response();
        }
        if let Err(e) = state.db.upsert_package_blob(&digest, size as i64).await {
            tracing::warn!(error = %e, "upsert blob");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if let Err(e) = state.db.adjust_package_blob_refcount(&digest, 1).await {
            tracing::warn!(error = %e, "refcount");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if let Err(e) = state
            .db
            .add_package_blob_ref(&ver.id, &digest, &filename)
            .await
        {
            tracing::warn!(error = %e, "blob ref");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        meta.files.push(FileMeta {
            name: filename.clone(),
            digest: digest.clone(),
            size,
        });
        let meta_json = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".into());
        if let Err(e) = state
            .db
            .update_package_version_metadata(&ver.id, &meta_json)
            .await
        {
            tracing::warn!(error = %e, "update meta");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    } else {
        let version_id = Uuid::new_v4().to_string();
        let meta = VersionMeta {
            files: vec![FileMeta {
                name: filename.clone(),
                digest: digest.clone(),
                size,
            }],
        };
        let meta_json = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".into());
        if let Err(e) = state.db.upsert_package_blob(&digest, size as i64).await {
            tracing::warn!(error = %e, "upsert blob");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if let Err(e) = state
            .db
            .insert_package_version(
                &version_id,
                &package.id,
                &version,
                Some(&digest),
                &meta_json,
                Some(&identity.user_id),
            )
            .await
        {
            // Unique conflict → treat as immutable version race
            if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
                return StatusCode::CONFLICT.into_response();
            }
            tracing::warn!(error = %e, "insert version");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if let Err(e) = state.db.adjust_package_blob_refcount(&digest, 1).await {
            tracing::warn!(error = %e, "refcount");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if let Err(e) = state
            .db
            .add_package_blob_ref(&version_id, &digest, &filename)
            .await
        {
            tracing::warn!(error = %e, "blob ref");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    // API-04: repo-linked packages fan out `registry_package` (no-op when the
    // package is not linked to a repository).
    let action = if is_new_version { "published" } else { "updated" };
    crate::webhook::dispatch::notify_package_publish(
        &state.db,
        &package,
        &version,
        action,
        &identity.user_id,
        &state.env_name,
    )
    .await;

    (StatusCode::CREATED, Json(json!({ "ok": true, "digest": digest, "size": size }))).into_response()
}

async fn get_file(
    State(state): State<AppState>,
    Path((owner, name, version, filename)): Path<(String, String, String, String)>,
    headers: HeaderMap,
) -> Response {
    if !valid_segment(&owner) || !valid_segment(&name) || !valid_segment(&version) || !valid_filename(&filename)
    {
        return (StatusCode::BAD_REQUEST, "invalid path segment").into_response();
    }

    let identity = match auth::authenticate_registry(&state.db, &headers).await {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(error = %e, "registry auth error");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Some((owner_type, owner_id)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(package) = (match state
        .db
        .find_package(&owner_type, &owner_id, &name, FORMAT)
        .await
    {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let ok = match authorize_action(&state.db, identity.as_ref(), &package, PackageAction::Pull)
        .await
    {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        // Anti-enumeration for private: 404
        if !acl::is_public(&package.visibility) && identity.is_none() {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic realm=\"Oxidean Packages\"")],
            )
                .into_response();
        }
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(ver) = (match state.db.find_package_version(&package.id, &version).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let meta = parse_meta(&ver.metadata_json);
    let Some(file) = meta.files.iter().find(|f| f.name == filename) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match store::get_blob(&state.packages_dir, &file.digest) {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/octet-stream")],
            bytes,
        )
            .into_response(),
        Err(store::StoreError::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "get blob");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn delete_version(
    State(state): State<AppState>,
    Path((owner, name, version)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    if !valid_segment(&owner) || !valid_segment(&name) || !valid_segment(&version) {
        return (StatusCode::BAD_REQUEST, "invalid path segment").into_response();
    }

    let identity = match auth::authenticate_registry(&state.db, &headers).await {
        Ok(i) => i,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(identity) = identity else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"Oxidean Packages\"")],
        )
            .into_response();
    };
    if !identity.allows(PackageAction::Delete) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some((owner_type, owner_id)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(package) = (match state
        .db
        .find_package(&owner_type, &owner_id, &name, FORMAT)
        .await
    {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let ok = match authorize_action(&state.db, Some(&identity), &package, PackageAction::Delete)
        .await
    {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Some(ver) = (match state.db.find_package_version(&package.id, &version).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let digests = match state.db.list_package_version_blob_digests(&ver.id).await {
        Ok(d) => d,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if let Err(e) = state.db.delete_package_version(&ver.id).await {
        tracing::warn!(error = %e, "delete version");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    for d in digests {
        let _ = state.db.adjust_package_blob_refcount(&d, -1).await;
    }

    StatusCode::NO_CONTENT.into_response()
}

async fn list_package(
    State(state): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if !valid_segment(&owner) || !valid_segment(&name) {
        return (StatusCode::BAD_REQUEST, "invalid path segment").into_response();
    }

    let identity = match auth::authenticate_registry(&state.db, &headers).await {
        Ok(i) => i,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let Some((owner_type, owner_id)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Some(package) = (match state
        .db
        .find_package(&owner_type, &owner_id, &name, FORMAT)
        .await
    {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let ok = match authorize_action(&state.db, identity.as_ref(), &package, PackageAction::Pull)
        .await
    {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        if !acl::is_public(&package.visibility) && identity.is_none() {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic realm=\"Oxidean Packages\"")],
            )
                .into_response();
        }
        return StatusCode::FORBIDDEN.into_response();
    }

    let versions = match state.db.list_package_versions(&package.id).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let list: Vec<_> = versions
        .into_iter()
        .map(|v| {
            let meta = parse_meta(&v.metadata_json);
            json!({
                "version": v.version,
                "files": meta.files,
                "created_at": v.created_at,
            })
        })
        .collect();

    Json(json!({
        "owner": owner,
        "name": name,
        "format": FORMAT,
        "visibility": package.visibility,
        "repository_id": package.repository_id,
        "versions": list,
    }))
    .into_response()
}

pub fn router() -> Router<AppState> {
    let max = max_blob_bytes_from_env().min(usize::MAX as u64) as usize;
    Router::new()
        .route("/{owner}/{name}", get(list_package))
        .route("/{owner}/{name}/{version}", delete(delete_version))
        .route(
            "/{owner}/{name}/{version}/{filename}",
            put(put_file).get(get_file),
        )
        .layer(DefaultBodyLimit::max(max))
}
