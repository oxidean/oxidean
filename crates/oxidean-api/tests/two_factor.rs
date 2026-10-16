//! Two-factor auth end-to-end (issue #130): TOTP enrollment → login challenge
//! → factor completion mints the session; recovery codes are single-use;
//! instance/org policy produces enroll-kind challenges; admin reset clears.

mod support;

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::auth::two_factor::totp;
use oxidean_api::email::{EmailError, EmailSender, OutboundEmail};
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

async fn test_db(name: &str) -> Database {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join(format!("{name}.db")).display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    std::mem::forget(dir); // keep the tempdir alive for the db lifetime
    db
}

async fn test_app(db: Database) -> (axum::Router, Arc<RecordingSender>) {
    // TOTP secrets encrypt at rest — provide the env key every test needs.
    // Per-test-process isolation (nextest) keeps this env mutation safe.
    std::env::set_var("OXIDEAN_SESSION_SECRET", "two-factor-integration-key");
    let recorder = Arc::new(RecordingSender::default());
    let state = AppState::new(db, recorder.clone() as Arc<dyn EmailSender>, "development");
    let cors = build_cors("development", None).expect("cors");
    (router_with_state(state, cors), recorder)
}

fn rpc_req(body: &str, cookie: Option<&str>) -> Request<Body> {
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

async fn rpc(
    app: &axum::Router,
    body: &str,
    cookie: Option<&str>,
) -> (StatusCode, serde_json::Value, Vec<String>) {
    let res = app.clone().oneshot(rpc_req(body, cookie)).await.unwrap();
    let status = res.status();
    let cookies: Vec<String> = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| {
            v.to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .trim()
                .to_string()
        })
        .collect();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, v, cookies)
}

fn cookie_named<'a>(cookies: &'a [String], name: &str) -> Option<&'a str> {
    cookies
        .iter()
        .map(|s| s.as_str())
        .find(|c| c.starts_with(&format!("{name}=")))
}

async fn signup(app: &axum::Router, email: &str, username: &str) {
    let body = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
    );
    let (status, v, _) = rpc(app, &body, None).await;
    assert_eq!(status, StatusCode::OK, "signup {v}");
    assert_eq!(v["ok"], true, "signup {v}");
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

fn login_body(identifier: &str) -> String {
    format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{identifier}","password":"password1","remember_me":false}}}}"#
    )
}

async fn find_user_id(db: &Database, username: &str) -> String {
    db.find_user_by_username(username)
        .await
        .expect("find user")
        .expect("user exists")
        .id
}

fn totp_code(secret_b32: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    totp::code_at_unix(secret_b32, now).expect("totp code")
}

fn two_factor_req(procedure: &str, input: serde_json::Value) -> String {
    serde_json::json!({"procedure": procedure, "input": input}).to_string()
}

