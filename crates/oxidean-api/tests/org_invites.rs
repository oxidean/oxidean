//! ORG-01 / D-ORG-03: organization email invites (create/list/revoke/accept).
//!
//! Tokens hashed at rest; accept may create accounts when allow_signup is false (A2).

mod support;

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailError, EmailSender, OutboundEmail};
use oxidean_api::{build_cors, router_with_state, AppState};
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

async fn setup_owner_org(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
    slug: &str,
) -> String {
    let (cookie, login_v) = signup_and_login(app, email, username).await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(db, user_id).await;
    let body = format!(
        r#"{{"procedure":"org.create","input":{{"slug":"{slug}","display_name":"Invite Org"}}}}"#
    );
    let (_, create_v) = rpc_json(app, &body, Some(&cookie)).await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    cookie
}

/// `invites.create` issues an email invite for a role (ORG-01 / D-ORG-03).
#[tokio::test]
async fn org_invites_create() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_invites_create.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let cookie = setup_owner_org(&app, &db, "invowner@ex.com", "invowner1", "inv-create").await;

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.create","input":{"slug":"inv-create","emails":["newbie@ex.com"],"role":"member"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "invites.create — {v}");
    assert_eq!(v["ok"], true, "{v}");
    let first = &v["data"]["results"][0];
    assert_eq!(first["ok"], true, "{v}");
    assert_eq!(first["invite"]["email"], "newbie@ex.com");
    assert_eq!(first["invite"]["role"], "member");
    assert!(first["invite"]["id"].as_str().is_some());
    let invite_url = first["invite_url"].as_str().expect("invite_url");
    assert!(invite_url.contains("/invites/"), "{v}");
    assert!(
        first["invite"].get("token").is_none()
            && first["invite"].get("token_hash").is_none(),
        "create must not return plaintext token or hash: {v}"
    );

    let sent = recorder.sent.lock().expect("lock");
    let invite = invite_email(&sent);
    assert_eq!(invite.to, "newbie@ex.com");
    let token = extract_invite_token(&invite.text);
    assert_eq!(token.len(), 64, "32-byte hex magic expected");
}

