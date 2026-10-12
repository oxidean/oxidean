//! Repository email invites (`repo.invites.*`) + accept kind `repo`.

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

async fn test_app_with_recorder(
    db: Database,
    repos_dir: std::path::PathBuf,
) -> (axum::Router, Arc<RecordingSender>) {
    let recorder = Arc::new(RecordingSender::default());
    let state = AppState::new(db, recorder.clone() as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
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

async fn setup_owner_repo(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
    repo_name: &str,
) -> String {
    let (cookie, login_v) = signup_and_login(app, email, username).await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(db, user_id).await;
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{repo_name}","visibility":"private"}}}}"#
    );
    let (_, create_v) = rpc_json(app, &body, Some(&cookie)).await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    cookie
}

#[tokio::test]
async fn repo_invites_create_list_revoke_and_accept_closed_signup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_invites_flow.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).expect("repos dir");
    let (app, recorder) = test_app_with_recorder(db.clone(), repos_dir).await;

    let cookie = setup_owner_repo(&app, &db, "rown@ex.com", "rown1", "shared").await;

    // Non-admin outsider cannot create.
    let (outsider, out_v) = signup_and_login(&app, "outsider@ex.com", "outsider1").await;
    verify_user(&db, out_v["data"]["id"].as_str().expect("id")).await;
    let (status, forbidden) = rpc_json(
        &app,
        r#"{"procedure":"repo.invites.create","input":{"owner":"rown1","name":"shared","emails":["x@ex.com"],"permission":"read"}}"#,
        Some(&outsider),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{forbidden}");
    assert_eq!(forbidden["error"]["code"], "repo.not_found");

    let (status, create_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.invites.create","input":{"owner":"rown1","name":"shared","emails":["invitee@ex.com"],"permission":"write"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{create_v}");
    assert_eq!(create_v["ok"], true, "{create_v}");
    let first = &create_v["data"]["results"][0];
    assert_eq!(first["ok"], true, "{create_v}");
    assert_eq!(first["invite"]["email"], "invitee@ex.com");
    assert_eq!(first["invite"]["permission"], "write");
    let invite_url = first["invite_url"].as_str().expect("invite_url");
    assert!(invite_url.contains("/invites/"), "{create_v}");
    let invite_id = first["invite"]["id"].as_str().expect("id");

    let token = {
        let sent = recorder.sent.lock().expect("lock");
        let invite = invite_email(&sent);
        assert_eq!(invite.to, "invitee@ex.com");
        extract_invite_token(&invite.text)
    };
    assert_eq!(token.len(), 64);
    let expected_hash = sha256_hex(token.as_bytes());
    let row = db
        .find_repo_invite_by_id(invite_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(row.token_hash, expected_hash);

    let (_, list_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.invites.list","input":{"owner":"rown1","name":"shared"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(list_v["ok"], true, "{list_v}");
    assert_eq!(list_v["data"]["invites"].as_array().unwrap().len(), 1);

    // Revoke then recreate for accept path.
    let (_, revoke_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.invites.revoke","input":{{"owner":"rown1","name":"shared","invite_id":"{invite_id}"}}}}"#
        ),
        Some(&cookie),
    )
    .await;
    assert_eq!(revoke_v["ok"], true, "{revoke_v}");

    let (_, create2) = rpc_json(
        &app,
        r#"{"procedure":"repo.invites.create","input":{"owner":"rown1","name":"shared","emails":["invitee2@ex.com"],"permission":"admin"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create2["ok"], true, "{create2}");
    let token2 = {
        let sent = recorder.sent.lock().expect("lock");
        // Last invite email.
        sent.iter()
            .rev()
            .find(|m| m.to == "invitee2@ex.com")
            .map(|m| extract_invite_token(&m.text))
            .expect("invitee2 email")
    };

    close_signup(&db).await;

    let (status, accept_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"invites.accept","input":{{"token":"{token2}","username":"invitee2","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accept_v}");
    assert_eq!(accept_v["data"]["kind"], "repo");
    assert_eq!(accept_v["data"]["owner"], "rown1");
    assert_eq!(accept_v["data"]["name"], "shared");
    assert_eq!(accept_v["data"]["permission"], "admin");

    // Grant exists.
    let (_, collabs) = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.list","input":{"owner":"rown1","name":"shared"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(collabs["ok"], true, "{collabs}");
    let rows = collabs["data"]["collaborators"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|c| c["username"] == "invitee2" && c["permission"] == "admin"),
        "{collabs}"
    );
}

#[tokio::test]
async fn invites_accept_repo_returns_existing_collaborator_permission() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_invites_existing_perm.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).expect("repos dir");
    let (app, recorder) = test_app_with_recorder(db.clone(), repos_dir).await;

    let owner_cookie = setup_owner_repo(&app, &db, "eown@ex.com", "eown1", "shared").await;

    let (invitee_cookie, invitee_v) = signup_and_login(&app, "already@ex.com", "already1").await;
    verify_user(&db, invitee_v["data"]["id"].as_str().expect("id")).await;

    // Invite first (create rejects already-collaborators), then raise grant to admin.
    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.invites.create","input":{"owner":"eown1","name":"shared","emails":["already@ex.com"],"permission":"read"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    let token = {
        let sent = recorder.sent.lock().expect("lock");
        extract_invite_token(
            &sent
                .iter()
                .rev()
                .find(|m| m.to == "already@ex.com")
                .expect("invite email")
                .text,
        )
    };

    let (_, add_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"eown1","name":"shared","username":"already1","permission":"admin"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_v["ok"], true, "{add_v}");

    let (status, accept_v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"invites.accept","input":{{"token":"{token}"}}}}"#),
        Some(&invitee_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accept_v}");
    assert_eq!(accept_v["ok"], true, "{accept_v}");
    assert_eq!(accept_v["data"]["kind"], "repo");
    assert_eq!(
        accept_v["data"]["permission"], "admin",
        "must report existing grant, not invite read — {accept_v}"
    );

    let (_, collabs) = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.list","input":{"owner":"eown1","name":"shared"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(collabs["ok"], true, "{collabs}");
    let rows = collabs["data"]["collaborators"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|c| c["username"] == "already1" && c["permission"] == "admin"),
        "existing grant must not be downgraded — {collabs}"
    );
}

