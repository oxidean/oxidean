//! Wave 1 tracer: LFS Batch + basic PUT/GET into OXIDEAN_LFS_DIR (GIT-12 / D-LFS-07).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::auth::session::sha256_hex;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(
    db: Database,
    repos_dir: std::path::PathBuf,
    lfs_dir: std::path::PathBuf,
) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir)
        .with_lfs_dir(lfs_dir);
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

fn rpc_req(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn rpc_req_with_cookie(body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", cookie)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn session_cookie_from_response(res: &axum::http::Response<Body>) -> String {
    let set_cookie = res
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap();
    set_cookie.split(';').next().unwrap().trim().to_string()
}

async fn signup_and_login(
    app: &axum::Router,
    email: &str,
    username: &str,
) -> (String, serde_json::Value) {
    let signup_body = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
    );
    let signup = app.clone().oneshot(rpc_req(&signup_body)).await.unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login_body = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
    );
    let login = app.clone().oneshot(rpc_req(&login_body)).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (cookie, v)
}

fn encode_b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn basic_header(user: &str, password: &str) -> String {
    let raw = format!("{user}:{password}");
    format!("Basic {}", encode_b64(raw.as_bytes()))
}

/// GIT-12: POST batch upload → PUT → GET download happy path (basic transfer).
#[tokio::test]
async fn lfs_batch_upload_put_download_happy_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_happy.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos, lfs.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "lfs@ex.com", "lfsown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"blobs","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let repo_id = create_v["data"]["id"].as_str().expect("repo id");
    db.set_repo_lfs_enabled(repo_id, true)
        .await
        .expect("enable lfs");

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"lfs","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().expect("token");

    let payload = b"hello-lfs-object";
    let oid = sha256_hex(payload);
    let size = payload.len() as i64;

    let batch_body = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": size }]
    });
    let batch_req = Request::builder()
        .method("POST")
        .uri("/lfsown/blobs.git/info/lfs/objects/batch")
        .header(header::AUTHORIZATION, basic_header("git", token))
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(batch_body.to_string()))
        .unwrap();
    let batch_res = app.clone().oneshot(batch_req).await.unwrap();
    assert_eq!(batch_res.status(), StatusCode::OK, "batch upload");
    let batch_bytes = batch_res.into_body().collect().await.unwrap().to_bytes();
    let batch_v: serde_json::Value = serde_json::from_slice(&batch_bytes).unwrap();
    assert_eq!(batch_v["transfer"], "basic");
    let href = batch_v["objects"][0]["actions"]["upload"]["href"]
        .as_str()
        .expect("upload href");
    assert!(href.contains(&oid), "href={href}");

    let put_req = Request::builder()
        .method("PUT")
        .uri(href)
        .header(header::AUTHORIZATION, basic_header("git", token))
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(payload.as_slice()))
        .unwrap();
    let put_res = app.clone().oneshot(put_req).await.unwrap();
    assert_eq!(put_res.status(), StatusCode::OK, "put object");

    let shard = lfs.join(&oid[0..2]).join(&oid[2..4]).join(&oid);
    assert!(shard.is_file(), "shard missing at {}", shard.display());
    let on_disk = std::fs::read(&shard).unwrap();
    assert_eq!(on_disk, payload);

    let dl_batch = serde_json::json!({
        "operation": "download",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": size }]
    });
    let dl_req = Request::builder()
        .method("POST")
        .uri("/lfsown/blobs.git/info/lfs/objects/batch")
        .header(header::AUTHORIZATION, basic_header("git", token))
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(dl_batch.to_string()))
        .unwrap();
    let dl_res = app.clone().oneshot(dl_req).await.unwrap();
    assert_eq!(dl_res.status(), StatusCode::OK);
    let dl_bytes = dl_res.into_body().collect().await.unwrap().to_bytes();
    let dl_v: serde_json::Value = serde_json::from_slice(&dl_bytes).unwrap();
    let dl_href = dl_v["objects"][0]["actions"]["download"]["href"]
        .as_str()
        .expect("download href");

    let get_req = Request::builder()
        .method("GET")
        .uri(dl_href)
        .header(header::AUTHORIZATION, basic_header("git", token))
        .body(Body::empty())
        .unwrap();
    let get_res = app.oneshot(get_req).await.unwrap();
    assert_eq!(get_res.status(), StatusCode::OK);
    let got = get_res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&got[..], payload);
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify email");
}

