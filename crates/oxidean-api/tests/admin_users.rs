//! Admin users + instance invites (issue #52 / A2+A3).

mod support;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
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
    test_app_with_recorder_repos(db, None).await
}

async fn test_app_with_recorder_repos(
    db: Database,
    repos_dir: Option<PathBuf>,
) -> (axum::Router, Arc<RecordingSender>) {
    let recorder = Arc::new(RecordingSender::default());
    let mut state = AppState::new(db, recorder.clone() as Arc<dyn EmailSender>, "development");
    if let Some(dir) = repos_dir {
        state = state.with_repos_dir(dir);
    }
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

async fn login_as(_app: &axum::Router, db: &Database, user_id: &str) -> String {
    // Mint a session directly so we don't need the ENV admin password.
    let sessions = oxidean_api::auth::SessionService::new("development");
    let (raw, _cookie) = sessions
        .create(db, user_id, false, None, None)
        .await
        .expect("create session");
    format!("oxidean_session={raw}")
}

fn info_refs_uri(owner: &str, repo: &str) -> String {
    format!("/{owner}/{repo}.git/info/refs?service=git-upload-pack")
}

fn basic_header(user: &str, password: &str) -> String {
    let raw = format!("{user}:{password}");
    format!("Basic {}", encode_b64(raw.as_bytes()))
}

fn encode_b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
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
    let url = format!("sqlite:{}", dir.path().join("admin_invites.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (status, create_v) = rpc_json(
        &app,
        r#"{"procedure":"admin.invites.create","input":{"emails":["newbie@ex.com"]}}"#,
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{create_v}");
    assert_eq!(create_v["ok"], true, "{create_v}");
    let first = &create_v["data"]["results"][0];
    assert_eq!(first["ok"], true, "{create_v}");
    assert_eq!(first["invite"]["email"], "newbie@ex.com");
    let invite_url = first["invite_url"].as_str().expect("invite_url");
    assert!(invite_url.contains("/invites/"), "{create_v}");
    assert!(
        first["invite"].get("token").is_none() && first["invite"].get("token_hash").is_none(),
        "must not return token: {create_v}"
    );

    let token_from_url = invite_url
        .rsplit('/')
        .next()
        .expect("token segment")
        .to_string();
    {
        let sent = recorder.sent.lock().expect("lock");
        let email_token = extract_invite_token(&invite_email(&sent).text);
        assert_eq!(email_token, token_from_url);
    }

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
        r#"{"procedure":"admin.invites.create","input":{"emails":["revokee@ex.com"]}}"#,
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(create2["ok"], true, "{create2}");
    let invite2_id = create2["data"]["results"][0]["invite"]["id"]
        .as_str()
        .expect("id");

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
        pending
            .iter()
            .all(|i| i["id"] != invite_id && i["id"] != invite2_id),
        "accepted/revoked must leave pending list: {list2}"
    );
}

#[tokio::test]
async fn admin_users_role_ban_unban_and_guards() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("admin_users_ban.db").display());
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
        r#"{"procedure":"admin.invites.create","input":{"emails":["hashed@ex.com"]}}"#,
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    let invite_id = create_v["data"]["results"][0]["invite"]["id"]
        .as_str()
        .expect("id");
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
async fn admin_invites_bulk_results_and_link_seats() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_invites_bulk.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _recorder) = test_app_with_recorder(db.clone()).await;

    let (admin_id, _name) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;
    let admin_row = db
        .find_user_by_id(&admin_id)
        .await
        .expect("find")
        .expect("admin");

    // Bulk create: one valid, one invalid, one already-a-user.
    let (status, bulk) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.invites.create","input":{{"emails":["one@ex.com","not-an-email","{}"]}}}}"#,
            admin_row.email
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bulk}");
    let results = bulk["data"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 3, "{bulk}");
    assert_eq!(results[0]["email"], "one@ex.com");
    assert_eq!(results[0]["ok"], true, "{bulk}");
    assert_eq!(results[1]["ok"], false, "{bulk}");
    assert_eq!(results[2]["ok"], false, "{bulk}");
    assert_eq!(
        results[2]["error"], "an account with this email already exists",
        "{bulk}"
    );

    // Shareable link: 2 seats.
    let (status, link) = rpc_json(
        &app,
        r#"{"procedure":"admin.invites.createLink","input":{"max_uses":2}}"#,
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{link}");
    let invite_url = link["data"]["invite_url"].as_str().expect("invite_url");
    let token = invite_url.rsplit('/').next().expect("token");

    // Preview: anonymous-safe shape.
    let (status, preview) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"invites.get","input":{{"token":"{token}"}}}}"#),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["kind"], "instance");
    assert!(preview["data"]["email"].is_null(), "{preview}");
    assert_eq!(preview["data"]["seats_remaining"], 2, "{preview}");
    assert_eq!(preview["data"]["acceptable"], true, "{preview}");

    close_signup(&db).await;

    // Anonymous link accept requires an email.
    let (status, miss) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"invites.accept","input":{{"token":"{token}","username":"linkuser1","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{miss}");
    assert_eq!(miss["error"]["code"], "rpc.bad_input", "{miss}");

    // Two accepts consume both seats.
    for (email, username) in [("l1@ex.com", "linkuser1"), ("l2@ex.com", "linkuser2")] {
        let (status, acc) = rpc_json(
            &app,
            &format!(
                r#"{{"procedure":"invites.accept","input":{{"token":"{token}","email":"{email}","username":"{username}","password":"password1"}}}}"#
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{acc}");
        assert_eq!(acc["data"]["kind"], "instance", "{acc}");
    }
    let (_, exhausted) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"invites.get","input":{{"token":"{token}"}}}}"#),
        None,
    )
    .await;
    assert_eq!(exhausted["data"]["acceptable"], false, "{exhausted}");
    assert_eq!(exhausted["data"]["reason"], "exhausted", "{exhausted}");
    assert_eq!(exhausted["data"]["seats_remaining"], 0, "{exhausted}");

    // Third accept fails.
    let (status, third) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"invites.accept","input":{{"token":"{token}","email":"l3@ex.com","username":"linkuser3","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{third}");
    assert_eq!(third["error"]["code"], "invite.invalid", "{third}");
}

