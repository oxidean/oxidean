//! GitHub REST compatibility layer for third-party actions (#160).
//!
//! Behind `OXIDEAN_ACTIONS_REST_SHIM` the `/api/v1` surface switches from
//! Oxidean DTO wire shapes to GitHub-exact envelopes so Octokit /
//! `@actions/github` consumers (labeler, stale, github-script, release
//! actions) deserialize correctly:
//!
//! - list responses `{items, total}` unwrap to bare arrays with `Link`
//!   pagination headers (`per_page`/`page` ↔ `limit`/`offset`, cap 100);
//! - payloads gain GitHub keys (`html_url`, `user.login`, `node_id`,
//!   `labels[]` objects, `permissions`, …) — fields without an Oxidean
//!   analogue return GitHub-shaped placeholders rather than dropping keys;
//! - errors render `{message, documentation_url, errors[]}`;
//! - `X-RateLimit-*` headers ride every response; `ETag`/`If-None-Match`
//!   give conditional reads on single-resource GETs;
//! - meta endpoints `GET /`, `GET /rate_limit`, `GET /zen` mount under the
//!   flag, plus `GET /_compat/stats` (admin) reporting observed-vs-covered
//!   route counts for the action-compat diagnostics report.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

use axum::extract::{OriginalUri, Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use serde_json::{json, Value};
use url::form_urlencoded;

use oxidean_api_core::actions::job_tokens::JOB_TOKEN_PREFIX;
use oxidean_api_core::app::AppState;
use oxidean_api_core::auth::admin::require_admin;
use oxidean_core::AppError;

use crate::{ctx_for, status_for_error};

/// `per_page` hard cap (GitHub: 100).
const PER_PAGE_MAX: i64 = 100;
const PER_PAGE_DEFAULT: i64 = 30;
/// Static rate-limit window advertised to API clients (no REST quota today).
const RATE_LIMIT: i64 = 5000;
/// Pseudo-header stashing the full external URI for `Link` construction.
const URI_HEADER: &str = "x-oxidean-request-uri";

/// Observed-vs-covered counters keyed by route template — feeds the
/// action-compat diagnostics report (`/_compat/stats`).
static COMPAT_STATS: LazyLock<Mutex<BTreeMap<String, (u64, u64)>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub fn record_observed(route: &str, covered: bool) {
    let mut m = COMPAT_STATS.lock().unwrap_or_else(|p| p.into_inner());
    let e = m.entry(route.to_string()).or_default();
    e.0 += 1;
    if covered {
        e.1 += 1;
    }
}

/// Request carried a job-token (`oxidean_job_*`) Bearer — used for headers.
pub fn job_token_request(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|a| a.strip_prefix("Bearer "))
        .map(|t| t.starts_with(JOB_TOKEN_PREFIX))
        .unwrap_or(false)
}

/// Public base URL for `html_url`/`url` fields — the configured instance
/// origin, falling back to the request's `Host` when unset.
fn origin(headers: &HeaderMap) -> String {
    let configured = oxidean_api_core::public_origin::resolve_public_origin();
    if !configured.is_empty() {
        return configured.trim_end_matches('/').to_string();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    format!("http://{host}")
}

// ---------------------------------------------------------------------------
// Query translation middleware — `per_page`/`page` → `limit`/`offset`
// ---------------------------------------------------------------------------

/// Layer applied once for the whole `/api/v1` router: rewrites GitHub
/// pagination params into the RPC `limit`/`offset` convention and stashes the
/// full request URI (nested routers see the stripped path) for `Link`
/// headers — handlers never see the pseudo-header.
pub async fn githubify(mut req: Request, next: Next) -> Response {
    let orig_uri = req.uri().clone();
    let uri = req.uri().clone();
    if let Some(qs) = uri.query() {
        if let Some(new_q) = rewrite_github_query(qs) {
            let path = uri.path().to_string();
            let mut parts = uri.into_parts();
            parts.path_and_query = format!("{path}?{new_q}").parse().ok();
            if let Ok(new_uri) = Uri::from_parts(parts) {
                *req.uri_mut() = new_uri;
            }
        }
    }
    // Stash = full external path (`OriginalUri`, since `.nest` strips
    // `/api/v1` from `req.uri()`) + the *rewritten* query, so `emit`'s Link
    // math sees `limit`/`offset`, not the client's `page`/`per_page`.
    let full = req
        .extensions()
        .get::<OriginalUri>()
        .map(|o| o.0.clone())
        .unwrap_or(orig_uri);
    let combined = format!(
        "{}{}",
        full.path(),
        req.uri()
            .query()
            .map(|q| format!("?{q}"))
            .unwrap_or_default()
    );
    if let Ok(v) = HeaderValue::from_str(&combined) {
        req.headers_mut().insert(URI_HEADER, v);
    }
    next.run(req).await
}

/// `per_page`/`page` → `limit`/`offset` (cap 100, `page` is 1-based).
/// `None` when the query carries neither GitHub param.
fn rewrite_github_query(qs: &str) -> Option<String> {
    let mut pairs: Vec<(String, String)> = form_urlencoded::parse(qs.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let per_page = take_int(&pairs, "per_page").map(|n| n.clamp(1, PER_PAGE_MAX));
    let page = take_int(&pairs, "page").map(|n| n.max(1));
    if per_page.is_none() && page.is_none() {
        return None;
    }
    let limit = per_page.unwrap_or(PER_PAGE_DEFAULT);
    let offset = page.map(|p| (p - 1) * limit).unwrap_or(0);
    pairs.retain(|(k, _)| k != "per_page" && k != "page" && k != "offset");
    pairs.push(("limit".into(), limit.to_string()));
    pairs.push(("offset".into(), offset.to_string()));
    Some(
        form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .finish(),
    )
}

fn take_int(pairs: &[(String, String)], key: &str) -> Option<i64> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.parse::<i64>().ok())
}

