//! AUTH-01: local signup with Set-Cookie, uniqueness, reserved username, welcome email.

mod support;

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailError, EmailSender, LogSink, OutboundEmail};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

#[derive(Default)]
struct RecordingSender {
    sent: Mutex<Vec<OutboundEmail>>,
}

#[async_trait::async_trait]
impl EmailSender for RecordingSender {
    async fn send(&self, msg: OutboundEmail) -> Result<(), EmailError> {
        self.sent.lock().expect("lock").push(msg);
        Ok(())
    }
}

async fn app_with_recorder(db: Database) -> (axum::Router, Arc<RecordingSender>) {
    let recorder = Arc::new(RecordingSender::default());
    let state = AppState::new(db, recorder.clone() as Arc<dyn EmailSender>, "development");
    let cors = build_cors("development", None).expect("cors");
    (router_with_state(state, cors), recorder)
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

#[tokio::test]
async fn signup_sets_cookie_and_sends_welcome() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("signup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = app_with_recorder(db).await;

    let res = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"Ada@Example.com","username":"ada","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let set_cookie = res
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap();
    assert!(
        set_cookie.contains("oxidean_session="),
        "cookie: {set_cookie}"
    );
    assert!(
        set_cookie.contains("HttpOnly") || set_cookie.to_ascii_lowercase().contains("httponly")
    );

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["email"], "ada@example.com");
    assert_eq!(v["data"]["username"], "ada");

    let sent = recorder.sent.lock().expect("lock");
    assert_eq!(sent.len(), 2, "welcome + verify emails");
    assert!(
        sent.iter().any(|m| m.subject == "Welcome to Oxidean"),
        "missing welcome"
    );
    let verify = sent
        .iter()
        .find(|m| m.subject == "Verify your Oxidean email")
        .expect("verify email");
    assert!(verify.text.contains("Or enter this 8-digit code:"));
    assert!(verify.text.contains("/verify?token="));
    assert_eq!(verify.to, "ada@example.com");
}

#[tokio::test]
async fn signup_duplicate_returns_taken() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("dup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = app_with_recorder(db).await;

    let res1 = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"a@ex.com","username":"alice","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res1.status(), StatusCode::OK);

    let res2 = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"a@ex.com","username":"alice2","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::BAD_REQUEST);
    let bytes = res2.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "auth.taken");
}

#[tokio::test]
async fn signup_reserved_username() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("reserved.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = app_with_recorder(db).await;

    let res = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"x@ex.com","username":"admin","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.reserved_username");
}