/// Enroll TOTP for the signed-in `cookie`; returns the one-time recovery codes.
async fn enroll_totp(app: &axum::Router, cookie: &str) -> Vec<String> {
    let (_, v, _) = rpc(
        app,
        &two_factor_req("auth.two_factor.totp_begin", serde_json::json!({})),
        Some(cookie),
    )
    .await;
    assert_eq!(v["ok"], true, "totp_begin {v}");
    let secret = v["data"]["secret"].as_str().unwrap().to_string();
    let uri = v["data"]["otpauth_uri"].as_str().unwrap();
    assert!(uri.starts_with("otpauth://totp/"), "otpauth uri {uri}");
    let code = totp_code(&secret);
    let (_, v, _) = rpc(
        app,
        &two_factor_req(
            "auth.two_factor.totp_confirm",
            serde_json::json!({"code": code}),
        ),
        Some(cookie),
    )
    .await;
    assert_eq!(v["ok"], true, "totp_confirm {v}");
    v["data"]["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn totp_enroll_then_login_challenge_mints_session_only_after_code() {
    let db = test_db("two_factor_totp").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "ada@example.com", "ada").await;
    let (status, v, cookies) = rpc(&app, &login_body("ada@example.com"), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "first login {v}");
    let session = cookie_named(&cookies, "oxidean_session")
        .expect("session cookie")
        .to_string();
    let user_id = find_user_id(&db, "ada").await;
    verify_user(&db, &user_id).await;

    // Status: no factor yet.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.status", serde_json::json!({})),
        Some(&session),
    )
    .await;
    assert_eq!(v["data"]["totp_enabled"], false, "{v}");
    assert_eq!(v["data"]["enroll_required"], false, "{v}");

    // Enroll TOTP — recovery codes come back exactly once.
    let codes = enroll_totp(&app, &session).await;
    assert_eq!(codes.len(), 10, "ten recovery codes");
    assert!(codes.iter().all(|c| c.len() == 11 && c.contains('-')));

    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.status", serde_json::json!({})),
        Some(&session),
    )
    .await;
    assert_eq!(v["data"]["totp_enabled"], true, "{v}");
    assert_eq!(v["data"]["recovery_codes_remaining"], 10, "{v}");

    // Next login: primary credential passes but no session — challenge instead
    // (RPC errors carry the matching HTTP status).
    let (status, v, cookies) = rpc(&app, &login_body("ada@example.com"), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["ok"], false, "login must not mint a session {v}");
    assert_eq!(v["error"]["code"], "auth.two_factor_required", "{v}");
    assert_eq!(v["error"]["data"]["kind"], "verify", "{v}");
    let methods: Vec<String> = v["error"]["data"]["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    for m in ["totp", "recovery", "email"] {
        assert!(methods.contains(&m.to_string()), "methods {methods:?}");
    }
    assert!(cookie_named(&cookies, "oxidean_session").is_none());
    let challenge_cookie = cookie_named(&cookies, "oxidean_2fa")
        .expect("2fa challenge cookie")
        .to_string();

    // Wrong code → error, challenge still alive.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.complete",
            serde_json::json!({"code": "000000"}),
        ),
        Some(&challenge_cookie),
    )
    .await;
    assert_eq!(v["ok"], false, "wrong code {v}");

    // Status via challenge cookie → pending verify metadata.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.status", serde_json::json!({})),
        Some(&challenge_cookie),
    )
    .await;
    assert_eq!(v["data"]["challenge_pending"], true, "{v}");
    assert_eq!(v["data"]["challenge_kind"], "verify", "{v}");

    // Correct factor → session minted (recovery code keeps it deterministic).
    let (status, v, cookies) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.complete",
            serde_json::json!({"code": codes[0]}),
        ),
        Some(&challenge_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "recovery-code complete {v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_some());
}

#[tokio::test]
async fn recovery_code_is_single_use() {
    let db = test_db("two_factor_recovery").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "bob@example.com", "bob").await;
    let (_, _v, cookies) = rpc(&app, &login_body("bob@example.com"), None).await;
    let session = cookie_named(&cookies, "oxidean_session")
        .unwrap()
        .to_string();
    let user_id = find_user_id(&db, "bob").await;
    verify_user(&db, &user_id).await;
    let codes = enroll_totp(&app, &session).await;

    // First use succeeds.
    let (_, _v, cookies) = rpc(&app, &login_body("bob@example.com"), None).await;
    let chal = cookie_named(&cookies, "oxidean_2fa").unwrap().to_string();
    let (_, v, cookies) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.complete",
            serde_json::json!({"code": codes[1]}),
        ),
        Some(&chal),
    )
    .await;
    assert_eq!(v["ok"], true, "first recovery use {v}");
    let session2 = cookie_named(&cookies, "oxidean_session")
        .unwrap()
        .to_string();

    // Same code a second time → rejected.
    let (_, _v, cookies) = rpc(&app, &login_body("bob@example.com"), None).await;
    let chal2 = cookie_named(&cookies, "oxidean_2fa").unwrap().to_string();
    let (_, v, _) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.complete",
            serde_json::json!({"code": codes[1]}),
        ),
        Some(&chal2),
    )
    .await;
    assert_eq!(v["ok"], false, "second use must fail {v}");

    // Remaining count dropped to 9.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.status", serde_json::json!({})),
        Some(&session2),
    )
    .await;
    assert_eq!(v["data"]["recovery_codes_remaining"], 9, "{v}");
}

