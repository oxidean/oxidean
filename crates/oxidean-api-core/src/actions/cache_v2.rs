//! `actions/cache@v4` wire protocol — the v2 cache backend speaks Twirp over
//! `{ACTIONS_RESULTS_URL}/twirp/github.actions.results.api.v1.CacheService/*`.
//! The toolkit's `CacheServiceClientJSON` always sends `application/json`
//! bodies (proto field names on the wire, i.e. snake_case); protobuf is only
//! exercised by the unused `CacheServiceClientProtobuf` variant, so this
//! surface is JSON-only.
//!
//!   POST /twirp/github.actions.results.api.v1.CacheService/CreateCacheEntry
//!   POST /twirp/github.actions.results.api.v1.CacheService/FinalizeCacheEntryUpload
//!   POST /twirp/github.actions.results.api.v1.CacheService/GetCacheEntryDownloadURL
//!
//! Blob bytes never cross the Twirp plane: `CreateCacheEntry` returns a
//! `signed_upload_url` the Azure SDK writes to (PutBlob, or StageBlock +
//! PutBlockList for archives over `maxSingleShotSize`), and
//! `GetCacheEntryDownloadURL` returns a `signed_download_url` fetched with a
//! plain GET. Both point at `/_apis/artifactcache2/blobs/{id}?t=<runtime
//! token>` — the client attaches no `Authorization` on the data plane, so the
//! URL itself carries the credential (SAS semantics, same auth check as v1).
//!
//! Control-plane auth is the same per-job `ACTIONS_RUNTIME_TOKEN` Bearer the
//! v1 endpoints use; scope rules are identical (saves land on the job's head
//! branch; restores see that branch plus the repo default). Entries share the
//! `action_cache_entries` table with v1 — a v3 save restores through v4 and
//! vice versa, matching GitHub's single logical store.

use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{post, put};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Deserializer, Serialize};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::app::AppState;

use super::cache::{
    cache_scope_from_headers, cache_scope_from_query, evict_over_cap, find_by_id_scoped,
    find_cache_row_by_id, lookup_hash, storage_rel, CacheScope,
};

/// Twirp JSON error body — `{"code": "<twirp-code>", "msg": "..."}`. Shared
/// by the artifact Twirp surface.
pub(crate) fn twirp_err(status: StatusCode, code: &str, msg: impl Into<String>) -> Response {
    (
        status,
        Json(serde_json::json!({ "code": code, "msg": msg.into() })),
    )
        .into_response()
}

pub(crate) fn twirp_unauthenticated() -> Response {
    twirp_err(
        StatusCode::UNAUTHORIZED,
        "unauthenticated",
        "missing or invalid runtime token",
    )
}

pub(crate) fn twirp_internal() -> Response {
    twirp_err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "internal error",
    )
}

/// Map the shared `StatusCode` auth/lookup failures onto Twirp error JSON.
/// Shared by the artifact Twirp surface.
pub(crate) fn twirp_status(e: StatusCode) -> Response {
    match e {
        StatusCode::UNAUTHORIZED => twirp_unauthenticated(),
        StatusCode::FORBIDDEN => twirp_err(StatusCode::FORBIDDEN, "permission_denied", "forbidden"),
        StatusCode::NOT_FOUND => twirp_err(StatusCode::NOT_FOUND, "not_found", "not found"),
        _ => twirp_internal(),
    }
}

/// `key`/`version`/`restore_keys` carrier shared by all three RPCs; the
/// request's `metadata` (GitHub repository id + scopes) is ignored — the
/// Bearer token already pins job/repo/branch.
#[derive(Debug, Deserialize)]
struct EntryRef {
    #[serde(default)]
    key: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    restore_keys: Vec<String>,
    /// `FinalizeCacheEntryUpload` — declared archive size. int64 arrives as a
    /// JSON string under protobuf-JSON mapping; accept numbers too.
    #[serde(default, deserialize_with = "de_i64_maybe_string")]
    size_bytes: i64,
}

/// int64 arrives as a JSON string under protobuf-JSON mapping; accept plain
/// numbers too. Shared by the artifact Twirp surface (`size` field).
pub(crate) fn de_i64_maybe_string<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| serde::de::Error::custom("size_bytes out of range")),
        serde_json::Value::String(s) => s.trim().parse::<i64>().map_err(serde::de::Error::custom),
        _ => Ok(0),
    }
}

#[derive(Debug, Serialize)]
struct CreateEntryResponse {
    ok: bool,
    signed_upload_url: String,
}

