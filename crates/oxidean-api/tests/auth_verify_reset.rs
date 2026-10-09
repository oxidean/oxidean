//! AUTH-04 / AUTH-12: verify + password-reset issue/consume.

mod support;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::auth::verify_reset;
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

async fn test_app(db: Database) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

async fn test_app_with_recorder(db: Database) -> (axum::Router, Arc<RecordingSender>) {
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

async fn signup_user(
    app: &axum::Router,
    email: &str,
    username: &str,
) -> (String, String, serde_json::Value) {
    let body = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
    );
    let signup = app.clone().oneshot(rpc_req(&body)).await.unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&signup);
    let bytes = signup.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().expect("id").to_string();
    (cookie, user_id, v)
}

/// Backdate token `created_at` so the 60s min-interval does not block the next issue.
async fn backdate_verify_created_at(db: &Database, user_id: &str, secs_ago: i64) {
    let at = (chrono::Utc::now() - chrono::Duration::seconds(secs_ago))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    db.set_email_token_created_at(user_id, "verify", &at)
        .await
        .expect("backdate");
}

fn extract_otp_from_verify_email(text: &str) -> String {
    let marker = "Or enter this 8-digit code:";
    let after = text
        .split_once(marker)
        .unwrap_or_else(|| panic!("missing otp phrase in: {text}"))
        .1;
    // OTP is the first 8-digit run after the marker (ignore later "30 minutes").
    let mut code = String::new();
    for c in after.chars() {
        if c.is_ascii_digit() {
            code.push(c);
            if code.len() == 8 {
                return code;
            }
        } else if !code.is_empty() {
            code.clear();
        }
    }
    panic!("no 8-digit otp after marker in: {text}");
}

#[tokio::test]
async fn issue_otp_consume_sets_email_verified_on_me() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("verify_reset.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, user_id, signup_v) = signup_user(&app, "user@ex.com", "user1").await;
    assert_eq!(signup_v["data"]["email_verified"], false);

    let me_before = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.me","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(me_before.status(), StatusCode::OK);
    let me_before_bytes = me_before.into_body().collect().await.unwrap().to_bytes();
    let me_before_v: serde_json::Value = serde_json::from_slice(&me_before_bytes).unwrap();
    assert_eq!(me_before_v["data"]["email_verified"], false);

    let secrets = verify_reset::issue_verify(&db, &user_id)
        .await
        .expect("issue_verify");
    assert_eq!(secrets.otp.len(), 8);
    assert!(secrets.otp.chars().all(|c| c.is_ascii_digit()));

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
    let verify_bytes = verify.into_body().collect().await.unwrap().to_bytes();
    let verify_v: serde_json::Value = serde_json::from_slice(&verify_bytes).unwrap();
    assert_eq!(verify_v["data"]["email_verified"], true);

    let me_after = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.me","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(me_after.status(), StatusCode::OK);
    let me_after_bytes = me_after.into_body().collect().await.unwrap().to_bytes();
    let me_after_v: serde_json::Value = serde_json::from_slice(&me_after_bytes).unwrap();
    assert_eq!(me_after_v["data"]["email_verified"], true);
}

#[tokio::test]
async fn request_verify_sends_magic_and_otp_email() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "https://app.example.com");
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("req_verify.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let (cookie, user_id, _) = signup_user(&app, "v@ex.com", "verifyme").await;
    // Clear welcome + signup auto-verify so we assert request_verify alone.
    recorder.sent.lock().expect("lock").clear();
    // Signup already issued a verify token — backdate so request_verify is not rate-limited.
    backdate_verify_created_at(&db, &user_id, 61).await;

    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.request_verify","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);

    let sent = recorder.sent.lock().expect("lock");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].subject, "Verify your Oxidean email");
    assert!(
        sent[0]
            .text
            .contains("https://app.example.com/verify?token="),
        "body: {}",
        sent[0].text
    );
    assert!(
        sent[0].text.contains("Or enter this 8-digit code:"),
        "body: {}",
        sent[0].text
    );
    assert!(
        sent[0].text.contains("30 minutes") || sent[0].text.contains("30 minute"),
        "body: {}",
        sent[0].text
    );
    std::env::remove_var("OXIDEAN_PUBLIC_ORIGIN");
}