#[tokio::test]
async fn instance_policy_enroll_challenge_completes_via_totp() {
    let db = test_db("two_factor_policy").await;
    support::unlock_signup(&db).await;
    // Require a local factor for password logins.
    let settings = db.get_auth_settings().await.expect("settings");
    db.update_auth_settings(
        &settings.provider_mode,
        &settings.email_provider,
        settings.from_address.as_deref(),
        settings.oidc_issuer.as_deref(),
        settings.oidc_client_id.as_deref(),
        settings.workos_client_id.as_deref(),
        settings.allow_signup,
        &settings.default_visibility,
        true, // require_two_factor_local
        settings.require_two_factor_external,
    )
    .await
    .expect("enable local 2fa policy");

    let (app, _mail) = test_app(db.clone()).await;

    // Signup under the policy opens the enroll challenge directly — the new
    // account cannot mint a session until a factor is enrolled.
    let body = r#"{"procedure":"auth.signup","input":{"email":"carol@example.com","username":"carol","password":"password1"}}"#;
    let (_, v, cookies) = rpc(&app, body, None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.two_factor_required", "{v}");
    assert_eq!(v["error"]["data"]["kind"], "enroll", "{v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_none());
    let chal = cookie_named(&cookies, "oxidean_2fa").unwrap().to_string();

    let user_id = find_user_id(&db, "carol").await;
    verify_user(&db, &user_id).await;

    // A factorless password login hits the same enroll challenge.
    let (_, v, cookies) = rpc(&app, &login_body("carol@example.com"), None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.two_factor_required", "{v}");
    assert_eq!(v["error"]["data"]["kind"], "enroll", "{v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_none());
    assert!(cookie_named(&cookies, "oxidean_2fa").is_some());

    // Enroll TOTP inside the challenge → codes + session minted.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.totp_begin", serde_json::json!({})),
        Some(&chal),
    )
    .await;
    assert_eq!(v["ok"], true, "challenge totp_begin {v}");
    let secret = v["data"]["secret"].as_str().unwrap().to_string();
    let code = totp_code(&secret);
    let (_, v, cookies) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.totp_confirm",
            serde_json::json!({"code": code}),
        ),
        Some(&chal),
    )
    .await;
    assert_eq!(v["ok"], true, "enroll-challenge confirm {v}");
    assert!(v["data"]["recovery_codes"].as_array().unwrap().len() == 10);
    let session = cookie_named(&cookies, "oxidean_session")
        .expect("session minted after enroll")
        .to_string();

    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.me", serde_json::json!({})),
        Some(&session),
    )
    .await;
    assert_eq!(v["ok"], true, "auth.me after enroll {v}");
    assert_eq!(v["data"]["two_factor_enabled"], true, "{v}");
}

#[tokio::test]
async fn org_require_two_factor_forces_enroll() {
    let db = test_db("two_factor_org").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "dave@example.com", "dave").await;
    let (_, _, cookies) = rpc(&app, &login_body("dave@example.com"), None).await;
    let session = cookie_named(&cookies, "oxidean_session")
        .unwrap()
        .to_string();
    let user_id = find_user_id(&db, "dave").await;
    verify_user(&db, &user_id).await;

    // Create an org (creator becomes owner) and flip require_two_factor.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req(
            "org.create",
            serde_json::json!({"slug": "acme-2fa", "display_name": "Acme"}),
        ),
        Some(&session),
    )
    .await;
    assert_eq!(v["ok"], true, "org.create {v}");
    let org_id = v["data"]["id"].as_str().unwrap().to_string();
    db.update_organization_settings(&org_id, None, None, Some(true))
        .await
        .expect("org require 2fa");

    // Next login → enroll challenge because the user belongs to the org.
    let (_, v, cookies) = rpc(&app, &login_body("dave@example.com"), None).await;
    assert_eq!(v["error"]["code"], "auth.two_factor_required", "{v}");
    assert_eq!(v["error"]["data"]["kind"], "enroll", "{v}");
    assert!(cookie_named(&cookies, "oxidean_2fa").is_some());
}