#[tokio::test]
async fn admin_users_sessions_and_activity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_activity.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _recorder) = test_app_with_recorder(db.clone()).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    // Signup + login a user with client metadata headers.
    let signup_body = r#"{"procedure":"auth.signup","input":{"email":"target@ex.com","username":"target1","password":"password1"}}"#;
    let signup = app.clone().oneshot(rpc_req(signup_body)).await.unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;
    let login_body = r#"{"procedure":"auth.login","input":{"identifier":"target@ex.com","password":"password1","remember_me":true}}"#;
    let login_req = Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("x-forwarded-for", "203.0.113.99")
        .header(header::USER_AGENT, "TestClient/1.0")
        .body(Body::from(login_body.to_owned()))
        .unwrap();
    let login = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let _ = login.into_body().collect().await;

    let target = db
        .find_user_by_username("target1")
        .await
        .expect("find")
        .expect("user");

    // Sessions: client details captured.
    let (status, sess) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.listSessions","input":{{"user_id":"{}"}}}}"#,
            target.id
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sess}");
    let sessions = sess["data"]["sessions"].as_array().expect("sessions");
    // Signup mints a session (no client metadata headers) and login mints one
    // with the request's X-Forwarded-For/User-Agent.
    assert_eq!(sessions.len(), 2, "{sess}");
    let login_session = sessions
        .iter()
        .find(|s| s["ip_address"] == "203.0.113.99")
        .expect("login session");
    assert_eq!(login_session["user_agent"], "TestClient/1.0", "{sess}");
    assert_eq!(login_session["remember_me"], true, "{sess}");
    assert!(
        sessions.iter().all(|s| s.get("token_hash").is_none()),
        "must not expose token_hash: {sess}"
    );

    // Activity: auth.signup + auth.login audit events for the target.
    let (status, act) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.getActivity","input":{{"user_id":"{}"}}}}"#,
            target.id
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{act}");
    let items = act["data"]["items"].as_array().expect("items");
    let types: Vec<&str> = items
        .iter()
        .map(|i| i["event_type"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"auth.signup"), "{act}");
    assert!(types.contains(&"auth.login"), "{act}");
    let login_item = items
        .iter()
        .find(|i| i["event_type"] == "auth.login")
        .expect("login item");
    assert_eq!(login_item["source"], "audit", "{act}");
    assert_eq!(login_item["ip_address"], "203.0.113.99", "{act}");
    assert_eq!(login_item["user_agent"], "TestClient/1.0", "{act}");

    // Source filter: repository only → empty for a user with no pushes.
    let (status, repo_only) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.getActivity","input":{{"user_id":"{}","source":"repository"}}}}"#,
            target.id
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{repo_only}");
    assert_eq!(
        repo_only["data"]["items"].as_array().expect("items").len(),
        0,
        "{repo_only}"
    );
    assert_eq!(
        repo_only["data"]["event_types"]
            .as_array()
            .expect("event_types")
            .len(),
        0,
        "repo-only source must not list audit types: {repo_only}"
    );

    // Event-type filter.
    let (status, filtered) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.getActivity","input":{{"user_id":"{}","event_type":"auth.login"}}}}"#,
            target.id
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{filtered}");
    let items = filtered["data"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{filtered}");
    assert_eq!(items[0]["event_type"], "auth.login", "{filtered}");

    // Non-admin cannot read sessions/activity.
    let (user_cookie, _) = signup_and_login(&app, "plain@ex.com", "plainuser").await;
    for proc in ["admin.users.listSessions", "admin.users.getActivity"] {
        let (status, v) = rpc_json(
            &app,
            &format!(
                r#"{{"procedure":"{proc}","input":{{"user_id":"{}"}}}}"#,
                target.id
            ),
            Some(&user_cookie),
        )
        .await;
        assert_ne!(status, StatusCode::OK, "{proc}: {v}");
    }
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
        r#"{"procedure":"org.invites.create","input":{"slug":"inv-unified","emails":["orginvitee@ex.com"],"role":"member"}}"#,
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

#[tokio::test]
async fn admin_users_revoke_sessions_forces_reauth() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_revoke.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = test_app_with_recorder(db.clone()).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (user_cookie, login_v) = signup_and_login(&app, "revoke@ex.com", "revokeuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;

    let (me_ok_status, me_ok) = rpc_json(
        &app,
        r#"{"procedure":"auth.me","input":{}}"#,
        Some(&user_cookie),
    )
    .await;
    assert_eq!(me_ok_status, StatusCode::OK, "{me_ok}");
    assert_eq!(me_ok["ok"], true, "{me_ok}");
    assert_eq!(me_ok["data"]["id"], user_id);

    let (_, revoke_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.revokeSessions","input":{{"user_id":"{user_id}"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(revoke_v["ok"], true, "{revoke_v}");
    assert!(
        revoke_v["data"]["revoked"].as_i64().unwrap_or(0) >= 1,
        "expected at least one revoked session — {revoke_v}"
    );

    let (me_status, me_v) = rpc_json(
        &app,
        r#"{"procedure":"auth.me","input":{}}"#,
        Some(&user_cookie),
    )
    .await;
    assert_eq!(me_status, StatusCode::UNAUTHORIZED, "{me_v}");
    assert_eq!(me_v["error"]["code"], "auth.unauthenticated");
}

#[tokio::test]
async fn admin_users_ban_blocks_classic_pat() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_ban_pat.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = test_app_with_recorder_repos(db.clone(), Some(repos)).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (user_cookie, login_v) = signup_and_login(&app, "patban@ex.com", "patbanuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;

    let (_, create_repo) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"hello","visibility":"public","description":""}}"#,
        Some(&user_cookie),
    )
    .await;
    assert_eq!(create_repo["ok"], true, "{create_repo}");

    let (_, create_pat) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"cli","scopes":["repo"]}}"#,
        Some(&user_cookie),
    )
    .await;
    assert_eq!(create_pat["ok"], true, "{create_pat}");
    let token = create_pat["data"]["token"]
        .as_str()
        .expect("token")
        .to_string();

    let ok_req = Request::builder()
        .method("GET")
        .uri(info_refs_uri("patbanuser", "hello"))
        .header(header::AUTHORIZATION, basic_header("patbanuser", &token))
        .body(Body::empty())
        .unwrap();
    let ok_res = app.clone().oneshot(ok_req).await.unwrap();
    assert_eq!(
        ok_res.status(),
        StatusCode::OK,
        "classic PAT must work before ban"
    );
    let _ = ok_res.into_body().collect().await;

    let (_, ban_v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"admin.users.ban","input":{{"user_id":"{user_id}"}}}}"#),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(ban_v["ok"], true, "{ban_v}");

    let banned_req = Request::builder()
        .method("GET")
        .uri(info_refs_uri("patbanuser", "hello"))
        .header(header::AUTHORIZATION, basic_header("patbanuser", &token))
        .body(Body::empty())
        .unwrap();
    let banned_res = app.clone().oneshot(banned_req).await.unwrap();
    assert_eq!(
        banned_res.status(),
        StatusCode::UNAUTHORIZED,
        "banned user's PAT must fail auth"
    );
    let _ = banned_res.into_body().collect().await;

    let (_, unban_v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"admin.users.unban","input":{{"user_id":"{user_id}"}}}}"#),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(unban_v["ok"], true, "{unban_v}");

    let restored_req = Request::builder()
        .method("GET")
        .uri(info_refs_uri("patbanuser", "hello"))
        .header(header::AUTHORIZATION, basic_header("patbanuser", &token))
        .body(Body::empty())
        .unwrap();
    let restored_res = app.oneshot(restored_req).await.unwrap();
    assert_eq!(
        restored_res.status(),
        StatusCode::OK,
        "unban must restore classic PAT auth"
    );
}