#[tokio::test]
async fn magic_token_consume_sets_verified() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("magic.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, user_id, _) = signup_user(&app, "magic@ex.com", "magicuser").await;
    let secrets = verify_reset::issue_verify(&db, &user_id)
        .await
        .expect("issue");

    let verify_body = format!(
        r#"{{"procedure":"auth.verify","input":{{"token":"{}"}}}}"#,
        secrets.magic
    );
    let verify = app
        .oneshot(rpc_req_with_cookie(&verify_body, &cookie))
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::OK);
    let bytes = verify.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["data"]["email_verified"], true);
}

#[tokio::test]
async fn verify_wrong_session_user_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("wrong_user.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_c1, user1, _) = signup_user(&app, "a@ex.com", "usera").await;
    let (cookie2, _, _) = signup_user(&app, "b@ex.com", "userb").await;
    let secrets = verify_reset::issue_verify(&db, &user1)
        .await
        .expect("issue");

    let verify_body = format!(
        r#"{{"procedure":"auth.verify","input":{{"code":"{}"}}}}"#,
        secrets.otp
    );
    let verify = app
        .oneshot(rpc_req_with_cookie(&verify_body, &cookie2))
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::BAD_REQUEST);
    let bytes = verify.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.invalid_token");
}

#[tokio::test]
async fn resend_replaces_prior_and_rate_limits_within_60s() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("resend.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let (cookie, user_id, _) = signup_user(&app, "r@ex.com", "resender").await;
    recorder.sent.lock().expect("lock").clear();
    backdate_verify_created_at(&db, &user_id, 61).await;

    let first = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.resend_verify","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let first_otp = {
        let sent = recorder.sent.lock().expect("lock");
        assert_eq!(sent.len(), 1);
        extract_otp_from_verify_email(&sent[0].text)
    };

    let second = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.resend_verify","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::BAD_REQUEST);
    let bytes = second.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.rate_limited");

    // Old OTP must be invalid after a successful replace — issue again after backdate.
    backdate_verify_created_at(&db, &user_id, 61).await;
    recorder.sent.lock().expect("lock").clear();
    let third = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.resend_verify","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(third.status(), StatusCode::OK);

    let old_verify = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            &format!(r#"{{"procedure":"auth.verify","input":{{"code":"{first_otp}"}}}}"#),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(old_verify.status(), StatusCode::BAD_REQUEST);

    let new_otp = {
        let sent = recorder.sent.lock().expect("lock");
        assert_eq!(sent.len(), 1);
        extract_otp_from_verify_email(&sent[0].text)
    };
    assert_ne!(first_otp, new_otp);
}

