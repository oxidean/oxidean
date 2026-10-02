//! OCI Distribution Spec v1.1.1 (PKG-01 / D-PKG-15) — end-1…10; referrers deferred.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use oxidean_db::{Database, PackageRow};

use crate::app::AppState;
use crate::packages::acl::{self, PackageAction};
use crate::packages::auth::{self, RegistryIdentity};
use crate::packages::store::{self, digest_of_bytes, max_blob_bytes_from_env, parse_sha256_digest};
use crate::repo::{coalesce, Capability, MemberBasePermission, OrgRole};

const FORMAT: &str = "oci";

#[derive(Clone)]
struct UploadSession {
    /// Set at session start for diagnostics / future TTL sweeps.
    #[allow(dead_code)]
    owner_login: String,
    #[allow(dead_code)]
    image: String,
    bytes: Vec<u8>,
    #[allow(dead_code)]
    created: Instant,
}

#[derive(Clone)]
struct BearerToken {
    /// Mirrored from identity for token introspection / future scope checks.
    #[allow(dead_code)]
    user_id: String,
    #[allow(dead_code)]
    scopes: Vec<String>,
    expires: Instant,
    identity: RegistryIdentity,
}

fn uploads() -> &'static Mutex<HashMap<String, UploadSession>> {
    static U: OnceLock<Mutex<HashMap<String, UploadSession>>> = OnceLock::new();
    U.get_or_init(|| Mutex::new(HashMap::new()))
}

fn tokens() -> &'static Mutex<HashMap<String, BearerToken>> {
    static T: OnceLock<Mutex<HashMap<String, BearerToken>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// GET /v2 or /v2/ — registry API version discovery.
pub async fn discovery() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(
            header::HeaderName::from_static("docker-distribution-api-version"),
            HeaderValue::from_static("registry/2.0"),
        )],
    )
}

fn www_auth(scope: &str) -> HeaderValue {
    let realm = format!(
        "{}/v2/token",
        crate::auth::verify_reset::public_origin().trim_end_matches('/')
    );
    HeaderValue::from_str(&format!(
        "Bearer realm=\"{realm}\",service=\"oxidean\",scope=\"{scope}\""
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("Bearer realm=\"/v2/token\",service=\"oxidean\""))
}

fn unauthorized(scope: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, www_auth(scope))],
    )
        .into_response()
}