#[tokio::test]
async fn admin_users_delete_removes_personal_repos_and_sole_owner_org() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_delete_cascade.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = test_app_with_recorder_repos(db.clone(), Some(repos.clone())).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (user_cookie, login_v) = signup_and_login(&app, "wipe@ex.com", "wipeuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;

    let (_, personal) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"personal-app","visibility":"public","description":""}}"#,
        Some(&user_cookie),
    )
    .await;
    assert_eq!(personal["ok"], true, "{personal}");
    let personal_repo_id = personal["data"]["id"]
        .as_str()
        .expect("repo id")
        .to_string();

    let (_, create_org) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"wipe-org","display_name":"Wipe Org"}}"#,
        Some(&user_cookie),
    )
    .await;
    assert_eq!(create_org["ok"], true, "{create_org}");
    let org_id = create_org["data"]["id"]
        .as_str()
        .expect("org id")
        .to_string();

    let (_, org_repo) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"org-app","visibility":"public","owner":"wipe-org"}}"#,
        Some(&user_cookie),
    )
    .await;
    assert_eq!(org_repo["ok"], true, "{org_repo}");
    let org_repo_id = org_repo["data"]["id"]
        .as_str()
        .expect("org repo id")
        .to_string();

    assert!(repos.join("wipeuser").join("personal-app.git").exists());
    assert!(repos.join("wipe-org").join("org-app.git").exists());

    let (_, delete_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.delete","input":{{"user_id":"{user_id}","confirmation":"wipeuser"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(delete_v["ok"], true, "{delete_v}");
    assert_eq!(delete_v["data"]["ok"], true);
    assert!(
        delete_v["data"]["deleted_repos"].as_i64().unwrap_or(0) >= 2,
        "expected personal + org repos deleted — {delete_v}"
    );
    assert!(
        delete_v["data"]["deleted_orgs"].as_i64().unwrap_or(0) >= 1,
        "expected sole-owner org deleted — {delete_v}"
    );

    assert!(db
        .find_user_by_id(&user_id)
        .await
        .expect("find user")
        .is_none());
    assert!(db
        .find_organization_by_id(&org_id)
        .await
        .expect("find org")
        .is_none());
    assert!(db
        .find_repository_by_id(&personal_repo_id)
        .await
        .expect("find personal repo")
        .is_none());
    assert!(db
        .find_repository_by_id(&org_repo_id)
        .await
        .expect("find org repo")
        .is_none());
    assert!(
        !repos.join("wipeuser").exists()
            || !repos.join("wipeuser").join("personal-app.git").exists(),
        "personal repo disk path should be wiped"
    );
    assert!(
        !repos.join("wipe-org").exists() || !repos.join("wipe-org").join("org-app.git").exists(),
        "org repo disk path should be wiped"
    );
}