/// POST `…/CacheService/CreateCacheEntry` — reserve `key` under the job's
/// branch scope and hand back the upload URL. A committed or still-pending
/// twin is `already_exists` (the client surfaces it as a non-fatal
/// ReserveCacheError).
async fn create_cache_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<EntryRef>,
) -> Response {
    let scope = match cache_scope_from_headers(&state, &headers).await {
        Ok(s) => s,
        Err(e) => return twirp_status(e),
    };
    if req.key.is_empty() || req.key.len() > 512 || req.version.is_empty() {
        return twirp_err(
            StatusCode::BAD_REQUEST,
            "invalid_argument",
            "key and version are required (key ≤ 512 chars)",
        );
    }
    // Numeric id: `entry_id` in the finalize response must survive the
    // client's `parseInt` (int64 wire field). millis + 6-digit suffix sorts
    // chronologically like the v1 `cache-{millis}-{uuid}` ids.
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let cache_id = format!("{}{:06}", millis, Uuid::new_v4().as_u128() % 1_000_000);
    let row = oxidean_db::actions_cache::ActionCacheRow {
        id: cache_id.clone(),
        repository_id: scope.repository_id.clone(),
        lookup_hash: lookup_hash(&req.key, &req.version, &scope.scope),
        cache_key: req.key.clone(),
        version: req.version.clone(),
        scope: scope.scope.clone(),
        storage_path: storage_rel(&scope.repository_id, &cache_id),
        size_bytes: 0,
        state: "reserved".into(),
        created_at: String::new(),
        last_accessed_at: String::new(),
    };
    match state.db.insert_action_cache(&row).await {
        Ok(()) => Json(CreateEntryResponse {
            ok: true,
            signed_upload_url: blob_url(&cache_id, &scope.token),
        })
        .into_response(),
        Err(_) => twirp_err(
            StatusCode::CONFLICT,
            "already_exists",
            format!("Unable to reserve cache with key {}", req.key),
        ),
    }
}

/// POST `…/CacheService/FinalizeCacheEntryUpload` — the reserved row is found
/// by (key, version, scope); the staged archive must match `size_bytes`
/// before the `.part` becomes the committed blob. `ok:false` + `message` is
/// how the receiver reports a failed finalize (client → FinalizeCacheError,
/// a warning — never fatal).
async fn finalize_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<EntryRef>,
) -> Response {
    let scope = match cache_scope_from_headers(&state, &headers).await {
        Ok(s) => s,
        Err(e) => return twirp_status(e),
    };
    let hash = lookup_hash(&req.key, &req.version, &scope.scope);
    let row = match state
        .db
        .find_action_cache_reservation(&scope.repository_id, &hash)
        .await
    {
        Ok(r) => r,
        Err(_) => return twirp_internal(),
    };
    let Some(row) = row else {
        return Json(serde_json::json!({
            "ok": false,
            "message": "no pending cache entry for this key",
        }))
        .into_response();
    };
    let dest = state.actions_cache_dir.join(&row.storage_path);
    let tmp = dest.with_extension("part");
    let on_disk = tokio::fs::metadata(&tmp)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if req.size_bytes < 0 || on_disk != req.size_bytes as u64 {
        return Json(serde_json::json!({
            "ok": false,
            "message": format!(
                "uploaded size {on_disk} does not match declared size_bytes {}",
                req.size_bytes
            ),
        }))
        .into_response();
    }
    if std::fs::rename(&tmp, &dest).is_err() {
        return twirp_internal();
    }
    let committed = match state.db.commit_action_cache(&row.id, req.size_bytes).await {
        Ok(c) => c,
        Err(_) => return twirp_internal(),
    };
    if !committed {
        let _ = std::fs::remove_file(&dest);
        return Json(
            serde_json::json!({ "ok": false, "message": "cache entry already committed" }),
        )
        .into_response();
    }
    evict_over_cap(&state, &scope.repository_id).await;
    Json(serde_json::json!({ "ok": true, "entry_id": row.id })).into_response()
}

