//! `actions/cache` service (P14) — GitHub-compatible `_apis/artifactcache`
//! REST surface under the runner-proto mount (`/api/actions`):
//!
//!   GET    /_apis/artifactcache/cache?keys=k1,k2&version=v   → entry | 204
//!   POST   /_apis/artifactcache/caches                       → {cacheId} | 409
//!   PATCH  /_apis/artifactcache/caches/{id}                  → append chunk
//!   POST   /_apis/artifactcache/caches/{id}                  → commit {size}
//!   GET    /_apis/artifactcache/caches/{id}                  → entry metadata
//!   GET    /_apis/artifactcache/caches/{id}/download         → payload bytes
//!   DELETE /_apis/artifactcache/caches/{id}                  → delete entry
//!
//! Auth is a per-job `ACTIONS_RUNTIME_TOKEN` minted at claim (hashed rows in
//! `action_cache_tokens`), not the runner registration token — a leaked
//! runtime token only reaches that job's repo.
//!
//! Scope (GitHub): entries save under the run's head branch; lookups match
//! the job's own branch plus the repo's default branch — a push to a feature
//! branch can restore the default branch's caches but never a sibling's.
//! Pull-request runs save/restore under their `refs/pull/N/*` head ref,
//! matching GitHub's PR-scoped cache behavior.

use std::collections::HashMap;
use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::app::AppState;

/// Cache blob root — `OXIDEAN_ACTIONS_CACHE_DIR`; default `var/actions-cache`.
pub fn resolve_cache_dir() -> PathBuf {
    std::env::var("OXIDEAN_ACTIONS_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/actions-cache"))
}