/// DB delete must succeed even when post-delete disk wipe fails (orphan dirs ok).
#[tokio::test]
async fn admin_users_delete_succeeds_when_disk_wipe_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_delete_wipe_fail.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = test_app_with_recorder_repos(db.clone(), Some(repos.clone())).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (user_cookie, login_v) = signup_and_login(&app, "wipefail@ex.com", "wipefail1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &user_id).await;

    let (_, personal) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"stuck-app","visibility":"public","description":""}}"#,
        Some(&user_cookie),
    )
    .await;
    assert_eq!(personal["ok"], true, "{personal}");
    let personal_repo_id = personal["data"]["id"]
        .as_str()
        .expect("repo id")
        .to_string();
    assert!(repos.join("wipefail1").join("stuck-app.git").exists());

    // Make a nested dir non-writable so remove_dir_all fails during wipe.
    let locked = repos.join("wipefail1").join("locked");
    std::fs::create_dir_all(&locked).expect("locked dir");
    std::fs::write(locked.join("pin"), b"x").expect("pin");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555))
            .expect("chmod locked");
    }

    let (_, delete_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.delete","input":{{"user_id":"{user_id}","confirmation":"wipefail1"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(delete_v["ok"], true, "{delete_v}");
    assert_eq!(delete_v["data"]["ok"], true);

    assert!(
        db.find_user_by_id(&user_id)
            .await
            .expect("find user")
            .is_none(),
        "user row must be deleted even if wipe fails"
    );
    assert!(
        db.find_repository_by_id(&personal_repo_id)
            .await
            .expect("find repo")
            .is_none(),
        "repo row must be deleted even if wipe fails"
    );

    // Restore perms so tempfile cleanup can remove the tree.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755));
    }
}