/// D-LFS-09: session Cookie alone never authorizes LFS — public download stays anon OK.
#[tokio::test]
async fn lfs_batch_cookie_ignored_as_anon() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_cookie.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos, lfs).await;

    let (cookie, login_v) = signup_and_login(&app, "ck@ex.com", "ckown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"pub","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let repo_id = create_v["data"]["id"].as_str().unwrap();
    db.set_repo_lfs_enabled(repo_id, true).await.unwrap();

    let oid = "a".repeat(64);
    let body = serde_json::json!({
        "operation": "download",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": 1 }]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/ckown/pub.git/info/lfs/objects/batch")
        .header(header::COOKIE, &cookie)
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "public download with cookie-only must be treated as anon"
    );

    // Private + cookie alone → 401 (not elevated by session).
    let priv_create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(priv_create.status(), StatusCode::OK);
    let priv_bytes = priv_create.into_body().collect().await.unwrap().to_bytes();
    let priv_v: serde_json::Value = serde_json::from_slice(&priv_bytes).unwrap();
    let priv_id = priv_v["data"]["id"].as_str().unwrap();
    db.set_repo_lfs_enabled(priv_id, true).await.unwrap();

    let priv_req = Request::builder()
        .method("POST")
        .uri("/ckown/secret.git/info/lfs/objects/batch")
        .header(header::COOKIE, &cookie)
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let priv_res = app.oneshot(priv_req).await.unwrap();
    assert_eq!(priv_res.status(), StatusCode::UNAUTHORIZED);
    let lfs_auth = priv_res
        .headers()
        .get("lfs-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        lfs_auth.contains("Basic"),
        "LFS-Authenticate required — {lfs_auth}"
    );
}

/// Private unauth → 401 + LFS-Authenticate Basic realm.
#[tokio::test]
async fn lfs_batch_unauth_private_returns_401_lfs_authenticate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_unauth.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos, lfs).await;

    let (cookie, login_v) = signup_and_login(&app, "ua@ex.com", "uaown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"priv","visibility":"private","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    db.set_repo_lfs_enabled(create_v["data"]["id"].as_str().unwrap(), true)
        .await
        .unwrap();

    let oid = "b".repeat(64);
    let body = serde_json::json!({
        "operation": "download",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": 1 }]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/uaown/priv.git/info/lfs/objects/batch")
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let lfs_auth = res
        .headers()
        .get("lfs-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        lfs_auth.contains("Basic") && lfs_auth.contains("Git LFS"),
        "LFS-Authenticate — {lfs_auth}"
    );
}

/// FG contents:read cannot upload → 403 (not 401).
#[tokio::test]
async fn lfs_batch_insufficient_pat_scope_forbidden() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_scope.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos, lfs).await;

    let (cookie, login_v) = signup_and_login(&app, "sc@ex.com", "scown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"blobs","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    db.set_repo_lfs_enabled(create_v["data"]["id"].as_str().unwrap(), true)
        .await
        .unwrap();

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createFineGrained","input":{"name":"ro","repo_access":"all","contents":"read"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().unwrap();

    let oid = "c".repeat(64);
    let body = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": 1 }]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/scown/blobs.git/info/lfs/objects/batch")
        .header(header::AUTHORIZATION, basic_header("git", token))
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "contents:read must 403 on upload — got {}",
        res.status()
    );
}

