//! AUTH-06 forced credential confirm (D-16/D-17).

mod support;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use std::sync::Arc;
use tower::ServiceExt;

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

fn rpc_req_cookie(body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", cookie)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn seed_env_admin(db: &Database) {
    std::env::set_var("OXIDEAN_ADMIN_EMAIL", "admin@example.com");
    std::env::set_var("OXIDEAN_ADMIN_PASSWORD", "adminpass1");
    std::env::remove_var("OXIDEAN_ALLOW_SIGNUP");
    oxidean_api::auth::seed::maybe_seed_admin(db)
        .await
        .expect("seed");
    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");
}

/// Login ENV-seeded admin; return Set-Cookie value.
async fn try_admin_cookie(app: axum::Router) -> Option<String> {
    let res = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"admin@example.com","password":"adminpass1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    res.headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// D-16: confirm rejects username equal to `system-administrator` (case-insensitive).
#[tokio::test]
async fn confirm_admin_rejects_default_system_administrator_username() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("forced_reject.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    seed_env_admin(&db).await;

    let app = test_app(db.clone()).await;
    let cookie = try_admin_cookie(app)
        .await
        .expect("ENV-seeded admin must be able to log in");
    let app2 = test_app(db).await;
    let res = app2
        .oneshot(rpc_req_cookie(
            r#"{"procedure":"auth.confirm_admin_credentials","input":{"username":"system-administrator","keep_password":true}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(
        v["error"]["code"], "auth.invalid_username",
        "forced confirm must reject default username system-administrator"
    );
}

/// D-17: keep-password path succeeds when username changes away from default.
#[tokio::test]
async fn confirm_admin_keep_password_ok_with_new_username() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("forced_keep.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    seed_env_admin(&db).await;

    let app = test_app(db.clone()).await;
    let cookie = try_admin_cookie(app)
        .await
        .expect("ENV-seeded admin must be able to log in");
    let app2 = test_app(db.clone()).await;
    let res = app2
        .oneshot(rpc_req_cookie(
            r#"{"procedure":"auth.confirm_admin_credentials","input":{"username":"forge-admin","keep_password":true}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        v["ok"], true,
        "keep_password=true with non-default username must succeed"
    );
    assert_eq!(v["data"]["username"], "forge-admin");
    assert_eq!(v["data"]["must_change_credentials"], false);

    let user = db
        .find_user_by_email("admin@example.com")
        .await
        .expect("find")
        .expect("user");
    assert!(
        !user.must_change_credentials,
        "successful confirm must clear must_change_credentials"
    );
    assert_eq!(user.username, "forge-admin");
}
