//! P14 — `actions/cache` service (`/api/actions/_apis/artifactcache/*`).
//! Covers claim-minted runtime tokens, reserve/upload/commit/restore
//! roundtrips, restore-key fallback, immutable-save 409s, scope isolation,
//! expiry, and LRU eviction.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::actions::dispatch::enqueue_run;
use oxidean_api::actions::{mint_registration_token, parse_workflow_yaml};
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use tower::ServiceExt;

async fn app_db() -> (axum::Router, Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("r.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_actions_enabled(true)
    .with_actions_cache_dir(dir.path().join("actions-cache"));
    let app = router_with_state(state, build_cors("development", None).unwrap());
    (app, db, dir)
}

async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::json!({}))
}

fn rpc_req(body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", cookie)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app.clone().oneshot(rpc_req(body, cookie)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn signup(app: &axum::Router, db: &Database, email: &str, username: &str) -> String {
    let r = rpc_json(
        app,
        "",
        &format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], true, "signup — {r}");
    let user_id = r["data"]["id"].as_str().expect("user id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(format!(
                    r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    login
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

async fn json_req(
    app: &axum::Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    body: serde_json::Value,
) -> axum::response::Response {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    app.clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

/// Register a repo + runner, enqueue a `push` run on `main`, claim it, and
/// hand back the runner token plus the minted cache creds.
async fn claim_with_cache(
    app: &axum::Router,
    db: &Database,
    repo_key: &str,
    head_ref: &str,
) -> (String, String, String, String) {
    let owner = db
        .create_user(
            &format!("u-{repo_key}"),
            &format!("{repo_key}@example.com"),
            repo_key,
            Some("h"),
            "F",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository(
            &format!("r-{repo_key}"),
            &owner.id,
            "user",
            repo_key,
            "private",
            "",
            "main",
        )
        .await
        .unwrap();
    let doc = parse_workflow_yaml(
        b"name: CI\non: [push]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n",
    )
    .unwrap();
    enqueue_run(
        db,
        &repo.id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "deadbeef",
        head_ref,
        Some(&owner.id),
        None,
        None,
        false,
        None,
    )
    .await
    .unwrap();
    let reg = mint_registration_token(db).await.unwrap();
    let reg_res = json_req(
        app,
        "POST",
        "/api/actions/register",
        None,
        serde_json::json!({"name": "r1", "labels": ["ubuntu-latest"], "token": reg}),
    )
    .await;
    let reg_v = body_json(reg_res).await;
    let runner_token = reg_v["runner_token"].as_str().unwrap().to_string();
    let fetch = json_req(
        app,
        "POST",
        "/api/actions/fetch_task",
        Some(&runner_token),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(fetch.status(), StatusCode::OK);
    let v = body_json(fetch).await;
    let job_id = v["job_id"].as_str().expect("claimed job").to_string();
    let cache_url = v["cache_url"]
        .as_str()
        .expect("fetch_task must mint ACTIONS_CACHE_URL")
        .to_string();
    let cache_token = v["cache_token"]
        .as_str()
        .expect("fetch_task must mint ACTIONS_RUNTIME_TOKEN")
        .to_string();
    assert!(
        cache_url.ends_with("/api/actions/_apis/artifactcache"),
        "cache_url shape: {cache_url}"
    );
    (runner_token, job_id, cache_url, cache_token)
}

/// Strip the origin off `cache_url` — oneshot requests need path-only URIs.
fn cache_path(url: &str) -> &str {
    url.find("/api/actions").map(|i| &url[i..]).unwrap_or(url)
}

#[tokio::test]
async fn cache_roundtrip_exact_restore() {
    let (app, db, _dir) = app_db().await;
    let (_rt, _job, url, tok) = claim_with_cache(&app, &db, "cc", "refs/heads/main").await;
    let base = cache_path(&url);

    // Reserve → upload → commit.
    let res = json_req(
        &app,
        "POST",
        &format!("{base}/caches"),
        Some(&tok),
        serde_json::json!({"key": "k-main", "version": "v1"}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let cache_id = body_json(res).await["cacheId"]
        .as_str()
        .unwrap()
        .to_string();

    let patch = Request::builder()
        .method("PATCH")
        .uri(format!("{base}/caches/{cache_id}"))
        .header("authorization", format!("Bearer {tok}"))
        .header("content-range", "bytes 0-3/*")
        .body(Body::from("data"))
        .unwrap();
    let res = app.clone().oneshot(patch).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = json_req(
        &app,
        "POST",
        &format!("{base}/caches/{cache_id}"),
        Some(&tok),
        serde_json::json!({"size": 4}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Exact restore → archiveLocation → download.
    let res = json_req(
        &app,
        "GET",
        &format!("{base}/cache?keys=k-main&version=v1"),
        Some(&tok),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let hit = body_json(res).await;
    assert_eq!(hit["cacheKey"], "k-main");
    let archive = hit["archiveLocation"].as_str().unwrap().to_string();
    let res = json_req(
        &app,
        "GET",
        cache_path(&archive),
        Some(&tok),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"data");
}

#[tokio::test]
async fn cache_immutable_save_409() {
    let (app, db, _dir) = app_db().await;
    let (_rt, _job, url, tok) = claim_with_cache(&app, &db, "ci", "refs/heads/main").await;
    let base = cache_path(&url);

    let res = json_req(
        &app,
        "POST",
        &format!("{base}/caches"),
        Some(&tok),
        serde_json::json!({"key": "dup", "version": "v1"}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    // Same (key, version, scope) — reserved OR committed both collide.
    let res = json_req(
        &app,
        "POST",
        &format!("{base}/caches"),
        Some(&tok),
        serde_json::json!({"key": "dup", "version": "v1"}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn cache_restore_key_prefix_fallback() {
    let (app, db, _dir) = app_db().await;
    let (_rt, _job, url, tok) = claim_with_cache(&app, &db, "cp", "refs/heads/main").await;
    let base = cache_path(&url);

    let res = json_req(
        &app,
        "POST",
        &format!("{base}/caches"),
        Some(&tok),
        serde_json::json!({"key": "linux-node-abc123", "version": "v1"}),
    )
    .await;
    let id = body_json(res).await["cacheId"]
        .as_str()
        .unwrap()
        .to_string();
    let patch = Request::builder()
        .method("PATCH")
        .uri(format!("{base}/caches/{id}"))
        .header("authorization", format!("Bearer {tok}"))
        .body(Body::from("x"))
        .unwrap();
    app.clone().oneshot(patch).await.unwrap();
    json_req(
        &app,
        "POST",
        &format!("{base}/caches/{id}"),
        Some(&tok),
        serde_json::json!({"size": 1}),
    )
    .await;

    // Primary key misses; restore-key prefix hits the saved entry.
    let res = json_req(
        &app,
        "GET",
        &format!("{base}/cache?keys=linux-node-zzz,linux-node-&version=v1"),
        Some(&tok),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let hit = body_json(res).await;
    assert_eq!(hit["cacheKey"], "linux-node-abc123");
}

#[tokio::test]
async fn cache_requires_runtime_token() {
    let (app, db, _dir) = app_db().await;
    let (_rt, _job, url, _tok) = claim_with_cache(&app, &db, "ca", "refs/heads/main").await;
    let base = cache_path(&url);

    // No Authorization → 401.
    let res = json_req(
        &app,
        "GET",
        &format!("{base}/cache?keys=x&version=v"),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Runner registration token is NOT a cache token — 401.
    let rt = _rt;
    let res = json_req(
        &app,
        "GET",
        &format!("{base}/cache?keys=x&version=v"),
        Some(&rt),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cache_cross_repo_isolation() {
    let (app, db, _dir) = app_db().await;
    let (_rt, _job, url_a, tok_a) = claim_with_cache(&app, &db, "ca", "refs/heads/main").await;
    let (_rt2, _job2, url_b, tok_b) = claim_with_cache(&app, &db, "cb", "refs/heads/main").await;
    let base_a = cache_path(&url_a);
    let base_b = cache_path(&url_b);

    // Save under repo A.
    let res = json_req(
        &app,
        "POST",
        &format!("{base_a}/caches"),
        Some(&tok_a),
        serde_json::json!({"key": "shared-name", "version": "v1"}),
    )
    .await;
    let id = body_json(res).await["cacheId"]
        .as_str()
        .unwrap()
        .to_string();

    // Repo B's token can't read A's entry metadata (404, not 403 — the entry
    // must be indistinguishable from absent).
    let res = json_req(
        &app,
        "GET",
        &format!("{base_b}/caches/{id}"),
        Some(&tok_b),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Repo B's restore probe can't see A's key.
    let res = json_req(
        &app,
        "GET",
        &format!("{base_b}/cache?keys=shared-name&version=v1"),
        Some(&tok_b),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn cache_expired_token_rejected() {
    let (app, db, _dir) = app_db().await;
    let (_rt, job, url, _tok) = claim_with_cache(&app, &db, "ce", "refs/heads/main").await;
    let base = cache_path(&url);
    let repo = db.find_repository_by_id("r-ce").await.unwrap().unwrap();

    // Forge a token row with a past expiry, then present it.
    let expired = format!("oxrt_expired_{job}");
    let past = "2000-01-01T00:00:00Z";
    db.insert_action_cache_token(
        &oxidean_api::actions::tokens::hash_token(&expired),
        &job,
        &repo.id,
        past,
    )
    .await
    .unwrap();
    let res = json_req(
        &app,
        "GET",
        &format!("{base}/cache?keys=x&version=v"),
        Some(&expired),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cache_reserved_not_restorable() {
    let (app, db, _dir) = app_db().await;
    let (_rt, _job, url, tok) = claim_with_cache(&app, &db, "cr", "refs/heads/main").await;
    let base = cache_path(&url);

    // Reserved but never committed → invisible to restores.
    json_req(
        &app,
        "POST",
        &format!("{base}/caches"),
        Some(&tok),
        serde_json::json!({"key": "ghost", "version": "v1"}),
    )
    .await;
    let res = json_req(
        &app,
        "GET",
        &format!("{base}/cache?keys=ghost&version=v1"),
        Some(&tok),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn cache_lru_eviction_over_cap() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("r.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_actions_enabled(true)
    .with_actions_cache_dir(dir.path().join("actions-cache"))
    .with_actions_cache_max_bytes(8); // 8-byte cap — two 4-byte entries evict the third.
    let app = router_with_state(state, build_cors("development", None).unwrap());
    let (_rt, _job, url, tok) = claim_with_cache(&app, &db, "cl", "refs/heads/main").await;
    let base = cache_path(&url).to_string();

    async fn save(app: &axum::Router, base: &str, tok: &str, key: &str, bytes: &[u8]) {
        let res = json_req(
            app,
            "POST",
            &format!("{base}/caches"),
            Some(tok),
            serde_json::json!({"key": key, "version": "v1"}),
        )
        .await;
        assert_eq!(res.status(), StatusCode::CREATED, "reserve {key}");
        let id = body_json(res).await["cacheId"]
            .as_str()
            .unwrap()
            .to_string();
        let patch = Request::builder()
            .method("PATCH")
            .uri(format!("{base}/caches/{id}"))
            .header("authorization", format!("Bearer {tok}"))
            .body(Body::from(bytes.to_vec()))
            .unwrap();
        app.clone().oneshot(patch).await.unwrap();
        let res = json_req(
            app,
            "POST",
            &format!("{base}/caches/{id}"),
            Some(tok),
            serde_json::json!({"size": bytes.len()}),
        )
        .await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT, "commit {key}");
    }

    save(&app, &base, &tok, "ev-1", b"aaaa").await;
    save(&app, &base, &tok, "ev-2", b"bbbb").await;
    // Third commit pushes total to 12 > 8 → ev-1 (oldest) evicted.
    save(&app, &base, &tok, "ev-3", b"cccc").await;

    for (key, want) in [
        ("ev-1", StatusCode::NO_CONTENT),
        ("ev-2", StatusCode::OK),
        ("ev-3", StatusCode::OK),
    ] {
        let res = json_req(
            &app,
            "GET",
            &format!("{base}/cache?keys={key}&version=v1"),
            Some(&tok),
            serde_json::json!({}),
        )
        .await;
        assert_eq!(res.status(), want, "lookup {key}");
    }
}

#[tokio::test]
async fn cache_rpc_list_and_delete() {
    let (app, db, _dir) = app_db().await;
    let cookie = signup(&app, &db, "crpc@example.com", "crpc").await;
    let me = db.find_user_by_username("crpc").await.unwrap().unwrap();
    // claim_with_cache mints its own owner — give the RPC session a repo it owns.
    let repo = db
        .insert_repository("r-crpc", &me.id, "user", "crpc", "private", "", "main")
        .await
        .unwrap();
    let doc = parse_workflow_yaml(
        b"name: CI\non: [push]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n",
    )
    .unwrap();
    enqueue_run(
        &db,
        &repo.id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(&me.id),
        None,
        None,
        false,
        None,
    )
    .await
    .unwrap();
    let reg = mint_registration_token(&db).await.unwrap();
    let reg_res = json_req(
        &app,
        "POST",
        "/api/actions/register",
        None,
        serde_json::json!({"name": "r1", "labels": ["ubuntu-latest"], "token": reg}),
    )
    .await;
    let runner_token = body_json(reg_res).await["runner_token"]
        .as_str()
        .unwrap()
        .to_string();
    let v = body_json(
        json_req(
            &app,
            "POST",
            "/api/actions/fetch_task",
            Some(&runner_token),
            serde_json::json!({}),
        )
        .await,
    )
    .await;
    let url = v["cache_url"].as_str().unwrap().to_string();
    let tok = v["cache_token"].as_str().unwrap().to_string();
    let base = cache_path(&url).to_string();
    let res = json_req(
        &app,
        "POST",
        &format!("{base}/caches"),
        Some(&tok),
        serde_json::json!({"key": "rpc-k", "version": "v1"}),
    )
    .await;
    let id = body_json(res).await["cacheId"]
        .as_str()
        .unwrap()
        .to_string();
    let patch = Request::builder()
        .method("PATCH")
        .uri(format!("{base}/caches/{id}"))
        .header("authorization", format!("Bearer {tok}"))
        .body(Body::from("rpc!"))
        .unwrap();
    app.clone().oneshot(patch).await.unwrap();
    json_req(
        &app,
        "POST",
        &format!("{base}/caches/{id}"),
        Some(&tok),
        serde_json::json!({"size": 4}),
    )
    .await;

    // listCaches — Read+, committed entry + usage rollup.
    let r = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.actions.listCaches","input":{"owner":"crpc","name":"crpc"}}"#,
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["data"]["caches"][0]["key"], "rpc-k");
    assert_eq!(r["data"]["total_size_bytes"], 4);

    // deleteCache — Write+; drops row and payload.
    let r = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.actions.deleteCache","input":{{"owner":"crpc","name":"crpc","cache_id":"{id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["data"]["deleted"], true);

    let res = json_req(
        &app,
        "GET",
        &format!("{base}/cache?keys=rpc-k&version=v1"),
        Some(&tok),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn cache_sweep_reaps_stale_rows() {
    let (app, db, dir) = app_db().await;
    let (_rt, job, url, tok) = claim_with_cache(&app, &db, "cs", "refs/heads/main").await;
    let base = cache_path(&url).to_string();
    let repo = db.find_repository_by_id("r-cs").await.unwrap().unwrap();

    // A reserved-but-uncommitted row + an already-expired token.
    json_req(
        &app,
        "POST",
        &format!("{base}/caches"),
        Some(&tok),
        serde_json::json!({"key": "stale", "version": "v1"}),
    )
    .await;
    db.insert_action_cache_token(
        &oxidean_api::actions::tokens::hash_token("oxrt_dead"),
        &job,
        &repo.id,
        "2000-01-01T00:00:00Z",
    )
    .await
    .unwrap();

    // Reap with generous cutoffs — the reservation's created_at is `now`,
    // so `now + 1h` catches it without backdating SQL.
    let future = "2999-01-01T00:00:00Z";
    let paths = db
        .reap_stale_action_cache_reservations(future)
        .await
        .unwrap();
    assert_eq!(paths.len(), 1, "reserved 'stale' row reaped");
    let tokens = db
        .reap_action_cache_tokens("2100-01-01T00:00:00Z")
        .await
        .unwrap();
    assert!(
        tokens >= 2,
        "expired + the (now-past) live token both reaped"
    );

    // And the composed sweeper still runs clean on a now-cutoff.
    let n =
        oxidean_api::actions::cache::sweep_action_caches(&db, &dir.path().join("actions-cache"))
            .await
            .unwrap();
    assert_eq!(n, 0);
}