/// Upload requires Write capability path + verified email (mirrors receive-pack).
#[tokio::test]
async fn lfs_batch_upload_requires_write_and_verified() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_unv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos, lfs).await;

    let (cookie, login_v) = signup_and_login(&app, "unv@ex.com", "unvown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"blobs","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    db.set_repo_lfs_enabled(create_v["data"]["id"].as_str().unwrap(), true)
        .await
        .unwrap();

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"pre","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().unwrap().to_string();

    db.clear_email_verified_at(user_id).await.expect("clear");

    let oid = "d".repeat(64);
    let body = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": 1 }]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/unvown/blobs.git/info/lfs/objects/batch")
        .header(header::AUTHORIZATION, basic_header("git", &token))
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body_s = String::from_utf8_lossy(&bytes);
    assert!(
        body_s.contains("email") || body_s.contains("auth.email_unverified"),
        "must surface email_unverified — {body_s}"
    );

    // Download still OK for unverified + PAT on public.
    let dl = serde_json::json!({
        "operation": "download",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": 1 }]
    });
    let dl_req = Request::builder()
        .method("POST")
        .uri("/unvown/blobs.git/info/lfs/objects/batch")
        .header(header::AUTHORIZATION, basic_header("git", &token))
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(dl.to_string()))
        .unwrap();
    let dl_res = app.oneshot(dl_req).await.unwrap();
    assert_eq!(
        dl_res.status(),
        StatusCode::OK,
        "unverified may still download"
    );
}

/// Admin RPC can enable; disabled repos reject batch with LFS JSON error.
#[tokio::test]
async fn lfs_enable_disabled_repo_rejects_batch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_en.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos, lfs).await;

    let (cookie, login_v) = signup_and_login(&app, "en@ex.com", "enown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"blobs","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);

    // Default disabled → batch forbidden.
    let oid = "e".repeat(64);
    let body = serde_json::json!({
        "operation": "download",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": 1 }]
    });
    let disabled_req = Request::builder()
        .method("POST")
        .uri("/enown/blobs.git/info/lfs/objects/batch")
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let disabled_res = app.clone().oneshot(disabled_req).await.unwrap();
    assert_eq!(disabled_res.status(), StatusCode::FORBIDDEN);
    let disabled_bytes = disabled_res.into_body().collect().await.unwrap().to_bytes();
    let disabled_v: serde_json::Value = serde_json::from_slice(&disabled_bytes).unwrap();
    assert!(
        disabled_v["message"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("lfs"),
        "clear LFS error — {disabled_v}"
    );

    // Admin owner enables via RPC.
    let enable = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.lfs.setEnabled","input":{"owner":"enown","name":"blobs","enabled":true}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(enable.status(), StatusCode::OK);
    let enable_bytes = enable.into_body().collect().await.unwrap().to_bytes();
    let enable_v: serde_json::Value = serde_json::from_slice(&enable_bytes).unwrap();
    assert_eq!(enable_v["data"]["enabled"], true);

    let enabled_req = Request::builder()
        .method("POST")
        .uri("/enown/blobs.git/info/lfs/objects/batch")
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let enabled_res = app.clone().oneshot(enabled_req).await.unwrap();
    assert_eq!(enabled_res.status(), StatusCode::OK);

    // Non-admin (Write collaborator) cannot toggle.
    let (collab_cookie, collab_v) = signup_and_login(&app, "col@ex.com", "coluser").await;
    let collab_id = collab_v["data"]["id"].as_str().unwrap();
    verify_user(&db, collab_id).await;
    let add = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"enown","name":"blobs","username":"coluser","permission":"write"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(add.status(), StatusCode::OK, "add collab");

    let deny = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.lfs.setEnabled","input":{"owner":"enown","name":"blobs","enabled":false}}"#,
            &collab_cookie,
        ))
        .await
        .unwrap();
    // Soft-deny: Admin gate returns repo.not_found → HTTP 404 (same as visibility).
    assert_eq!(
        deny.status(),
        StatusCode::NOT_FOUND,
        "non-Admin toggle must soft-deny"
    );
    let deny_bytes = deny.into_body().collect().await.unwrap().to_bytes();
    let deny_v: serde_json::Value = serde_json::from_slice(&deny_bytes).unwrap();
    assert!(
        deny_v["error"].is_object(),
        "non-Admin must not toggle — {deny_v}"
    );
}

