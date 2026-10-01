//! AUTH-04 tracer: require_verified + auth.dev.privileged_ping.


mod support;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::auth::verify_reset;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(db: Database, env_name: &str) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, env_name);
    let cors = build_cors(env_name, None).unwrap_or_else(|_| {
        // production cors needs origins; use development builder for test isolation
        build_cors("development", None).expect("cors")
    });
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

#[tokio::test]
async fn unverified_privileged_ping_forbidden_then_ok_after_otp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("verify_gate.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), "development").await;

    // Login while unverified must succeed (D-06).
    let signup = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"user@ex.com","username":"user1","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"user@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let login_bytes = login.into_body().collect().await.unwrap().to_bytes();
    let login_v: serde_json::Value = serde_json::from_slice(&login_bytes).unwrap();
    assert_eq!(login_v["data"]["email_verified"], false);
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();

    let ping_denied = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.dev.privileged_ping","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(ping_denied.status(), StatusCode::FORBIDDEN);
    let denied_bytes = ping_denied.into_body().collect().await.unwrap().to_bytes();
    let denied_v: serde_json::Value = serde_json::from_slice(&denied_bytes).unwrap();
    assert_eq!(denied_v["error"]["code"], "auth.email_unverified");

    let secrets = verify_reset::issue_verify(&db, &user_id)
        .await
        .expect("issue_verify");
    let verify_body = format!(
        r#"{{"procedure":"auth.verify","input":{{"code":"{}"}}}}"#,
        secrets.otp
    );
    let verify = app
        .clone()
        .oneshot(rpc_req_with_cookie(&verify_body, &cookie))
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::OK);

    let ping_ok = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.dev.privileged_ping","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(ping_ok.status(), StatusCode::OK);
    let ok_bytes = ping_ok.into_body().collect().await.unwrap().to_bytes();
    let ok_v: serde_json::Value = serde_json::from_slice(&ok_bytes).unwrap();
    assert_eq!(ok_v["data"]["ok"], true);
}

#[tokio::test]
async fn privileged_ping_unknown_outside_env_allowlist() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("verify_gate_prod.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    // Production is outside {development,dev,test,compose}; procedure must not exist.
    let app = test_app(db, "production").await;

    let signup = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"user@ex.com","username":"user1","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&signup);

    let ping = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.dev.privileged_ping","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(ping.status(), StatusCode::NOT_FOUND);
    let bytes = ping.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "rpc.unknown_procedure");
}

/// D-03/D-15: SSO IdP email_verified=true → me.email_verified and privileged_ping without local OTP.
#[tokio::test]
async fn idp_trust_verified_sso_user_privileged_ping_ok() {
    use oxidean_api::auth::external::{link_or_create_user, ExternalIdentity};
    use oxidean_api::auth::session::SessionService;

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("verify_gate_idp.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), "development").await;

    let identity = ExternalIdentity {
        provider: "workos".into(),
        provider_subject: "user_idp_trust".into(),
        email: "idp-trust@ex.com".into(),
        display_name: Some("IdP Trust".into()),
        email_verified: true,
    };
    let (user, _) = link_or_create_user(&db, &identity)
        .await
        .expect("link_or_create");
    assert!(
        user.email_verified_at.is_some(),
        "IdP-trust must set email_verified_at"
    );

    let sessions = SessionService::new("development");
    let (_token, cookie) = sessions
        .create(&db, &user.id, false, None, None)
        .await
        .expect("session");
    let cookie_header = cookie.to_string();
    let cookie_pair = cookie_header.split(';').next().unwrap().trim();

    let me = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.me","input":{}}"#,
            cookie_pair,
        ))
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);
    let me_bytes = me.into_body().collect().await.unwrap().to_bytes();
    let me_v: serde_json::Value = serde_json::from_slice(&me_bytes).unwrap();
    assert_eq!(me_v["data"]["email_verified"], true);

    let ping = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.dev.privileged_ping","input":{}}"#,
            cookie_pair,
        ))
        .await
        .unwrap();
    assert_eq!(ping.status(), StatusCode::OK);
    let ping_bytes = ping.into_body().collect().await.unwrap().to_bytes();
    let ping_v: serde_json::Value = serde_json::from_slice(&ping_bytes).unwrap();
    assert_eq!(ping_v["data"]["ok"], true);
}