#[tokio::test]
async fn email_otp_completes_verify_challenge() {
    let db = test_db("two_factor_email").await;
    support::unlock_signup(&db).await;
    let (app, mail) = test_app(db.clone()).await;

    signup(&app, "erin@example.com", "erin").await;
    let (_, _, cookies) = rpc(&app, &login_body("erin@example.com"), None).await;
    let session = cookie_named(&cookies, "oxidean_session")
        .unwrap()
        .to_string();
    let user_id = find_user_id(&db, "erin").await;
    verify_user(&db, &user_id).await;
    enroll_totp(&app, &session).await;

    // Login → challenge; request the email code.
    let (_, v, cookies) = rpc(&app, &login_body("erin@example.com"), None).await;
    assert_eq!(v["error"]["code"], "auth.two_factor_required", "{v}");
    let chal = cookie_named(&cookies, "oxidean_2fa").unwrap().to_string();
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.email_send", serde_json::json!({})),
        Some(&chal),
    )
    .await;
    assert_eq!(v["ok"], true, "email_send {v}");

    // Pull the 8-digit code out of the recorded email.
    let otp = {
        let sent = mail.sent.lock().unwrap();
        sent.iter()
            .find(|m| m.subject.contains("sign-in code"))
            .expect("2fa email")
            .text
            .split_whitespace()
            .find(|w| w.len() == 8 && w.chars().all(|c| c.is_ascii_digit()))
            .expect("8-digit otp in email")
            .to_string()
    };

    let (_, v, cookies) = rpc(
        &app,
        &two_factor_req("auth.two_factor.complete", serde_json::json!({"code": otp})),
        Some(&chal),
    )
    .await;
    assert_eq!(v["ok"], true, "email otp complete {v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_some());
}

#[tokio::test]
async fn admin_reset_clears_all_factors() {
    let db = test_db("two_factor_admin").await;

    // Explicit sys-admin (independent of unlock_signup's seed helper).
    let admin_id = uuid::Uuid::new_v4().to_string();
    let admin_hash = oxidean_api::auth::hash_password_str("admin-pass-1").expect("hash");
    db.create_user(
        &admin_id,
        "root@example.com",
        "root",
        Some(&admin_hash),
        "Root",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .expect("create sys-admin");
    verify_user(&db, &admin_id).await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    let (_, v, cookies) = rpc(
        &app,
        r#"{"procedure":"auth.login","input":{"identifier":"root@example.com","password":"admin-pass-1","remember_me":false}}"#,
        None,
    )
    .await;
    assert_eq!(v["ok"], true, "admin login {v}");
    let admin_session = cookie_named(&cookies, "oxidean_session")
        .expect("admin session")
        .to_string();

    signup(&app, "fran@example.com", "fran").await;
    let (_, _, cookies) = rpc(&app, &login_body("fran@example.com"), None).await;
    let session = cookie_named(&cookies, "oxidean_session")
        .unwrap()
        .to_string();
    let user_id = find_user_id(&db, "fran").await;
    verify_user(&db, &user_id).await;
    enroll_totp(&app, &session).await;

    // Admin reset wipes TOTP + passkeys + recovery codes.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req(
            "admin.users.resetTwoFactor",
            serde_json::json!({"user_id": user_id}),
        ),
        Some(&admin_session),
    )
    .await;
    assert_eq!(v["ok"], true, "admin reset {v}");

    // Login now succeeds directly (no policy in this test).
    let (_, v, cookies) = rpc(&app, &login_body("fran@example.com"), None).await;
    assert_eq!(v["ok"], true, "login after reset {v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_some());
}