#[tokio::test]
async fn signup_works_with_log_sink_default_router() {
    // Smoke: default router path (LogSink) still accepts signup.
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("logsink.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
    let cors = build_cors("development", None).expect("cors");
    let app = router_with_state(state, cors);

    let res = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"b@ex.com","username":"bob","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn signup_open_without_invite_fields_auth05() {
    // AUTH-05 / D-08: signup succeeds with only email/username/password — no invite schema.
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("open.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = app_with_recorder(db).await;

    let body = r#"{"procedure":"auth.signup","input":{"email":"open@ex.com","username":"opener","password":"password1"}}"#;
    assert!(
        !body.contains("invite"),
        "signup request must not carry invite fields"
    );
    let res = app.oneshot(rpc_req(body)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["email_verified"], false);
}

#[tokio::test]
async fn seeded_admin_is_auto_verified() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("admin_seed.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::set_var("OXIDEAN_ADMIN_EMAIL", "Admin@Example.com");
    std::env::set_var("OXIDEAN_ADMIN_PASSWORD", "adminpass1");
    oxidean_api::auth::seed::maybe_seed_admin(&db)
        .await
        .expect("seed");
    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");

    let user = db
        .find_user_by_email("admin@example.com")
        .await
        .expect("find")
        .expect("seeded user");
    assert_eq!(user.role, oxidean_core::Role::SysAdmin);
    assert!(
        user.email_verified_at.is_some(),
        "D-04: seeded admin must be auto-verified"
    );
    assert_eq!(
        user.username, "system-administrator",
        "ENV seed must use username system-administrator"
    );
    assert!(
        user.must_change_credentials,
        "ENV-seeded system-administrator must have must_change_credentials=true (D-16)"
    );
    let settings = db.get_auth_settings().await.expect("settings");
    assert!(
        !settings.allow_signup,
        "OXIDEAN_ALLOW_SIGNUP unset must leave allow_signup=false"
    );

    // Second seed is a no-op when users exist.
    std::env::set_var("OXIDEAN_ADMIN_EMAIL", "other@example.com");
    std::env::set_var("OXIDEAN_ADMIN_PASSWORD", "adminpass1");
    oxidean_api::auth::seed::maybe_seed_admin(&db)
        .await
        .expect("second seed");
    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");
    assert_eq!(db.count_users().await.expect("count"), 1);
}

/// Wave 0: OXIDEAN_ALLOW_SIGNUP=true/1 must persist open signup on ENV seed (D-15).
#[tokio::test]
async fn seeded_admin_parses_allow_signup_true() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("admin_allow.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::set_var("OXIDEAN_ADMIN_EMAIL", "admin@example.com");
    std::env::set_var("OXIDEAN_ADMIN_PASSWORD", "adminpass1");
    std::env::set_var("OXIDEAN_ALLOW_SIGNUP", "true");
    oxidean_api::auth::seed::maybe_seed_admin(&db)
        .await
        .expect("seed");
    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");
    std::env::remove_var("OXIDEAN_ALLOW_SIGNUP");

    let settings = db.get_auth_settings().await.expect("settings");
    assert!(
        settings.allow_signup,
        "OXIDEAN_ALLOW_SIGNUP=true must set allow_signup on instance settings"
    );
}

/// D-13: empty-string ADMIN ENV is treated as absent (same as unset).
#[tokio::test]
async fn seed_partial_env_empty_string_email_does_not_seed() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("empty_email.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::set_var("OXIDEAN_ADMIN_EMAIL", "   ");
    std::env::set_var("OXIDEAN_ADMIN_PASSWORD", "adminpass1");
    oxidean_api::auth::seed::maybe_seed_admin(&db)
        .await
        .expect("empty email must not error");
    assert_eq!(
        db.count_users().await.expect("count"),
        0,
        "whitespace/empty OXIDEAN_ADMIN_EMAIL must not seed"
    );
    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");
}

/// D-13: empty-string password is treated as absent.
#[tokio::test]
async fn seed_partial_env_empty_string_password_does_not_seed() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("empty_pw.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::set_var("OXIDEAN_ADMIN_EMAIL", "admin@example.com");
    std::env::set_var("OXIDEAN_ADMIN_PASSWORD", "");
    oxidean_api::auth::seed::maybe_seed_admin(&db)
        .await
        .expect("empty password must not error");
    assert_eq!(
        db.count_users().await.expect("count"),
        0,
        "empty OXIDEAN_ADMIN_PASSWORD must not seed"
    );
    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");
}

/// D-05/D-07: after users exist with allow_signup false, auth.signup is rejected server-side.
#[tokio::test]
async fn signup_rejects_when_allow_signup_false() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("closed_signup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    // Bootstrap with a sys-admin but leave allow_signup at default false (do not unlock_signup).
    let id = uuid::Uuid::new_v4().to_string();
    let hash = oxidean_api::auth::hash_password_str("password1").expect("hash");
    db.create_user(
        &id,
        "admin@ex.com",
        "adminuser",
        Some(&hash),
        "Admin",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .expect("create admin");
    let settings = db.get_auth_settings().await.expect("settings");
    assert!(
        !settings.allow_signup,
        "default instance settings must fail closed"
    );

    let (app, _) = app_with_recorder(db).await;
    let res = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"ada@ex.com","username":"ada","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(
        v["error"]["code"], "auth.signup_closed",
        "closed signup must reject with auth.signup_closed (not invite)"
    );
}

/// D-05: allow_signup true → signup succeeds (welcome/verify retained).
#[tokio::test]
async fn signup_succeeds_when_allow_signup_true() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("open_signup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let settings = db.get_auth_settings().await.expect("settings");
    assert!(
        settings.allow_signup,
        "unlock_signup must open registration"
    );

    let (app, recorder) = app_with_recorder(db).await;
    let res = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"open2@ex.com","username":"opener2","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    let sent = recorder.sent.lock().expect("lock");
    assert!(
        sent.iter().any(|m| m.subject == "Welcome to Oxidean"),
        "open signup must retain welcome email"
    );
}

/// Empty instance: needs_setup still blocks signup before bootstrap (AUTH-07).
#[tokio::test]
async fn signup_setup_required_before_bootstrap() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("needs_setup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    assert_eq!(db.count_users().await.expect("count"), 0);

    let (app, _) = app_with_recorder(db).await;
    let res = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"early@ex.com","username":"early","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.setup_required");
}

/// D-06/D-07: auth.provider_config exposes allow_signup for public chrome (fail closed).
#[tokio::test]
async fn provider_config_includes_allow_signup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("provider_cfg.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    // Post-bootstrap, leave allow_signup at default false (do not unlock_signup).
    let id = uuid::Uuid::new_v4().to_string();
    let hash = oxidean_api::auth::hash_password_str("password1").expect("hash");
    db.create_user(
        &id,
        "admin@ex.com",
        "adminuser",
        Some(&hash),
        "Admin",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .expect("create admin");

    let (app, _) = app_with_recorder(db.clone()).await;
    let res = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.provider_config","input":{}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(
        v["data"]["allow_signup"], false,
        "provider_config must expose allow_signup=false by default"
    );
    assert!(v["data"]["mode"].is_string());

    // After opening signup, public config reflects true.
    support::unlock_signup(&db).await;
    let (app2, _) = app_with_recorder(db).await;
    let res2 = app2
        .oneshot(rpc_req(
            r#"{"procedure":"auth.provider_config","input":{}}"#,
        ))
        .await
        .unwrap();
    let bytes2 = res2.into_body().collect().await.unwrap().to_bytes();
    let v2: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
    assert_eq!(v2["data"]["allow_signup"], true);
}

/// AUTH-06 ordering / AUTH-07 adjacency: second seed with users present is a no-op.
#[tokio::test]
async fn seed_second_run_idempotent_when_users_exist() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("seed_idem.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::set_var("OXIDEAN_ADMIN_EMAIL", "admin@example.com");
    std::env::set_var("OXIDEAN_ADMIN_PASSWORD", "adminpass1");
    oxidean_api::auth::seed::maybe_seed_admin(&db)
        .await
        .expect("first seed");
    assert_eq!(db.count_users().await.expect("count"), 1);

    std::env::set_var("OXIDEAN_ADMIN_EMAIL", "other@example.com");
    std::env::set_var("OXIDEAN_ADMIN_PASSWORD", "differentpass1");
    std::env::set_var("OXIDEAN_ALLOW_SIGNUP", "true");
    oxidean_api::auth::seed::maybe_seed_admin(&db)
        .await
        .expect("second seed");
    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");
    std::env::remove_var("OXIDEAN_ALLOW_SIGNUP");

    assert_eq!(db.count_users().await.expect("count"), 1);
    let user = db
        .find_user_by_email("admin@example.com")
        .await
        .expect("find")
        .expect("original seeded user");
    assert_eq!(user.username, "system-administrator");
    // Second run must not rewrite allow_signup after users exist.
    let settings = db.get_auth_settings().await.expect("settings");
    assert!(
        !settings.allow_signup,
        "idempotent seed must not apply OXIDEAN_ALLOW_SIGNUP when users already exist"
    );
}