fn request_uri(headers: &HeaderMap) -> String {
    headers
        .get(URI_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

// ---------------------------------------------------------------------------
// Response post-processing layer — catches handlers that bypass `call`/
// `call_map` (direct `dispatch` + hand-built `Json` returns, e.g. git-refs).
// `emit`-produced responses carry `PROCESSED_HEADER` and pass through
// untouched; everything else gets the GitHub envelope, headers, ETag, and
// compat-stats coverage recorded by route template.
// ---------------------------------------------------------------------------

/// Private marker set by `emit` so the layer knows a response is already
/// shaped/counted. Stripped before the response leaves the server.
const PROCESSED_HEADER: &str = "x-oxidean-compat-processed";

fn mark_processed(res: &mut Response) {
    res.headers_mut()
        .insert(PROCESSED_HEADER, HeaderValue::from_static("1"));
}

pub async fn compat_responses(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let is_get = method == Method::GET;
    let if_none_match = req.headers().get(header::IF_NONE_MATCH).cloned();
    let route_key = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|m| format!("{method} {}", m.as_str()));
    let headers = req.headers().clone();
    let res = next.run(req).await;

    if res.headers().contains_key(PROCESSED_HEADER) {
        let mut res = res;
        res.headers_mut().remove(PROCESSED_HEADER);
        return res;
    }
    let mut res = res;

    // Coverage: matched spec routes count as covered; the `/{*rest}` wildcard
    // is the unmatched fallback — its handler records the literal path itself.
    if let Some(rk) = route_key.as_deref() {
        if !rk.ends_with("/{*rest}") {
            record_observed(rk, true);
        }
    }

    if res.status().is_client_error() || res.status().is_server_error() {
        let has_envelope = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.contains("json"))
            .unwrap_or(false)
            && {
                let (parts, body) = res.into_parts();
                let bytes = axum::body::to_bytes(body, 64 * 1024)
                    .await
                    .unwrap_or_default();
                let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                res = Response::from_parts(parts, axum::body::Body::from(bytes));
                v.get("documentation_url").is_some()
            };
        if !has_envelope {
            let (mut parts, body) = res.into_parts();
            let bytes = axum::body::to_bytes(body, 256 * 1024)
                .await
                .unwrap_or_default();
            let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            let msg = v
                .get("message")
                .or_else(|| v.get("error"))
                .and_then(|m| m.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    parts
                        .status
                        .canonical_reason()
                        .unwrap_or("Error")
                        .to_string()
                });
            let code = v
                .get("code")
                .and_then(|c| c.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| "internal".into());
            // Keep original headers (www-authenticate et al.) — only the
            // body/content-length get replaced.
            parts.headers.remove(header::CONTENT_LENGTH);
            let env = serde_json::to_vec(&github_error_json(&AppError::new(&code, &msg), &headers))
                .unwrap_or_else(|_| b"{}".to_vec());
            parts.headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            res = Response::from_parts(parts, axum::body::Body::from(env));
        }
    } else if is_get && res.status().is_success() && !res.headers().contains_key(header::ETAG) {
        let is_json = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.contains("json"))
            .unwrap_or(false);
        // 4 MB ceiling — bigger payloads pass through un-ETagged rather
        // than being emptied by an over-limit `to_bytes` error.
        let too_big = res
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .map(|n| n > 4 * 1024 * 1024)
            .unwrap_or(false);
        if is_json && !too_big {
            let (parts, body) = res.into_parts();
            let bytes = axum::body::to_bytes(body, 4 * 1024 * 1024)
                .await
                .unwrap_or_default();
            if let Some(tag) = weak_etag(&String::from_utf8_lossy(&bytes)) {
                let hit = if_none_match
                    .as_ref()
                    .and_then(|v| v.to_str().ok())
                    .map(|im| {
                        let tag_str = tag.to_str().unwrap_or("");
                        im.split(',')
                            .any(|t| t.trim() == tag_str || t.trim() == "*")
                    })
                    .unwrap_or(false);
                if hit {
                    let mut r304 = StatusCode::NOT_MODIFIED.into_response();
                    rate_limit_headers(&mut r304);
                    return r304;
                }
                res = Response::from_parts(parts, axum::body::Body::from(bytes));
                res.headers_mut().insert(header::ETAG, tag);
            } else {
                res = Response::from_parts(parts, axum::body::Body::from(bytes));
            }
        }
    }
    rate_limit_headers(&mut res);
    res
}

// ---------------------------------------------------------------------------
// Error envelope — {message, documentation_url, errors[]}
// ---------------------------------------------------------------------------

pub fn github_error_json(err: &AppError, headers: &HeaderMap) -> Value {
    json!({
        "message": err.message,
        "documentation_url": format!("{}/docs", origin(headers)),
        "errors": err.data.clone().map(|d| vec![d]).unwrap_or_default(),
        "code": err.code,
    })
}

// ---------------------------------------------------------------------------
// Headers — Link / ETag / RateLimit
// ---------------------------------------------------------------------------

/// `Link` rel=next/last from a known `total`, `limit`, `offset`, preserving
/// the request's other query params. URLs carry GitHub `per_page`/`page`
/// params — the middleware translates them back when a client follows a rel.
fn link_header(uri: &str, total: i64, limit: i64, offset: i64) -> Option<HeaderValue> {
    if limit <= 0 || total <= offset + limit {
        return None;
    }
    let base = uri.split('?').next().unwrap_or(uri);
    let keep: Vec<(String, String)> = uri
        .split('?')
        .nth(1)
        .map(|q| {
            form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .filter(|(k, _)| k != "limit" && k != "offset" && k != "page" && k != "per_page")
                .collect()
        })
        .unwrap_or_default();
    let url = |off: i64| {
        let mut all = keep.clone();
        all.push(("per_page".into(), limit.to_string()));
        all.push(("page".into(), (off / limit + 1).to_string()));
        let q = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(all.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .finish();
        format!("{base}?{q}")
    };
    let last = ((total - 1) / limit) * limit;
    HeaderValue::from_str(&format!(
        "<{}>; rel=\"next\", <{}>; rel=\"last\"",
        url(offset + limit),
        url(last)
    ))
    .ok()
}

fn rate_limit_headers(res: &mut Response) {
    let h = res.headers_mut();
    h.insert("x-ratelimit-limit", HeaderValue::from(RATE_LIMIT));
    h.insert("x-ratelimit-remaining", HeaderValue::from(RATE_LIMIT - 1));
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() + 3600)
        .unwrap_or(0);
    if let Ok(v) = HeaderValue::from_str(&reset.to_string()) {
        h.insert("x-ratelimit-reset", v);
    }
    h.insert("x-ratelimit-resource", HeaderValue::from_static("core"));
    h.insert("x-ratelimit-used", HeaderValue::from_static("1"));
}

fn weak_etag(body: &str) -> Option<HeaderValue> {
    let mut h: u128 = 0xcbf29ce484222325;
    for &x in body.as_bytes() {
        h = (h ^ x as u128).wrapping_mul(0x100000001b3);
    }
    HeaderValue::from_str(&format!("W/\"{h:x}\"")).ok()
}

// ---------------------------------------------------------------------------
// Shape transforms — Oxidean DTO JSON → GitHub-exact JSON
// ---------------------------------------------------------------------------