/// `org.invites.createLink` — shareable seat-limited link; preview + accept.
#[tokio::test]
async fn org_invites_create_link_accept() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_invites_link.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _recorder) = test_app_with_recorder(db.clone()).await;

    let cookie = setup_owner_org(&app, &db, "linkowner@ex.com", "linkowner1", "inv-link").await;

    let (status, link) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.createLink","input":{"slug":"inv-link","role":"member","max_uses":2}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{link}");
    let invite_url = link["data"]["invite_url"].as_str().expect("invite_url");
    assert_eq!(link["data"]["invite"]["max_uses"], 2, "{link}");
    assert!(link["data"]["invite"]["email"].is_null(), "{link}");
    let token = invite_url.rsplit('/').next().expect("token");

    let (status, preview) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"invites.get","input":{{"token":"{token}"}}}}"#),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["kind"], "org", "{preview}");
    assert_eq!(preview["data"]["org_slug"], "inv-link", "{preview}");
    assert_eq!(preview["data"]["grant"], "member", "{preview}");
    assert_eq!(preview["data"]["seats_remaining"], 2, "{preview}");

    // Two anonymous accepts consume the seats and grant membership.
    for (email, username) in [("m1@ex.com", "member1"), ("m2@ex.com", "member2")] {
        let (status, acc) = rpc_json(
            &app,
            &format!(
                r#"{{"procedure":"invites.accept","input":{{"token":"{token}","email":"{email}","username":"{username}","password":"password1"}}}}"#
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{acc}");
        assert_eq!(acc["data"]["kind"], "org", "{acc}");
        assert_eq!(acc["data"]["org"]["slug"], "inv-link", "{acc}");
    }
    let org = db
        .find_organization_by_slug("inv-link")
        .await
        .expect("find")
        .expect("org");
    let members = db.list_org_members(&org.id).await.expect("members");
    assert_eq!(members.len(), 3, "owner + 2 invitees: {members:?}");

    // Third accept is rejected — seats exhausted.
    let (status, third) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"invites.accept","input":{{"token":"{token}","email":"m3@ex.com","username":"member3","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{third}");
    assert_eq!(third["error"]["code"], "invite.invalid", "{third}");

    // Legacy org.invites.accept must reject a non-org token *without* the
    // unified accept consuming it (instance invite here).
    let inst_token = "a".repeat(64);
    let inst_hash = sha256_hex(inst_token.as_bytes());
    let owner_id = members[0].user_id.clone();
    db.insert_instance_invite(
        "inst-inv-1",
        None,
        &inst_hash,
        None,
        &owner_id,
        Some(1),
    )
    .await
    .expect("insert instance invite");
    let (status, wrong_kind) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.invites.accept","input":{{"token":"{inst_token}","email":"m4@ex.com","username":"member4","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{wrong_kind}");
    assert_eq!(wrong_kind["error"]["code"], "org.invalid_invite", "{wrong_kind}");
    let inst_row = db
        .find_instance_invite_by_token_hash(&inst_hash)
        .await
        .expect("find")
        .expect("instance invite");
    assert_eq!(
        inst_row.use_count, 0,
        "cross-kind accept must not consume the invite"
    );
    assert!(inst_row.accepted_at.is_none());
}

/// `invites.list` returns pending invites without plaintext tokens (ORG-01 / T-10-SC).
#[tokio::test]
async fn org_invites_list_omits_plaintext_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_invites_list.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _recorder) = test_app_with_recorder(db.clone()).await;

    let cookie = setup_owner_org(&app, &db, "listowner@ex.com", "listowner1", "inv-list").await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.create","input":{"slug":"inv-list","emails":["listed@ex.com"],"role":"admin"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.list","input":{"slug":"inv-list"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "invites.list — {v}");
    assert_eq!(v["ok"], true, "{v}");
    let invites = v["data"]["invites"].as_array().expect("invites array");
    assert_eq!(invites.len(), 1, "{v}");
    assert_eq!(invites[0]["email"], "listed@ex.com");
    assert_eq!(invites[0]["role"], "admin");
    assert!(
        invites[0].get("token").is_none() && invites[0].get("token_hash").is_none(),
        "list must omit plaintext token and hash: {v}"
    );
}

/// `invites.revoke` removes a pending invite (ORG-01).
#[tokio::test]
async fn org_invites_revoke() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_invites_revoke.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, _recorder) = test_app_with_recorder(db.clone()).await;

    let cookie = setup_owner_org(&app, &db, "revowner@ex.com", "revowner1", "inv-revoke").await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.create","input":{"slug":"inv-revoke","emails":["revokee@ex.com"],"role":"member"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    let invite_id = create_v["data"]["results"][0]["invite"]["id"]
        .as_str()
        .expect("invite id");

    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.invites.revoke","input":{{"slug":"inv-revoke","invite_id":"{invite_id}"}}}}"#
        ),
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "invites.revoke — {v}");
    assert_eq!(v["ok"], true, "{v}");

    let (_, list_v) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.list","input":{"slug":"inv-revoke"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(list_v["ok"], true, "{list_v}");
    let invites = list_v["data"]["invites"].as_array().expect("invites");
    assert!(
        invites.is_empty(),
        "revoked invite must leave pending list: {list_v}"
    );
}

