//! RFC 8628 device authorization grant — `POST /oauth/device/code`,
//! session-gated `oauth.deviceInfo` / `oauth.deviceAuthorize`, and the
//! device_code arm of `POST /oauth/token` (pending → approved/denied →
//! minted classic PAT, single-use, poll-interval enforcement).

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

fn rpc_req(body: &str, cookie: &str) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1");
    if !cookie.is_empty() {
        b = b.header("cookie", cookie);
    }
    b.body(Body::from(body.to_owned())).unwrap()
}

fn session_cookie_from_response(res: &axum::http::Response<Body>) -> String {
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

async fn signup_login_verify(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
) -> (String, String) {
    let signup_body = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
    );
    let signup = app
        .clone()
        .oneshot(rpc_req(&signup_body, ""))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login_body = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
    );
    let login = app.clone().oneshot(rpc_req(&login_body, "")).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    (cookie, user_id)
}

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> (StatusCode, serde_json::Value) {
    let res = app.clone().oneshot(rpc_req(body, cookie)).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn device_code_post(scope: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/oauth/device/code")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(format!("scope={scope}")))
        .unwrap()
}

fn device_poll(device_code: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/oauth/token")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&device_code={device_code}"
        )))
        .unwrap()
}

async fn request_grant(app: &axum::Router, scope: &str) -> serde_json::Value {
    let res = app.clone().oneshot(device_code_post(scope)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn poll(app: &axum::Router, device_code: &str) -> (StatusCode, serde_json::Value) {
    let res = app.clone().oneshot(device_poll(device_code)).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn device_grant_full_flow() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("oauth_device.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, uid) = signup_login_verify(&app, &db, "dev@ex.com", "devuser").await;

    // 1. Device code request (public endpoint — CLI calls it unauthenticated).
    let grant = request_grant(&app, "repo package:read").await;
    let device_code = grant["device_code"].as_str().expect("device_code");
    let user_code = grant["user_code"].as_str().expect("user_code");
    assert!(
        device_code.starts_with(oxidean_core::OAUTH_DEVICE_CODE_PREFIX),
        "{device_code}"
    );
    assert_eq!(user_code.len(), 9, "XXXX-XXXX — {user_code}");
    assert!(user_code.chars().nth(4) == Some('-'));
    assert!(grant["verification_uri"]
        .as_str()
        .unwrap()
        .ends_with("/oauth/device"));
    assert!(grant["verification_uri_complete"]
        .as_str()
        .unwrap()
        .contains(&format!("user_code={user_code}")));
    assert_eq!(grant["expires_in"], 900);
    assert_eq!(grant["interval"], 5);

    // 2. First poll → authorization_pending.
    let (status, v) = poll(&app, device_code).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"], "authorization_pending");

    // 3. Immediate second poll → slow_down (interval enforcement).
    let (status, v) = poll(&app, device_code).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"], "slow_down");

    // 4. deviceInfo needs a session; with one it returns the pending grant.
    let (status, v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"oauth.deviceInfo","input":{{"user_code":"{user_code}"}}}}"#),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    let (status, v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"oauth.deviceInfo","input":{{"user_code":"{user_code}"}}}}"#),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["user_code"], user_code);
    assert_eq!(
        v["data"]["scopes"],
        serde_json::json!(["repo", "package:read"])
    );
    // Dashes/case are normalized for lookup.
    let squashed = user_code.replace('-', "").to_lowercase();
    let (status, v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"oauth.deviceInfo","input":{{"user_code":"{squashed}"}}}}"#),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");

    // 5. Approve → next poll mints a classic PAT usable as Bearer.
    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"oauth.deviceAuthorize","input":{{"user_code":"{user_code}","approve":true}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");

    let (status, v) = poll(&app, device_code).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let pat = v["access_token"].as_str().expect("access_token");
    assert!(
        pat.starts_with(oxidean_core::CLASSIC_PAT_PREFIX),
        "device grant mints a classic PAT — {pat}"
    );
    assert_eq!(v["token_type"], "bearer");
    assert_eq!(v["scope"], "repo package:read");

    // The minted PAT authenticates as the approving user.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .header("authorization", format!("Bearer {pat}"))
                .body(Body::from(r#"{"procedure":"auth.me","input":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let me: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(me["data"]["id"], uid);
    assert_eq!(me["data"]["username"], "devuser");

    // 6. Single-use — the code is consumed; re-poll → invalid_grant.
    let (status, v) = poll(&app, device_code).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"], "invalid_grant");
}

#[tokio::test]
async fn device_grant_denied_and_unknown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("oauth_device_deny.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;
    let (cookie, _uid) = signup_login_verify(&app, &db, "dev@ex.com", "devuser").await;

    let grant = request_grant(&app, "repo").await;
    let device_code = grant["device_code"].as_str().unwrap();
    let user_code = grant["user_code"].as_str().unwrap();

    // Deny → access_denied on poll.
    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"oauth.deviceAuthorize","input":{{"user_code":"{user_code}","approve":false}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (status, v) = poll(&app, device_code).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"], "access_denied");

    // Second decision on a settled code → not found.
    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"oauth.deviceAuthorize","input":{{"user_code":"{user_code}","approve":true}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "oauth.device_code_not_found");

    // Unknown device_code → invalid_grant.
    let (status, v) = poll(&app, "oxidean_odc_bogus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"], "invalid_grant");

    // Unknown user_code for deviceInfo → 404-class error.
    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"oauth.deviceInfo","input":{"user_code":"ZZZZ-ZZZZ"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "oauth.device_code_not_found");
}

#[tokio::test]
async fn device_code_scope_validation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("oauth_device_scope.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    // Unknown scope name → invalid_scope.
    let res = app
        .clone()
        .oneshot(device_code_post("bogus:scope"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"], "invalid_scope");

    // Empty scope → first-party set (approver can narrow on the device page).
    let grant = request_grant(&app, "").await;
    assert!(grant["device_code"].as_str().unwrap().len() > 8);
    let (cookie, _uid) = signup_login_verify(&app, &db, "scopes@ex.com", "scopeuser").await;
    let user_code = grant["user_code"].as_str().unwrap();
    let (status, v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"oauth.deviceInfo","input":{{"user_code":"{user_code}"}}}}"#),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let scopes = v["data"]["scopes"].as_array().unwrap();
    assert!(
        scopes.len() >= 10,
        "scope-less grant should resolve the first-party set — {scopes:?}"
    );
}

#[tokio::test]
async fn device_approval_scope_narrowing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("oauth_device_narrow.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;
    let (cookie, _uid) = signup_login_verify(&app, &db, "dev@ex.com", "devuser").await;

    let device_authorize = |user_code: &str, granted: Option<&str>, approve: bool| {
        let granted_json = granted
            .map(|g| format!(r#""granted_scope":"{g}","#))
            .unwrap_or_default();
        format!(
            r#"{{"procedure":"oauth.deviceAuthorize","input":{{"user_code":"{user_code}",{granted_json}"approve":{approve}}}}}"#
        )
    };

    // Approver keeps a subset → minted PAT carries only the kept scopes.
    let grant = request_grant(&app, "repo workflow delete_repo").await;
    let device_code = grant["device_code"].as_str().unwrap().to_string();
    let user_code = grant["user_code"].as_str().unwrap().to_string();
    let (status, v) = rpc_json(
        &app,
        &device_authorize(&user_code, Some("repo workflow"), true),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    let (status, v) = poll(&app, &device_code).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["scope"], "repo workflow", "{v}");

    // Superset (scope not in the grant's request) → oauth.invalid_scope.
    let grant = request_grant(&app, "repo").await;
    let user_code = grant["user_code"].as_str().unwrap().to_string();
    let (_s, v) = rpc_json(
        &app,
        &device_authorize(&user_code, Some("repo workflow"), true),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "oauth.invalid_scope", "{v}");

    // Unknown scope name → oauth.invalid_scope.
    let (_s, v) = rpc_json(
        &app,
        &device_authorize(&user_code, Some("bogus-scope"), true),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "oauth.invalid_scope", "{v}");

    // Empty granted string → rpc.bad_input.
    let (_s, v) = rpc_json(&app, &device_authorize(&user_code, Some(""), true), &cookie).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "rpc.bad_input", "{v}");

    // Deny ignores granted_scope entirely (bad value does not block denial).
    let (_s, v) = rpc_json(
        &app,
        &device_authorize(&user_code, Some("bogus-scope"), false),
        &cookie,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");

    // Un-narrowed approve keeps the requested set.
    let grant = request_grant(&app, "repo package:read").await;
    let device_code = grant["device_code"].as_str().unwrap().to_string();
    let user_code = grant["user_code"].as_str().unwrap().to_string();
    let (_s, v) = rpc_json(&app, &device_authorize(&user_code, None, true), &cookie).await;
    assert_eq!(v["ok"], true, "{v}");
    let (status, v) = poll(&app, &device_code).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["scope"], "repo package:read", "{v}");
}