/// Quota / max-size rejects with clear LFS errors (D-LFS-12/14).
#[tokio::test]
async fn lfs_quota_over_quota_upload_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_quota.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    // Tiny repo quota via Admin override storage.
    db.update_lfs_settings(Some(1024), Some(32), Some(1024 * 1024))
        .await
        .expect("set quotas");
    let app = test_app(db.clone(), repos, lfs).await;

    let (cookie, login_v) = signup_and_login(&app, "q@ex.com", "qown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"blobs","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    db.set_repo_lfs_enabled(create_v["data"]["id"].as_str().unwrap(), true)
        .await
        .unwrap();

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"lfs","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().unwrap();

    // Over max object (1024).
    let big = vec![0u8; 2000];
    let oid = sha256_hex(&big);
    let body = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": big.len() }]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/qown/blobs.git/info/lfs/objects/batch")
        .header(header::AUTHORIZATION, basic_header("git", token))
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["objects"][0]["error"]["code"], 422);

    // Within max but over repo quota (32).
    let mid = b"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"; // 33 bytes
    let oid2 = sha256_hex(mid);
    let body2 = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{ "oid": oid2, "size": mid.len() }]
    });
    let req2 = Request::builder()
        .method("POST")
        .uri("/qown/blobs.git/info/lfs/objects/batch")
        .header(header::AUTHORIZATION, basic_header("git", token))
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body2.to_string()))
        .unwrap();
    let res2 = app.oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let bytes2 = res2.into_body().collect().await.unwrap().to_bytes();
    let v2: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
    assert_eq!(v2["objects"][0]["error"]["code"], 507, "repo quota — {v2}");
}

/// Admin override RPC affects subsequent upload reject threshold.
#[tokio::test]
async fn lfs_quota_admin_override_affects_enforcement() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_admin_q.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let hash = oxidean_api::auth::hash_password_str("password1").expect("hash");
    let admin_id = uuid::Uuid::new_v4().to_string();
    db.create_user(
        &admin_id,
        "adm@ex.com",
        "admlfs",
        Some(&hash),
        "Admin",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .expect("create admin");

    let app = test_app(db.clone(), repos, lfs).await;
    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"adm@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);

    let upd = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.lfs.updateSettings","input":{"max_object_bytes":64,"quota_repo_bytes":10000,"quota_user_bytes":100000}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(upd.status(), StatusCode::OK, "admin update");
    let upd_bytes = upd.into_body().collect().await.unwrap().to_bytes();
    let upd_v: serde_json::Value = serde_json::from_slice(&upd_bytes).unwrap();
    assert_eq!(upd_v["data"]["max_object_bytes"], 64);
    assert_eq!(upd_v["data"]["max_object_bytes_overridden"], true);

    verify_user(&db, &admin_id).await;
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"lim","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    db.set_repo_lfs_enabled(create_v["data"]["id"].as_str().unwrap(), true)
        .await
        .unwrap();

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"lfs","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().unwrap();

    let payload = vec![1u8; 100];
    let oid = sha256_hex(&payload);
    let body = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": payload.len() }]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/admlfs/lim.git/info/lfs/objects/batch")
        .header(header::AUTHORIZATION, basic_header("git", token))
        .header(header::ACCEPT, "application/vnd.git-lfs+json")
        .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["objects"][0]["error"]["code"], 422, "{v}");
}