/// POST `…/CacheService/GetCacheEntryDownloadURL` — restore probe. Exact
/// `key` match wins; each `restore_keys` entry then prefix-matches in order.
/// Scopes: the job's branch first, then the repo default. A miss is
/// `{ok:false}` at 200 (the client maps it to a silent cache miss).
async fn get_download_url(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<EntryRef>,
) -> Response {
    let scope = match cache_scope_from_headers(&state, &headers).await {
        Ok(s) => s,
        Err(e) => return twirp_status(e),
    };
    if req.key.is_empty() || req.version.is_empty() {
        return twirp_err(
            StatusCode::BAD_REQUEST,
            "invalid_argument",
            "key and version are required",
        );
    }
    let scopes = {
        let mut v = vec![scope.scope.clone()];
        if scope.default_scope != scope.scope {
            v.push(scope.default_scope.clone());
        }
        v
    };
    // Primary key is exact-only; restore_keys are the prefix candidates.
    for s in &scopes {
        let hit = match state
            .db
            .find_action_cache(
                &scope.repository_id,
                &lookup_hash(&req.key, &req.version, s),
            )
            .await
        {
            Ok(h) => h,
            Err(_) => return twirp_internal(),
        };
        if let Some(hit) = hit {
            let _ = state.db.touch_action_cache(&hit.id).await;
            return download_hit(&hit, &scope);
        }
    }
    for key in &req.restore_keys {
        for s in &scopes {
            let mut hits = match state
                .db
                .find_action_caches_by_key_prefix(&scope.repository_id, key, s)
                .await
            {
                Ok(h) => h,
                Err(_) => return twirp_internal(),
            };
            hits.retain(|c| c.version == req.version);
            if let Some(hit) = hits.into_iter().next() {
                let _ = state.db.touch_action_cache(&hit.id).await;
                return download_hit(&hit, &scope);
            }
        }
    }
    Json(serde_json::json!({ "ok": false })).into_response()
}

fn download_hit(row: &oxidean_db::actions_cache::ActionCacheRow, scope: &CacheScope) -> Response {
    Json(serde_json::json!({
        "ok": true,
        "signed_download_url": blob_url(&row.id, &scope.token),
        "matched_key": row.cache_key,
    }))
    .into_response()
}

/// `signed_{upload,download}_url` — self-authenticating `?t=` URL under the
/// `/api/actions` mount (same proxy path family as the v1 surface).
fn blob_url(cache_id: &str, token: &str) -> String {
    let origin = crate::public_origin::resolve_public_origin();
    format!("{origin}/api/actions/_apis/artifactcache2/blobs/{cache_id}?t={token}")
}

// ---------------------------------------------------------------------------
// Blob plane — Azure block-blob REST shape on top of our storage.
// ---------------------------------------------------------------------------

/// Hex-encode the client's opaque `blockid` (base64 in practice) into a
/// filename-safe segment — `/` and `+` are legal Azure block id chars and must
/// never reach the filesystem verbatim. Shared by the artifact blob plane.
pub(crate) fn block_file(blocks_dir: &std::path::Path, block_id: &str) -> std::path::PathBuf {
    blocks_dir.join(hex::encode(block_id))
}

/// `{storage_path}.blocks/` staging dir under the given store root —
/// shared by the artifact blob plane (`actions_artifacts_dir`).
pub(crate) fn blocks_dir(root: &std::path::Path, storage_path: &str) -> std::path::PathBuf {
    root.join(storage_path).with_extension("blocks")
}