fn user_ref(v: &Value, o: &str) -> Value {
    let username = v
        .get("username")
        .or_else(|| v.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("ghost");
    let id = v
        .get("id")
        .or_else(|| v.get("user_id"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    json!({
        "login": username,
        "id": id.clone().map(Value::from).unwrap_or(Value::Null),
        "node_id": id.map(|i| format!("U_{i}")).map(Value::from).unwrap_or(Value::Null),
        "avatar_url": v.get("avatar_url").cloned().unwrap_or(Value::Null),
        "gravatar_id": "",
        "url": format!("{o}/api/v1/users/{username}"),
        "html_url": format!("{o}/{username}"),
        "type": "User",
        "site_admin": false,
    })
}

fn label_ref(v: &Value, o: &str) -> Value {
    let name = v.get("name").and_then(Value::as_str).unwrap_or("");
    json!({
        "id": v.get("id").cloned().unwrap_or(Value::Null),
        "node_id": format!("L_{}", v.get("id").and_then(Value::as_str).unwrap_or("")),
        "url": format!("{o}/api/v1/labels/{name}"),
        "name": name,
        "color": v.get("color").cloned().unwrap_or_else(|| json!("ededed")),
        "default": false,
        "description": v.get("description").cloned().unwrap_or(Value::Null),
    })
}

fn milestone_ref(v: &Value, o: &str, owner: &str, repo: &str) -> Value {
    let number = v.get("number").cloned().unwrap_or(json!(0));
    json!({
        "id": v.get("id").cloned().unwrap_or(Value::Null),
        "node_id": format!("M_{}", v.get("id").and_then(Value::as_str).unwrap_or("")),
        "number": number,
        "state": v.get("state").cloned().unwrap_or(json!("open")),
        "title": v.get("title").cloned().unwrap_or(Value::Null),
        "description": v.get("description").cloned().unwrap_or(Value::Null),
        "creator": v.get("creator_username").map(|u| user_ref(&json!({"username": u}), o)),
        "open_issues": v.get("open_issues").cloned().unwrap_or(json!(0)),
        "closed_issues": v.get("closed_issues").cloned().unwrap_or(json!(0)),
        "created_at": v.get("created_at").cloned().unwrap_or(Value::Null),
        "updated_at": v.get("updated_at").cloned().unwrap_or(Value::Null),
        "closed_at": v.get("closed_at").cloned().unwrap_or(Value::Null),
        "due_on": v.get("due_on").cloned().unwrap_or(Value::Null),
        "html_url": format!("{o}/{owner}/{repo}/milestone/{number}"),
        "url": format!("{o}/api/v1/repos/{owner}/{repo}/milestones/{number}"),
        "labels_url": format!("{o}/api/v1/repos/{owner}/{repo}/milestones/{number}/labels"),
    })
}

fn repo_ref_gh(v: &Value, o: &str) -> Value {
    let owner = v
        .get("owner_username")
        .and_then(Value::as_str)
        .unwrap_or("");
    let name = v.get("name").and_then(Value::as_str).unwrap_or("");
    let full = format!("{owner}/{name}");
    let host = o
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    json!({
        "id": v.get("id").cloned().unwrap_or(Value::Null),
        "node_id": format!("R_{}", v.get("id").and_then(Value::as_str).unwrap_or("")),
        "name": name,
        "full_name": full,
        "private": v.get("visibility").and_then(Value::as_str) == Some("private"),
        "owner": user_ref(&json!({"username": owner, "id": v.get("owner_id")}), o),
        "html_url": format!("{o}/{full}"),
        "description": v.get("description").cloned().unwrap_or(Value::Null),
        "fork": v.get("is_fork").cloned().unwrap_or(json!(false)),
        "url": format!("{o}/api/v1/repos/{full}"),
        "created_at": v.get("updated_at").cloned().unwrap_or(Value::Null),
        "updated_at": v.get("updated_at").cloned().unwrap_or(Value::Null),
        "pushed_at": v.get("updated_at").cloned().unwrap_or(Value::Null),
        "git_url": format!("git://{host}/{full}.git"),
        "ssh_url": format!("git@{host}:{full}.git"),
        "clone_url": format!("{o}/{full}.git"),
        "homepage": v.get("homepage").cloned().unwrap_or(Value::Null),
        "stargazers_count": v.get("star_count").cloned().unwrap_or(json!(0)),
        "watchers_count": v.get("watch_count").cloned().unwrap_or(json!(0)),
        "forks_count": v.get("fork_count").cloned().unwrap_or(json!(0)),
        "open_issues_count": v.get("open_issue_count").cloned().unwrap_or(json!(0)),
        "default_branch": v.get("default_branch").cloned().unwrap_or(json!("main")),
        "archived": v.get("archived").cloned().unwrap_or(json!(false)),
        "disabled": false,
        "visibility": v.get("visibility").cloned().unwrap_or(json!("public")),
        "topics": v.get("topics").cloned().unwrap_or_else(|| json!([])),
        "is_template": v.get("is_template").cloned().unwrap_or(json!(false)),
        "permissions": {
            "admin": v.get("can_admin").cloned().unwrap_or(json!(false)),
            "push": v.get("can_write").cloned().unwrap_or(json!(false)),
            "pull": true,
            "maintain": v.get("can_admin").cloned().unwrap_or(json!(false)),
            "triage": v.get("can_write").cloned().unwrap_or(json!(false)),
        },
    })
}

fn issue_ref_gh(v: &Value, o: &str, owner: &str, repo: &str) -> Value {
    let number = v.get("number").cloned().unwrap_or(json!(0));
    let api_base = format!("{o}/api/v1/repos/{owner}/{repo}");
    json!({
        "id": v.get("id").cloned().unwrap_or(Value::Null),
        "node_id": format!("I_{}", v.get("id").and_then(Value::as_str).unwrap_or("")),
        "url": format!("{api_base}/issues/{number}"),
        "repository_url": api_base,
        "labels_url": format!("{api_base}/issues/{number}/labels{{/name}}"),
        "comments_url": format!("{api_base}/issues/{number}/comments"),
        "events_url": format!("{api_base}/issues/{number}/events"),
        "html_url": format!("{o}/{owner}/{repo}/issues/{number}"),
        "number": number,
        "state": v.get("state").and_then(Value::as_str).unwrap_or("open"),
        "title": v.get("title").cloned().unwrap_or(Value::Null),
        "body": v.get("body").cloned().unwrap_or(Value::Null),
        "user": user_ref(&json!({"username": v.get("author_username"), "id": v.get("author_id")}), o),
        "labels": v.get("labels").and_then(Value::as_array).map(|a| a.iter().map(|l| label_ref(l, o)).collect::<Vec<_>>()).unwrap_or_default(),
        "assignee": v.get("assignees").and_then(Value::as_array).and_then(|a| a.first()).map(|u| user_ref(u, o)),
        "assignees": v.get("assignees").and_then(Value::as_array).map(|a| a.iter().map(|u| user_ref(u, o)).collect::<Vec<_>>()).unwrap_or_default(),
        "milestone": v.get("milestone").map(|m| milestone_ref(m, o, owner, repo)),
        "locked": false,
        "comments": v.get("comment_count").cloned().unwrap_or(json!(0)),
        "closed_at": v.get("closed_at").cloned().unwrap_or(Value::Null),
        "created_at": v.get("created_at").cloned().unwrap_or(Value::Null),
        "updated_at": v.get("updated_at").cloned().unwrap_or(Value::Null),
        "author_association": "MEMBER",
        "state_reason": Value::Null,
    })
}

fn pull_ref_gh(v: &Value, o: &str, owner: &str, repo: &str) -> Value {
    let number = v.get("number").cloned().unwrap_or(json!(0));
    let api_base = format!("{o}/api/v1/repos/{owner}/{repo}");
    let head_owner = v.get("head_owner").and_then(Value::as_str).unwrap_or(owner);
    let head_name = v.get("head_name").and_then(Value::as_str).unwrap_or(repo);
    let merged = v.get("merged_at").map(|m| !m.is_null()).unwrap_or(false);
    let state = if merged {
        "closed"
    } else {
        v.get("state").and_then(Value::as_str).unwrap_or("open")
    };
    json!({
        "id": v.get("id").cloned().unwrap_or(Value::Null),
        "node_id": format!("PR_{}", v.get("id").and_then(Value::as_str).unwrap_or("")),
        "url": format!("{api_base}/pulls/{number}"),
        "html_url": format!("{o}/{owner}/{repo}/pull/{number}"),
        "diff_url": format!("{o}/{owner}/{repo}/pull/{number}.diff"),
        "patch_url": format!("{o}/{owner}/{repo}/pull/{number}.patch"),
        "issue_url": format!("{api_base}/issues/{number}"),
        "number": number,
        "state": state,
        "locked": false,
        "title": v.get("title").cloned().unwrap_or(Value::Null),
        "user": user_ref(&json!({"username": v.get("author_username"), "id": v.get("author_id")}), o),
        "body": v.get("body").cloned().unwrap_or(Value::Null),
        "created_at": v.get("created_at").cloned().unwrap_or(Value::Null),
        "updated_at": v.get("updated_at").cloned().unwrap_or(Value::Null),
        "closed_at": v.get("closed_at").cloned().unwrap_or(Value::Null),
        "merged_at": v.get("merged_at").cloned().unwrap_or(Value::Null),
        "merge_commit_sha": v.get("merge_commit_sha").cloned().unwrap_or(Value::Null),
        "assignee": v.get("assignees").and_then(Value::as_array).and_then(|a| a.first()).map(|u| user_ref(u, o)),
        "assignees": v.get("assignees").and_then(Value::as_array).map(|a| a.iter().map(|u| user_ref(u, o)).collect::<Vec<_>>()).unwrap_or_default(),
        "requested_reviewers": [],
        "draft": v.get("draft").cloned().unwrap_or(json!(false)),
        "merged": merged,
        "mergeable": Value::Null,
        "rebaseable": Value::Null,
        "comments": v.get("comment_count").cloned().unwrap_or(json!(0)),
        "head": {
            "label": format!("{head_owner}:{}", v.get("head_ref").and_then(Value::as_str).unwrap_or("")),
            "ref": v.get("head_ref").cloned().unwrap_or(Value::Null),
            "sha": v.get("head_sha").cloned().unwrap_or(Value::Null),
            "repo": { "owner": user_ref(&json!({"username": head_owner}), o), "name": head_name, "full_name": format!("{head_owner}/{head_name}") },
            "user": user_ref(&json!({"username": head_owner}), o),
        },
        "base": {
            "label": format!("{owner}:{}", v.get("base_ref").and_then(Value::as_str).unwrap_or("")),
            "ref": v.get("base_ref").cloned().unwrap_or(Value::Null),
            "sha": v.get("base_sha").cloned().unwrap_or(Value::Null),
            "repo": Value::Null,
            "user": user_ref(&json!({"username": owner}), o),
        },
        "author_association": "MEMBER",
        "auto_merge": Value::Null,
    })
}

fn commit_ref_gh(v: &Value, o: &str, owner: &str, repo: &str) -> Value {
    let sha = v.get("sha").and_then(Value::as_str).unwrap_or("");
    let subject = v.get("subject").and_then(Value::as_str).unwrap_or("");
    let body = v.get("body").and_then(Value::as_str).unwrap_or("");
    let message = if body.is_empty() {
        subject.to_string()
    } else {
        format!("{subject}\n\n{body}")
    };
    let sig = json!({
        "name": v.get("author_name").cloned().unwrap_or(Value::Null),
        "email": v.get("author_email").cloned().unwrap_or(Value::Null),
        "date": v.get("authored_at").cloned().unwrap_or(Value::Null),
    });
    let author = user_ref(
        &json!({"username": v.get("author_username"), "id": v.get("author_user_id"), "avatar_url": v.get("author_avatar_url")}),
        o,
    );
    let parents = v
        .get("parents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .map(|p| json!({"sha": p, "url": format!("{o}/api/v1/repos/{owner}/{repo}/commits/{p}"), "html_url": format!("{o}/{owner}/{repo}/commit/{p}")}))
        .collect::<Vec<_>>();
    let files = v
        .get("files")
        .and_then(Value::as_array)
        .map(|fs| {
            Value::Array(
                fs.iter()
                    .map(|f| {
                        json!({
                            "filename": f.get("path").cloned().unwrap_or(Value::Null),
                            "status": f.get("status").cloned().unwrap_or(Value::Null),
                            "patch": f.get("patch").cloned().unwrap_or(Value::Null),
                        })
                    })
                    .collect(),
            )
        })
        .unwrap_or(Value::Null);
    let verified = v.get("signature_status").and_then(Value::as_str) == Some("valid");
    json!({
        "sha": sha,
        "node_id": format!("C_{sha}"),
        "commit": {
            "author": sig,
            "committer": sig,
            "message": message,
            "tree": {"sha": sha, "url": format!("{o}/api/v1/repos/{owner}/{repo}/git/trees/{sha}")},
            "url": format!("{o}/api/v1/repos/{owner}/{repo}/git/commits/{sha}"),
            "comment_count": 0,
            "verification": {"verified": verified, "reason": v.get("signature_status").cloned().unwrap_or(Value::Null), "signature": Value::Null, "payload": Value::Null},
        },
        "url": format!("{o}/api/v1/repos/{owner}/{repo}/commits/{sha}"),
        "html_url": format!("{o}/{owner}/{repo}/commit/{sha}"),
        "comments_url": format!("{o}/api/v1/repos/{owner}/{repo}/commits/{sha}/comments"),
        "author": author,
        "committer": author,
        "parents": parents,
        "files": files,
    })
}

fn comment_ref_gh(v: &Value, o: &str, owner: &str, repo: &str, issue_number: Option<i64>) -> Value {
    let id = v.get("id").and_then(Value::as_str).unwrap_or("");
    let number = v
        .get("issue_number")
        .or_else(|| v.get("number"))
        .and_then(Value::as_i64)
        .or(issue_number);
    json!({
        "id": v.get("id").cloned().unwrap_or(Value::Null),
        "node_id": format!("IC_{id}"),
        "url": format!("{o}/api/v1/repos/{owner}/{repo}/issues/comments/{id}"),
        "html_url": number.map(|n| json!(format!("{o}/{owner}/{repo}/issues/{n}#issuecomment-{id}"))).unwrap_or(Value::Null),
        "issue_url": number.map(|n| json!(format!("{o}/api/v1/repos/{owner}/{repo}/issues/{n}"))).unwrap_or(Value::Null),
        "body": v.get("body").cloned().unwrap_or(Value::Null),
        "user": user_ref(&json!({"username": v.get("author_username"), "id": v.get("author_id")}), o),
        "created_at": v.get("created_at").cloned().unwrap_or(Value::Null),
        "updated_at": v.get("updated_at").cloned().unwrap_or(Value::Null),
        "author_association": "MEMBER",
        "performed_via_github_app": Value::Null,
    })
}

fn asset_ref_gh(v: &Value, o: &str) -> Value {
    let id = v.get("id").and_then(Value::as_str).unwrap_or("");
    json!({
        "id": id,
        "node_id": format!("RA_{id}"),
        "url": format!("{o}/api/releases/assets/{id}"),
        "browser_download_url": format!("{o}/api/releases/assets/{id}"),
        "name": v.get("filename").cloned().unwrap_or(Value::Null),
        "label": Value::Null,
        "state": "uploaded",
        "content_type": v.get("content_type").cloned().unwrap_or(json!("application/octet-stream")),
        "size": v.get("byte_size").cloned().unwrap_or(json!(0)),
        "download_count": 0,
        "created_at": v.get("created_at").cloned().unwrap_or(Value::Null),
        "updated_at": v.get("updated_at").cloned().unwrap_or(Value::Null),
        "uploader": user_ref(&json!({"id": v.get("uploader_id")}), o),
    })
}

fn release_ref_gh(v: &Value, o: &str, owner: &str, repo: &str) -> Value {
    let id = v.get("id").and_then(Value::as_str).unwrap_or("");
    let tag = v.get("tag_name").and_then(Value::as_str).unwrap_or("");
    let api_base = format!("{o}/api/v1/repos/{owner}/{repo}");
    json!({
        "id": id,
        "node_id": format!("RE_{id}"),
        "url": format!("{api_base}/releases/{id}"),
        "html_url": format!("{o}/{owner}/{repo}/releases/tag/{tag}"),
        "assets_url": format!("{api_base}/releases/{id}/assets"),
        "upload_url": format!("{o}/api/repos/{owner}/{repo}/releases/{id}/assets{{?name,label}}"),
        "tarball_url": format!("{api_base}/tarball/{tag}"),
        "zipball_url": format!("{api_base}/zipball/{tag}"),
        "tag_name": tag,
        "target_commitish": Value::Null,
        "name": v.get("title").cloned().unwrap_or(Value::Null),
        "body": v.get("body").cloned().unwrap_or(Value::Null),
        "draft": v.get("draft").cloned().unwrap_or(json!(false)),
        "prerelease": v.get("prerelease").cloned().unwrap_or(json!(false)),
        "created_at": v.get("created_at").cloned().unwrap_or(Value::Null),
        "published_at": v.get("updated_at").cloned().unwrap_or(Value::Null),
        "author": user_ref(&json!({"username": v.get("author_username"), "id": v.get("author_id")}), o),
        "assets": v.get("assets").and_then(Value::as_array).map(|a| a.iter().map(|x| asset_ref_gh(x, o)).collect::<Vec<_>>()).unwrap_or_default(),
    })
}

/// `issue.history` revisions → GitHub `/issues/{n}/timeline` items. Oxidean
/// records title/body snapshots rather than a labelled-event feed, so each
/// entry is the closest supported verb — `edited` with the snapshot's
/// title/body — and the route is marked partial coverage in the compat doc.
fn timeline_ref_gh(v: &Value, o: &str) -> Value {
    let actor = v
        .get("editor_username")
        .or_else(|| v.get("author_username"));
    json!({
        "id": v.get("id").cloned().unwrap_or(Value::Null),
        "node_id": format!("TL_{}", v.get("id").and_then(Value::as_str).unwrap_or("")),
        "event": "edited",
        "created_at": v.get("created_at").cloned().unwrap_or(Value::Null),
        "actor": actor.map(|u| user_ref(&json!({"username": u}), o)),
        "rename": { "to": v.get("title").cloned().unwrap_or(Value::Null) },
        "body": v.get("body").cloned().unwrap_or(Value::Null),
    })
}

/// Search responses → `{total_count, incomplete_results, items}`.
fn search_gh(v: &Value, items_key: &str, elem: impl Fn(&Value) -> Value) -> Value {
    let items = v
        .get(items_key)
        .or_else(|| v.get("items"))
        .and_then(Value::as_array)
        .map(|a| a.iter().map(elem).collect::<Vec<_>>())
        .unwrap_or_default();
    let total = v
        .get("total")
        .or_else(|| v.get("total_count"))
        .and_then(Value::as_i64)
        .unwrap_or(items.len() as i64);
    json!({
        "total_count": total,
        "incomplete_results": false,
        "items": items,
    })
}

// ---------------------------------------------------------------------------
// Dispatch — procedure name → transform
// ---------------------------------------------------------------------------

fn list_total(v: &Value, key: &str) -> (Vec<Value>, Option<i64>) {
    let items = v
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| v.as_array().cloned())
        .unwrap_or_default();
    (items, v.get("total").and_then(Value::as_i64))
}

/// Marker keys the shaped data must carry for the single-object transform to
/// apply — `call_map` projections (e.g. `{names}` for `/topics`, reaction
/// aggregates) lack them and fall through untouched instead of being mangled
/// into a partial DTO.
fn shape_keys(procedure: &str) -> Option<&'static [&'static str]> {
    Some(match procedure {
        "issue.get"
        | "issue.create"
        | "issue.update"
        | "issue.close"
        | "issue.reopen"
        | "issue.labels.set"
        | "issue.assignees.set"
        | "issue.milestone.set"
        | "pull.get"
        | "pull.create"
        | "pull.update"
        | "pull.close"
        | "pull.reopen"
        | "milestone.get"
        | "milestone.create"
        | "milestone.update" => &["id", "number"],
        "repo.get" | "repo.create" | "repo.update" | "repo.transfer" => &["id", "owner_username"],
        "repo.commit" => &["sha", "subject"],
        "release.get" | "release.create" | "release.update" => &["id", "tag_name"],
        "label.get" | "label.create" | "label.update" | "repo.labels.create"
        | "repo.labels.update" | "org.labels.create" | "org.labels.update" => &["id", "name"],
        "issue.comments.create" | "issue.comments.update" | "user.get" | "user.public.get" => {
            &["id"]
        }
        _ => return None,
    })
}

/// Envelope key for list procedures — absent ⇒ the payload isn't the list
/// DTO (a `call_map` projection), so it passes through unchanged.
fn list_key(procedure: &str) -> Option<&'static str> {
    Some(match procedure {
        "issue.list" | "search.issues" => "issues",
        "issue.comments.list" | "issue.comments.history" | "pull.comments.list" => "comments",
        "issue.history" => "revisions",
        "pull.list" => "pulls",
        "repo.list" | "org.repos.list" | "repo.listMine" | "search.repositories" => "repos",
        "label.list" | "repo.labels.list" | "org.labels.list" => "labels",
        "milestone.list" => "milestones",
        "release.list" => "releases",
        "release.assets.list" => "assets",
        "repo.commits" => "commits",
        "search.users" => "users",
        _ => return None,
    })
}

fn dto(data: &Value, keys: &[&str]) -> bool {
    data.is_object() && keys.iter().all(|k| data.get(*k).is_some())
}

/// Applies the GitHub shape for `procedure` to `data`. `fields` is the merged
/// path/query identity (`{owner, name, …}`) already sent to the RPC layer.
/// Returns `(shaped_data, total_for_link)`.
pub fn shape(procedure: &str, data: Value, fields: &Value, origin: &str) -> (Value, Option<i64>) {
    let owner = fields.get("owner").and_then(Value::as_str).unwrap_or("");
    let repo = fields.get("name").and_then(Value::as_str).unwrap_or("");
    let issue_number = fields.get("number").and_then(Value::as_i64);
    if let Some(keys) = shape_keys(procedure) {
        if !dto(&data, keys) {
            return (data, None);
        }
    }
    if let Some(k) = list_key(procedure) {
        if data.get(k).is_none() {
            return (data, None);
        }
    }
    match procedure {
        "issue.list" => {
            let (items, t) = list_total(&data, "issues");
            (
                Value::Array(
                    items
                        .iter()
                        .map(|i| issue_ref_gh(i, origin, owner, repo))
                        .collect(),
                ),
                t,
            )
        }
        "issue.get"
        | "issue.create"
        | "issue.update"
        | "issue.close"
        | "issue.reopen"
        | "issue.labels.set"
        | "issue.assignees.set"
        | "issue.milestone.set" => (issue_ref_gh(&data, origin, owner, repo), None),
        "issue.comments.list" | "issue.comments.history" => {
            let (items, t) = list_total(&data, "comments");
            (
                Value::Array(
                    items
                        .iter()
                        .map(|c| comment_ref_gh(c, origin, owner, repo, issue_number))
                        .collect(),
                ),
                t,
            )
        }
        "issue.comments.create" | "issue.comments.update" => (
            comment_ref_gh(&data, origin, owner, repo, issue_number),
            None,
        ),
        "issue.history" => {
            let (items, t) = list_total(&data, "revisions");
            (
                Value::Array(items.iter().map(|e| timeline_ref_gh(e, origin)).collect()),
                t,
            )
        }
        "pull.list" => {
            let (items, t) = list_total(&data, "pulls");
            (
                Value::Array(
                    items
                        .iter()
                        .map(|p| pull_ref_gh(p, origin, owner, repo))
                        .collect(),
                ),
                t,
            )
        }
        // MergePullResponse `{pull, merge_commit_sha}` → GitHub's merge
        // result `{sha, merged, message}` (not the pull object itself).
        "pull.merge" => {
            if let Some(sha) = data.get("merge_commit_sha").and_then(Value::as_str) {
                (
                    json!({
                        "sha": sha,
                        "merged": true,
                        "message": "Pull Request successfully merged",
                    }),
                    None,
                )
            } else {
                (data, None)
            }
        }
        "pull.get" | "pull.create" | "pull.update" | "pull.close" | "pull.reopen" => {
            (pull_ref_gh(&data, origin, owner, repo), None)
        }
        "pull.comments.list" => {
            let (items, t) = list_total(&data, "comments");
            (
                Value::Array(
                    items
                        .iter()
                        .map(|c| comment_ref_gh(c, origin, owner, repo, issue_number))
                        .collect(),
                ),
                t,
            )
        }
        "repo.commits" => {
            let (items, t) = list_total(&data, "commits");
            (
                Value::Array(
                    items
                        .iter()
                        .map(|c| commit_ref_gh(c, origin, owner, repo))
                        .collect(),
                ),
                t,
            )
        }
        "repo.commit" => (commit_ref_gh(&data, origin, owner, repo), None),
        "repo.get" | "repo.create" | "repo.update" | "repo.transfer" => {
            (repo_ref_gh(&data, origin), None)
        }
        "repo.list" | "org.repos.list" | "repo.listMine" => {
            let (items, t) = list_total(&data, "repos");
            (
                Value::Array(items.iter().map(|r| repo_ref_gh(r, origin)).collect()),
                t,
            )
        }
        "label.list" | "repo.labels.list" | "org.labels.list" => {
            let (items, t) = list_total(&data, "labels");
            (
                Value::Array(items.iter().map(|l| label_ref(l, origin)).collect()),
                t,
            )
        }
        "label.get" | "label.create" | "label.update" | "repo.labels.create"
        | "repo.labels.update" | "org.labels.create" | "org.labels.update" => {
            (label_ref(&data, origin), None)
        }
        "milestone.list" => {
            let (items, t) = list_total(&data, "milestones");
            (
                Value::Array(
                    items
                        .iter()
                        .map(|m| milestone_ref(m, origin, owner, repo))
                        .collect(),
                ),
                t,
            )
        }
        "milestone.get" | "milestone.create" | "milestone.update" => {
            (milestone_ref(&data, origin, owner, repo), None)
        }
        "release.get" | "release.create" | "release.update" => {
            (release_ref_gh(&data, origin, owner, repo), None)
        }
        "release.list" => {
            let (items, t) = list_total(&data, "releases");
            (
                Value::Array(
                    items
                        .iter()
                        .map(|r| release_ref_gh(r, origin, owner, repo))
                        .collect(),
                ),
                t,
            )
        }
        "release.assets.list" => {
            let (items, t) = list_total(&data, "assets");
            (
                Value::Array(items.iter().map(|a| asset_ref_gh(a, origin)).collect()),
                t,
            )
        }
        "search.issues" => (
            search_gh(&data, "issues", |i| issue_ref_gh(i, origin, owner, repo)),
            None,
        ),
        "search.repositories" => (search_gh(&data, "repos", |r| repo_ref_gh(r, origin)), None),
        "search.users" => (search_gh(&data, "users", |u| user_ref(u, origin)), None),
        "user.get" | "user.public.get" => (user_ref(&data, origin), None),
        _ => (data, None),
    }
}

// ---------------------------------------------------------------------------
// Respond pipeline — replaces `respond` output when the flag is on
// ---------------------------------------------------------------------------

/// GitHub-mode response: shapes `data` for `procedure`, unwraps list totals
/// into `Link`, attaches `ETag`/`X-RateLimit-*`, renders errors in GitHub's
/// envelope. `fields` = the merged path/query identity sent to the RPC.
pub fn respond_compat(
    resp: oxidean_core::RpcResponse,
    ok_status: StatusCode,
    procedure: &str,
    fields: &Value,
    headers: &HeaderMap,
) -> Response {
    respond_compat_map(resp, ok_status, procedure, fields, headers, |d| d)
}

/// `respond_compat` + a `call_map`-style transform applied before shaping.
pub fn respond_compat_map(
    resp: oxidean_core::RpcResponse,
    ok_status: StatusCode,
    procedure: &str,
    fields: &Value,
    headers: &HeaderMap,
    f: impl FnOnce(Value) -> Value,
) -> Response {
    let o = origin(headers);
    let job_token = job_token_request(headers);
    match resp {
        oxidean_core::RpcResponse::Ok { data, .. } => {
            let (shaped, total) = shape(procedure, f(data), fields, &o);
            emit(
                shaped, total, ok_status, procedure, fields, headers, job_token,
            )
        }
        oxidean_core::RpcResponse::Err { error, .. } => {
            let status = status_for_error(&error);
            let mut res = (status, Json(github_error_json(&error, headers))).into_response();
            rate_limit_headers(&mut res);
            mark_processed(&mut res);
            res
        }
    }
}

/// Compat response when the handler already fetched the RPC `data` itself —
/// the id-addressed release routes resolve `release_id`/`latest` through
/// `release.list` before touching `call`.
pub fn respond_data_compat(
    data: Value,
    ok_status: StatusCode,
    procedure: &str,
    fields: &Value,
    headers: &HeaderMap,
) -> Response {
    let o = origin(headers);
    let (shaped, total) = shape(procedure, data, fields, &o);
    emit(
        shaped,
        total,
        ok_status,
        procedure,
        fields,
        headers,
        job_token_request(headers),
    )
}

/// GitHub-envelope error for handlers constructing their own failures
/// (`release.not_found` from the id-resolution helpers).
pub fn error_compat(err: &AppError, headers: &HeaderMap) -> Response {
    let mut res = (status_for_error(err), Json(github_error_json(err, headers))).into_response();
    rate_limit_headers(&mut res);
    mark_processed(&mut res);
    res
}

fn emit(
    shaped: Value,
    total: Option<i64>,
    ok_status: StatusCode,
    procedure: &str,
    fields: &Value,
    headers: &HeaderMap,
    job_token: bool,
) -> Response {
    let body = serde_json::to_string(&shaped).unwrap_or_else(|_| "null".into());
    let mut res = (ok_status, Json(shaped)).into_response();
    let h = res.headers_mut();
    if procedure.ends_with(".get") {
        if let Some(tag) = weak_etag(&body) {
            let tag_str = tag.to_str().unwrap_or("").to_string();
            let hit = headers
                .get(header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok())
                .map(|im| {
                    im.split(',')
                        .any(|t| t.trim() == tag_str || t.trim() == "*")
                })
                .unwrap_or(false);
            h.insert(header::ETAG, tag);
            if hit {
                let mut r304 = StatusCode::NOT_MODIFIED.into_response();
                rate_limit_headers(&mut r304);
                mark_processed(&mut r304);
                return r304;
            }
        }
    }
    if let Some(t) = total {
        let uri = request_uri(headers);
        let limit = take_int(
            &form_urlencoded::parse(uri.split('?').nth(1).unwrap_or("").as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect::<Vec<_>>(),
            "limit",
        )
        .unwrap_or(PER_PAGE_DEFAULT);
        let offset = take_int(
            &form_urlencoded::parse(uri.split('?').nth(1).unwrap_or("").as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect::<Vec<_>>(),
            "offset",
        )
        .unwrap_or(0);
        if let Some(link) = link_header(&uri, t, limit, offset) {
            h.insert(header::LINK, link);
        }
        if let Ok(v) = HeaderValue::from_str(&t.to_string()) {
            h.insert("x-total-count", v);
        }
    }
    let _ = fields;
    if job_token {
        h.insert("x-github-media-type", HeaderValue::from_static("github.v3"));
    }
    rate_limit_headers(&mut res);
    mark_processed(&mut res);
    res
}

// ---------------------------------------------------------------------------
// Meta + diagnostics routes (mounted when the flag is on)
// ---------------------------------------------------------------------------

pub fn compat_router() -> Router<AppState> {
    Router::new()
        .route("/", get(meta_root))
        .route("/rate_limit", get(rate_limit))
        .route("/zen", get(zen))
        .route("/_compat/stats", get(compat_stats))
        // Source-archive compat — GitHub's zipball/tarball redirect onto the
        // existing `serve_archive` plumbing (`{treeish}.zip` / `.tar.gz`).
        .route(
            "/repos/{owner}/{repo}/zipball",
            get(
                |h: HeaderMap, Path((o, r)): Path<(String, String)>| async move {
                    source_archive(h, o, r, "HEAD".into(), "zip").await
                },
            ),
        )
        .route(
            "/repos/{owner}/{repo}/zipball/{*treeish}",
            get(
                |h: HeaderMap, Path((o, r, t)): Path<(String, String, String)>| async move {
                    source_archive(h, o, r, t, "zip").await
                },
            ),
        )
        .route(
            "/repos/{owner}/{repo}/tarball",
            get(
                |h: HeaderMap, Path((o, r)): Path<(String, String)>| async move {
                    source_archive(h, o, r, "HEAD".into(), "tar.gz").await
                },
            ),
        )
        .route(
            "/repos/{owner}/{repo}/tarball/{*treeish}",
            get(
                |h: HeaderMap, Path((o, r, t)): Path<(String, String, String)>| async move {
                    source_archive(h, o, r, t, "tar.gz").await
                },
            ),
        )
        // Release-asset download compat — GitHub's
        // `GET /repos/{o}/{r}/releases/assets/{id}` redirects to the binary;
        // the native route is `GET /api/releases/assets/{id}`.
        .route(
            "/repos/{owner}/{repo}/releases/assets/{asset_id}",
            get(
                |Path((_o, _r, id)): Path<(String, String, String)>| async move {
                    axum::response::Redirect::to(&format!("/api/releases/assets/{id}"))
                },
            ),
        )
        // `/api/v1/` (trailing slash) lands on `/{*rest}` with an empty rest
        // under axum's `nest` — serve the meta doc; everything else unmatched
        // gets the GitHub 404 + coverage miss.
        .route("/{*rest}", axum::routing::any(root_or_unmatched))
}

async fn root_or_unmatched(
    State(state): State<AppState>,
    Path(rest): Path<String>,
    req: Request,
) -> Response {
    if rest.is_empty() {
        return meta_root(State(state), req.headers().clone()).await;
    }
    unmatched(req).await
}

/// `GET /repos/{o}/{r}/(zip|tar)ball[/{treeish}]` → 302 to the existing
/// `/api/repos/{o}/{r}/archive/{treeish}.{ext}` handler (same-origin redirect —
/// Octokit / `actions/checkout` / `curl -L` keep auth across it).
async fn source_archive(
    headers: HeaderMap,
    owner: String,
    repo: String,
    treeish: String,
    ext: &str,
) -> Response {
    record_observed(&format!("GET /repos/{owner}/{repo}/{ext}ball"), true);
    let target = format!(
        "{}/api/repos/{owner}/{repo}/archive/{treeish}.{ext}",
        origin(&headers)
    );
    let mut res = StatusCode::FOUND.into_response();
    if let Ok(v) = HeaderValue::from_str(&target) {
        res.headers_mut().insert(header::LOCATION, v);
    }
    rate_limit_headers(&mut res);
    res
}

/// Fallback for `/api/v1` requests no `RouteDef` covers — records the route as
/// observed-but-uncovered for the diagnostics report, then answers GitHub's
/// 404 envelope instead of the bare RPC `not_found`.
pub async fn unmatched(req: Request) -> Response {
    let method = req.method().to_string();
    let path = req
        .extensions()
        .get::<OriginalUri>()
        .map(|o| o.0.path().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    record_observed(&format!("{method} {path}"), false);
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "message": "Not Found",
            "documentation_url": "https://docs.github.com/rest",
        })),
    )
        .into_response()
}