/// Dedup: second repo linking existing OID omits upload actions (D-LFS-02).
#[tokio::test]
async fn lfs_dedup_existing_oid_omits_upload_actions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_dedup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos, lfs.clone()).await;

    let (cookie_a, login_a) = signup_and_login(&app, "a@ex.com", "aown").await;
    verify_user(&db, login_a["data"]["id"].as_str().unwrap()).await;
    let (cookie_b, login_b) = signup_and_login(&app, "b@ex.com", "bown").await;
    verify_user(&db, login_b["data"]["id"].as_str().unwrap()).await;

    async fn create_enabled(app: &axum::Router, db: &Database, cookie: &str, name: &str) -> String {
        let body = format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"public","description":""}}}}"#
        );
        let create = app
            .clone()
            .oneshot(rpc_req_with_cookie(&body, cookie))
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::OK);
        let bytes = create.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let id = v["data"]["id"].as_str().unwrap().to_string();
        db.set_repo_lfs_enabled(&id, true).await.unwrap();
        id
    }

    create_enabled(&app, &db, &cookie_a, "one").await;
    let repo_b = create_enabled(&app, &db, &cookie_b, "two").await;

    let pat_a = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"a","scopes":["repo"]}}"#,
            &cookie_a,
        ))
        .await
        .unwrap();
    let token_a = {
        let b = pat_a.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
        v["data"]["token"].as_str().unwrap().to_string()
    };
    let pat_b = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"b","scopes":["repo"]}}"#,
            &cookie_b,
        ))
        .await
        .unwrap();
    let token_b = {
        let b = pat_b.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
        v["data"]["token"].as_str().unwrap().to_string()
    };

    let payload = b"shared-lfs-bytes";
    let oid = sha256_hex(payload);
    let size = payload.len() as i64;

    let batch1 = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": size }]
    });
    let r1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/aown/one.git/info/lfs/objects/batch")
                .header(header::AUTHORIZATION, basic_header("git", &token_a))
                .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
                .body(Body::from(batch1.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let b1 = r1.into_body().collect().await.unwrap().to_bytes();
    let v1: serde_json::Value = serde_json::from_slice(&b1).unwrap();
    let href = v1["objects"][0]["actions"]["upload"]["href"]
        .as_str()
        .expect("upload href");
    let put = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(href)
                .header(header::AUTHORIZATION, basic_header("git", &token_a))
                .body(Body::from(payload.as_slice()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);

    let batch2 = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": size }]
    });
    let r2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bown/two.git/info/lfs/objects/batch")
                .header(header::AUTHORIZATION, basic_header("git", &token_b))
                .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
                .body(Body::from(batch2.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::OK);
    let b2 = r2.into_body().collect().await.unwrap().to_bytes();
    let v2: serde_json::Value = serde_json::from_slice(&b2).unwrap();
    assert!(
        v2["objects"][0]["actions"].is_null() || v2["objects"][0]["actions"]["upload"].is_null(),
        "second repo must omit upload — {v2}"
    );
    assert!(
        db.has_lfs_link(&repo_b, &oid).await.unwrap(),
        "link must exist after dedup batch"
    );
}

/// Verify endpoint checks oid+size (D-LFS-07).
#[tokio::test]
async fn lfs_verify_post_checks_size_and_oid() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_verify.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos, lfs).await;

    let (cookie, login_v) = signup_and_login(&app, "v@ex.com", "vown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"blobs","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    db.set_repo_lfs_enabled(create_v["data"]["id"].as_str().unwrap(), true)
        .await
        .unwrap();

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createClassic","input":{"name":"lfs","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let pat_bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().unwrap();

    let payload = b"hello-verify";
    let oid = sha256_hex(payload);
    let size = payload.len() as i64;

    let batch_body = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "objects": [{ "oid": oid, "size": size }]
    });
    let batch_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/vown/blobs.git/info/lfs/objects/batch")
                .header(header::AUTHORIZATION, basic_header("git", token))
                .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
                .body(Body::from(batch_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let batch_bytes = batch_res.into_body().collect().await.unwrap().to_bytes();
    let batch_v: serde_json::Value = serde_json::from_slice(&batch_bytes).unwrap();
    let href = batch_v["objects"][0]["actions"]["upload"]["href"]
        .as_str()
        .unwrap();
    assert!(
        batch_v["objects"][0]["actions"]["verify"]["href"]
            .as_str()
            .unwrap_or("")
            .contains("/verify"),
        "verify action — {batch_v}"
    );

    let put = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(href)
                .header(header::AUTHORIZATION, basic_header("git", token))
                .body(Body::from(payload.as_slice()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);

    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/vown/blobs.git/info/lfs/objects/verify")
                .header(header::AUTHORIZATION, basic_header("git", token))
                .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
                .body(Body::from(
                    serde_json::json!({"oid": oid, "size": size}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);

    let bad = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/vown/blobs.git/info/lfs/objects/verify")
                .header(header::AUTHORIZATION, basic_header("git", token))
                .header(header::CONTENT_TYPE, "application/vnd.git-lfs+json")
                .body(Body::from(
                    serde_json::json!({"oid": oid, "size": 999}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    // Range GET
    let get = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/vown/blobs.git/info/lfs/objects/{oid}"))
                .header(header::AUTHORIZATION, basic_header("git", token))
                .header(header::RANGE, "bytes=0-4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::PARTIAL_CONTENT);
    let got = get.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&got[..], &payload[0..=4]);
}
/// Session RPC surface for Settings/Admin/browser (14-08 / D-LFS-16/18/19).
#[tokio::test]
async fn lfs_session_rpc_status_usage_list_download() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_rpc.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos, lfs.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "rpc@ex.com", "rpcown").await;
    let user_id = login_v["data"]["id"].as_str().unwrap();
    verify_user(&db, user_id).await;

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"blobs","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let repo_id = create_v["data"]["id"].as_str().unwrap();

    app.clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.lfs.setEnabled","input":{"owner":"rpcown","name":"blobs","enabled":true}}"#,
            &cookie,
        ))
        .await
        .unwrap();

    let oid = "a".repeat(64);
    let payload = b"hello-lfs-rpc";
    db.upsert_lfs_object(&oid, payload.len() as i64)
        .await
        .unwrap();
    db.link_lfs_object(repo_id, &oid).await.unwrap();
    let shard = lfs.join(&oid[0..2]).join(&oid[2..4]);
    tokio::fs::create_dir_all(&shard).await.unwrap();
    tokio::fs::write(shard.join(&oid), payload).await.unwrap();

    let status = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.lfs.getStatus","input":{"owner":"rpcown","name":"blobs"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let status_v: serde_json::Value =
        serde_json::from_slice(&status.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(status_v["data"]["enabled"], true);
    assert_eq!(status_v["data"]["object_count"], 1);
    assert_eq!(status_v["data"]["logical_bytes"], payload.len() as i64);

    let usage = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.lfs.getUsage","input":{"owner":"rpcown","name":"blobs"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let usage_v: serde_json::Value =
        serde_json::from_slice(&usage.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(usage_v["data"]["objects"].as_array().unwrap().len(), 1);

    let list = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.lfs.listObjects","input":{"owner":"rpcown","name":"blobs"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let list_v: serde_json::Value =
        serde_json::from_slice(&list.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(list_v["data"]["objects"][0]["oid"], oid);

    let dl = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            &format!(
                r#"{{"procedure":"repo.lfs.download","input":{{"owner":"rpcown","name":"blobs","oid":"{oid}"}}}}"#
            ),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(dl.status(), StatusCode::OK);
    let dl_v: serde_json::Value =
        serde_json::from_slice(&dl.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(dl_v["data"]["encoding"], "base64");
    assert!(!dl_v["data"]["content"].as_str().unwrap().is_empty());
}

/// admin.lfs.getUsage requires sys-admin (D-LFS-19).
#[tokio::test]
async fn lfs_admin_get_usage_breakdown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let lfs = dir.path().join("lfs");
    let url = format!("sqlite:{}", dir.path().join("lfs_adm_u.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let hash = oxidean_api::auth::hash_password_str("password1").expect("hash");
    let admin_id = uuid::Uuid::new_v4().to_string();
    db.create_user(
        &admin_id,
        "admu@ex.com",
        "admu",
        Some(&hash),
        "Admin",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .expect("create admin");
    verify_user(&db, &admin_id).await;

    let app = test_app(db.clone(), repos, lfs.clone()).await;
    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"admu@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    let cookie = session_cookie_from_response(&login);

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"lim","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let create_bytes = create.into_body().collect().await.unwrap().to_bytes();
    let create_v: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let repo_id = create_v["data"]["id"].as_str().unwrap();

    let oid = "b".repeat(64);
    let payload = b"admin-usage";
    db.upsert_lfs_object(&oid, payload.len() as i64)
        .await
        .unwrap();
    db.link_lfs_object(repo_id, &oid).await.unwrap();
    let shard = lfs.join(&oid[0..2]).join(&oid[2..4]);
    tokio::fs::create_dir_all(&shard).await.unwrap();
    tokio::fs::write(shard.join(&oid), payload).await.unwrap();

    let usage = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"admin.lfs.getUsage","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(usage.status(), StatusCode::OK);
    let usage_v: serde_json::Value =
        serde_json::from_slice(&usage.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(usage_v["ok"], true);
    assert!(usage_v["data"]["physical_bytes"].as_i64().unwrap() >= payload.len() as i64);
    assert_eq!(usage_v["data"]["object_count"], 1);
    assert!(!usage_v["data"]["by_repo"].as_array().unwrap().is_empty());
    assert!(!usage_v["data"]["by_owner"].as_array().unwrap().is_empty());
}