fn split_name(name: &str) -> Option<(String, String)> {
    let (owner, rest) = name.split_once('/')?;
    if owner.is_empty() || rest.is_empty() {
        return None;
    }
    Some((owner.to_string(), rest.to_string()))
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
        return Ok(coalesce(false, None, MemberBasePermission::None, None, public));
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

async fn auth_identity(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<RegistryIdentity>, String> {
    if let Some(bearer) = auth::decode_bearer(headers) {
        if let Some(tok) = tokens().lock().ok().and_then(|g| g.get(&bearer).cloned()) {
            if tok.expires > Instant::now() {
                return Ok(Some(tok.identity));
            }
        }
    }
    auth::authenticate_registry(&state.db, headers).await
}

async fn authorize_pkg(
    db: &Database,
    identity: Option<&RegistryIdentity>,
    package: &PackageRow,
    action: PackageAction,
) -> Result<bool, String> {
    let linked = match &package.repository_id {
        Some(id) => db.find_repository_by_id(id).await?,
        None => None,
    };
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

async fn ensure_package(
    db: &Database,
    identity: &RegistryIdentity,
    owner_login: &str,
    image: &str,
    visibility: &str,
) -> Result<PackageRow, StatusCode> {
    let (owner_type, owner_id) = resolve_owner(db, owner_login)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if let Some(pkg) = db
        .find_package(&owner_type, &owner_id, image, FORMAT)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        let ok = authorize_pkg(db, Some(identity), &pkg, PackageAction::Publish)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if !ok {
            return Err(StatusCode::FORBIDDEN);
        }
        return Ok(pkg);
    }
    let have = capability_for_owner(
        db,
        Some(&identity.user_id),
        &owner_type,
        &owner_id,
        visibility,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !acl::authorize(visibility, have, identity.allows(PackageAction::Publish), PackageAction::Publish)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let id = Uuid::new_v4().to_string();
    db.insert_package(
        &id,
        &owner_type,
        &owner_id,
        image,
        FORMAT,
        visibility,
        None,
        "",
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    db.find_package(&owner_type, &owner_id, image, FORMAT)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)
}

async fn find_pkg(
    db: &Database,
    owner_login: &str,
    image: &str,
) -> Result<Option<PackageRow>, StatusCode> {
    let Some((owner_type, owner_id)) = resolve_owner(db, owner_login)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Ok(None);
    };
    db.find_package(&owner_type, &owner_id, image, FORMAT)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    scope: Option<String>,
    service: Option<String>,
    account: Option<String>,
}

/// Docker token realm — Basic PAT → short-lived Bearer.
async fn token_endpoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Response {
    let identity = match auth::authenticate_registry(&state.db, &headers).await {
        Ok(Some(i)) => i,
        Ok(None) => return unauthorized(q.scope.as_deref().unwrap_or("repository:*:*")),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let token = format!("oxidean_oci_{}", Uuid::new_v4());
    let scopes = q
        .scope
        .map(|s| vec![s])
        .unwrap_or_else(|| vec!["repository:*:*".into()]);
    if let Ok(mut g) = tokens().lock() {
        g.insert(
            token.clone(),
            BearerToken {
                user_id: identity.user_id.clone(),
                scopes: scopes.clone(),
                expires: Instant::now() + Duration::from_secs(300),
                identity,
            },
        );
    }
    Json(json!({
        "token": token,
        "access_token": token,
        "expires_in": 300,
        "issued_at": chrono::Utc::now().to_rfc3339(),
        "service": q.service.unwrap_or_else(|| "oxidean".into()),
        "account": q.account,
    }))
    .into_response()
}

async fn start_upload(
    State(state): State<AppState>,
    Path((owner, image)): Path<(String, String)>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let name = format!("{owner}/{image}");
    let scope = format!("repository:{name}:push");
    let identity = match auth_identity(&state, &headers).await {
        Ok(Some(i)) => i,
        Ok(None) => return unauthorized(&scope),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !identity.allows(PackageAction::Publish) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(code) = ensure_package(&state.db, &identity, &owner, &image, "public").await {
        return code.into_response();
    }

    // Monolithic upload POST ?digest= is accepted via upload session finalize (end-4b).
    let _ = q.get("digest");

    let id = Uuid::new_v4().to_string();
    if let Ok(mut g) = uploads().lock() {
        g.insert(
            id.clone(),
            UploadSession {
                owner_login: owner,
                image,
                bytes: Vec::new(),
                created: Instant::now(),
            },
        );
    }
    let location = format!("/v2/{name}/blobs/uploads/{id}");
    (
        StatusCode::ACCEPTED,
        [
            (header::LOCATION, HeaderValue::from_str(&location).unwrap()),
            (
                header::HeaderName::from_static("docker-upload-uuid"),
                HeaderValue::from_str(&id).unwrap(),
            ),
            (header::RANGE, HeaderValue::from_static("0-0")),
        ],
    )
        .into_response()
}

async fn patch_upload(
    State(state): State<AppState>,
    Path((owner, image, uuid)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let name = format!("{owner}/{image}");
    let scope = format!("repository:{name}:push");
    let identity = match auth_identity(&state, &headers).await {
        Ok(Some(i)) => i,
        Ok(None) => return unauthorized(&scope),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !identity.allows(PackageAction::Publish) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let end = {
        let Ok(mut g) = uploads().lock() else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        let Some(sess) = g.get_mut(&uuid) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        sess.bytes.extend_from_slice(&body);
        sess.bytes.len().saturating_sub(1)
    };
    let location = format!("/v2/{name}/blobs/uploads/{uuid}");
    (
        StatusCode::ACCEPTED,
        [
            (header::LOCATION, HeaderValue::from_str(&location).unwrap()),
            (
                header::RANGE,
                HeaderValue::from_str(&format!("0-{end}")).unwrap(),
            ),
            (
                header::HeaderName::from_static("docker-upload-uuid"),
                HeaderValue::from_str(&uuid).unwrap(),
            ),
        ],
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
struct DigestQuery {
    digest: String,
}

async fn put_upload(
    State(state): State<AppState>,
    Path((owner, image, uuid)): Path<(String, String, String)>,
    Query(q): Query<DigestQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let name = format!("{owner}/{image}");
    let scope = format!("repository:{name}:push");
    let identity = match auth_identity(&state, &headers).await {
        Ok(Some(i)) => i,
        Ok(None) => return unauthorized(&scope),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !identity.allows(PackageAction::Publish) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if parse_sha256_digest(&q.digest).is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let bytes = {
        let Ok(mut g) = uploads().lock() else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        let Some(mut sess) = g.remove(&uuid) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if !body.is_empty() {
            sess.bytes.extend_from_slice(&body);
        }
        sess.bytes
    };
    let actual = digest_of_bytes(&bytes);
    if actual != q.digest {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some((owner_type, owner_id)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Err(msg) = crate::packages::quota::check_can_store(
        &state.db,
        &owner_type,
        &owner_id,
        bytes.len() as u64,
    )
    .await
    {
        tracing::warn!(error = %msg, "quota");
        return StatusCode::INSUFFICIENT_STORAGE.into_response();
    }
    let max = max_blob_bytes_from_env();
    match store::put_blob(&state.packages_dir, &bytes, max) {
        Ok((digest, size)) => {
            let _ = state.db.upsert_package_blob(&digest, size as i64).await;
            (
                StatusCode::CREATED,
                [
                    (
                        header::LOCATION,
                        HeaderValue::from_str(&format!("/v2/{name}/blobs/{digest}")).unwrap(),
                    ),
                    (
                        header::HeaderName::from_static("docker-content-digest"),
                        HeaderValue::from_str(&digest).unwrap(),
                    ),
                ],
            )
                .into_response()
        }
        Err(store::StoreError::TooLarge { .. }) => StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn get_blob(
    State(state): State<AppState>,
    Path((owner, image, digest)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    blob_read(state, format!("{owner}/{image}"), digest, headers, true).await
}

async fn head_blob(
    State(state): State<AppState>,
    Path((owner, image, digest)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    blob_read(state, format!("{owner}/{image}"), digest, headers, false).await
}

async fn blob_read(
    state: AppState,
    name: String,
    digest: String,
    headers: HeaderMap,
    with_body: bool,
) -> Response {
    let Some((owner, image)) = split_name(&name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let scope = format!("repository:{name}:pull");
    let identity = match auth_identity(&state, &headers).await {
        Ok(i) => i,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(pkg) = (match find_pkg(&state.db, &owner, &image).await {
        Ok(p) => p,
        Err(c) => return c.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, identity.as_ref(), &pkg, PackageAction::Pull).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        if identity.is_none() {
            return unauthorized(&scope);
        }
        return StatusCode::FORBIDDEN.into_response();
    }
    match store::get_blob(&state.packages_dir, &digest) {
        Ok(bytes) => {
            let len = bytes.len();
            let mut res = if with_body {
                bytes.into_response()
            } else {
                StatusCode::OK.into_response()
            };
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            res.headers_mut().insert(
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&len.to_string()).unwrap(),
            );
            res.headers_mut().insert(
                header::HeaderName::from_static("docker-content-digest"),
                HeaderValue::from_str(&digest).unwrap(),
            );
            *res.status_mut() = StatusCode::OK;
            res
        }
        Err(store::StoreError::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn delete_blob(
    State(state): State<AppState>,
    Path((owner, image, digest)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let name = format!("{owner}/{image}");
    let scope = format!("repository:{name}:delete");
    let identity = match auth_identity(&state, &headers).await {
        Ok(Some(i)) => i,
        Ok(None) => return unauthorized(&scope),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !identity.allows(PackageAction::Delete) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(pkg) = (match find_pkg(&state.db, &owner, &image).await {
        Ok(p) => p,
        Err(c) => return c.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, Some(&identity), &pkg, PackageAction::Delete).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        return StatusCode::FORBIDDEN.into_response();
    }
    let _ = digest;
    // Blob GC is deferred to quota plan; acknowledge delete.
    StatusCode::ACCEPTED.into_response()
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ManifestMeta {
    digest: String,
    media_type: String,
    size: u64,
}

async fn put_manifest(
    State(state): State<AppState>,
    Path((owner, image, reference)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let name = format!("{owner}/{image}");
    let scope = format!("repository:{name}:push");
    let identity = match auth_identity(&state, &headers).await {
        Ok(Some(i)) => i,
        Ok(None) => return unauthorized(&scope),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !identity.allows(PackageAction::Publish) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let pkg = match ensure_package(&state.db, &identity, &owner, &image, "public").await {
        Ok(p) => p,
        Err(c) => return c.into_response(),
    };
    let digest = digest_of_bytes(&body);
    let media_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/vnd.oci.image.manifest.v1+json")
        .to_string();

    // Digest reference: immutable content
    if reference.starts_with("sha256:") {
        if reference != digest {
            return StatusCode::BAD_REQUEST.into_response();
        }
        if let Ok(Some(existing)) = state.db.find_package_version(&pkg.id, &reference).await {
            if let Ok(stored) = store::get_blob(&state.packages_dir, existing.digest.as_deref().unwrap_or("")) {
                if stored != body.as_ref() {
                    return StatusCode::CONFLICT.into_response();
                }
            }
            // same bytes — idempotent OK
            return (
                StatusCode::CREATED,
                [(
                    header::HeaderName::from_static("docker-content-digest"),
                    HeaderValue::from_str(&digest).unwrap(),
                )],
            )
                .into_response();
        }
    }

    let max = max_blob_bytes_from_env();
    if let Err(msg) = crate::packages::quota::check_can_store(
        &state.db,
        &pkg.owner_type,
        &pkg.owner_id,
        body.len() as u64,
    )
    .await
    {
        tracing::warn!(error = %msg, "quota");
        return StatusCode::INSUFFICIENT_STORAGE.into_response();
    }
    if let Err(e) = store::put_blob(&state.packages_dir, &body, max) {
        return match e {
            store::StoreError::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE.into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    }
    let _ = state.db.upsert_package_blob(&digest, body.len() as i64).await;

    let meta = ManifestMeta {
        digest: digest.clone(),
        media_type: media_type.clone(),
        size: body.len() as u64,
    };
    let meta_json = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".into());

    // Store under digest key for immutability
    if state
        .db
        .find_package_version(&pkg.id, &digest)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        let vid = Uuid::new_v4().to_string();
        let _ = state
            .db
            .insert_package_version(
                &vid,
                &pkg.id,
                &digest,
                Some(&digest),
                &meta_json,
                Some(&identity.user_id),
            )
            .await;
        let _ = state.db.adjust_package_blob_refcount(&digest, 1).await;
        let _ = state.db.add_package_blob_ref(&vid, &digest, "manifest").await;
    }

    // Tag reference (mutable retarget)
    let mut tag_retargeted = false;
    if !reference.starts_with("sha256:") {
        if let Ok(Some(existing)) = state.db.find_package_version(&pkg.id, &reference).await {
            tag_retargeted = true;
            // Retarget tag: update metadata/digest (version row stays)
            let _ = state
                .db
                .update_package_version_metadata(&existing.id, &meta_json)
                .await;
            // Update digest column via metadata-only path — re-insert by delete+insert if needed
            let digests = state
                .db
                .list_package_version_blob_digests(&existing.id)
                .await
                .unwrap_or_default();
            for d in digests {
                let _ = state.db.adjust_package_blob_refcount(&d, -1).await;
            }
            let _ = state.db.delete_package_version(&existing.id).await;
        }
        let vid = Uuid::new_v4().to_string();
        let _ = state
            .db
            .insert_package_version(
                &vid,
                &pkg.id,
                &reference,
                Some(&digest),
                &meta_json,
                Some(&identity.user_id),
            )
            .await;
        let _ = state.db.adjust_package_blob_refcount(&digest, 1).await;
        let _ = state.db.add_package_blob_ref(&vid, &digest, "manifest").await;
    }

    // API-04: `registry_package` for repo-linked OCI packages (no-op when the
    // package has no repository link, which `ensure_package` leaves unset).
    let action = if tag_retargeted { "updated" } else { "published" };
    crate::webhook::dispatch::notify_package_publish(
        &state.db,
        &pkg,
        &reference,
        action,
        &identity.user_id,
        &state.env_name,
    )
    .await;

    (
        StatusCode::CREATED,
        [
            (
                header::LOCATION,
                HeaderValue::from_str(&format!("/v2/{name}/manifests/{reference}")).unwrap(),
            ),
            (
                header::HeaderName::from_static("docker-content-digest"),
                HeaderValue::from_str(&digest).unwrap(),
            ),
            (header::CONTENT_TYPE, HeaderValue::from_str(&media_type).unwrap()),
        ],
    )
        .into_response()
}

async fn get_manifest(
    State(state): State<AppState>,
    Path((owner, image, reference)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    manifest_read(state, format!("{owner}/{image}"), reference, headers, true).await
}

async fn head_manifest(
    State(state): State<AppState>,
    Path((owner, image, reference)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    manifest_read(state, format!("{owner}/{image}"), reference, headers, false).await
}

async fn manifest_read(
    state: AppState,
    name: String,
    reference: String,
    headers: HeaderMap,
    with_body: bool,
) -> Response {
    let Some((owner, image)) = split_name(&name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let scope = format!("repository:{name}:pull");
    let identity = match auth_identity(&state, &headers).await {
        Ok(i) => i,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(pkg) = (match find_pkg(&state.db, &owner, &image).await {
        Ok(p) => p,
        Err(c) => return c.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, identity.as_ref(), &pkg, PackageAction::Pull).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        if identity.is_none() {
            return unauthorized(&scope);
        }
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(ver) = (match state.db.find_package_version(&pkg.id, &reference).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let digest = ver.digest.clone().unwrap_or_default();
    let meta: ManifestMeta = serde_json::from_str(&ver.metadata_json).unwrap_or_default();
    let media = if meta.media_type.is_empty() {
        "application/vnd.oci.image.manifest.v1+json".into()
    } else {
        meta.media_type
    };
    match store::get_blob(&state.packages_dir, &digest) {
        Ok(bytes) => {
            let mut res = if with_body {
                bytes.into_response()
            } else {
                StatusCode::OK.into_response()
            };
            res.headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_str(&media).unwrap());
            res.headers_mut().insert(
                header::HeaderName::from_static("docker-content-digest"),
                HeaderValue::from_str(&digest).unwrap(),
            );
            *res.status_mut() = StatusCode::OK;
            res
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn delete_manifest(
    State(state): State<AppState>,
    Path((owner, image, reference)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let name = format!("{owner}/{image}");
    let scope = format!("repository:{name}:delete");
    let identity = match auth_identity(&state, &headers).await {
        Ok(Some(i)) => i,
        Ok(None) => return unauthorized(&scope),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !identity.allows(PackageAction::Delete) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(pkg) = (match find_pkg(&state.db, &owner, &image).await {
        Ok(p) => p,
        Err(c) => return c.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, Some(&identity), &pkg, PackageAction::Delete).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(ver) = (match state.db.find_package_version(&pkg.id, &reference).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let digests = state
        .db
        .list_package_version_blob_digests(&ver.id)
        .await
        .unwrap_or_default();
    let _ = state.db.delete_package_version(&ver.id).await;
    for d in digests {
        let _ = state.db.adjust_package_blob_refcount(&d, -1).await;
    }
    StatusCode::ACCEPTED.into_response()
}

#[derive(Debug, Deserialize)]
struct TagsQuery {
    n: Option<u32>,
    last: Option<String>,
}

async fn tags_list(
    State(state): State<AppState>,
    Path((owner, image)): Path<(String, String)>,
    Query(q): Query<TagsQuery>,
    headers: HeaderMap,
) -> Response {
    let name = format!("{owner}/{image}");
    let scope = format!("repository:{name}:pull");
    let identity = match auth_identity(&state, &headers).await {
        Ok(i) => i,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(pkg) = (match find_pkg(&state.db, &owner, &image).await {
        Ok(p) => p,
        Err(c) => return c.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, identity.as_ref(), &pkg, PackageAction::Pull).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        if identity.is_none() {
            return unauthorized(&scope);
        }
        return StatusCode::FORBIDDEN.into_response();
    }
    let versions = match state.db.list_package_versions(&pkg.id).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut tags: Vec<String> = versions
        .into_iter()
        .map(|v| v.version)
        .filter(|v| !v.starts_with("sha256:"))
        .collect();
    tags.sort();
    if let Some(last) = q.last {
        tags.retain(|t| t.as_str() > last.as_str());
    }
    if let Some(n) = q.n {
        tags.truncate(n as usize);
    }
    Json(json!({ "name": name, "tags": tags })).into_response()
}

async fn referrers_deferred(
    Path((_owner, _image, _digest)): Path<(String, String, String)>,
) -> StatusCode {
    StatusCode::NOT_FOUND
}

/// Nested under `/v2` (discovery + token + distribution endpoints).
/// Repository name is `{owner}/{image}` (single image segment; nested images as `owner/a/b`
/// can be added later via a path dispatcher).
pub fn router() -> Router<AppState> {
    let max = max_blob_bytes_from_env().min(usize::MAX as u64) as usize;
    Router::new()
        .route("/token", get(token_endpoint).post(token_endpoint))
        .route("/{owner}/{image}/blobs/uploads/", post(start_upload))
        .route(
            "/{owner}/{image}/blobs/uploads/{uuid}",
            patch(patch_upload).put(put_upload),
        )
        .route(
            "/{owner}/{image}/blobs/{digest}",
            get(get_blob).head(head_blob).delete(delete_blob),
        )
        .route(
            "/{owner}/{image}/manifests/{reference}",
            get(get_manifest)
                .head(head_manifest)
                .put(put_manifest)
                .delete(delete_manifest),
        )
        .route("/{owner}/{image}/tags/list", get(tags_list))
        .route("/{owner}/{image}/referrers/{digest}", get(referrers_deferred))
        .layer(DefaultBodyLimit::max(max))
}