#[tokio::test]
async fn sixth_issue_within_hour_rate_limited() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("hourly.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, user_id, _) = signup_user(&app, "h@ex.com", "hourly").await;
    // Signup consumed issue #1 — backdate and count the next four + sixth.
    backdate_verify_created_at(&db, &user_id, 61).await;

    for i in 0..4 {
        if i > 0 {
            backdate_verify_created_at(&db, &user_id, 61).await;
        }
        let res = app
            .clone()
            .oneshot(rpc_req_with_cookie(
                r#"{"procedure":"auth.request_verify","input":{}}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "issue {i} should succeed");
    }

    backdate_verify_created_at(&db, &user_id, 61).await;
    let sixth = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.request_verify","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(sixth.status(), StatusCode::BAD_REQUEST);
    let bytes = sixth.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.rate_limited");
}

#[tokio::test]
async fn ten_failed_otp_attempts_invalidate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("attempts.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, user_id, _) = signup_user(&app, "t@ex.com", "tryhard").await;
    let secrets = verify_reset::issue_verify(&db, &user_id)
        .await
        .expect("issue");

    for i in 0..10 {
        let res = app
            .clone()
            .oneshot(rpc_req_with_cookie(
                r#"{"procedure":"auth.verify","input":{"code":"00000000"}}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "attempt {i}");
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["code"], "auth.invalid_token");
    }

    let good = app
        .oneshot(rpc_req_with_cookie(
            &format!(
                r#"{{"procedure":"auth.verify","input":{{"code":"{}"}}}}"#,
                secrets.otp
            ),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(good.status(), StatusCode::BAD_REQUEST);
    let bytes = good.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.invalid_token");
}

fn extract_otp_from_reset_email(text: &str) -> String {
    let marker = "Or enter this 8-digit code:";
    let after = text
        .split_once(marker)
        .unwrap_or_else(|| panic!("missing otp phrase in: {text}"))
        .1;
    let mut code = String::new();
    for c in after.chars() {
        if c.is_ascii_digit() {
            code.push(c);
            if code.len() == 8 {
                return code;
            }
        } else if !code.is_empty() {
            code.clear();
        }
    }
    panic!("no 8-digit otp after marker in: {text}");
}

async fn rpc_json(app: &axum::Router, body: &str) -> (StatusCode, serde_json::Value) {
    let res = app.clone().oneshot(rpc_req(body)).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, v)
}

/// AUTH-12 / D-28: unknown, local-password, and SSO-only share identical success payload.
#[tokio::test]
async fn request_password_reset_anti_enumeration_identical_success() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "https://app.example.com");
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("reset_req.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let (_cookie, _user_id, _) = signup_user(&app, "local@ex.com", "localuser").await;
    recorder.sent.lock().expect("lock").clear();

    // SSO-only: password_hash null
    db.create_user(
        "sso-user-id",
        "sso@ex.com",
        "ssouser",
        None,
        "ssouser",
        "",
        None,
        oxidean_core::Role::User,
    )
    .await
    .expect("create sso user");

    let unknown_body =
        r#"{"procedure":"auth.request_password_reset","input":{"email":"nobody@ex.com"}}"#;
    let local_body =
        r#"{"procedure":"auth.request_password_reset","input":{"email":"Local@ex.com"}}"#;
    let sso_body = r#"{"procedure":"auth.request_password_reset","input":{"email":"sso@ex.com"}}"#;

    let (st_u, v_u) = rpc_json(&app, unknown_body).await;
    let (st_l, v_l) = rpc_json(&app, local_body).await;
    let (st_s, v_s) = rpc_json(&app, sso_body).await;

    assert_eq!(st_u, StatusCode::OK);
    assert_eq!(st_l, StatusCode::OK);
    assert_eq!(st_s, StatusCode::OK);
    assert_eq!(v_u, v_l, "unknown vs local must match");
    assert_eq!(v_u, v_s, "unknown vs sso-only must match");
    assert_eq!(v_u["ok"], true);
    assert_eq!(v_u["data"]["ok"], true);

    let sent = recorder.sent.lock().expect("lock");
    assert_eq!(sent.len(), 1, "only local-password account gets email");
    assert_eq!(sent[0].subject, "Reset your Oxidean password");
    assert!(
        sent[0].text.contains("/reset-password?token="),
        "body: {}",
        sent[0].text
    );
    assert!(
        sent[0].text.contains("Or enter this 8-digit code:"),
        "body: {}",
        sent[0].text
    );
    let _otp = extract_otp_from_reset_email(&sent[0].text);
    std::env::remove_var("OXIDEAN_PUBLIC_ORIGIN");
}

/// Soft rate limit: second reset within 60s still returns anti-enumeration ok (no extra mail).
#[tokio::test]
async fn request_password_reset_rate_limit_swallows_into_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("reset_rl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let (_cookie, user_id, _) = signup_user(&app, "rl@ex.com", "rluser").await;
    recorder.sent.lock().expect("lock").clear();

    let body = r#"{"procedure":"auth.request_password_reset","input":{"email":"rl@ex.com"}}"#;
    let (st1, v1) = rpc_json(&app, body).await;
    assert_eq!(st1, StatusCode::OK);
    assert_eq!(v1["data"]["ok"], true);
    assert_eq!(recorder.sent.lock().expect("lock").len(), 1);

    // No backdate — within 60s min interval.
    let (st2, v2) = rpc_json(&app, body).await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(v2["data"]["ok"], true);
    assert_eq!(
        recorder.sent.lock().expect("lock").len(),
        1,
        "rate-limited issue must not send another email"
    );

    // After interval, another send is allowed.
    let at = (chrono::Utc::now() - chrono::Duration::seconds(61))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    db.set_email_token_created_at(&user_id, "reset", &at)
        .await
        .expect("backdate reset");
    let (st3, _) = rpc_json(&app, body).await;
    assert_eq!(st3, StatusCode::OK);
    assert_eq!(recorder.sent.lock().expect("lock").len(), 2);
}

/// AUTH-12 / D-27: redeem sets password, revokes other sessions, signs in this device.
#[tokio::test]
async fn reset_password_token_revokes_others_and_signs_in() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("reset_redeem.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie_a, user_id, _) = signup_user(&app, "reset@ex.com", "resetme").await;
    let login_b = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"reset@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login_b.status(), StatusCode::OK);
    let cookie_b = session_cookie_from_response(&login_b);
    assert_ne!(cookie_a, cookie_b);

    let secrets = verify_reset::issue_reset(&db, &user_id)
        .await
        .expect("issue_reset");

    let reset_body = format!(
        r#"{{"procedure":"auth.reset_password","input":{{"token":"{}","password":"newpass99"}}}}"#,
        secrets.magic
    );
    let reset = app.clone().oneshot(rpc_req(&reset_body)).await.unwrap();
    assert_eq!(reset.status(), StatusCode::OK);
    let new_cookie = session_cookie_from_response(&reset);
    let bytes = reset.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["email"], "reset@ex.com");

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
        let me_bytes = me.into_body().collect().await.unwrap().to_bytes();
        let me_v: serde_json::Value = serde_json::from_slice(&me_bytes).unwrap();
        assert_eq!(me_v["error"]["code"], "auth.unauthenticated");
    }

    let me_new = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"auth.me","input":{}}"#,
            &new_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(me_new.status(), StatusCode::OK);

    let login_old = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"reset@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login_old.status(), StatusCode::BAD_REQUEST);
    let old_bytes = login_old.into_body().collect().await.unwrap().to_bytes();
    let old_v: serde_json::Value = serde_json::from_slice(&old_bytes).unwrap();
    assert_eq!(old_v["error"]["code"], "auth.invalid_credentials");

    let login_new = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"reset@ex.com","password":"newpass99","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login_new.status(), StatusCode::OK);
}