// ---------------------------------------------------------------------------
// WebAuthn/passkey ceremonies (real `webauthn-authenticator-rs` SoftPasskey)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn passkey_register_then_login_completes_via_assertion() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "http://localhost:8080");
    let db = test_db("two_factor_passkey").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "gwen@example.com", "gwen").await;
    let (_, _, cookies) = rpc(&app, &login_body("gwen@example.com"), None).await;
    let session = cookie_named(&cookies, "oxidean_session")
        .unwrap()
        .to_string();
    let user_id = find_user_id(&db, "gwen").await;
    verify_user(&db, &user_id).await;

    // Register a platform passkey while signed in.
    let origin =
        webauthn_authenticator_rs::prelude::Url::parse("http://localhost:8080").expect("origin");
    let mut authenticator = webauthn_authenticator_rs::prelude::WebauthnAuthenticator::new(
        webauthn_authenticator_rs::softpasskey::SoftPasskey::new(true),
    );

    let (_, v, _) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.passkey_register_begin",
            serde_json::json!({}),
        ),
        Some(&session),
    )
    .await;
    assert_eq!(v["ok"], true, "register_begin {v}");
    let ccr: webauthn_authenticator_rs::prelude::CreationChallengeResponse =
        serde_json::from_value(v["data"].clone()).expect("ccr json");
    let attestation = authenticator
        .do_registration(origin.clone(), ccr)
        .expect("softpasskey attestation");

    let (_, v, _) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.passkey_register_complete",
            serde_json::json!({
                "nickname": "Test SoftPasskey",
                "attestation": serde_json::to_value(&attestation).unwrap(),
            }),
        ),
        Some(&session),
    )
    .await;
    assert_eq!(v["ok"], true, "register_complete {v}");
    assert_eq!(v["data"]["nickname"], "Test SoftPasskey", "{v}");

    // Status now reports the passkey factor.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.status", serde_json::json!({})),
        Some(&session),
    )
    .await;
    assert_eq!(v["data"]["passkeys"].as_array().unwrap().len(), 1, "{v}");

    // Next login → verify challenge advertising `passkey`.
    let (_, v, cookies) = rpc(&app, &login_body("gwen@example.com"), None).await;
    assert_eq!(v["error"]["code"], "auth.two_factor_required", "{v}");
    let methods: Vec<String> = v["error"]["data"]["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    assert!(methods.contains(&"passkey".to_string()), "{methods:?}");
    let chal = cookie_named(&cookies, "oxidean_2fa").unwrap().to_string();

    // Assertion ceremony → session minted.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.passkey_begin", serde_json::json!({})),
        Some(&chal),
    )
    .await;
    assert_eq!(v["ok"], true, "passkey_begin {v}");
    let rcr: webauthn_authenticator_rs::prelude::RequestChallengeResponse =
        serde_json::from_value(v["data"].clone()).expect("rcr json");
    let assertion = authenticator
        .do_authentication(origin, rcr)
        .expect("softpasskey assertion");

    let (_, v, cookies) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.passkey_complete",
            serde_json::json!({
                "assertion": serde_json::to_value(&assertion).unwrap(),
            }),
        ),
        Some(&chal),
    )
    .await;
    assert_eq!(v["ok"], true, "passkey_complete {v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_some());
    std::env::remove_var("OXIDEAN_PUBLIC_ORIGIN");
}

// ---------------------------------------------------------------------------
// Identifier-first sign-in (issue #130 follow-up): `auth.login_begin` opens
// the challenge before the password leg; factor→password and password→factor
// orderings both mint; phantom challenges hide unknown/banned accounts.
// ---------------------------------------------------------------------------

fn login_begin_body(identifier: &str) -> String {
    format!(
        r#"{{"procedure":"auth.login_begin","input":{{"identifier":"{identifier}","remember_me":false}}}}"#
    )
}

/// Password leg of a pending challenge — the challenge binds the account, so
/// `identifier` is omitted (the reload-resume shape).
fn challenge_login_body(challenge: &str, password: &str) -> String {
    format!(
        r#"{{"procedure":"auth.login","input":{{"password":"{password}","remember_me":false,"challenge":"{challenge}"}}}}"#
    )
}

#[tokio::test]
async fn identifier_first_password_only_account() {
    let db = test_db("two_factor_flip_basic").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "ivy@example.com", "ivy").await;

    let (_, v, cookies) = rpc(&app, &login_begin_body("ivy@example.com"), None).await;
    assert_eq!(v["ok"], true, "login_begin {v}");
    assert_eq!(v["data"]["next"], "password", "{v}");
    assert!(v["data"]["kind"].is_null(), "{v}");
    let challenge = v["data"]["challenge"].as_str().unwrap().to_string();
    let chal_cookie = cookie_named(&cookies, "oxidean_2fa")
        .expect("challenge cookie")
        .to_string();
    assert!(cookie_named(&cookies, "oxidean_session").is_none());

    // Status reports the outstanding step via the challenge cookie.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.status", serde_json::json!({})),
        Some(&chal_cookie),
    )
    .await;
    assert_eq!(v["data"]["challenge_pending"], true, "{v}");
    assert_eq!(v["data"]["challenge_next"], "password", "{v}");

    // Password leg — no identifier needed; the challenge binds the account.
    let (_, v, cookies) = rpc(&app, &challenge_login_body(&challenge, "password1"), None).await;
    assert_eq!(v["ok"], true, "password leg {v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_some());
}