/// `GET /api/v1/` — GitHub API root/meta. `pub` so the shell can also mount
/// the trailing-slash spelling (axum `nest` keeps `/api/v1/` outside the
/// nested router).
pub async fn meta_root(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if !s.rest_shim_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    let o = origin(&headers);
    Json(json!({
        "current_user_url": format!("{o}/api/v1/user"),
        "current_user_authorizations_html_url": format!("{o}/settings/tokens"),
        "emojis_url": format!("{o}/api/v1/emojis"),
        "events_url": format!("{o}/api/v1/events"),
        "feeds_url": format!("{o}/api/v1/feeds"),
        "notifications_url": format!("{o}/api/v1/notifications"),
        "organization_url": format!("{o}/api/v1/orgs/{{org}}"),
        "rate_limit_url": format!("{o}/api/v1/rate_limit"),
        "repository_url": format!("{o}/api/v1/repos/{{owner}}/{{repo}}"),
        "repository_search_url": format!("{o}/api/v1/search/repositories?q={{query}}{{&page,per_page,sort,order}}"),
        "user_url": format!("{o}/api/v1/users/{{user}}"),
        "user_search_url": format!("{o}/api/v1/search/users?q={{query}}{{&page,per_page,sort,order}}"),
    }))
    .into_response()
}

