//! gpgKey.add / list / revoke + SSH usage flags.

mod support;

use std::process::Stdio;
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

async fn make_test_gpg_armor(email: &str) -> String {
    let dir = tempfile::tempdir().expect("gpg home");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let status = tokio::process::Command::new("gpg")
        .args([
            "--batch",
            "--passphrase",
            "",
            "--quick-generate-key",
            &format!("Oxidean Fixture <{email}>"),
            "ed25519",
            "default",
            "never",
        ])
        .env("GNUPGHOME", dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .expect("spawn gpg");
    assert!(status.success(), "gpg --quick-generate-key failed");

    let output = tokio::process::Command::new("gpg")
        .args(["--armor", "--export", email])
        .env("GNUPGHOME", dir.path())
        .output()
        .await
        .expect("gpg export");
    assert!(output.status.success(), "gpg --export failed");
    String::from_utf8(output.stdout).expect("utf8 armor")
}

#[tokio::test]
async fn ssh_key_add_rejects_both_usages_false() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("ssh_usage.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "usage@ex.com", "usageuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let body = format!(
        r#"{{"procedure":"sshKey.add","input":{{"title":"x","public_key":{},"can_authenticate":false,"can_sign":false}}}}"#,
        serde_json::to_string(ED25519_A).unwrap()
    );
    let (status, v) = rpc_json(&app, &body, &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "sshKey.usage_required");
}

#[tokio::test]
async fn gpg_key_add_list_revoke_roundtrip() {
    if tokio::process::Command::new("gpg")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        eprintln!("skipping: gpg not available");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("gpg_round.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "gpg@ex.com", "gpguser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let armor = make_test_gpg_armor("fixture@example.com").await;
    let body = serde_json::json!({
        "procedure": "gpgKey.add",
        "input": {
            "title": "laptop",
            "armored_public_key": armor,
        }
    })
    .to_string();

    let (status, v) = rpc_json(&app, &body, &cookie).await;
    assert_eq!(status, StatusCode::OK, "gpgKey.add — {v}");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["title"], "laptop");
    assert!(!v["data"]["fingerprint"].as_str().unwrap_or("").is_empty());
    assert!(!v["data"]["key_id"].as_str().unwrap_or("").is_empty());
    let id = v["data"]["id"].as_str().expect("id").to_string();
    let emails = v["data"]["uid_emails"].as_array().expect("uid_emails");
    assert!(
        emails
            .iter()
            .any(|e| e.as_str() == Some("fixture@example.com")),
        "{v}"
    );

    let (status, list_v) =
        rpc_json(&app, r#"{"procedure":"gpgKey.list","input":{}}"#, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{list_v}");
    assert_eq!(list_v["data"].as_array().unwrap().len(), 1);

    let revoke = serde_json::json!({
        "procedure": "gpgKey.revoke",
        "input": { "id": id }
    })
    .to_string();
    let (status, rev_v) = rpc_json(&app, &revoke, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{rev_v}");
    assert_eq!(rev_v["data"]["ok"], true);

    let (status, list_v) =
        rpc_json(&app, r#"{"procedure":"gpgKey.list","input":{}}"#, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{list_v}");
    assert!(list_v["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn gpg_key_add_rejects_garbage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("gpg_bad.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "gpgbad@ex.com", "gpgbad").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let body = serde_json::json!({
        "procedure": "gpgKey.add",
        "input": {
            "title": "bad",
            "armored_public_key": "not-a-key",
        }
    })
    .to_string();
    let (status, v) = rpc_json(&app, &body, &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "gpgKey.invalid_key");
}
