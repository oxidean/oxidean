//! npm registry API under `/npm/{owner}/` (PKG-02 / D-PKG-14).

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest as _, Sha512};
use uuid::Uuid;

use oxidean_db::{Database, PackageRow};

use crate::app::AppState;
use crate::auth::verify_reset::public_origin;
use crate::packages::acl::{self, PackageAction};
use crate::packages::auth::{self, RegistryIdentity};
use crate::packages::store::{self, max_blob_bytes_from_env};
use crate::repo::{coalesce, Capability, MemberBasePermission, OrgRole};

const FORMAT: &str = "npm";

fn decode_pkg_name(raw: &str) -> String {
    // Accept @scope%2fname or @scope/name
    percent_decode(raw)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((a << 4) | b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn b64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let bytes: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in &bytes {
        if b == b'=' {
            break;
        }
        let val = TABLE.iter().position(|&c| c == b)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

fn hex_sha1(bytes: &[u8]) -> String {
    let digest = Sha1::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for &b in digest.iter() {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('0'));
    }
    out
}

fn sri_sha512(bytes: &[u8]) -> String {
    let digest = Sha512::digest(bytes);
    format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode(digest)
    )
}

fn tarball_filename(pkg: &str, version: &str) -> String {
    let base = pkg.rsplit('/').next().unwrap_or(pkg);
    format!("{base}-{version}.tgz")
}

fn tarball_url(owner: &str, pkg: &str, version: &str) -> String {
    let origin = public_origin().trim_end_matches('/').to_string();
    let enc = pkg.replace('/', "%2f");
    format!(
        "{origin}/npm/{owner}/{enc}/-/{tarball}",
        tarball = tarball_filename(pkg, version)
    )
}

async fn resolve_owner(db: &Database, login: &str) -> Result<Option<(String, String)>, String> {
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

#[derive(Debug, Serialize, Deserialize, Default)]
struct NpmVersionMeta {
    #[serde(default)]
    digest: String,
    #[serde(default)]
    shasum: String,
    #[serde(default)]
    integrity: String,
    #[serde(default)]
    deprecated: Option<String>,
    #[serde(default)]
    manifest: Value,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NpmPackageMeta {
    #[serde(default)]
    dist_tags: serde_json::Map<String, Value>,
    #[serde(default)]
    description: String,
}

fn package_meta(pkg: &PackageRow) -> NpmPackageMeta {
    serde_json::from_str(&pkg.description).unwrap_or_else(|_| {
        let mut m = NpmPackageMeta::default();
        m.dist_tags.insert("latest".into(), json!(""));
        m
    })
}

#[allow(clippy::result_large_err)]
async fn require_write(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<RegistryIdentity, Response> {
    match auth::authenticate_registry(&state.db, headers).await {
        Ok(Some(i)) if i.allows(PackageAction::Publish) => Ok(i),
        Ok(Some(_)) => Err(StatusCode::FORBIDDEN.into_response()),
        Ok(None) => Err((
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer realm=\"Oxidean npm\"")],
        )
            .into_response()),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    }
}

async fn get_packument(
    State(state): State<AppState>,
    Path((owner, package)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let pkg_name = decode_pkg_name(&package);
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
    let Some(pkg) = (match state
        .db
        .find_package(&owner_type, &owner_id, &pkg_name, FORMAT)
        .await
    {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, identity.as_ref(), &pkg, PackageAction::Pull).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        return if identity.is_none() {
            StatusCode::UNAUTHORIZED.into_response()
        } else {
            StatusCode::FORBIDDEN.into_response()
        };
    }
    let versions = match state.db.list_package_versions(&pkg.id).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let meta = package_meta(&pkg);
    let mut versions_obj = serde_json::Map::new();
    let mut times = serde_json::Map::new();
    for v in &versions {
        let vm: NpmVersionMeta = serde_json::from_str(&v.metadata_json).unwrap_or_default();
        let mut man = if vm.manifest.is_object() {
            vm.manifest.clone()
        } else {
            json!({"name": pkg_name, "version": v.version})
        };
        if let Some(obj) = man.as_object_mut() {
            let mut dist = json!({
                "tarball": tarball_url(&owner, &pkg_name, &v.version),
                "shasum": vm.shasum,
            });
            if !vm.integrity.is_empty() {
                dist["integrity"] = json!(vm.integrity);
            }
            obj.insert("dist".into(), dist);
            if let Some(dep) = &vm.deprecated {
                obj.insert("deprecated".into(), json!(dep));
            }
        }
        versions_obj.insert(v.version.clone(), man);
        times.insert(v.version.clone(), json!(v.created_at));
    }
    Json(json!({
        "name": pkg_name,
        "dist-tags": meta.dist_tags,
        "versions": versions_obj,
        "time": times,
    }))
    .into_response()
}

async fn put_publish(
    State(state): State<AppState>,
    Path((owner, package)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let identity = match require_write(&state, &headers).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let pkg_name = decode_pkg_name(&package);
    let doc: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Deprecate-only update: versions present without new attachments
    let attachments = doc.get("_attachments").and_then(|a| a.as_object());
    let versions = doc.get("versions").and_then(|v| v.as_object());

    let Some((owner_type, owner_id)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let existing = match state
        .db
        .find_package(&owner_type, &owner_id, &pkg_name, FORMAT)
        .await
    {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    // Deprecate path: update metadata only
    if attachments.map(|a| a.is_empty()).unwrap_or(true) && versions.is_some() {
        if let Some(pkg) = existing.as_ref() {
            let ok = match authorize_pkg(&state.db, Some(&identity), pkg, PackageAction::Publish)
                .await
            {
                Ok(v) => v,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            if !ok {
                return StatusCode::FORBIDDEN.into_response();
            }
            if let Some(vers) = versions {
                for (ver, man) in vers {
                    if let Ok(Some(row)) = state.db.find_package_version(&pkg.id, ver).await {
                        let mut vm: NpmVersionMeta =
                            serde_json::from_str(&row.metadata_json).unwrap_or_default();
                        if let Some(dep) = man.get("deprecated").and_then(|d| d.as_str()) {
                            vm.deprecated = Some(dep.to_string());
                        } else if man.get("deprecated").is_some() {
                            vm.deprecated = None;
                        }
                        vm.manifest = man.clone();
                        let meta_json = serde_json::to_string(&vm).unwrap_or_else(|_| "{}".into());
                        let _ = state
                            .db
                            .update_package_version_metadata(&row.id, &meta_json)
                            .await;
                    }
                }
            }
            if let Some(tags) = doc.get("dist-tags").and_then(|t| t.as_object()) {
                let mut meta = package_meta(pkg);
                meta.dist_tags = tags.clone();
                let desc = serde_json::to_string(&meta).unwrap_or_default();
                let _ = update_package_description(&state.db, &pkg.id, &desc).await;
            }
            return StatusCode::OK.into_response();
        }
    }

    let Some(attachments) = attachments else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some((filename, att)) = attachments.iter().next() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let data_b64 = att.get("data").and_then(|d| d.as_str()).unwrap_or("");
    let tarball = match b64_decode(data_b64) {
        Some(b) => b,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };
    let version = versions
        .and_then(|v| v.keys().next().cloned())
        .or_else(|| {
            // parse from filename pkg-1.0.0.tgz
            filename
                .rsplit_once('-')
                .and_then(|(_, rest)| rest.strip_suffix(".tgz").map(|s| s.to_string()))
        })
        .unwrap_or_else(|| "0.0.0".into());

    let manifest = versions
        .and_then(|v| v.get(&version).cloned())
        .unwrap_or_else(|| json!({"name": pkg_name, "version": version}));

    let visibility = doc
        .get("access")
        .and_then(|a| a.as_str())
        .map(|a| if a == "public" { "public" } else { "private" })
        .unwrap_or("public");

    let pkg = if let Some(pkg) = existing {
        let ok = match authorize_pkg(&state.db, Some(&identity), &pkg, PackageAction::Publish).await
        {
            Ok(v) => v,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        if !ok {
            return StatusCode::FORBIDDEN.into_response();
        }
        if state
            .db
            .find_package_version(&pkg.id, &version)
            .await
            .ok()
            .flatten()
            .is_some()
        {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "EPUBLISHCONFLICT",
                    "reason": "cannot publish over existing version"
                })),
            )
                .into_response();
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
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        if !acl::authorize(visibility, have, true, PackageAction::Publish) {
            return StatusCode::FORBIDDEN.into_response();
        }
        let id = Uuid::new_v4().to_string();
        let mut meta = NpmPackageMeta::default();
        meta.dist_tags.insert("latest".into(), json!(version));
        let desc = serde_json::to_string(&meta).unwrap_or_default();
        if state
            .db
            .insert_package(
                &id,
                &owner_type,
                &owner_id,
                &pkg_name,
                FORMAT,
                visibility,
                None,
                &desc,
            )
            .await
            .is_err()
        {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        match state
            .db
            .find_package(&owner_type, &owner_id, &pkg_name, FORMAT)
            .await
        {
            Ok(Some(p)) => p,
            _ => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    };

    let max = max_blob_bytes_from_env();
    if let Err(msg) = crate::packages::quota::check_can_store(
        &state.db,
        &owner_type,
        &owner_id,
        tarball.len() as u64,
    )
    .await
    {
        tracing::warn!(error = %msg, "quota");
        return StatusCode::INSUFFICIENT_STORAGE.into_response();
    }
    let (digest, size) = match store::put_blob(&state.packages_dir, &tarball, max) {
        Ok(v) => v,
        Err(store::StoreError::TooLarge { .. }) => {
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let shasum = hex_sha1(&tarball);
    let integrity = sri_sha512(&tarball);
    let vm = NpmVersionMeta {
        digest: digest.clone(),
        shasum,
        integrity,
        deprecated: None,
        manifest,
    };
    let meta_json = serde_json::to_string(&vm).unwrap_or_else(|_| "{}".into());
    let vid = Uuid::new_v4().to_string();
    if let Err(e) = state
        .db
        .insert_package_version(
            &vid,
            &pkg.id,
            &version,
            Some(&digest),
            &meta_json,
            Some(&identity.user_id),
        )
        .await
    {
        if e.contains("UNIQUE") || e.contains("unique") {
            return StatusCode::CONFLICT.into_response();
        }
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let _ = state.db.upsert_package_blob(&digest, size as i64).await;
    let _ = state.db.adjust_package_blob_refcount(&digest, 1).await;
    let _ = state
        .db
        .add_package_blob_ref(&vid, &digest, "tarball")
        .await;

    // Update dist-tags latest
    let mut meta = package_meta(&pkg);
    if let Some(tags) = doc.get("dist-tags").and_then(|t| t.as_object()) {
        meta.dist_tags = tags.clone();
    } else {
        meta.dist_tags.insert("latest".into(), json!(version));
    }
    let desc = serde_json::to_string(&meta).unwrap_or_default();
    let _ = update_package_description(&state.db, &pkg.id, &desc).await;

    // API-04: fans out `registry_package` only when the package is linked to
    // a repository (npm publish does not carry a repository link today).
    crate::webhook::dispatch::notify_package_publish(
        &state.db,
        &pkg,
        &version,
        "published",
        &identity.user_id,
        &state.env_name,
    )
    .await;

    StatusCode::CREATED.into_response()
}

async fn update_package_description(
    db: &Database,
    id: &str,
    description: &str,
) -> Result<(), String> {
    // Reuse description column for npm package-level JSON (dist-tags).
    db.update_package_description(id, description).await
}

async fn get_tarball(
    State(state): State<AppState>,
    Path((owner, package, filename)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let pkg_name = decode_pkg_name(&package);
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
    let Some(pkg) = (match state
        .db
        .find_package(&owner_type, &owner_id, &pkg_name, FORMAT)
        .await
    {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, identity.as_ref(), &pkg, PackageAction::Pull).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // filename like name-1.0.0.tgz
    let version = filename
        .rsplit_once('-')
        .and_then(|(_, rest)| rest.strip_suffix(".tgz"))
        .unwrap_or("");
    let Some(ver) = (match state.db.find_package_version(&pkg.id, version).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let digest = ver.digest.unwrap_or_default();
    match store::get_blob(&state.packages_dir, &digest) {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/octet-stream")],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn get_dist_tags(
    State(state): State<AppState>,
    Path((owner, package)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let pkg_name = decode_pkg_name(&package);
    let identity = match auth::authenticate_registry(&state.db, &headers).await {
        Ok(i) => i,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some((ot, oid)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(pkg) = (match state.db.find_package(&ot, &oid, &pkg_name, FORMAT).await {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, identity.as_ref(), &pkg, PackageAction::Pull).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(package_meta(&pkg).dist_tags).into_response()
}

async fn put_dist_tag(
    State(state): State<AppState>,
    Path((owner, package, tag)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let identity = match require_write(&state, &headers).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let pkg_name = decode_pkg_name(&package);
    let version = String::from_utf8_lossy(&body)
        .trim()
        .trim_matches('"')
        .to_string();
    let Some((ot, oid)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(pkg) = (match state.db.find_package(&ot, &oid, &pkg_name, FORMAT).await {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, Some(&identity), &pkg, PackageAction::Publish).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut meta = package_meta(&pkg);
    meta.dist_tags.insert(tag, json!(version));
    let desc = serde_json::to_string(&meta).unwrap_or_default();
    let _ = update_package_description(&state.db, &pkg.id, &desc).await;
    StatusCode::OK.into_response()
}

async fn delete_dist_tag(
    State(state): State<AppState>,
    Path((owner, package, tag)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let identity = match require_write(&state, &headers).await {
        Ok(i) => i,
        Err(r) => return r,
    };
    let pkg_name = decode_pkg_name(&package);
    let Some((ot, oid)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(pkg) = (match state.db.find_package(&ot, &oid, &pkg_name, FORMAT).await {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = match authorize_pkg(&state.db, Some(&identity), &pkg, PackageAction::Publish).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !ok {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut meta = package_meta(&pkg);
    meta.dist_tags.remove(&tag);
    let desc = serde_json::to_string(&meta).unwrap_or_default();
    let _ = update_package_description(&state.db, &pkg.id, &desc).await;
    StatusCode::OK.into_response()
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    text: Option<String>,
    size: Option<u32>,
    from: Option<u32>,
}

async fn search_v1(
    State(state): State<AppState>,
    Path(owner): Path<String>,
    Query(q): Query<SearchQuery>,
    headers: HeaderMap,
) -> Response {
    let identity = match auth::authenticate_registry(&state.db, &headers).await {
        Ok(i) => i,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some((ot, oid)) = (match resolve_owner(&state.db, &owner).await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let pkgs = match state.db.list_packages_by_owner(&ot, &oid).await {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let text = q.text.unwrap_or_default().to_ascii_lowercase();
    let mut objects = Vec::new();
    for pkg in pkgs {
        if pkg.format != FORMAT {
            continue;
        }
        // Authenticated callers must still pass Pull ACL — do not list private/linked-private.
        let allowed =
            match authorize_pkg(&state.db, identity.as_ref(), &pkg, PackageAction::Pull).await {
                Ok(v) => v,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
        if !allowed {
            continue;
        }
        if !text.is_empty() && !pkg.name.to_ascii_lowercase().contains(&text) {
            continue;
        }
        objects.push(json!({
            "package": {
                "name": pkg.name,
                "description": package_meta(&pkg).description,
                "version": package_meta(&pkg).dist_tags.get("latest").cloned().unwrap_or(json!("")),
            }
        }));
    }
    let from = q.from.unwrap_or(0) as usize;
    let size = q.size.unwrap_or(20) as usize;
    let slice: Vec<_> = objects.into_iter().skip(from).take(size).collect();
    Json(json!({ "objects": slice, "total": slice.len() })).into_response()
}

pub fn router() -> Router<AppState> {
    let max = max_blob_bytes_from_env().min(usize::MAX as u64) as usize;
    Router::new()
        .route("/{owner}/-/v1/search", get(search_v1))
        .route("/{owner}/-/package/{package}/dist-tags", get(get_dist_tags))
        .route(
            "/{owner}/-/package/{package}/dist-tags/{tag}",
            put(put_dist_tag).delete(delete_dist_tag),
        )
        .route("/{owner}/{package}/-/{filename}", get(get_tarball))
        .route("/{owner}/{package}", get(get_packument).put(put_publish))
        .layer(DefaultBodyLimit::max(max))
}