/// `GET /api/v1/rate_limit` — static quota block (no REST quota today).
async fn rate_limit() -> Response {
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() + 3600)
        .unwrap_or(0);
    let bucket = |res: &str| json!({ "limit": RATE_LIMIT, "used": 0, "remaining": RATE_LIMIT, "reset": reset, "resource": res });
    Json(json!({
        "resources": {
            "core": bucket("core"),
            "search": bucket("search"),
            "graphql": bucket("graphql"),
            "code_search": bucket("code_search"),
        },
        "rate": bucket("core"),
    }))
    .into_response()
}

/// `GET /api/v1/zen` — GitHub's zen of the day (static).
async fn zen() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain;charset=utf-8")],
        "Keep it simple.",
    )
}

/// `GET /api/v1/_compat/stats` — admin-only observed-vs-covered report.
async fn compat_stats(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    if let Err(e) = require_admin(&ctx).await {
        let status = status_for_error(&e);
        return (status, Json(github_error_json(&e, &headers))).into_response();
    }
    let snap = COMPAT_STATS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let rows: Vec<Value> = snap
        .iter()
        .map(|(route, (seen, covered))| {
            json!({ "route": route, "observed": seen, "covered": covered })
        })
        .collect();
    Json(json!({ "rest_calls": rows })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const O: &str = "http://localhost";

    fn issue_dto() -> Value {
        json!({
            "id": "iss_1", "repo_id": "repo_1", "number": 7,
            "title": "Broken build", "body": "it fails",
            "state": "open", "author_id": "u_1", "author_username": "octocat",
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
            "labels": [{"id": "l_1", "name": "bug", "color": "d73a4a", "description": "a bug"}],
            "assignees": [{"user_id": "u_2", "username": "hubot", "display_name": "Hubot"}],
            "milestone": null, "comment_count": 3
        })
    }

    #[test]
    fn issue_list_unwraps_to_bare_array_with_github_fields() {
        let data = json!({ "issues": [issue_dto()], "total": 41 });
        let fields = json!({ "owner": "jessedev", "name": "demo" });
        let (shaped, total) = shape("issue.list", data, &fields, O);
        assert_eq!(total, Some(41));
        let arr = shaped.as_array().expect("bare array");
        let it = &arr[0];
        assert_eq!(it["number"], 7);
        assert_eq!(it["title"], "Broken build");
        assert_eq!(it["user"]["login"], "octocat");
        assert_eq!(it["assignee"]["login"], "hubot");
        assert_eq!(it["labels"][0]["name"], "bug");
        assert_eq!(it["labels"][0]["color"], "d73a4a");
        assert_eq!(it["comments"], 3);
        assert_eq!(
            it["html_url"].as_str().unwrap(),
            "http://localhost/jessedev/demo/issues/7"
        );
        assert_eq!(
            it["url"].as_str().unwrap(),
            "http://localhost/api/v1/repos/jessedev/demo/issues/7"
        );
        assert!(it["node_id"].as_str().unwrap().starts_with("I_"));
        assert!(it.get("author_username").is_none(), "Oxidean key leaked");
    }

    #[test]
    fn issue_get_carries_full_github_shape() {
        let fields = json!({ "owner": "o", "name": "r" });
        let (shaped, _) = shape("issue.get", issue_dto(), &fields, O);
        for key in [
            "id",
            "node_id",
            "url",
            "repository_url",
            "labels_url",
            "comments_url",
            "events_url",
            "html_url",
            "number",
            "state",
            "title",
            "body",
            "user",
            "labels",
            "assignee",
            "assignees",
            "milestone",
            "locked",
            "comments",
            "created_at",
            "updated_at",
            "closed_at",
            "author_association",
        ] {
            assert!(shaped.get(key).is_some(), "missing GitHub key {key}");
        }
    }

    #[test]
    fn pull_get_has_head_base_objects() {
        let dto = json!({
            "id": "pr_9", "number": 42, "state": "open", "title": "add thing",
            "author_username": "octocat", "author_id": "u_1",
            "head_ref": "feature", "base_ref": "main",
            "head_sha": "abc123", "base_sha": "def456",
            "draft": false, "merged_at": null,
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-01T00:00:00Z"
        });
        let fields = json!({ "owner": "o", "name": "r" });
        let (shaped, _) = shape("pull.get", dto, &fields, O);
        assert_eq!(shaped["head"]["ref"], "feature");
        assert_eq!(shaped["head"]["label"], "o:feature");
        assert_eq!(shaped["base"]["ref"], "main");
        assert_eq!(shaped["merged"], false);
        assert_eq!(
            shaped["diff_url"].as_str().unwrap(),
            "http://localhost/o/r/pull/42.diff"
        );
    }

    #[test]
    fn repo_get_shape() {
        let dto = json!({
            "id": "repo_1", "name": "demo", "owner_username": "jessedev",
            "owner_id": "u_1", "visibility": "private", "default_branch": "main",
            "star_count": 3, "fork_count": 1, "open_issue_count": 5,
            "topics": ["rust"], "is_fork": false, "can_admin": true, "can_write": true,
        });
        let (shaped, _) = shape("repo.get", dto, &Value::Null, O);
        assert_eq!(shaped["full_name"], "jessedev/demo");
        assert_eq!(shaped["private"], true);
        assert_eq!(shaped["owner"]["login"], "jessedev");
        assert_eq!(shaped["permissions"]["admin"], true);
        assert_eq!(shaped["permissions"]["pull"], true);
        assert_eq!(shaped["default_branch"], "main");
        assert_eq!(
            shaped["clone_url"].as_str().unwrap(),
            "http://localhost/jessedev/demo.git"
        );
    }

    #[test]
    fn release_emits_upload_and_asset_urls() {
        let dto = json!({
            "id": "rel_1", "tag_name": "v1.0.0", "title": "First",
            "body": "notes", "draft": false, "prerelease": false,
            "author_username": "octocat",
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
            "assets": [{"id": "a_1", "filename": "bin.tgz", "content_type": "application/gzip", "byte_size": 100}]
        });
        let fields = json!({ "owner": "o", "name": "r" });
        let (shaped, _) = shape("release.get", dto, &fields, O);
        assert_eq!(shaped["tag_name"], "v1.0.0");
        assert_eq!(shaped["name"], "First");
        assert!(shaped["upload_url"]
            .as_str()
            .unwrap()
            .contains("/api/repos/o/r/releases/rel_1/assets"));
        assert_eq!(
            shaped["assets"][0]["browser_download_url"]
                .as_str()
                .unwrap(),
            "http://localhost/api/releases/assets/a_1"
        );
        assert_eq!(shaped["assets"][0]["name"], "bin.tgz");
        assert_eq!(shaped["assets"][0]["size"], 100);
    }

    #[test]
    fn search_envelope_is_github() {
        let data = json!({ "issues": [issue_dto()], "total": 120 });
        let fields = json!({ "owner": "o", "name": "r" });
        let (shaped, _) = shape("search.issues", data, &fields, O);
        assert_eq!(shaped["total_count"], 120);
        assert_eq!(shaped["incomplete_results"], false);
        assert_eq!(shaped["items"].as_array().unwrap().len(), 1);
        assert_eq!(shaped["items"][0]["user"]["login"], "octocat");
    }

    #[test]
    fn error_envelope_is_github() {
        let err = AppError::new("issue.not_found", "Issue not found");
        let headers = HeaderMap::new();
        let v = github_error_json(&err, &headers);
        assert_eq!(v["message"], "Issue not found");
        assert!(v["documentation_url"].as_str().unwrap().contains("/docs"));
        assert_eq!(v["code"], "issue.not_found");
    }

    #[test]
    fn query_translation_maps_github_pagination() {
        let q = rewrite_github_query("state=open&per_page=5&page=2").unwrap();
        let pairs: Vec<(String, String)> = form_urlencoded::parse(q.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(take_int(&pairs, "limit"), Some(5));
        assert_eq!(take_int(&pairs, "offset"), Some(5));
        assert!(pairs.iter().any(|(k, _)| k == "state"));
        assert!(!pairs.iter().any(|(k, _)| k == "per_page"));

        // Caps at 100; page alone assumes per_page=30.
        let q = rewrite_github_query("per_page=500").unwrap();
        assert!(q.contains("limit=100"));
        let q = rewrite_github_query("page=3").unwrap();
        assert!(q.contains("limit=30") && q.contains("offset=60"));

        // No GitHub params → untouched.
        assert!(rewrite_github_query("state=open&limit=10").is_none());
    }

    #[test]
    fn link_header_builds_next_last() {
        let uri = "/api/v1/repos/o/r/issues?state=open&limit=2&offset=0";
        let link = link_header(uri, 10, 2, 0).unwrap();
        let s = link.to_str().unwrap();
        assert!(s.contains("rel=\"next\""));
        assert!(s.contains("rel=\"last\""));
        // GitHub-shaped params: next page = page 2 at per_page=2.
        assert!(s.contains("per_page=2"));
        assert!(s.contains("page=2"));
        assert!(s.contains("state=open"));
        // Last page → no Link.
        assert!(link_header(uri, 10, 2, 8).is_none());
    }

    #[test]
    fn job_token_detection_uses_bearer_prefix() {
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer oxidean_job_abc123"),
        );
        assert!(job_token_request(&h));
        let mut h2 = HeaderMap::new();
        h2.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer oxidean_pat_abc123"),
        );
        assert!(!job_token_request(&h2));
        assert!(!job_token_request(&HeaderMap::new()));
    }

    #[test]
    fn timeline_items_render_as_events() {
        let rev = json!({
            "id": "rev_1", "editor_id": "u_1", "editor_username": "octocat",
            "title": "new title", "body": "new body",
            "created_at": "2024-01-01T00:00:00Z"
        });
        let item = timeline_ref_gh(&rev, O);
        assert_eq!(item["event"], "edited");
        assert_eq!(item["actor"]["login"], "octocat");
        assert_eq!(item["rename"]["to"], "new title");
    }
}
