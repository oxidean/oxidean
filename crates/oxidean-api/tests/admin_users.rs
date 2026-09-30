//! Admin users + instance invites (issue #52 / A2+A3).

mod support;

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailError, EmailSender, OutboundEmail};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use sha2::{Digest, Sha256};
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

async fn test_app_with_recorder(db: Database) -> (axum::Router, Arc<RecordingSender>) {
    let recorder = Arc::new(RecordingSender::default());
    let state = AppState::new(
        db,
        recorder.clone() as Arc<dyn EmailSender>,
        "development",
    );
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

async fn rpc_json(
    app: &axum::Router,
    body: &str,
    cookie: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let req = match cookie {
        Some(c) => rpc_req_with_cookie(body, c),
        None => rpc_req(body),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, v)
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

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

async fn close_signup(db: &Database) {
    let settings = db.get_auth_settings().await.expect("auth settings");
    db.update_auth_settings(
        &settings.provider_mode,
        &settings.email_provider,
        settings.from_address.as_deref(),
        settings.oidc_issuer.as_deref(),
        settings.oidc_client_id.as_deref(),
        settings.workos_client_id.as_deref(),
        false,
        &settings.default_visibility,
    )
    .await
    .expect("close allow_signup");
}

fn invite_email(sent: &[OutboundEmail]) -> &OutboundEmail {
    sent.iter()
        .find(|m| m.text.contains("/invites/"))
        .unwrap_or_else(|| panic!("no invite email in {} messages", sent.len()))
}

fn extract_invite_token(text: &str) -> String {
    let marker = "/invites/";
    let after = text
        .split_once(marker)
        .unwrap_or_else(|| panic!("missing invite link in: {text}"))
        .1;
    after
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect()
}

fn sha256_hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(digest.len() * 2);
    for &b in digest.as_slice() {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

async fn bootstrap_sysadmin(db: &Database) -> (String, String) {
    support::unlock_signup(db).await;
    let admins = db.list_users_page(None, 10, 0).await.expect("list");
    let admin = admins
        .0
        .into_iter()
        .find(|u| u.role.is_sys_admin())
        .expect("sys-admin from unlock_signup");
    (admin.id, admin.username)
}

async fn login_as(
    app: &axum::Router,
    db: &Database,
    user_id: &str,
) -> String {
    // Mint a session directly so we don't need the ENV admin password.
    let sessions = oxidean_api::auth::SessionService::new("development");
    let (raw, _cookie) = sessions
        .create(db, user_id, false)
        .await
        .expect("create session");
    format!("oxidean_session={raw}")
}

#[tokio::test]
async fn admin_users_list_requires_sys_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_list.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = test_app_with_recorder(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "user@ex.com", "normaluser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"admin.users.list","input":{}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["error"]["code"], "admin.forbidden");

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    // unlock_signup already created a sys-admin; get a fresh cookie for them.
    let admin_cookie = login_as(&app, &db, &admin_id).await;
    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"admin.users.list","input":{}}"#,
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    let users = v["data"]["users"].as_array().expect("users");
    assert!(users.len() >= 2, "{v}");
    assert!(v["data"]["total"].as_i64().unwrap() >= 2);
}

#[tokio::test]
async fn admin_invites_create_list_revoke_and_accept_closed_signup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_invites.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (status, create_v) = rpc_json(
        &app,
        r#"{"procedure":"admin.invites.create","input":{"email":"newbie@ex.com"}}"#,
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{create_v}");
    assert_eq!(create_v["ok"], true, "{create_v}");
    assert_eq!(create_v["data"]["invite"]["email"], "newbie@ex.com");
    let invite_url = create_v["data"]["invite_url"].as_str().expect("invite_url");
    assert!(invite_url.contains("/invites/"), "{create_v}");
    assert!(
        create_v["data"]["invite"].get("token").is_none()
            && create_v["data"]["invite"].get("token_hash").is_none(),
        "must not return token: {create_v}"
    );

    let token_from_url = invite_url
        .rsplit('/')
        .next()
        .expect("token segment")
        .to_string();
    let sent = recorder.sent.lock().expect("lock");
    let email_token = extract_invite_token(&invite_email(&sent).text);
    assert_eq!(email_token, token_from_url);
    drop(sent);

    let (_, list_v) = rpc_json(
        &app,
        r#"{"procedure":"admin.invites.list","input":{}}"#,
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(list_v["ok"], true, "{list_v}");
    let invites = list_v["data"]["invites"].as_array().expect("invites");
    assert_eq!(invites.len(), 1, "{list_v}");
    let invite_id = invites[0]["id"].as_str().expect("id").to_string();

    close_signup(&db).await;

    let (status, accept_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"invites.accept","input":{{"token":"{token_from_url}","username":"invitee1","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accept_v}");
    assert_eq!(accept_v["ok"], true, "{accept_v}");
    assert_eq!(accept_v["data"]["kind"], "instance");

    let user = db
        .find_user_by_username("invitee1")
        .await
        .expect("find")
        .expect("user");
    assert!(user.email_verified_at.is_some());
    assert_eq!(user.role, Role::User);

    // Single-use.
    let (reuse_status, reuse_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"invites.accept","input":{{"token":"{token_from_url}","username":"invitee2","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(reuse_status, StatusCode::BAD_REQUEST, "{reuse_v}");
    assert_eq!(reuse_v["error"]["code"], "invite.invalid");

    // Create + revoke another invite.
    let (_, create2) = rpc_json(
        &app,
        r#"{"procedure":"admin.invites.create","input":{"email":"revokee@ex.com"}}"#,
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(create2["ok"], true, "{create2}");
    let invite2_id = create2["data"]["invite"]["id"].as_str().expect("id");

    let (_, revoke_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.invites.revoke","input":{{"invite_id":"{invite2_id}"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(revoke_v["ok"], true, "{revoke_v}");

    let (_, list2) = rpc_json(
        &app,
        r#"{"procedure":"admin.invites.list","input":{}}"#,
        Some(&admin_cookie),
    )
    .await;
    let pending = list2["data"]["invites"].as_array().expect("invites");
    assert!(
        pending.iter().all(|i| i["id"] != invite_id && i["id"] != invite2_id),
        "accepted/revoked must leave pending list: {list2}"
    );
}

#[tokio::test]
async fn admin_users_role_ban_unban_and_guards() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_ban.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = test_app_with_recorder(db.clone()).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (user_cookie, login_v) = signup_and_login(&app, "banme@ex.com", "banmeuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;

    // Promote then refuse self-demotion.
    let (_, promote) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.updateRole","input":{{"user_id":"{user_id}","role":"sys-admin"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(promote["ok"], true, "{promote}");
    assert_eq!(promote["data"]["role"], "sys-admin");

    let (_, self_demote) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.updateRole","input":{{"user_id":"{admin_id}","role":"user"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(self_demote["error"]["code"], "admin.cannot_demote_self");

    // Demote target so ban is allowed.
    let (_, demote) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.updateRole","input":{{"user_id":"{user_id}","role":"user"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(demote["ok"], true, "{demote}");

    // Cannot ban sys-admin (re-promote and try).
    let (_, promote2) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.updateRole","input":{{"user_id":"{user_id}","role":"sys-admin"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(promote2["ok"], true, "{promote2}");
    let (_, ban_admin) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"admin.users.ban","input":{{"user_id":"{user_id}"}}}}"#),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(ban_admin["error"]["code"], "admin.cannot_ban_sys_admin");

    let (_, demote2) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.updateRole","input":{{"user_id":"{user_id}","role":"user"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(demote2["ok"], true, "{demote2}");

    let (_, ban_v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"admin.users.ban","input":{{"user_id":"{user_id}"}}}}"#),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(ban_v["ok"], true, "{ban_v}");
    assert!(ban_v["data"]["banned_at"].as_str().is_some());

    // Banned session is cleared on resolve.
    let (me_status, me_v) = rpc_json(
        &app,
        r#"{"procedure":"auth.me","input":{}}"#,
        Some(&user_cookie),
    )
    .await;
    assert_eq!(me_status, StatusCode::UNAUTHORIZED, "{me_v}");
    assert_eq!(me_v["error"]["code"], "auth.unauthenticated");

    // Public profile 404s.
    let (_, profile_v) = rpc_json(
        &app,
        r#"{"procedure":"user.getPublicProfile","input":{"username":"banmeuser"}}"#,
        None,
    )
    .await;
    assert_eq!(profile_v["error"]["code"], "user.not_found");

    let (_, unban_v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"admin.users.unban","input":{{"user_id":"{user_id}"}}}}"#),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(unban_v["ok"], true, "{unban_v}");
    assert!(unban_v["data"]["banned_at"].is_null());

    let (login_status, login2) = rpc_json(
        &app,
        r#"{"procedure":"auth.login","input":{"identifier":"banme@ex.com","password":"password1","remember_me":false}}"#,
        None,
    )
    .await;
    assert_eq!(login_status, StatusCode::OK, "{login2}");
    assert_eq!(login2["ok"], true, "{login2}");
}

#[tokio::test]
async fn admin_users_delete_requires_username_confirmation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_delete.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = test_app_with_recorder(db.clone()).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (_cookie, login_v) = signup_and_login(&app, "deleteme@ex.com", "deleteme1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;

    let (_, bad) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.delete","input":{{"user_id":"{user_id}","confirmation":"wrong"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(bad["error"]["code"], "admin.delete_confirm");

    let (_, ok) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.delete","input":{{"user_id":"{user_id}","confirmation":"deleteme1"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(ok["ok"], true, "{ok}");
    assert_eq!(ok["data"]["ok"], true);
    assert!(db.find_user_by_id(&user_id).await.expect("find").is_none());
}

#[tokio::test]
async fn admin_invites_token_hash_at_rest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_invites_hash.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"admin.invites.create","input":{"email":"hashed@ex.com"}}"#,
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    let invite_id = create_v["data"]["invite"]["id"].as_str().expect("id");
    let token = {
        let sent = recorder.sent.lock().expect("lock");
        extract_invite_token(&invite_email(&sent).text)
    };
    let expected = sha256_hex(token.as_bytes());
    let row = db
        .find_instance_invite_by_id(invite_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(row.token_hash, expected);
    assert_ne!(row.token_hash, token);
}

#[tokio::test]
async fn invites_accept_routes_org_tokens() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("invites_accept_org.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "orgowner@ex.com", "orgowner1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    let (_, create_org) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"inv-unified","display_name":"Unified"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create_org["ok"], true, "{create_org}");

    let (_, create_inv) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.create","input":{"slug":"inv-unified","email":"orginvitee@ex.com","role":"member"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create_inv["ok"], true, "{create_inv}");
    let token = {
        let sent = recorder.sent.lock().expect("lock");
        extract_invite_token(&invite_email(&sent).text)
    };

    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"invites.accept","input":{{"token":"{token}","username":"orginvitee1","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["kind"], "org");
    assert_eq!(v["data"]["org"]["slug"], "inv-unified");
    assert_eq!(v["data"]["member"]["username"], "orginvitee1");
}