#[tokio::test]
async fn reset_password_otp_consume() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("reset_otp.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_cookie, user_id, _) = signup_user(&app, "otp@ex.com", "otpreset").await;
    let secrets = verify_reset::issue_reset(&db, &user_id)
        .await
        .expect("issue_reset");

    let reset_body = format!(
        r#"{{"procedure":"auth.reset_password","input":{{"code":"{}","password":"newpass99"}}}}"#,
        secrets.otp
    );
    let reset = app.oneshot(rpc_req(&reset_body)).await.unwrap();
    assert_eq!(reset.status(), StatusCode::OK);
    let _cookie = session_cookie_from_response(&reset);
}

#[tokio::test]
async fn reset_password_sso_only_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("reset_sso.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    db.create_user(
        "sso-reset-id",
        "ssoreset@ex.com",
        "ssoreset",
        None,
        "ssoreset",
        "",
        None,
        oxidean_core::Role::User,
    )
    .await
    .expect("sso user");
    let secrets = verify_reset::issue_reset(&db, "sso-reset-id")
        .await
        .expect("plant token");

    let reset_body = format!(
        r#"{{"procedure":"auth.reset_password","input":{{"token":"{}","password":"newpass99"}}}}"#,
        secrets.magic
    );
    let reset = app.oneshot(rpc_req(&reset_body)).await.unwrap();
    assert_eq!(reset.status(), StatusCode::BAD_REQUEST);
    let bytes = reset.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.sso_only");
}

#[tokio::test]
async fn reset_password_invalid_token_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("reset_bad.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    let (_cookie, _, _) = signup_user(&app, "bad@ex.com", "badreset").await;
    let reset = app
        .oneshot(rpc_req(
            r#"{"procedure":"auth.reset_password","input":{"token":"deadbeef","password":"newpass99"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(reset.status(), StatusCode::BAD_REQUEST);
    let bytes = reset.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.invalid_token");
}

#[tokio::test]
async fn reset_password_weak_password_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("reset_weak.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_cookie, user_id, _) = signup_user(&app, "weak@ex.com", "weakreset").await;
    let secrets = verify_reset::issue_reset(&db, &user_id)
        .await
        .expect("issue");

    let reset_body = format!(
        r#"{{"procedure":"auth.reset_password","input":{{"token":"{}","password":"short"}}}}"#,
        secrets.magic
    );
    let reset = app.oneshot(rpc_req(&reset_body)).await.unwrap();
    assert_eq!(reset.status(), StatusCode::BAD_REQUEST);
    let bytes = reset.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "auth.weak_password");
}