/// Stream a request body to `dest`, enforcing the per-entry cap. Shared by
/// the artifact blob plane.
pub(crate) async fn write_body_to(
    dest: &std::path::Path,
    body: Body,
    max: u64,
) -> Result<u64, StatusCode> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut stream = body.into_data_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| StatusCode::BAD_REQUEST)?;
        written += chunk.len() as u64;
        if written > max {
            let _ = std::fs::remove_file(dest);
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        if file.write_all(&chunk).await.is_err() {
            let _ = std::fs::remove_file(dest);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    Ok(written)
}

/// Ordered block ids out of a Put Block List XML body — entries may carry
/// `Latest`, `Committed`, or `Uncommitted` tags; document order is preserved
/// (the earliest opening tag wins each step). Shared by the artifact plane.
pub(crate) fn parse_block_list(xml: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = xml;
    loop {
        // Earliest of the three entry tags.
        let (pos, tag) = ["Latest", "Committed", "Uncommitted"]
            .iter()
            .filter_map(|t| rest.find(&format!("<{t}>")).map(|p| (p, *t)))
            .min_by_key(|(p, _)| *p)
            .unwrap_or((usize::MAX, ""));
        if pos == usize::MAX {
            break;
        }
        let after = &rest[pos + tag.len() + 2..];
        let Some(close) = after.find(&format!("</{tag}>")) else {
            break;
        };
        ids.push(after[..close].trim().to_string());
        rest = &after[close + tag.len() + 3..];
    }
    ids
}

/// PUT `/_apis/artifactcache2/blobs/{id}?t=…` — the v2 upload target.
/// `?comp=block&blockid=x` stages one block; `?comp=blocklist` commits the
/// ordered concat into `.part`; bare PUT is PutBlob (small archives land in
/// one shot, `maxSingleShotSize` is 128 MiB client-side).
async fn put_blob(
    State(state): State<AppState>,
    Path(cache_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    body: Body,
) -> Result<StatusCode, StatusCode> {
    let scope = cache_scope_from_query(&state, q.get("t").map(String::as_str)).await?;
    let row = find_cache_row_by_id(&state, &cache_id).await?;
    if row.repository_id != scope.repository_id || row.state != "reserved" {
        return Err(StatusCode::NOT_FOUND);
    }
    let max = state.actions_cache_max_bytes;
    match q.get("comp").map(String::as_str) {
        Some("block") => {
            let block_id = q
                .get("blockid")
                .filter(|b| !b.is_empty())
                .ok_or(StatusCode::BAD_REQUEST)?;
            let dir = blocks_dir(&state.actions_cache_dir, &row.storage_path);
            write_body_to(&block_file(&dir, block_id), body, max)
                .await
                .map(|_| StatusCode::CREATED)
        }
        Some("blocklist") => {
            let bytes = body
                .into_data_stream()
                .fold(Vec::new(), |mut acc, c| async move {
                    if let Ok(c) = c {
                        acc.extend_from_slice(&c);
                    }
                    acc
                })
                .await;
            let xml = String::from_utf8_lossy(&bytes);
            let ids = parse_block_list(&xml);
            if ids.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            let dir = blocks_dir(&state.actions_cache_dir, &row.storage_path);
            let dest = state
                .actions_cache_dir
                .join(&row.storage_path)
                .with_extension("part");
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            let mut out = tokio::fs::File::create(&dest)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut total: u64 = 0;
            for id in &ids {
                let data = tokio::fs::read(block_file(&dir, id))
                    .await
                    .map_err(|_| StatusCode::BAD_REQUEST)?; // missing/unknown block
                total += data.len() as u64;
                if total > max * 2 {
                    let _ = std::fs::remove_file(&dest);
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                out.write_all(&data)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            drop(out);
            let _ = std::fs::remove_dir_all(&dir);
            Ok(StatusCode::CREATED)
        }
        _ => {
            // PutBlob — single-shot archive.
            let dest = state
                .actions_cache_dir
                .join(&row.storage_path)
                .with_extension("part");
            write_body_to(&dest, body, max * 2)
                .await
                .map(|_| StatusCode::CREATED)
        }
    }
}

/// GET `/_apis/artifactcache2/blobs/{id}?t=…` — the v2 download target. The
/// client validates the payload against `content-length`; scope rules match
/// v1 (own branch + repo default).
async fn get_blob(
    State(state): State<AppState>,
    Path(cache_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let scope = cache_scope_from_query(&state, q.get("t").map(String::as_str)).await?;
    let row = find_by_id_scoped(&state, &cache_id, &scope).await?;
    let path = state.actions_cache_dir.join(&row.storage_path);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/gzip")
        .header("content-length", row.size_bytes.to_string())
        .body(Body::from_stream(tokio_util::io::ReaderStream::new(file)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Twirp control plane — mounted at the API root (`/twirp/…`): the client
/// resolves `new URL("/twirp/<service>/<method>", ACTIONS_RESULTS_URL)` so
/// only the origin of the env var matters.
pub fn twirp_router() -> Router<AppState> {
    Router::new()
        .route(
            "/github.actions.results.api.v1.CacheService/CreateCacheEntry",
            post(create_cache_entry),
        )
        .route(
            "/github.actions.results.api.v1.CacheService/FinalizeCacheEntryUpload",
            post(finalize_upload),
        )
        .route(
            "/github.actions.results.api.v1.CacheService/GetCacheEntryDownloadURL",
            post(get_download_url),
        )
}

/// Blob plane — merged into the `/api/actions` runner-proto mount next to
/// the v1 `_apis/artifactcache` routes.
pub fn blob_router() -> Router<AppState> {
    Router::new().route(
        "/_apis/artifactcache2/blobs/{id}",
        put(put_blob).get(get_blob),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_list_parses_ordered_ids() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?><BlockList><Latest>AAA=</Latest><Latest>BBB=</Latest></BlockList>"#;
        assert_eq!(parse_block_list(xml), vec!["AAA=", "BBB="]);
        let committed =
            r#"<BlockList><Committed>AAA=</Committed><Uncommitted>BBB=</Uncommitted></BlockList>"#;
        assert_eq!(parse_block_list(committed), vec!["AAA=", "BBB="]);
        assert!(parse_block_list("<BlockList/>").is_empty());
    }

    #[test]
    fn i64_accepts_string_and_number() {
        #[derive(Deserialize)]
        struct W {
            #[serde(deserialize_with = "de_i64_maybe_string")]
            v: i64,
        }
        assert_eq!(serde_json::from_str::<W>(r#"{"v":"42"}"#).unwrap().v, 42);
        assert_eq!(serde_json::from_str::<W>(r#"{"v":7}"#).unwrap().v, 7);
        assert!(serde_json::from_str::<W>(r#"{"v":true}"#).unwrap().v == 0);
    }
}