#[tokio::test]
async fn invites_accept_repo_owner_short_circuit_returns_admin() {
    // RPC create rejects inviting the personal owner; seed the invite row directly.
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_invites_owner_accept.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).expect("repos dir");
    let (app, _) = test_app_with_recorder(db.clone(), repos_dir).await;

    let owner_cookie = setup_owner_repo(&app, &db, "oacc@ex.com", "oacc1", "mine").await;
    let owner = db
        .find_user_by_username("oacc1")
        .await
        .expect("find")
        .expect("owner");
    let repo = db
        .find_repository_by_owner_name(&owner.id, "mine")
        .await
        .expect("find repo")
        .expect("repo");

    let raw_token = "a".repeat(64);
    let token_hash = sha256_hex(raw_token.as_bytes());
    let expires = (chrono::Utc::now() + chrono::Duration::days(7))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.insert_repo_invite(
        "inv-owner-self",
        &repo.id,
        Some("oacc@ex.com"),
        "read",
        &token_hash,
        Some(&expires),
        &owner.id,
        Some(1),
    )
    .await
    .expect("insert invite");

    let (status, accept_v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"invites.accept","input":{{"token":"{raw_token}"}}}}"#),
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accept_v}");
    assert_eq!(accept_v["data"]["kind"], "repo");
    assert_eq!(
        accept_v["data"]["permission"], "admin",
        "owner short-circuit must report admin — {accept_v}"
    );
}

#[tokio::test]
async fn repo_invites_expired_token_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_invites_exp.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).expect("repos dir");
    let (app, recorder) = test_app_with_recorder(db.clone(), repos_dir).await;

    let cookie = setup_owner_repo(&app, &db, "expown@ex.com", "expown1", "exp-repo").await;
    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.invites.create","input":{"owner":"expown1","name":"exp-repo","emails":["late@ex.com"],"permission":"read"}}"#,
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
    db.set_repo_invite_expires_at(invite_id, &past)
        .await
        .expect("backdate");

    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"invites.accept","input":{{"token":"{token}","username":"late1","password":"password1"}}}}"#
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "invite.invalid");
}