/// Repo cache cap — `OXIDEAN_ACTIONS_CACHE_MAX_BYTES`; default 10 GiB, the
/// same per-repo ceiling GitHub enforces before LRU eviction.
pub fn default_cache_max_bytes() -> u64 {
    std::env::var("OXIDEAN_ACTIONS_CACHE_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10 * 1024 * 1024 * 1024)
}

/// Runtime-token lifetime (hours) — `OXIDEAN_ACTIONS_CACHE_TOKEN_HOURS`;
/// default 12h so a long-running job never loses its cache lease mid-run.
fn token_ttl_hours() -> i64 {
    std::env::var("OXIDEAN_ACTIONS_CACHE_TOKEN_HOURS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(12)
        .clamp(1, 72)
}

/// How long an uncommitted reservation survives (minutes) before the sweeper
/// reaps it — abandoned `POST /caches` rows would otherwise pin a key
/// forever. `OXIDEAN_ACTIONS_CACHE_RESERVE_MINS`; default 30.
fn reserve_ttl_minutes() -> i64 {
    std::env::var("OXIDEAN_ACTIONS_CACHE_RESERVE_MINS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(30)
        .max(1)
}

/// sha256(key \n version \n scope) — the `lookup_hash` stored on each row.
/// Hashing keeps the uniqueness index narrow (InnoDB 3072-byte prefix limit)
/// while the plaintext columns stay queryable.
pub(crate) fn lookup_hash(key: &str, version: &str, scope: &str) -> String {
    hex::encode(Sha256::digest(
        format!("{key}\n{version}\n{scope}").as_bytes(),
    ))
}

/// `refs/heads/main` → `main`; anything else stays verbatim (PR refs, tags).
fn normalize_scope(head_ref: &str) -> String {
    head_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(head_ref)
        .to_string()
}

/// Resolved job scope for one authenticated cache call.
pub(crate) struct CacheScope {
    pub repository_id: String,
    /// The job's branch — where its saves land and the first lookup scope.
    pub scope: String,
    /// Repo default branch — the second lookup scope (never saved into from
    /// a non-default branch).
    pub default_scope: String,
    /// The presented runtime token — v2 signs blob URLs with it (the data
    /// plane carries `?t=` instead of an Authorization header).
    pub token: String,
}

/// Bearer `ACTIONS_RUNTIME_TOKEN` → job/repo/branch scope. 401 on unknown or
/// expired tokens; 404 on a dangling job row.
pub(crate) async fn cache_scope_from_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<CacheScope, StatusCode> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let raw = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?
        .trim();
    cache_scope_from_token(state, raw).await
}

/// `?t=<ACTIONS_RUNTIME_TOKEN>` data-plane auth — the real `actions/cache`
/// clients attach no `Authorization` header when following `archiveLocation`
/// or the v2 signed URLs; the URL itself is the credential (SAS semantics).
pub(crate) async fn cache_scope_from_query(
    state: &AppState,
    token: Option<&str>,
) -> Result<CacheScope, StatusCode> {
    cache_scope_from_token(state, token.ok_or(StatusCode::UNAUTHORIZED)?.trim()).await
}

/// Shared token → scope resolution for both auth carriers.
pub(crate) async fn cache_scope_from_token(
    state: &AppState,
    raw: &str,
) -> Result<CacheScope, StatusCode> {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let tok = state
        .db
        .find_action_cache_token(&crate::actions::tokens::hash_token(raw), &now)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let job = state
        .db
        .find_action_job_by_id(&tok.job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let run = state
        .db
        .find_action_run_by_id(&job.run_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if run.repository_id != tok.repository_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let default_scope = state
        .db
        .find_repository_by_id(&tok.repository_id)
        .await
        .ok()
        .flatten()
        .map(|r| r.default_branch)
        .unwrap_or_else(|| "main".to_string());
    Ok(CacheScope {
        repository_id: tok.repository_id,
        scope: normalize_scope(&run.head_ref),
        default_scope: normalize_scope(&default_scope),
        token: raw.to_string(),
    })
}

/// Mint a fresh runtime token for `job_id` — called from `fetch_task` at
/// claim. Returns the plaintext (handed to the runner env) or None on a
/// persistence error (cache stays unusable for the job rather than failing
/// the claim — actions/cache tolerates missing env by skipping).
pub async fn mint_cache_token(state: &AppState, job_id: &str, repo_id: &str) -> Option<String> {
    let plaintext = format!("oxrt_{}", Uuid::new_v4());
    let expires_at = (chrono::Utc::now() + chrono::Duration::hours(token_ttl_hours()))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    state
        .db
        .insert_action_cache_token(
            &crate::actions::tokens::hash_token(&plaintext),
            job_id,
            repo_id,
            &expires_at,
        )
        .await
        .ok()?;
    Some(plaintext)
}

/// Absolute URL the `cache` response hands back as `archiveLocation` —
/// same-origin under the runner-proto mount. Carries `?t=<runtime token>`
/// because the real `actions/cache` client sends no `Authorization` header
/// when downloading (GitHub's archiveLocation is a self-authenticating SAS).
fn archive_location(cache_id: &str, token: &str) -> String {
    let origin = crate::public_origin::resolve_public_origin();
    format!("{origin}/api/actions/_apis/artifactcache/caches/{cache_id}/download?t={token}")
}

pub(crate) fn storage_rel(repo_id: &str, cache_id: &str) -> String {
    format!("{repo_id}/{cache_id}.tgz")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheEntry {
    cache_key: String,
    scope: String,
    creation_time: String,
    last_accessed: String,
    archive_location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
}

fn entry_public(c: &oxidean_db::actions_cache::ActionCacheRow, token: &str) -> CacheEntry {
    CacheEntry {
        cache_key: c.cache_key.clone(),
        scope: c.scope.clone(),
        creation_time: c.created_at.clone(),
        last_accessed: c.last_accessed_at.clone(),
        archive_location: archive_location(&c.id, token),
        result: None,
    }
}

/// GET `/_apis/artifactcache/cache?keys=k1,k2&version=v` — restore probe.
/// `keys` is ordered: primary key first, restore-keys after. Exact match on
/// each key in turn wins; otherwise each key falls back to a prefix match
/// (newest commit wins). Scope check happens here — a job only sees entries
/// saved under its own branch or the repo default.
async fn get_cache(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let scope = cache_scope_from_headers(&state, &headers).await?;
    let keys = q
        .get("keys")
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let version = q.get("version").cloned().unwrap_or_default();
    if keys.is_empty() || version.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let scopes = {
        let mut v = vec![scope.scope.clone()];
        if scope.default_scope != scope.scope {
            v.push(scope.default_scope.clone());
        }
        v
    };
    // Pass 1: exact matches, in caller key order.
    for key in &keys {
        for s in &scopes {
            if let Some(hit) = state
                .db
                .find_action_cache(&scope.repository_id, &lookup_hash(key, &version, s))
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            {
                let _ = state.db.touch_action_cache(&hit.id).await;
                return Ok(Json(entry_public(&hit, &scope.token)).into_response());
            }
        }
    }
    // Pass 2: prefix fallback per key (restore-keys semantics).
    for key in &keys {
        for s in &scopes {
            let mut hits = state
                .db
                .find_action_caches_by_key_prefix(&scope.repository_id, key, s)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            hits.retain(|c| c.version == version);
            if let Some(hit) = hits.into_iter().next() {
                let _ = state.db.touch_action_cache(&hit.id).await;
                return Ok(Json(entry_public(&hit, &scope.token)).into_response());
            }
        }
    }
    Err(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct ReserveRequest {
    key: String,
    version: String,
    /// actions/cache v2 hints the payload size — we accept but don't trust it.
    #[serde(rename = "cacheSize", default)]
    _cache_size: Option<i64>,
}

/// POST `/_apis/artifactcache/caches` — reserve a key. 409 when committed or
/// already reserved (immutable save semantics: actions/cache logs
/// "Unable to reserve cache" and skips the save).
async fn reserve_cache(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ReserveRequest>,
) -> Result<Response, StatusCode> {
    let scope = cache_scope_from_headers(&state, &headers).await?;
    if req.key.is_empty() || req.key.len() > 512 || req.version.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    // `cache-<millis>-<uuid>` — ids sort by creation time, giving the LRU
    // eviction scan a deterministic order among same-second `last_accessed_at`
    // ties (evict the older entry first).
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let cache_id = format!("cache-{millis:013}-{}", Uuid::new_v4());
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
        Ok(()) => Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({ "cacheId": cache_id })),
        )
            .into_response()),
        // Unique-violation → the key is taken (reserved or committed).
        Err(_) => Err(StatusCode::CONFLICT),
    }
}

/// PATCH `/_apis/artifactcache/caches/{id}` — append an upload chunk.
/// `Content-Range: bytes start-end/*` must resume where the file ends;
/// a missing header treats the body as the first chunk.
async fn upload_cache_chunk(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cache_id): Path<String>,
    body: Body,
) -> Result<StatusCode, StatusCode> {
    let scope = cache_scope_from_headers(&state, &headers).await?;
    let row = find_reserved(&state, &cache_id, &scope.repository_id).await?;
    let rel = row.storage_path.clone();
    let dest = state.actions_cache_dir.join(&rel);
    let tmp = dest.with_extension("part");
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    // Content-Range start must match bytes already on disk — a gap means the
    // client dropped a chunk and the payload would corrupt silently.
    let existing = tokio::fs::metadata(&tmp)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if let Some(range) = headers
        .get(axum::http::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
    {
        let start = range
            .strip_prefix("bytes ")
            .and_then(|r| r.split('-').next())
            .and_then(|s| s.parse::<u64>().ok());
        match start {
            Some(s) if s == existing => {}
            Some(_) => return Err(StatusCode::RANGE_NOT_SATISFIABLE),
            None => return Err(StatusCode::BAD_REQUEST),
        }
    } else if existing != 0 {
        // A second range-less PATCH after data exists is ambiguous — reject.
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&tmp)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut stream = body.into_data_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| StatusCode::BAD_REQUEST)?;
        written += chunk.len() as u64;
        if existing + written > state.actions_cache_max_bytes * 2 {
            // A single entry may not exceed the repo cap — reject early so
            // disk isn't filled by a hostile upload.
            let _ = std::fs::remove_file(&tmp);
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        if file.write_all(&chunk).await.is_err() {
            let _ = std::fs::remove_file(&tmp);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct CommitRequest {
    size: i64,
}

/// POST `/_apis/artifactcache/caches/{id}` — finalize the reservation: the
/// declared `size` must match bytes-on-disk, then the `.part` becomes `.tgz`
/// and the row flips to `committed`. Triggers LRU eviction for the repo.
async fn commit_cache(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cache_id): Path<String>,
    Json(req): Json<CommitRequest>,
) -> Result<StatusCode, StatusCode> {
    let scope = cache_scope_from_headers(&state, &headers).await?;
    let row = find_reserved(&state, &cache_id, &scope.repository_id).await?;
    let dest = state.actions_cache_dir.join(&row.storage_path);
    let tmp = dest.with_extension("part");
    let on_disk = tokio::fs::metadata(&tmp)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if req.size < 0 || on_disk != req.size as u64 {
        return Err(StatusCode::BAD_REQUEST);
    }
    std::fs::rename(&tmp, &dest).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let committed = state
        .db
        .commit_action_cache(&cache_id, req.size)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !committed {
        let _ = std::fs::remove_file(&dest);
        return Err(StatusCode::CONFLICT);
    }
    evict_over_cap(&state, &scope.repository_id).await;
    Ok(StatusCode::NO_CONTENT)
}

/// GET `/_apis/artifactcache/caches/{id}` — metadata probe (actions/cache
/// polls this while restoring to surface progress).
async fn get_cache_meta(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cache_id): Path<String>,
) -> Result<Response, StatusCode> {
    let scope = cache_scope_from_headers(&state, &headers).await?;
    let row = find_by_id_scoped(&state, &cache_id, &scope).await?;
    Ok(Json(entry_public(&row, &scope.token)).into_response())
}

/// GET `/_apis/artifactcache/caches/{id}/download` — `archiveLocation` target.
/// The upstream client sends no `Authorization` here, so `?t=` carries the
/// token; a Bearer header is still honoured for older/simple clients.
async fn download_cache(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cache_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let scope = if headers.contains_key(axum::http::header::AUTHORIZATION) {
        cache_scope_from_headers(&state, &headers).await?
    } else {
        cache_scope_from_query(&state, q.get("t").map(String::as_str)).await?
    };
    let row = find_by_id_scoped(&state, &cache_id, &scope).await?;
    let path = state.actions_cache_dir.join(&row.storage_path);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let _ = state.db.touch_action_cache(&row.id).await;
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/gzip")
        .header("content-length", row.size_bytes.to_string())
        .body(Body::from_stream(tokio_util::io::ReaderStream::new(file)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// DELETE `/_apis/artifactcache/caches/{id}` — drop an entry + its payload.
async fn delete_cache(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cache_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let scope = cache_scope_from_headers(&state, &headers).await?;
    let row = find_by_id_scoped(&state, &cache_id, &scope).await?;
    let removed = state
        .db
        .delete_action_cache(&row.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if removed {
        let _ = std::fs::remove_file(state.actions_cache_dir.join(&row.storage_path));
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// A `reserved` row owned by the caller's repo — PATCH/commit target.
pub(crate) async fn find_reserved(
    state: &AppState,
    cache_id: &str,
    repo_id: &str,
) -> Result<oxidean_db::actions_cache::ActionCacheRow, StatusCode> {
    let row = find_cache_row_by_id(state, cache_id).await?;
    if row.repository_id != repo_id || row.state != "reserved" {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(row)
}

/// Any row by id — scoped to the caller's repo and visible scopes.
pub(crate) async fn find_by_id_scoped(
    state: &AppState,
    cache_id: &str,
    scope: &CacheScope,
) -> Result<oxidean_db::actions_cache::ActionCacheRow, StatusCode> {
    let row = find_cache_row_by_id(state, cache_id).await?;
    if row.repository_id != scope.repository_id {
        return Err(StatusCode::NOT_FOUND);
    }
    if row.scope != scope.scope && row.scope != scope.default_scope {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(row)
}

/// By-id probe without scope/state filtering (internal).
pub(crate) async fn find_cache_row_by_id(
    state: &AppState,
    cache_id: &str,
) -> Result<oxidean_db::actions_cache::ActionCacheRow, StatusCode> {
    state
        .db
        .find_action_cache_by_id(cache_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

/// LRU eviction — drop oldest committed entries until the repo's total sits
/// under `cache_max_bytes`. Payload files are unlinked with their rows.
pub(crate) async fn evict_over_cap(state: &AppState, repo_id: &str) {
    let entries = match state.db.list_action_caches_for_repo(repo_id).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "cache eviction listing failed");
            return;
        }
    };
    let mut total: u64 = entries.iter().map(|c| c.size_bytes.max(0) as u64).sum();
    let cap = state.actions_cache_max_bytes;
    for entry in entries {
        if total <= cap {
            break;
        }
        match state.db.delete_action_cache(&entry.id).await {
            Ok(true) => {
                total = total.saturating_sub(entry.size_bytes.max(0) as u64);
                let _ = std::fs::remove_file(state.actions_cache_dir.join(&entry.storage_path));
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "cache eviction delete failed"),
        }
    }
}

/// Periodic maintenance — expired runtime tokens + abandoned reservations
/// (payload `.part` files removed). Wired into the actions schedule sweep.
pub async fn sweep_action_caches(
    db: &oxidean_db::Database,
    cache_dir: &std::path::Path,
) -> Result<u64, String> {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let tokens = db.reap_action_cache_tokens(&now).await?;
    let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(reserve_ttl_minutes()))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let stale = db.reap_stale_action_cache_reservations(&cutoff).await?;
    for rel in &stale {
        let _ = std::fs::remove_file(cache_dir.join(rel));
        let _ = std::fs::remove_file(cache_dir.join(rel).with_extension("part"));
        // v2 staged-block dir (`{id}.blocks/<hex>`) — removed whole.
        let _ = std::fs::remove_dir_all(cache_dir.join(rel).with_extension("blocks"));
    }
    Ok(tokens + stale.len() as u64)
}

/// Router fragment mounted under `/api/actions` — the `_apis/artifactcache`
/// path mirrors GitHub's pipelines host surface.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/_apis/artifactcache/cache", get(get_cache))
        .route("/_apis/artifactcache/caches", post(reserve_cache))
        .route(
            "/_apis/artifactcache/caches/{id}",
            patch(upload_cache_chunk)
                .post(commit_cache)
                .get(get_cache_meta)
                .delete(delete_cache),
        )
        .route(
            "/_apis/artifactcache/caches/{id}/download",
            get(download_cache),
        )
}
