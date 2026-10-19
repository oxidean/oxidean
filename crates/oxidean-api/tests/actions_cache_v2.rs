//! P14b — `actions/cache@v4` Twirp surface
//! (`/twirp/github.actions.results.api.v1.CacheService/*`) + the
//! `/_apis/artifactcache2/blobs/{id}` signed-URL data plane. Covers the
//! create→upload→finalize→restore roundtrip (PutBlob and staged-block forms),
//! restore-key prefix hits, `ok:false` misses, `already_exists` conflicts,
//! `?t=` query auth (the real client sends no `Authorization` on the data
//! plane), and the v1 `archiveLocation` download without a Bearer header.

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

const SVC: &str = "github.actions.results.api.v1.CacheService";

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

async fn twirp(
    app: &axum::Router,
    method: &str,
    bearer: Option<&str>,
    body: serde_json::Value,
) -> axum::response::Response {
    let mut b = Request::builder()
        .method("POST")
        .uri(format!("/twirp/{SVC}/{method}"))
        .header("content-type", "application/json");
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    app.clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

/// Strip `{origin}` off a signed/cache URL — oneshot wants path+query only.
fn rel(url: &str) -> &str {
    url.find("/api/actions").map(|i| &url[i..]).unwrap_or(url)
}

/// Claim a job on `main` for a fresh repo; returns the runtime token pair.
async fn claim(app: &axum::Router, db: &Database, key: &str, head_ref: &str) -> (String, String) {
    let owner = db
        .create_user(
            &format!("u-{key}"),
            &format!("{key}@example.com"),
            key,
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
            &format!("r-{key}"),
            &owner.id,
            "user",
            key,
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
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"name":"r1","labels":["ubuntu-latest"],"token":reg})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let runner_token = body_json(reg_res).await["runner_token"]
        .as_str()
        .unwrap()
        .to_string();
    let v = body_json(
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/actions/fetch_task")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {runner_token}"))
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    (
        v["cache_token"].as_str().unwrap().to_string(),
        v["cache_url"].as_str().unwrap().to_string(),
    )
}