#[tokio::test]
async fn identifier_first_factor_then_password() {
    let db = test_db("two_factor_flip_factor").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "jo@example.com", "jo").await;
    let (_, _, cookies) = rpc(&app, &login_body("jo@example.com"), None).await;
    let session = cookie_named(&cookies, "oxidean_session")
        .unwrap()
        .to_string();
    let user_id = find_user_id(&db, "jo").await;
    verify_user(&db, &user_id).await;
    let codes = enroll_totp(&app, &session).await;

    // Identifier-first: the factor step comes before the password step.
    let (_, v, cookies) = rpc(&app, &login_begin_body("jo@example.com"), None).await;
    assert_eq!(v["ok"], true, "login_begin {v}");
    assert_eq!(v["data"]["next"], "factor", "{v}");
    assert_eq!(v["data"]["kind"], "verify", "{v}");
    let methods: Vec<String> = v["data"]["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    for m in ["totp", "recovery", "email"] {
        assert!(methods.contains(&m.to_string()), "methods {methods:?}");
    }
    let challenge = v["data"]["challenge"].as_str().unwrap().to_string();
    let chal_cookie = cookie_named(&cookies, "oxidean_2fa").unwrap().to_string();

    // Factor success advances to the password step — no session yet.
    let (_, v, cookies) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.complete",
            serde_json::json!({"code": codes[0]}),
        ),
        Some(&chal_cookie),
    )
    .await;
    assert_eq!(v["ok"], true, "factor complete {v}");
    assert_eq!(v["data"]["next"], "password", "{v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_none());

    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.status", serde_json::json!({})),
        Some(&chal_cookie),
    )
    .await;
    assert_eq!(v["data"]["challenge_next"], "password", "{v}");

    // A wrong password burns challenge attempts and keeps it alive.
    let (_, v, _) = rpc(&app, &challenge_login_body(&challenge, "wrong-pass"), None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.invalid_credentials", "{v}");

    // Correct password mints the session.
    let (_, v, cookies) = rpc(&app, &challenge_login_body(&challenge, "password1"), None).await;
    assert_eq!(v["ok"], true, "password leg {v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_some());
}

#[tokio::test]
async fn identifier_first_password_then_factor_also_works() {
    let db = test_db("two_factor_flip_reverse").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "kate@example.com", "kate").await;
    let (_, _, cookies) = rpc(&app, &login_body("kate@example.com"), None).await;
    let session = cookie_named(&cookies, "oxidean_session")
        .unwrap()
        .to_string();
    let user_id = find_user_id(&db, "kate").await;
    verify_user(&db, &user_id).await;
    let codes = enroll_totp(&app, &session).await;

    let (_, v, cookies) = rpc(&app, &login_begin_body("kate@example.com"), None).await;
    assert_eq!(v["data"]["next"], "factor", "{v}");
    let challenge = v["data"]["challenge"].as_str().unwrap().to_string();
    let chal_cookie = cookie_named(&cookies, "oxidean_2fa").unwrap().to_string();

    // The password leg before the factor re-surfaces the verify challenge.
    let (_, v, _) = rpc(&app, &challenge_login_body(&challenge, "password1"), None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.two_factor_required", "{v}");
    assert_eq!(v["error"]["data"]["kind"], "verify", "{v}");

    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.status", serde_json::json!({})),
        Some(&chal_cookie),
    )
    .await;
    assert_eq!(v["data"]["challenge_next"], "factor", "{v}");

    // Factor completion now mints.
    let (_, v, cookies) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.complete",
            serde_json::json!({"code": codes[0]}),
        ),
        Some(&chal_cookie),
    )
    .await;
    assert_eq!(v["ok"], true, "factor complete {v}");
    assert_eq!(v["data"]["next"], "done", "{v}");
    assert!(v["data"]["user"]["username"].as_str().is_some(), "{v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_some());
}

