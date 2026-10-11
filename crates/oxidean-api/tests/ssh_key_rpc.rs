//! GIT-04: sshKey.add / list / revoke + verified gate (D-SSH-05).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

const ED25519_A: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJqxgqAG6vw46mOJ8QZKNpHEoPuP5sW2YoBlT/24OycR laptop@oxidean";
const FP_A: &str = "SHA256:QfwEFKimvVWrMaGsq4bBXsvtQgICjbmQryZxKXx/WyI";
const ED25519_B: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFkMMC69ejkQKaKcprTo6FkLAcsqsUEGD5dbJ7ma5tyi ci@oxidean";

async fn test_app(db: Database) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
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

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> (StatusCode, serde_json::Value) {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, v)
}

fn add_body(title: &str, public_key: &str) -> String {
    let pk = serde_json::to_string(public_key).unwrap();
    format!(
        r#"{{"procedure":"sshKey.add","input":{{"title":{title},"public_key":{pk}}}}}"#,
        title = serde_json::to_string(title).unwrap(),
        pk = pk
    )
}

/// Verified `sshKey.add` with title + ed25519 OpenSSH line returns list item with SHA256 fingerprint
/// and no secret/private-key reveal (D-SSH-05).
#[tokio::test]
async fn ssh_key_add_verified_ed25519_returns_fingerprint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("ssh_add.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "ssh@ex.com", "sshuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (status, v) = rpc_json(&app, &add_body("laptop", ED25519_A), &cookie).await;
    assert_eq!(status, StatusCode::OK, "sshKey.add — {v}");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["title"], "laptop");
    assert_eq!(v["data"]["fingerprint"], FP_A);
    assert_eq!(v["data"]["key_type"], "ssh-ed25519");
    assert!(v["data"].get("token").is_none());
    assert!(v["data"].get("secret").is_none());
    assert!(v["data"].get("private_key").is_none());
}

/// `sshKey.list` after add includes the new key (created_at DESC — ASSUME GIT-04 ordering).
#[tokio::test]
async fn ssh_key_list_after_add() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("ssh_list.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "list@ex.com", "listuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (_, a) = rpc_json(&app, &add_body("first", ED25519_A), &cookie).await;
    assert_eq!(a["ok"], true, "{a}");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let (_, b) = rpc_json(&app, &add_body("second", ED25519_B), &cookie).await;
    assert_eq!(b["ok"], true, "{b}");

    let (status, v) = rpc_json(&app, r#"{"procedure":"sshKey.list","input":{}}"#, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true);
    let arr = v["data"].as_array().expect("array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["title"], "second", "newest first");
    assert_eq!(arr[1]["title"], "first");
}

/// `sshKey.revoke` hard-deletes; key absent from subsequent list (ASSUME hard-delete).
#[tokio::test]
async fn ssh_key_revoke_removes_from_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("ssh_revoke.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "rev@ex.com", "revuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (_, added) = rpc_json(&app, &add_body("laptop", ED25519_A), &cookie).await;
    assert_eq!(added["ok"], true, "{added}");
    let id = added["data"]["id"].as_str().expect("id");

    let (status, rev) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"sshKey.revoke","input":{{"id":"{id}"}}}}"#),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rev}");
    assert_eq!(rev["ok"], true, "{rev}");

    let (_, listed) = rpc_json(&app, r#"{"procedure":"sshKey.list","input":{}}"#, &cookie).await;
    let arr = listed["data"].as_array().expect("array");
    assert!(arr.is_empty(), "revoked key must be absent — {listed}");

    let (st2, missing) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"sshKey.revoke","input":{{"id":"{id}"}}}}"#),
        &cookie,
    )
    .await;
    assert_eq!(st2, StatusCode::BAD_REQUEST, "{missing}");
    assert_eq!(missing["ok"], false);
    assert_eq!(missing["error"]["code"], "sshKey.not_found");
}

/// Unverified session cannot add keys → `auth.email_unverified` (require_verified / D-SSH-05).
#[tokio::test]
async fn ssh_key_add_unverified_email_unverified() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("ssh_unv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    let (cookie, _) = signup_and_login(&app, "unv@ex.com", "unvuser").await;
    // no set_email_verified_at
    let (status, v) = rpc_json(&app, &add_body("laptop", ED25519_A), &cookie).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.email_unverified");
}

/// Empty / whitespace title on add → `sshKey.title_required` (D-SSH-05).
#[tokio::test]
async fn ssh_key_add_empty_title_required() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("ssh_title.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "title@ex.com", "titleuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (status, v) = rpc_json(&app, &add_body("   ", ED25519_A), &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "sshKey.title_required");
}

/// Duplicate fingerprint (same public key) → domain uniqueness error (D-SSH-05 / T-09-02).
#[tokio::test]
async fn ssh_key_add_duplicate_fingerprint_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("ssh_dup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "dup@ex.com", "dupuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (_, first) = rpc_json(&app, &add_body("one", ED25519_A), &cookie).await;
    assert_eq!(first["ok"], true, "{first}");
    let (status, v) = rpc_json(&app, &add_body("two", ED25519_A), &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "sshKey.fingerprint_taken");
}

/// 26th key for one user → max 25 rejected (GIT-04 / D-SSH-05).
#[tokio::test]
async fn ssh_key_add_26th_key_max_25() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("ssh_max.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "max@ex.com", "maxuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    // Seed 25 keys directly (unique fingerprints) then RPC the 26th.
    for i in 0..25 {
        let fp = format!("SHA256:seed{i:064}");
        let pk = format!("ssh-ed25519 AAAA{i:044} seed{i}");
        db.create_ssh_key(
            &format!("seed-key-{i}"),
            &user_id,
            &format!("seed-{i}"),
            &pk,
            &fp,
            "ssh-ed25519",
            true,
            true,
        )
        .await
        .expect("seed key");
    }

    let (status, v) = rpc_json(&app, &add_body("overflow", ED25519_A), &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "sshKey.limit_exceeded");
}
