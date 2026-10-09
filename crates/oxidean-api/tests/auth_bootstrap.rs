//! AUTH-07 bootstrap + D-13 partial ENV (AUTH-06 adjacency).
//! Strict RPC allowlist (D-11) covered here; SSO start reject is in auth_callbacks.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
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

async fn rpc_json(app: axum::Router, body: &str) -> serde_json::Value {
    let res = app.oneshot(rpc_req(body)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// D-13: email-only ENV must not seed — wizard path (`needs_setup` true).
#[tokio::test]
async fn bootstrap_partial_env_email_only_needs_setup() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("partial_email.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::set_var("OXIDEAN_ADMIN_EMAIL", "admin@example.com");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");
    std::env::remove_var("OXIDEAN_ALLOW_SIGNUP");

    oxidean_api::auth::seed::maybe_seed_admin(&db)
        .await
        .expect("partial ENV must not error");
    assert_eq!(
        db.count_users().await.expect("count"),
        0,
        "partial ENV (email-only) must not seed"
    );

    let app = test_app(db).await;
    let v = rpc_json(app, r#"{"procedure":"auth.bootstrap_status","input":{}}"#).await;
    assert_eq!(v["ok"], true);
    assert_eq!(
        v["data"]["needs_setup"], true,
        "D-13: partial ENV → needs_setup true (wizard path)"
    );

    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
}

/// D-13: password-only ENV must not seed — wizard path.
#[tokio::test]
async fn bootstrap_partial_env_password_only_needs_setup() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("partial_pw.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::set_var("OXIDEAN_ADMIN_PASSWORD", "adminpass1");

    oxidean_api::auth::seed::maybe_seed_admin(&db)
        .await
        .expect("partial ENV must not error");
    assert_eq!(db.count_users().await.expect("count"), 0);

    let app = test_app(db).await;
    let v = rpc_json(app, r#"{"procedure":"auth.bootstrap_status","input":{}}"#).await;
    assert_eq!(v["ok"], true);
    assert_eq!(
        v["data"]["needs_setup"], true,
        "D-13: partial ENV → needs_setup true"
    );

    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");
}

/// D-11: while needs_setup, non-allowlisted RPC fails; allowlisted procs succeed.
#[tokio::test]
async fn bootstrap_strict_rpc_allowlist_while_needs_setup() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("allowlist.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");

    let app = test_app(db.clone()).await;
    let login = rpc_json(
        app,
        r#"{"procedure":"auth.login","input":{"identifier":"x@ex.com","password":"password1","remember_me":false}}"#,
    )
    .await;
    assert_eq!(login["ok"], false);
    assert_eq!(
        login["error"]["code"], "auth.setup_required",
        "D-11: auth.login blocked while needs_setup"
    );

    let app_me = test_app(db.clone()).await;
    let me = rpc_json(app_me, r#"{"procedure":"auth.me","input":{}}"#).await;
    assert_eq!(me["ok"], false);
    assert_eq!(
        me["error"]["code"], "auth.setup_required",
        "D-11: auth.me blocked while needs_setup"
    );

    let app_su = test_app(db.clone()).await;
    let signup = rpc_json(
        app_su,
        r#"{"procedure":"auth.signup","input":{"email":"ada@example.com","username":"ada","password":"password1"}}"#,
    )
    .await;
    assert_eq!(signup["ok"], false);
    assert_eq!(
        signup["error"]["code"], "auth.setup_required",
        "D-11: auth.signup blocked while needs_setup"
    );

    let app2 = test_app(db.clone()).await;
    let status = rpc_json(app2, r#"{"procedure":"auth.bootstrap_status","input":{}}"#).await;
    assert_eq!(status["ok"], true, "allowlist: auth.bootstrap_status");

    let app3 = test_app(db.clone()).await;
    let health = app3
        .oneshot(rpc_req(r#"{"procedure":"system.health","input":{}}"#))
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    let health_v: serde_json::Value =
        serde_json::from_slice(&health.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(health_v["ok"], true, "allowlist: system.health");

    let app4 = test_app(db).await;
    let setup = rpc_json(
        app4,
        r#"{"procedure":"auth.bootstrap_setup","input":{"email":"owner@example.com","username":"owner","password":"password1","allow_signup":false}}"#,
    )
    .await;
    assert_eq!(
        setup["ok"], true,
        "allowlist: auth.bootstrap_setup must succeed while needs_setup"
    );
}

/// Setup wizard must reject path-colliding reserved usernames (e.g. `admin` vs `/admin/*`).
#[tokio::test]
async fn bootstrap_rejects_reserved_admin_username() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("reserved.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");

    let app = test_app(db).await;
    let res = rpc_json(
        app,
        r#"{"procedure":"auth.bootstrap_setup","input":{"email":"owner@example.com","username":"admin","password":"password1","allow_signup":false}}"#,
    )
    .await;
    assert_eq!(res["ok"], false, "{res}");
    assert_eq!(
        res["error"]["code"], "auth.reserved_username",
        "expected reserved username rejection, got {res}"
    );
}

/// AUTH-07: second bootstrap_setup → auth.setup_unavailable.
#[tokio::test]
async fn bootstrap_second_setup_unavailable() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("second.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");

    let app = test_app(db.clone()).await;
    let first = rpc_json(
        app,
        r#"{"procedure":"auth.bootstrap_setup","input":{"email":"owner@example.com","username":"owner","password":"password1","allow_signup":false}}"#,
    )
    .await;
    assert_eq!(first["ok"], true);

    let app2 = test_app(db).await;
    let second = rpc_json(
        app2,
        r#"{"procedure":"auth.bootstrap_setup","input":{"email":"other@example.com","username":"other","password":"password1","allow_signup":false}}"#,
    )
    .await;
    assert_eq!(second["ok"], false);
    assert_eq!(
        second["error"]["code"], "auth.setup_unavailable",
        "second bootstrap_setup must be unavailable"
    );
}

/// Wizard persists allow_signup; closed signup rejects auth.signup after bootstrap.
#[tokio::test]
async fn bootstrap_allow_signup_false_blocks_signup() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("closed.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");

    // Pre-open so default-false cannot mask a missing wizard write (D-08).
    let open = db.get_auth_settings().await.expect("settings");
    db.update_auth_settings(
        &open.provider_mode,
        &open.email_provider,
        open.from_address.as_deref(),
        open.oidc_issuer.as_deref(),
        open.oidc_client_id.as_deref(),
        open.workos_client_id.as_deref(),
        true,
        &open.default_visibility,
    )
    .await
    .expect("pre-open signup");

    let app = test_app(db.clone()).await;
    let setup = rpc_json(
        app,
        r#"{"procedure":"auth.bootstrap_setup","input":{"email":"owner@example.com","username":"owner","password":"password1","allow_signup":false}}"#,
    )
    .await;
    assert_eq!(setup["ok"], true);
    assert_eq!(
        setup["data"]["must_change_credentials"], false,
        "D-17: wizard-chosen credentials must not force credential change"
    );

    let settings = db.get_auth_settings().await.expect("settings");
    assert!(
        !settings.allow_signup,
        "wizard allow_signup=false must persist on instance_auth_settings"
    );

    let app2 = test_app(db).await;
    let signup = rpc_json(
        app2,
        r#"{"procedure":"auth.signup","input":{"email":"ada@example.com","username":"ada","password":"password1"}}"#,
    )
    .await;
    assert_eq!(signup["ok"], false);
    assert_eq!(
        signup["error"]["code"], "auth.signup_closed",
        "after bootstrap with allow_signup false, signup must be rejected"
    );
}

/// Wizard allow_signup=true persists open registration.
#[tokio::test]
async fn bootstrap_allow_signup_true_persists() {
    let _env = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("open_signup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    std::env::remove_var("OXIDEAN_ADMIN_EMAIL");
    std::env::remove_var("OXIDEAN_ADMIN_PASSWORD");

    let app = test_app(db.clone()).await;
    let setup = rpc_json(
        app,
        r#"{"procedure":"auth.bootstrap_setup","input":{"email":"owner@example.com","username":"owner","password":"password1","allow_signup":true}}"#,
    )
    .await;
    assert_eq!(setup["ok"], true);

    let settings = db.get_auth_settings().await.expect("settings");
    assert!(
        settings.allow_signup,
        "wizard allow_signup=true must persist on instance_auth_settings"
    );
}