#[tokio::test]
async fn identifier_first_passkey_completes_without_password() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "http://localhost:8080");
    let db = test_db("two_factor_flip_passkey").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "leah@example.com", "leah").await;
    let (_, _, cookies) = rpc(&app, &login_body("leah@example.com"), None).await;
    let session = cookie_named(&cookies, "oxidean_session")
        .unwrap()
        .to_string();
    let user_id = find_user_id(&db, "leah").await;
    verify_user(&db, &user_id).await;

    let origin =
        webauthn_authenticator_rs::prelude::Url::parse("http://localhost:8080").expect("origin");
    let mut authenticator = webauthn_authenticator_rs::prelude::WebauthnAuthenticator::new(
        webauthn_authenticator_rs::softpasskey::SoftPasskey::new(true),
    );
    let (_, v, _) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.passkey_register_begin",
            serde_json::json!({}),
        ),
        Some(&session),
    )
    .await;
    let ccr: webauthn_authenticator_rs::prelude::CreationChallengeResponse =
        serde_json::from_value(v["data"].clone()).expect("ccr json");
    let attestation = authenticator
        .do_registration(origin.clone(), ccr)
        .expect("softpasskey attestation");
    let (_, v, _) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.passkey_register_complete",
            serde_json::json!({
                "nickname": "Test SoftPasskey",
                "attestation": serde_json::to_value(&attestation).unwrap(),
            }),
        ),
        Some(&session),
    )
    .await;
    assert_eq!(v["ok"], true, "register_complete {v}");

    // Identifier-first: passkey assertion alone mints the session — the
    // password step is skipped for user-verified assertions.
    let (_, v, cookies) = rpc(&app, &login_begin_body("leah@example.com"), None).await;
    assert_eq!(v["data"]["next"], "factor", "{v}");
    let chal = cookie_named(&cookies, "oxidean_2fa").unwrap().to_string();

    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.passkey_begin", serde_json::json!({})),
        Some(&chal),
    )
    .await;
    assert_eq!(v["ok"], true, "passkey_begin {v}");
    let rcr: webauthn_authenticator_rs::prelude::RequestChallengeResponse =
        serde_json::from_value(v["data"].clone()).expect("rcr json");
    let assertion = authenticator
        .do_authentication(origin, rcr)
        .expect("softpasskey assertion");

    let (_, v, cookies) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.passkey_complete",
            serde_json::json!({
                "assertion": serde_json::to_value(&assertion).unwrap(),
            }),
        ),
        Some(&chal),
    )
    .await;
    assert_eq!(v["ok"], true, "passkey_complete {v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_some());
    std::env::remove_var("OXIDEAN_PUBLIC_ORIGIN");
}

