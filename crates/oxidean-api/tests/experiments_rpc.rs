//! WI-0001: `experiments.list` / `experiments.set` over the RPC shell —
//! anonymous list fails closed, unknown keys are rejected, enabled flags
//! surface on `auth.me` and the returned `UserPublic`.

mod support;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(db: Database) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

fn rpc(body: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1");
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    b.body(Body::from(body.to_owned())).unwrap()
}

fn session_cookie(res: &axum::http::Response<Body>) -> String {
    res.headers()
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

async fn seed_and_login(app: &axum::Router) -> String {
    let res = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"auth.signup","input":{"email":"flag@ex.com","username":"flaguser","password":"password1"}}"#,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let login = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"auth.login","input":{"identifier":"flag@ex.com","password":"password1","remember_me":false}}"#,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    session_cookie(&login)
}

async fn rpc_json(app: &axum::Router, body: &str, cookie: Option<&str>) -> serde_json::Value {
    let res = app.clone().oneshot(rpc(body, cookie)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn anonymous_list_fails_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("exp-anon.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    let v = rpc_json(&app, r#"{"procedure":"experiments.list","input":{}}"#, None).await;
    let experiments = v["data"]["experiments"].as_array().expect("experiments");
    assert!(
        experiments.iter().any(|e| e["key"] == "pr-new-ui"),
        "registry must include pr-new-ui: {v}"
    );
    assert!(
        experiments.iter().all(|e| e["enabled"] == false),
        "anonymous callers must see every flag disabled: {v}"
    );
}

#[tokio::test]
async fn set_list_and_auth_me_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("exp-user.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;
    let cookie = seed_and_login(&app).await;

    // Fresh session → no flags.
    let v = rpc_json(
        &app,
        r#"{"procedure":"experiments.list","input":{}}"#,
        Some(&cookie),
    )
    .await;
    assert!(
        v["data"]["experiments"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["enabled"] == false),
        "new user starts with all flags off: {v}"
    );

    // Unknown keys are rejected before persistence.
    let res = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"experiments.set","input":{"key":"does-not-exist","enabled":true}}"#,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "experiments.unknown_key");

    // Enable → echoed on the returned UserPublic.
    let v = rpc_json(
        &app,
        r#"{"procedure":"experiments.set","input":{"key":"pr-new-ui","enabled":true}}"#,
        Some(&cookie),
    )
    .await;
    assert!(
        v["data"]["experiments"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("pr-new-ui")),
        "set must return the updated UserPublic: {v}"
    );

    // Persisted: list + auth.me both reflect it.
    let v = rpc_json(
        &app,
        r#"{"procedure":"experiments.list","input":{}}"#,
        Some(&cookie),
    )
    .await;
    let pr_flag = v["data"]["experiments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["key"] == "pr-new-ui")
        .expect("pr-new-ui");
    assert_eq!(pr_flag["enabled"], true);

    let v = rpc_json(&app, r#"{"procedure":"auth.me","input":{}}"#, Some(&cookie)).await;
    assert!(
        v["data"]["experiments"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("pr-new-ui")),
        "auth.me must carry enabled flags: {v}"
    );

    // Disable → gone again, idempotent.
    for _ in 0..2 {
        rpc_json(
            &app,
            r#"{"procedure":"experiments.set","input":{"key":"pr-new-ui","enabled":false}}"#,
            Some(&cookie),
        )
        .await;
    }
    let v = rpc_json(
        &app,
        r#"{"procedure":"experiments.list","input":{}}"#,
        Some(&cookie),
    )
    .await;
    let pr_flag = v["data"]["experiments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["key"] == "pr-new-ui")
        .expect("pr-new-ui");
    assert_eq!(pr_flag["enabled"], false);
}

#[tokio::test]
async fn set_requires_session() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("exp-noauth.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    let res = app
        .oneshot(rpc(
            r#"{"procedure":"experiments.set","input":{"key":"pr-new-ui","enabled":true}}"#,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.unauthenticated");
}