/// `invites.accept` creates account when allow_signup is false (ORG-01 / D-ORG-03 / A2).
#[tokio::test]
async fn org_invites_accept_closed_signup_creates_or_links_account() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_invites_accept.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let cookie = setup_owner_org(&app, &db, "accowner@ex.com", "accowner1", "inv-accept").await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.create","input":{"slug":"inv-accept","emails":["invitee@ex.com"],"role":"member"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");

    let token = {
        let sent = recorder.sent.lock().expect("lock");
        extract_invite_token(&invite_email(&sent).text)
    };

    close_signup(&db).await;

    // Public signup must stay closed.
    let (signup_status, signup_v) = rpc_json(
        &app,
        r#"{"procedure":"auth.signup","input":{"email":"other@ex.com","username":"otheruser","password":"password1"}}"#,
        None,
    )
    .await;
    assert_eq!(signup_status, StatusCode::BAD_REQUEST, "{signup_v}");
    assert_eq!(signup_v["error"]["code"], "auth.signup_closed");

    // Invite accept provisions despite closed signup.
    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.invites.accept","input":{{"token":"{token}","username":"invitee1","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "invites.accept — {v}");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["member"]["username"], "invitee1");
    assert_eq!(v["data"]["member"]["role"], "member");
    assert_eq!(v["data"]["org"]["slug"], "inv-accept");

    // Single-use: second accept fails with stable invalid token.
    let (reuse_status, reuse_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.invites.accept","input":{{"token":"{token}","username":"invitee2","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(reuse_status, StatusCode::BAD_REQUEST, "{reuse_v}");
    assert_eq!(reuse_v["error"]["code"], "org.invalid_invite");
}

/// Invite tokens stored as hash-at-rest only (ORG-01 / T-10-11).
#[tokio::test]
async fn org_invites_token_hash_at_rest() {
    let sql = include_str!("../../oxidean-db/migrations/sqlite/0010_orgs_acl.sql");
    assert!(
        sql.contains("token_hash"),
        "organization_invites must define token_hash"
    );
    assert!(
        !has_plaintext_token_column(sql),
        "organization_invites must not store plaintext token"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_invites_hash.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let cookie = setup_owner_org(&app, &db, "hashowner@ex.com", "hashowner1", "inv-hash").await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.create","input":{"slug":"inv-hash","emails":["hashed@ex.com"],"role":"member"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    assert!(
        create_v["data"]["results"][0]["invite"].get("token").is_none()
            && create_v["data"]["results"][0]["invite"].get("token_hash").is_none()
            && create_v["data"].get("token").is_none()
            && create_v["data"].get("token_hash").is_none(),
        "RPC must not expose token or hash: {create_v}"
    );

    let token = {
        let sent = recorder.sent.lock().expect("lock");
        extract_invite_token(&invite_email(&sent).text)
    };
    assert_eq!(token.len(), 64);
    let expected_hash = sha256_hex(token.as_bytes());
    let row = db
        .find_org_invite_by_id(
            create_v["data"]["results"][0]["invite"]["id"]
                .as_str()
                .expect("id"),
        )
        .await
        .expect("find invite")
        .expect("invite row");
    assert_eq!(row.token_hash, expected_hash);
    assert_ne!(row.token_hash, token);
}

/// Expired invite → stable `org.invalid_invite` (ASSUME TTL).
#[tokio::test]
async fn org_invites_accept_expired_token_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("org_invites_expired.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let (app, recorder) = test_app_with_recorder(db.clone()).await;

    let cookie = setup_owner_org(&app, &db, "expowner@ex.com", "expowner1", "inv-exp").await;

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.invites.create","input":{"slug":"inv-exp","emails":["expired@ex.com"],"role":"member"}}"#,
        Some(&cookie),
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

    let past = (chrono::Utc::now() - chrono::Duration::days(8))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_org_invite_expires_at(invite_id, &past)
        .await
        .expect("backdate expiry");

    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.invites.accept","input":{{"token":"{token}","username":"expired1","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "org.invalid_invite");
}

fn has_plaintext_token_column(sql: &str) -> bool {
    let lower = sql.to_lowercase();
    let Some(start) = lower.find("create table if not exists organization_invites") else {
        return true;
    };
    let rest = &lower[start..];
    let end = rest.find(')').unwrap_or(rest.len());
    let block = &rest[..end];
    block.split(',').any(|col| {
        let t = col.trim();
        t.starts_with("token ") || t.starts_with("token\t") || t == "token"
    })
}