#[tokio::test]
async fn admin_users_get_access_lists_orgs_and_repos() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_get_access.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = test_app_with_recorder_repos(db.clone(), Some(repos)).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "accessowner@ex.com", "accessowner").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &owner_id).await;

    let (target_cookie, target_v) =
        signup_and_login(&app, "accesstarget@ex.com", "accesstarget").await;
    let target_id = target_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &target_id).await;

    let (_, create_org) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"access-org","display_name":"Access Org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_org["ok"], true, "{create_org}");

    let (_, add_member) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"access-org","username":"accesstarget","role":"member"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_member["ok"], true, "{add_member}");

    let (_, create_repo) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"shared","visibility":"private","description":""}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_repo["ok"], true, "{create_repo}");

    let (_, add_collab) = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"accessowner","name":"shared","username":"accesstarget","permission":"write"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_collab["ok"], true, "{add_collab}");

    let (forbidden_status, forbidden_v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"admin.users.getAccess","input":{{"user_id":"{target_id}"}}}}"#),
        Some(&target_cookie),
    )
    .await;
    assert_eq!(forbidden_status, StatusCode::FORBIDDEN, "{forbidden_v}");
    assert_eq!(forbidden_v["error"]["code"], "admin.forbidden");

    let (status, access_v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"admin.users.getAccess","input":{{"user_id":"{target_id}"}}}}"#),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{access_v}");
    assert_eq!(access_v["ok"], true, "{access_v}");

    let orgs = access_v["data"]["orgs"].as_array().expect("orgs");
    assert!(
        orgs.iter()
            .any(|o| { o["slug"] == "access-org" && o["role"] == "member" }),
        "expected access-org membership — {access_v}"
    );

    let repos_list = access_v["data"]["repos"].as_array().expect("repos");
    assert!(
        repos_list.iter().any(|r| {
            r["owner"] == "accessowner" && r["name"] == "shared" && r["permission"] == "write"
        }),
        "expected shared collaborator grant — {access_v}"
    );
}

/// Deleting a sole owner of an org that still has other members requires the
/// explicit `delete_orgs` opt-in; without it the call refuses before writes.
#[tokio::test]
async fn admin_users_delete_requires_delete_orgs_for_shared_org() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("admin_users_delete_orgs.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _) = test_app_with_recorder(db.clone()).await;

    let (admin_id, _) = bootstrap_sysadmin(&db).await;
    let admin_cookie = login_as(&app, &db, &admin_id).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "sharedowner@ex.com", "sharedowner").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &owner_id).await;

    let (_member_cookie, member_v) = signup_and_login(&app, "orgmember@ex.com", "orgmember1").await;
    let member_id = member_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(&db, &member_id).await;

    let (_, create_org) = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"shared-org","display_name":"Shared Org"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_org["ok"], true, "{create_org}");
    let org_id = create_org["data"]["id"]
        .as_str()
        .expect("org id")
        .to_string();

    let (_, add_member) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"shared-org","username":"orgmember1","role":"member"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_member["ok"], true, "{add_member}");

    let (_, refused) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.delete","input":{{"user_id":"{owner_id}","confirmation":"sharedowner"}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(
        refused["error"]["code"], "admin.delete_orgs_confirm",
        "{refused}"
    );
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("shared-org"),
        "error should name the shared orgs — {refused}"
    );
    assert!(
        db.find_user_by_id(&owner_id)
            .await
            .expect("find user")
            .is_some(),
        "refused delete must leave the user row intact"
    );
    assert!(
        db.find_organization_by_id(&org_id)
            .await
            .expect("find org")
            .is_some(),
        "refused delete must leave the org intact"
    );

    let (_, deleted) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"admin.users.delete","input":{{"user_id":"{owner_id}","confirmation":"sharedowner","delete_orgs":true}}}}"#
        ),
        Some(&admin_cookie),
    )
    .await;
    assert_eq!(deleted["ok"], true, "{deleted}");
    assert_eq!(deleted["data"]["deleted_orgs"].as_i64().unwrap_or(0), 1);
    assert!(db
        .find_user_by_id(&owner_id)
        .await
        .expect("find user")
        .is_none());
    assert!(db
        .find_organization_by_id(&org_id)
        .await
        .expect("find org")
        .is_none());
    // The other member's account survives the org deletion.
    assert!(db
        .find_user_by_id(&member_id)
        .await
        .expect("find member")
        .is_some());
}
