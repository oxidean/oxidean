//! AUTH-02/03: login cookie → auth.me; logout; logout_all across devices.

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
    // Cookie header value is `name=value` (drop attributes).
    let pair = set_cookie.split(';').next().unwrap().trim();
    assert!(
        pair.starts_with("oxidean_session="),
        "unexpected Set-Cookie: {set_cookie}"
    );
    pair.to_string()
}

async fn seed_user(app: &axum::Router) {
    let res = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"user@ex.com","username":"user1","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_me_logout_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("session.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;
    seed_user(&app).await;

    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"user@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);

    let me = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.me","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);
    let me_bytes = me.into_body().collect().await.unwrap().to_bytes();
    let me_v: serde_json::Value = serde_json::from_slice(&me_bytes).unwrap();
    assert_eq!(me_v["data"]["username"], "user1");

    let logout = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.logout","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::OK);
    let clear = logout
        .headers()
        .get("set-cookie")
        .expect("clear Set-Cookie")
        .to_str()
        .unwrap();
    assert!(clear.contains("oxidean_session="));

    let me_after = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.me","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(me_after.status(), StatusCode::UNAUTHORIZED);
    let bytes = me_after.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.unauthenticated");
}

#[tokio::test]
async fn login_by_username_works() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("userlogin.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;
    seed_user(&app).await;

    let login = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"user1","password":"password1","remember_me":true}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["data"]["email"], "user@ex.com");
}

#[tokio::test]
async fn logout_all_revokes_other_sessions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("logoutall.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;
    seed_user(&app).await;

    let device_a = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"user@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    let cookie_a = session_cookie_from_response(&device_a);

    let device_b = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"user@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    let cookie_b = session_cookie_from_response(&device_b);
    assert_ne!(cookie_a, cookie_b);

    let logout_all = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.logout_all","input":{}}"#,
            &cookie_a,
        ))
        .await
        .unwrap();
    assert_eq!(logout_all.status(), StatusCode::OK);

    for cookie in [&cookie_a, &cookie_b] {
        let me = app
            .clone()
            .oneshot(rpc_req_with_cookie(
                r#"{"procedure":"auth.me","input":{}}"#,
                cookie,
            ))
            .await
            .unwrap();
        assert_eq!(me.status(), StatusCode::UNAUTHORIZED);
        let bytes = me.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["code"], "auth.unauthenticated");
    }
}