#[tokio::test]
async fn identifier_first_unknown_identifier_is_indistinguishable() {
    let db = test_db("two_factor_flip_phantom").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    // Unknown identifier gets the same password-step response a real
    // password-only account gets.
    let (_, v, cookies) = rpc(&app, &login_begin_body("ghost@example.com"), None).await;
    assert_eq!(v["ok"], true, "phantom login_begin {v}");
    assert_eq!(v["data"]["next"], "password", "{v}");
    assert!(v["data"]["kind"].is_null(), "{v}");
    let challenge = v["data"]["challenge"].as_str().unwrap().to_string();
    assert!(cookie_named(&cookies, "oxidean_2fa").is_some());

    // The dead-end challenge fails exactly like a bad password.
    let (_, v, cookies) = rpc(&app, &challenge_login_body(&challenge, "password1"), None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.invalid_credentials", "{v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_none());
}

#[tokio::test]
async fn identifier_first_banned_account_is_indistinguishable() {
    let db = test_db("two_factor_flip_banned").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "mia@example.com", "mia").await;
    let user_id = find_user_id(&db, "mia").await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_user_banned_at(&user_id, &now).await.expect("ban");

    let (_, v, _) = rpc(&app, &login_begin_body("mia@example.com"), None).await;
    assert_eq!(v["ok"], true, "banned login_begin {v}");
    assert_eq!(v["data"]["next"], "password", "{v}");
    let challenge = v["data"]["challenge"].as_str().unwrap().to_string();

    let (_, v, _) = rpc(&app, &challenge_login_body(&challenge, "password1"), None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.invalid_credentials", "{v}");
}

#[tokio::test]
async fn identifier_first_challenge_binds_its_own_account() {
    let db = test_db("two_factor_flip_binding").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "nina@example.com", "nina").await;
    signup(&app, "olga@example.com", "olga").await;

    let (_, v, _) = rpc(&app, &login_begin_body("nina@example.com"), None).await;
    assert_eq!(v["ok"], true, "{v}");
    let challenge = v["data"]["challenge"].as_str().unwrap().to_string();

    // Nina's challenge + Olga's identifier/password must not cross-bind.
    let body = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"olga@example.com","password":"password1","remember_me":false,"challenge":"{challenge}"}}}}"#
    );
    let (_, v, cookies) = rpc(&app, &body, None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.invalid_credentials", "{v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_none());

    // A dead challenge id fails explicitly instead of guessing.
    let (_, v, _) = rpc(&app, &challenge_login_body("deadbeef", "password1"), None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.two_factor_expired", "{v}");
}

#[tokio::test]
async fn identifier_first_enroll_required_runs_password_before_enroll() {
    let db = test_db("two_factor_flip_enroll").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    // Sign up before the policy lands — the account has no factor yet.
    signup(&app, "pete@example.com", "pete").await;
    let user_id = find_user_id(&db, "pete").await;
    verify_user(&db, &user_id).await;

    let settings = db.get_auth_settings().await.expect("settings");
    db.update_auth_settings(
        &settings.provider_mode,
        &settings.email_provider,
        settings.from_address.as_deref(),
        settings.oidc_issuer.as_deref(),
        settings.oidc_client_id.as_deref(),
        settings.workos_client_id.as_deref(),
        settings.allow_signup,
        &settings.default_visibility,
        true,
        settings.require_two_factor_external,
    )
    .await
    .expect("enable local 2fa policy");

    let (_, v, cookies) = rpc(&app, &login_begin_body("pete@example.com"), None).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["next"], "password", "{v}");
    assert_eq!(v["data"]["kind"], "enroll", "{v}");
    let challenge = v["data"]["challenge"].as_str().unwrap().to_string();
    let chal_cookie = cookie_named(&cookies, "oxidean_2fa").unwrap().to_string();

    // Enrollment is not possible before the password leg — a factor must not
    // be plantable on an account pre-auth.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.totp_begin", serde_json::json!({})),
        Some(&chal_cookie),
    )
    .await;
    assert_eq!(v["ok"], false, "pre-password enroll must fail {v}");
    assert_eq!(v["error"]["code"], "auth.two_factor_unavailable", "{v}");

    // Password leg → enroll challenge resurfaces.
    let (_, v, _) = rpc(&app, &challenge_login_body(&challenge, "password1"), None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.two_factor_required", "{v}");
    assert_eq!(v["error"]["data"]["kind"], "enroll", "{v}");

    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.status", serde_json::json!({})),
        Some(&chal_cookie),
    )
    .await;
    assert_eq!(v["data"]["challenge_next"], "enroll", "{v}");

    // Enroll now succeeds and mints the session.
    let (_, v, _) = rpc(
        &app,
        &two_factor_req("auth.two_factor.totp_begin", serde_json::json!({})),
        Some(&chal_cookie),
    )
    .await;
    assert_eq!(v["ok"], true, "post-password totp_begin {v}");
    let secret = v["data"]["secret"].as_str().unwrap().to_string();
    let code = totp_code(&secret);
    let (_, v, cookies) = rpc(
        &app,
        &two_factor_req(
            "auth.two_factor.totp_confirm",
            serde_json::json!({"code": code}),
        ),
        Some(&chal_cookie),
    )
    .await;
    assert_eq!(v["ok"], true, "enroll confirm {v}");
    assert!(cookie_named(&cookies, "oxidean_session").is_some());
}

#[tokio::test]
async fn login_begin_is_rate_limited_per_identifier() {
    let db = test_db("two_factor_flip_ratelimit").await;
    support::unlock_signup(&db).await;
    let (app, _mail) = test_app(db.clone()).await;

    signup(&app, "quinn@example.com", "quinn").await;

    for i in 0..10 {
        let (_, v, _) = rpc(&app, &login_begin_body("quinn@example.com"), None).await;
        assert_eq!(v["ok"], true, "begin {i} {v}");
    }
    let (_, v, _) = rpc(&app, &login_begin_body("quinn@example.com"), None).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.rate_limited", "{v}");

    // A different identifier still works.
    let (_, v, _) = rpc(&app, &login_begin_body("other@example.com"), None).await;
    assert_eq!(v["ok"], true, "other identifier {v}");
}