/// Raw request helper for the blob plane (PUT/GET with query, no JSON sugar).
async fn blob_req(
    app: &axum::Router,
    method: &str,
    path_query: &str,
    bytes: &[u8],
    headers: &[(&str, &str)],
) -> axum::response::Response {
    let mut b = Request::builder().method(method).uri(path_query);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    app.clone()
        .oneshot(b.body(Body::from(bytes.to_vec())).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn v2_roundtrip_putblob_then_restore() {
    let (app, db, _dir) = app_db().await;
    let (tok, _url) = claim(&app, &db, "v2rt", "refs/heads/main").await;

    // CreateCacheEntry → signed_upload_url.
    let res = twirp(
        &app,
        "CreateCacheEntry",
        Some(&tok),
        serde_json::json!({
            "metadata": {"repository_id": "1", "scope": [{"scope":"refs/heads/main","permission":"1"}]},
            "key": "k2",
            "version": "v1",
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["ok"], true, "{v}");
    let up = v["signed_upload_url"].as_str().unwrap().to_string();
    assert!(
        up.contains("/api/actions/_apis/artifactcache2/blobs/"),
        "{up}"
    );
    assert!(up.contains("?t="), "{up}");

    // PutBlob — no Authorization header on the data plane.
    let res = blob_req(&app, "PUT", rel(&up), b"payload-v2", &[]).await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // Finalize — int64 fields travel as strings on the wire.
    let res = twirp(
        &app,
        "FinalizeCacheEntryUpload",
        Some(&tok),
        serde_json::json!({"key":"k2","version":"v1","size_bytes":"10"}),
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["ok"], true, "{v}");
    assert!(v["entry_id"].as_str().unwrap().parse::<i64>().is_ok());

    // GetCacheEntryDownloadURL — hit returns matched_key + download url.
    let res = twirp(
        &app,
        "GetCacheEntryDownloadURL",
        Some(&tok),
        serde_json::json!({"key":"k2","version":"v1","restore_keys":[]}),
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["matched_key"], "k2");
    let down = v["signed_download_url"].as_str().unwrap().to_string();

    // Plain GET on the signed URL — no headers at all.
    let res = blob_req(&app, "GET", rel(&down), &[], &[]).await;
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"payload-v2");
}

#[tokio::test]
async fn v2_miss_is_ok_false_not_error() {
    let (app, db, _dir) = app_db().await;
    let (tok, _url) = claim(&app, &db, "v2ms", "refs/heads/main").await;
    let res = twirp(
        &app,
        "GetCacheEntryDownloadURL",
        Some(&tok),
        serde_json::json!({"key":"absent","version":"v1"}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["ok"], false);
}

#[tokio::test]
async fn v2_restore_key_prefix_match() {
    let (app, db, _dir) = app_db().await;
    let (tok, _url) = claim(&app, &db, "v2pk", "refs/heads/main").await;

    let res = twirp(
        &app,
        "CreateCacheEntry",
        Some(&tok),
        serde_json::json!({"key":"linux-node-abc","version":"v1"}),
    )
    .await;
    let up = body_json(res).await["signed_upload_url"]
        .as_str()
        .unwrap()
        .to_string();
    blob_req(&app, "PUT", rel(&up), b"x", &[]).await;
    twirp(
        &app,
        "FinalizeCacheEntryUpload",
        Some(&tok),
        serde_json::json!({"key":"linux-node-abc","version":"v1","size_bytes":"1"}),
    )
    .await;

    // Primary misses, restore-key prefix hits.
    let res = twirp(
        &app,
        "GetCacheEntryDownloadURL",
        Some(&tok),
        serde_json::json!({"key":"linux-node-zzz","restore_keys":["linux-node-"],"version":"v1"}),
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["matched_key"], "linux-node-abc");
}

#[tokio::test]
async fn v2_staged_block_upload() {
    let (app, db, _dir) = app_db().await;
    let (tok, _url) = claim(&app, &db, "v2bl", "refs/heads/main").await;

    let res = twirp(
        &app,
        "CreateCacheEntry",
        Some(&tok),
        serde_json::json!({"key":"blk","version":"v1"}),
    )
    .await;
    let up = body_json(res).await["signed_upload_url"]
        .as_str()
        .unwrap()
        .to_string();

    // Stage two blocks, then commit the ordered list (Azure PutBlockList).
    for (bid, chunk) in [("YQ==", "hello-"), ("Yg==", "world")] {
        let res = blob_req(
            &app,
            "PUT",
            &format!("{up}&comp=block&blockid={bid}"),
            chunk.as_bytes(),
            &[("x-ms-blob-type", "BlockBlob")],
        )
        .await;
        assert_eq!(res.status(), StatusCode::CREATED, "stage {bid}");
    }
    let list = r#"<?xml version="1.0" encoding="utf-8"?><BlockList><Latest>YQ==</Latest><Latest>Yg==</Latest></BlockList>"#;
    let res = blob_req(
        &app,
        "PUT",
        &format!("{up}&comp=blocklist"),
        list.as_bytes(),
        &[],
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let res = twirp(
        &app,
        "FinalizeCacheEntryUpload",
        Some(&tok),
        serde_json::json!({"key":"blk","version":"v1","size_bytes":"11"}),
    )
    .await;
    assert_eq!(body_json(res).await["ok"], true);

    let res = twirp(
        &app,
        "GetCacheEntryDownloadURL",
        Some(&tok),
        serde_json::json!({"key":"blk","version":"v1"}),
    )
    .await;
    let down = body_json(res).await["signed_download_url"]
        .as_str()
        .unwrap()
        .to_string();
    let res = blob_req(&app, "GET", rel(&down), &[], &[]).await;
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"hello-world");
}

#[tokio::test]
async fn v2_conflict_is_already_exists() {
    let (app, db, _dir) = app_db().await;
    let (tok, _url) = claim(&app, &db, "v2cx", "refs/heads/main").await;
    twirp(
        &app,
        "CreateCacheEntry",
        Some(&tok),
        serde_json::json!({"key":"dup","version":"v1"}),
    )
    .await;
    let res = twirp(
        &app,
        "CreateCacheEntry",
        Some(&tok),
        serde_json::json!({"key":"dup","version":"v1"}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let v = body_json(res).await;
    assert_eq!(v["code"], "already_exists", "{v}");
}

#[tokio::test]
async fn v2_size_mismatch_finalize_fails_soft() {
    let (app, db, _dir) = app_db().await;
    let (tok, _url) = claim(&app, &db, "v2sz", "refs/heads/main").await;
    let res = twirp(
        &app,
        "CreateCacheEntry",
        Some(&tok),
        serde_json::json!({"key":"sz","version":"v1"}),
    )
    .await;
    let up = body_json(res).await["signed_upload_url"]
        .as_str()
        .unwrap()
        .to_string();
    blob_req(&app, "PUT", rel(&up), b"abc", &[]).await;
    // Declared size disagrees with bytes on disk → ok:false, not a 5xx.
    let res = twirp(
        &app,
        "FinalizeCacheEntryUpload",
        Some(&tok),
        serde_json::json!({"key":"sz","version":"v1","size_bytes":"99"}),
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["ok"], false, "{v}");
    assert!(v["message"].as_str().unwrap().contains("size"));
}

#[tokio::test]
async fn v2_requires_runtime_token() {
    let (app, db, _dir) = app_db().await;
    let (tok, _url) = claim(&app, &db, "v2au", "refs/heads/main").await;

    // No Bearer on the Twirp plane → 401 {code: unauthenticated}.
    let res = twirp(
        &app,
        "CreateCacheEntry",
        None,
        serde_json::json!({"key":"x","version":"v1"}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(res).await["code"], "unauthenticated");

    // Signed URL without `t` → 401.
    let res = twirp(
        &app,
        "CreateCacheEntry",
        Some(&tok),
        serde_json::json!({"key":"x","version":"v1"}),
    )
    .await;
    let up = body_json(res).await["signed_upload_url"]
        .as_str()
        .unwrap()
        .to_string();
    let bare = rel(&up).split("?t=").next().unwrap().to_string();
    let res = blob_req(&app, "PUT", &bare, b"x", &[]).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

/// Small JSON-body helper for the v1 REST surface.
async fn v1_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    tok: &str,
    body: serde_json::Value,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {tok}"))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn v1_download_works_without_bearer() {
    // The real cache@v3 client follows `archiveLocation` with no
    // Authorization header — the `?t=` param on the URL authenticates it.
    let (app, db, _dir) = app_db().await;
    let (tok, url) = claim(&app, &db, "v1dl", "refs/heads/main").await;
    let base = url
        .find("/api/actions")
        .map(|i| &url[i..])
        .unwrap_or(&url)
        .to_string();

    let res = v1_json(
        &app,
        "POST",
        &format!("{base}/caches"),
        &tok,
        serde_json::json!({"key":"v1k","version":"v1"}),
    )
    .await;
    let id = body_json(res).await["cacheId"]
        .as_str()
        .unwrap()
        .to_string();
    blob_req(
        &app,
        "PATCH",
        &format!("{base}/caches/{id}"),
        b"blob",
        &[("authorization", &format!("Bearer {tok}"))],
    )
    .await;
    v1_json(
        &app,
        "POST",
        &format!("{base}/caches/{id}"),
        &tok,
        serde_json::json!({"size":4}),
    )
    .await;

    // Probe → archiveLocation carries ?t=; GET it with NO headers.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("{base}/cache?keys=v1k&version=v1"))
                .header("authorization", format!("Bearer {tok}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let loc = body_json(res).await["archiveLocation"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(loc.contains("?t="), "{loc}");
    let res = blob_req(&app, "GET", rel(&loc), &[], &[]).await;
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"blob");
}
