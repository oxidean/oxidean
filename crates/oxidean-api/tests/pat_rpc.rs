//! GIT-11: pat.createClassic / createFineGrained / list / revoke + verified gate.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::auth::session::sha256_hex;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::{CLASSIC_PAT_PREFIX, FINE_GRAINED_PAT_PREFIX};
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

fn rpc_req_with_bearer(body: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("authorization", format!("Bearer {token}"))
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

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> (StatusCode, serde_json::Value) {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, v)
}

async fn rpc_json_bearer(
    app: &axum::Router,
    body: &str,
    token: &str,
) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
    let res = app
        .clone()
        .oneshot(rpc_req_with_bearer(body, token))
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, headers, v)
}

/// Verified `pat.createClassic` returns a one-time plaintext `token` field (D-15).
#[tokio::test]
async fn pat_create_classic_returns_one_time_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_create.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "pat@ex.com", "patuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"laptop","scopes":["repo"]}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "createClassic — {v}");
    assert_eq!(v["ok"], true, "{v}");
    let token = v["data"]["token"].as_str().expect("token");
    assert!(
        token.starts_with(CLASSIC_PAT_PREFIX),
        "must mint {CLASSIC_PAT_PREFIX}* — got {token}"
    );
    assert_eq!(v["data"]["item"]["kind"], "classic");
    assert_eq!(v["data"]["item"]["name"], "laptop");
    let prefix = v["data"]["item"]["token_prefix"]
        .as_str()
        .expect("token_prefix");
    assert!(
        prefix.starts_with(CLASSIC_PAT_PREFIX) && prefix.len() == CLASSIC_PAT_PREFIX.len() + 8,
        "token_prefix must be brand + 8 hex fingerprint — {prefix}"
    );
    assert!(
        token.starts_with(prefix),
        "plaintext must start with display prefix — {token} / {prefix}"
    );
    assert!(v["data"]["item"].get("token").is_none());
}

/// `pat.list` never returns plaintext token secrets (D-15 / T-08-01).
#[tokio::test]
async fn pat_list_omits_secret_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_list.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "list@ex.com", "listuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"ci","scopes":["repo"]}}"#,
        &cookie,
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    let plaintext = create_v["data"]["token"].as_str().unwrap().to_string();

    let (status, list_v) = rpc_json(&app, r#"{"procedure":"pat.list","input":{}}"#, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{list_v}");
    assert_eq!(list_v["ok"], true);
    let items = list_v["data"].as_array().expect("list array");
    assert_eq!(items.len(), 1);
    assert!(items[0].get("token").is_none(), "list must omit secret");
    let dumped = list_v.to_string();
    assert!(
        !dumped.contains(&plaintext),
        "plaintext must not appear in list response"
    );
    assert_eq!(
        items[0]["token_prefix"].as_str().unwrap().len(),
        CLASSIC_PAT_PREFIX.len() + 8
    );
    assert!(plaintext.starts_with(items[0]["token_prefix"].as_str().unwrap()));
}

/// `pat.revoke` removes the token from subsequent list results.
#[tokio::test]
async fn pat_revoke_removes_from_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_revoke.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "rev@ex.com", "revuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (_, create_v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"temp","scopes":["repo"]}}"#,
        &cookie,
    )
    .await;
    let id = create_v["data"]["item"]["id"].as_str().unwrap();

    let (status, rev_v) = rpc_json(
        &app,
        &format!(r#"{{"procedure":"pat.revoke","input":{{"id":"{id}"}}}}"#),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rev_v}");
    assert_eq!(rev_v["ok"], true);

    let (_, list_v) = rpc_json(&app, r#"{"procedure":"pat.list","input":{}}"#, &cookie).await;
    let items = list_v["data"].as_array().expect("list");
    assert!(
        items.iter().all(|i| i["id"] != id),
        "revoked id must be absent — {list_v}"
    );
}

/// Unverified session cannot create PATs → `auth.email_unverified` (D-24).
#[tokio::test]
async fn pat_create_unverified_email_unverified() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_unverified.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    let (cookie, login_v) = signup_and_login(&app, "newbie@ex.com", "newbie1").await;
    assert_eq!(login_v["data"]["email_verified"], false);

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"x","scopes":["repo"]}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["error"]["code"], "auth.email_unverified");
}

/// Empty / whitespace note on create → `pat.note_required` (D-16).
#[tokio::test]
async fn pat_create_empty_note_required() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_note.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "note@ex.com", "noteuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createClassic","input":{"name":"   ","scopes":["repo"]}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "pat.note_required");
}

/// Verified `pat.createFineGrained` all + contents write returns one-time `oxidean_fg_` token.
#[tokio::test]
async fn pat_create_fine_grained_all_returns_fg_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_fg_all.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "fgall@ex.com", "fgalluser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createFineGrained","input":{"name":"ci-all","repo_access":"all","contents":"write"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "createFineGrained all — {v}");
    assert_eq!(v["ok"], true, "{v}");
    let token = v["data"]["token"].as_str().expect("token");
    assert!(
        token.starts_with(FINE_GRAINED_PAT_PREFIX),
        "must mint {FINE_GRAINED_PAT_PREFIX}* — got {token}"
    );
    assert_eq!(v["data"]["item"]["kind"], "fine_grained");
    assert_eq!(v["data"]["item"]["name"], "ci-all");
    let prefix = v["data"]["item"]["token_prefix"]
        .as_str()
        .expect("token_prefix");
    assert!(
        prefix.starts_with(FINE_GRAINED_PAT_PREFIX)
            && prefix.len() == FINE_GRAINED_PAT_PREFIX.len() + 8,
        "token_prefix must be brand + 8 hex fingerprint — {prefix}"
    );
    assert!(
        token.starts_with(prefix),
        "plaintext must start with display prefix — {token} / {prefix}"
    );
    assert_eq!(v["data"]["item"]["repo_access"], "all");
    assert_eq!(v["data"]["item"]["contents"], "write");
    let repos = v["data"]["item"]["repository_ids"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        repos.is_empty(),
        "all mode must not persist join rows — {v}"
    );
    assert!(v["data"]["item"].get("token").is_none());
}

/// Selected FG with owned repo ids persists join rows; list shows fine_grained kind.
#[tokio::test]
async fn pat_create_fine_grained_selected_persists_repos() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_fg_sel.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "fgsel@ex.com", "fgseluser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let repo = db
        .insert_repository("r-fg-1", &user_id, "user", "demo", "public", "", "main")
        .await
        .expect("insert repo");

    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pat.createFineGrained","input":{{"name":"laptop-fg","repo_access":"selected","contents":"read","repository_ids":["{}"]}}}}"#,
            repo.id
        ),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "createFineGrained selected — {v}");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["item"]["kind"], "fine_grained");
    assert_eq!(v["data"]["item"]["repo_access"], "selected");
    assert_eq!(v["data"]["item"]["contents"], "read");
    let ids = v["data"]["item"]["repository_ids"]
        .as_array()
        .expect("repository_ids");
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0], repo.id);

    let (list_status, list_v) =
        rpc_json(&app, r#"{"procedure":"pat.list","input":{}}"#, &cookie).await;
    assert_eq!(list_status, StatusCode::OK, "{list_v}");
    let items = list_v["data"].as_array().expect("list");
    let fg = items
        .iter()
        .find(|i| i["kind"] == "fine_grained")
        .expect("fine_grained in list");
    let fg_prefix = fg["token_prefix"].as_str().expect("token_prefix");
    assert!(
        fg_prefix.starts_with(FINE_GRAINED_PAT_PREFIX)
            && fg_prefix.len() == FINE_GRAINED_PAT_PREFIX.len() + 8,
        "token_prefix must be brand + 8 hex fingerprint — {fg_prefix}"
    );
    assert_eq!(fg["repository_ids"].as_array().unwrap()[0], repo.id);
}

/// Selected with empty repository_ids → pat.repos_required.
#[tokio::test]
async fn pat_create_fine_grained_selected_empty_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_fg_empty.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "fgempty@ex.com", "fgemptyu").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createFineGrained","input":{"name":"bad","repo_access":"selected","contents":"write","repository_ids":[]}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "pat.repos_required");
}

/// Unverified createFineGrained → auth.email_unverified (D-24).
#[tokio::test]
async fn pat_create_fine_grained_unverified_email_unverified() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_fg_unv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    let (cookie, login_v) = signup_and_login(&app, "fgnew@ex.com", "fgnewbie").await;
    assert_eq!(login_v["data"]["email_verified"], false);

    let (status, v) = rpc_json(
        &app,
        r#"{"procedure":"pat.createFineGrained","input":{"name":"x","repo_access":"all","contents":"read"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["error"]["code"], "auth.email_unverified");
}

/// Selected with a repo not owned by caller → pat.invalid_scope (T-08-06).
#[tokio::test]
async fn pat_create_fine_grained_foreign_repo_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_fg_foreign.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie_a, login_a) = signup_and_login(&app, "fga@ex.com", "fgauser").await;
    let user_a = login_a["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_a, &now)
        .await
        .expect("verify a");

    let (cookie_b, login_b) = signup_and_login(&app, "fgb@ex.com", "fgbuser").await;
    let user_b = login_b["data"]["id"].as_str().expect("id").to_string();
    db.set_email_verified_at(&user_b, &now)
        .await
        .expect("verify b");

    let foreign = db
        .insert_repository(
            "r-foreign",
            &user_a,
            "user",
            "secrets",
            "private",
            "",
            "main",
        )
        .await
        .expect("foreign repo");

    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pat.createFineGrained","input":{{"name":"steal","repo_access":"selected","contents":"write","repository_ids":["{}"]}}}}"#,
            foreign.id
        ),
        &cookie_b,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "pat.invalid_scope");
    let _ = cookie_a; // keep a session created for ownership fixture
}

/// ORG-04 / A4: FG Selected may include repos where subject has ACL capability.
#[tokio::test]
async fn pat_create_fine_grained_selected_allows_collaborator_repo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_fg_collab.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_owner_cookie, owner_v) = signup_and_login(&app, "fgco@ex.com", "fgcoown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&owner_id, &now)
        .await
        .expect("verify owner");

    let (collab_cookie, collab_v) = signup_and_login(&app, "fgcc@ex.com", "fgccoll").await;
    let collab_id = collab_v["data"]["id"].as_str().expect("id").to_string();
    db.set_email_verified_at(&collab_id, &now)
        .await
        .expect("verify collab");

    let repo = db
        .insert_repository(
            "r-collab-fg",
            &owner_id,
            "user",
            "shared",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");
    db.insert_repo_collaborator(&repo.id, &collab_id, "write")
        .await
        .expect("grant write");

    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pat.createFineGrained","input":{{"name":"collab-fg","repo_access":"selected","contents":"write","repository_ids":["{}"]}}}}"#,
            repo.id
        ),
        &collab_cookie,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "collaborator Write must mint Selected FG — {v}"
    );
    assert_eq!(v["ok"], true, "{v}");
    let ids = v["data"]["item"]["repository_ids"]
        .as_array()
        .expect("repository_ids");
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0], repo.id);
}

/// Read collaborator cannot mint Selected FG with contents:write (capability mismatch).
#[tokio::test]
async fn pat_create_fine_grained_selected_read_collab_write_contents_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("pat_fg_read_deny.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_owner_cookie, owner_v) = signup_and_login(&app, "fgrd@ex.com", "fgrdown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&owner_id, &now)
        .await
        .expect("verify owner");

    let (collab_cookie, collab_v) = signup_and_login(&app, "fgrc@ex.com", "fgrcoll").await;
    let collab_id = collab_v["data"]["id"].as_str().expect("id").to_string();
    db.set_email_verified_at(&collab_id, &now)
        .await
        .expect("verify collab");

    let repo = db
        .insert_repository("r-read-fg", &owner_id, "user", "ro", "private", "", "main")
        .await
        .expect("repo");
    db.insert_repo_collaborator(&repo.id, &collab_id, "read")
        .await
        .expect("grant read");

    let (status, v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pat.createFineGrained","input":{{"name":"too-much","repo_access":"selected","contents":"write","repository_ids":["{}"]}}}}"#,
            repo.id
        ),
        &collab_cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "pat.invalid_scope");
}
// --- API-02: Bearer PAT auth on /api/rpc ---

/// Signup + verify, then mint a classic `repo` PAT over the session cookie.
async fn verified_user_with_classic_pat(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
    scopes: &str,
) -> (String, String, String) {
    let (cookie, login_v) = signup_and_login(app, email, username).await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    let (_, create_v) = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"pat.createClassic","input":{{"name":"bearer","scopes":{scopes}}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    let token = create_v["data"]["token"]
        .as_str()
        .expect("token")
        .to_string();
    (user_id, cookie, token)
}

/// API-02: classic `repo` PAT resolves the owner identity on repo-domain RPCs;
/// the response never carries `Set-Cookie` (Bearer calls mint nothing).
#[tokio::test]
async fn pat_bearer_repo_scope_rpc_success_no_cookie() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_bearer_ok.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (user_id, _cookie, token) =
        verified_user_with_classic_pat(&app, &db, "bearer@ex.com", "beareruser", r#"["repo"]"#)
            .await;
    db.insert_repository(
        "r-bearer-1",
        &user_id,
        "user",
        "demo",
        "private",
        "",
        "main",
    )
    .await
    .expect("insert repo");

    // Identity resolves the token owner.
    let (status, headers, v) =
        rpc_json_bearer(&app, r#"{"procedure":"auth.me","input":{}}"#, &token).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["id"], user_id, "PAT must resolve owner — {v}");
    assert!(
        headers.get("set-cookie").is_none(),
        "Bearer calls must not set cookies"
    );

    // Repo-domain read on the owner's private repo via {owner, name}.
    let (status, headers, v) = rpc_json_bearer(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"beareruser","name":"demo"}}"#,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["name"], "demo", "{v}");
    assert!(headers.get("set-cookie").is_none());
}

/// API-02: unknown / non-PAT bearer tokens → 401 `auth.unauthenticated` with
/// `WWW-Authenticate` — never a silent downgrade to anonymous.
#[tokio::test]
async fn pat_bearer_wrong_token_unauthenticated() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_bearer_bad.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    let unknown = format!("{CLASSIC_PAT_PREFIX}{}", "f".repeat(64));
    let (status, headers, v) =
        rpc_json_bearer(&app, r#"{"procedure":"auth.me","input":{}}"#, &unknown).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["error"]["code"], "auth.unauthenticated", "{v}");
    assert_eq!(v["ok"], false);
    assert!(
        headers
            .get("www-authenticate")
            .is_some_and(|h| h.to_str().unwrap().starts_with("Bearer")),
        "must carry WWW-Authenticate: Bearer — {headers:?}"
    );

    // A non-PAT bearer string is rejected the same way.
    let (status, _, v) =
        rpc_json_bearer(&app, r#"{"procedure":"auth.me","input":{}}"#, "not-a-pat").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["error"]["code"], "auth.unauthenticated");
}

/// API-02: expired PAT → 401 `auth.unauthenticated`.
#[tokio::test]
async fn pat_bearer_expired_token_unauthenticated() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_bearer_exp.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_cookie, login_v) = signup_and_login(&app, "exp@ex.com", "expuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();

    let token = format!("{CLASSIC_PAT_PREFIX}{}", "e".repeat(64));
    let hash = sha256_hex(token.as_bytes());
    let prefix = &token[..CLASSIC_PAT_PREFIX.len() + 8];
    let past = (chrono::Utc::now() - chrono::Duration::hours(2))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.create_pat(
        "pat-exp-1",
        &user_id,
        "classic",
        "old",
        prefix,
        &hash,
        Some(r#"["repo"]"#),
        None,
        None,
        Some(&past),
        &[],
    )
    .await
    .expect("insert expired pat");

    let (status, _, v) =
        rpc_json_bearer(&app, r#"{"procedure":"auth.me","input":{}}"#, &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["error"]["code"], "auth.unauthenticated", "{v}");
}

/// API-02: revoked PAT → 401 `auth.unauthenticated` (revoked rows never match).
#[tokio::test]
async fn pat_bearer_revoked_token_unauthenticated() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_bearer_rev.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_user_id, cookie, token) =
        verified_user_with_classic_pat(&app, &db, "revbearer@ex.com", "revbearer", r#"["repo"]"#)
            .await;
    let (_, list_v) = rpc_json(&app, r#"{"procedure":"pat.list","input":{}}"#, &cookie).await;
    let pat_id = list_v["data"][0]["id"]
        .as_str()
        .expect("pat id")
        .to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.revoke_pat(&pat_id, &now).await.expect("revoke");

    let (status, _, v) =
        rpc_json_bearer(&app, r#"{"procedure":"auth.me","input":{}}"#, &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["error"]["code"], "auth.unauthenticated", "{v}");
}

/// API-02: scope denial is not an auth failure — a classic `repo` token cannot
/// call `packages.*`, and a package-only token row (not mintable via
/// `pat.createClassic`, which requires `repo`, but valid at rest) cannot call
/// repo-domain procedures. Both → 403 `auth.pat_scope`.
#[tokio::test]
async fn pat_bearer_scope_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("pat_bearer_scope.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    // Minted classic tokens always carry `repo`; `packages.*` stays out of scope.
    let (_user_id, _cookie, repo_token) =
        verified_user_with_classic_pat(&app, &db, "scope@ex.com", "scopeuser", r#"["repo"]"#).await;
    let (status, _, v) = rpc_json_bearer(
        &app,
        r#"{"procedure":"packages.list","input":{"owner":"scopeuser"}}"#,
        &repo_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["error"]["code"], "auth.pat_scope", "{v}");

    // A package-only token at rest is denied on repo-domain procedures even
    // though the mint path refuses to create one.
    let (_cookie2, login2) = signup_and_login(&app, "scope2@ex.com", "scopeuser2").await;
    let user2 = login2["data"]["id"].as_str().expect("id").to_string();
    let pkg_token = format!("{CLASSIC_PAT_PREFIX}{}", "a".repeat(64));
    let pkg_hash = sha256_hex(pkg_token.as_bytes());
    let pkg_prefix = &pkg_token[..CLASSIC_PAT_PREFIX.len() + 8];
    db.create_pat(
        "pat-pkg-1",
        &user2,
        "classic",
        "pkg-only",
        pkg_prefix,
        &pkg_hash,
        Some(r#"["package:read"]"#),
        None,
        None,
        None,
        &[],
    )
    .await
    .expect("insert package-only pat");

    let (status, _, v) = rpc_json_bearer(
        &app,
        r#"{"procedure":"repo.listMine","input":{}}"#,
        &pkg_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["error"]["code"], "auth.pat_scope", "{v}");
}

/// API-02: admin procedures are never callable with a PAT — even when the
/// token owner is a system admin.
#[tokio::test]
async fn pat_bearer_admin_procedure_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("pat_bearer_admin.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (user_id, _cookie, token) = verified_user_with_classic_pat(
        &app,
        &db,
        "adminbearer@ex.com",
        "adminbearer",
        r#"["repo","package:write"]"#,
    )
    .await;
    // Make the owner a real sysadmin — the PAT gate must still refuse.
    db.set_user_role(&user_id, "sys-admin")
        .await
        .expect("promote admin");

    let (status, _, v) = rpc_json_bearer(
        &app,
        r#"{"procedure":"admin.users.list","input":{}}"#,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    // pat_scope (not admin.forbidden) proves the dispatch gate fired first.
    assert_eq!(v["error"]["code"], "auth.pat_scope", "{v}");
}

/// API-02: session-lifecycle / credential procedures are cookie-only.
#[tokio::test]
async fn pat_bearer_session_only_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_bearer_sess.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (_user_id, _cookie, token) = verified_user_with_classic_pat(
        &app,
        &db,
        "sessonly@ex.com",
        "sessonly",
        r#"["repo","package:read","package:write"]"#,
    )
    .await;

    for procedure in [
        r#"{"procedure":"pat.list","input":{}}"#,
        r#"{"procedure":"sshKey.list","input":{}}"#,
        r#"{"procedure":"auth.logout","input":{}}"#,
        r#"{"procedure":"org.create","input":{"slug":"x","display_name":"x"}}"#,
    ] {
        let (status, _, v) = rpc_json_bearer(&app, procedure, &token).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{procedure} — {v}");
        assert_eq!(v["error"]["code"], "auth.pat_scope", "{procedure} — {v}");
    }
}

/// API-02: when both credentials are present the session cookie wins — a
/// garbage Bearer header cannot break a cookie-authenticated call.
#[tokio::test]
async fn pat_bearer_cookie_takes_precedence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_bearer_prec.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db).await;

    let (cookie, login_v) = signup_and_login(&app, "prec@ex.com", "precuser").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();

    let req = Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", &cookie)
        .header("authorization", "Bearer oxidean_pat_garbage")
        .body(Body::from(
            r#"{"procedure":"auth.me","input":{}}"#.to_owned(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["id"], user_id, "{v}");
}

/// API-02: fine-grained `contents:read` selected-repo token reads the covered
/// repo, is denied on non-selected private repos (404, anti-enumeration), and
/// cannot write on the covered repo.
#[tokio::test]
async fn pat_bearer_fine_grained_selected_scope() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_bearer_fg.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "fgbearer@ex.com", "fgbearer").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let covered = db
        .insert_repository(
            "r-fg-cov", &user_id, "user", "covered", "private", "", "main",
        )
        .await
        .expect("covered repo");
    let _other = db
        .insert_repository("r-fg-oth", &user_id, "user", "other", "private", "", "main")
        .await
        .expect("other repo");

    let (status, create_v) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pat.createFineGrained","input":{{"name":"sel","repo_access":"selected","contents":"read","repository_ids":["{}"]}}}}"#,
            covered.id
        ),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{create_v}");
    let token = create_v["data"]["token"]
        .as_str()
        .expect("token")
        .to_string();

    // Covered repo → allowed.
    let (status, _, v) = rpc_json_bearer(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"fgbearer","name":"covered"}}"#,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["ok"], true, "{v}");

    // Non-selected private repo → 404 (anti-enumeration, same as unauthed).
    let (status, _, v) = rpc_json_bearer(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"fgbearer","name":"other"}}"#,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{v}");
    assert_eq!(v["error"]["code"], "repo.not_found", "{v}");

    // contents:read cannot write — even on the covered repo.
    let (status, _, v) = rpc_json_bearer(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"fgbearer","name":"covered","title":"x"}}"#,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["error"]["code"], "auth.pat_scope", "{v}");
}

/// API-02: `/api/rpc/ws` accepts `Authorization: Bearer <pat>` at the upgrade;
/// invalid tokens yield an `auth.unauthenticated` frame (no HTTP status on WS).
#[tokio::test]
async fn pat_bearer_ws_auth() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;
    use tokio_tungstenite::tungstenite::Message;

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pat_bearer_ws.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone()).await;

    let (user_id, _cookie, token) =
        verified_user_with_classic_pat(&app, &db, "wsbearer@ex.com", "wsbearer", r#"["repo"]"#)
            .await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Valid PAT → auth.me resolves the token owner.
    let mut req = format!("ws://{addr}/api/rpc/ws")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("Oxidean-RPC-Version", HeaderValue::from_static("1"));
    req.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("ws connect");
    ws.send(Message::Text(
        r#"{"procedure":"auth.me","input":{}}"#.into(),
    ))
    .await
    .unwrap();
    let msg = ws.next().await.unwrap().unwrap();
    let v: serde_json::Value = serde_json::from_str(&msg.into_text().unwrap()).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["id"], user_id, "{v}");

    // Invalid PAT → per-frame RPC error, not a refused upgrade.
    let mut req = format!("ws://{addr}/api/rpc/ws")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("Oxidean-RPC-Version", HeaderValue::from_static("1"));
    req.headers_mut().insert(
        "Authorization",
        HeaderValue::from_static("Bearer oxidean_pat_deadbeef"),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("ws connect");
    ws.send(Message::Text(
        r#"{"procedure":"auth.me","input":{}}"#.into(),
    ))
    .await
    .unwrap();
    let msg = ws.next().await.unwrap().unwrap();
    let v: serde_json::Value = serde_json::from_str(&msg.into_text().unwrap()).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.unauthenticated", "{v}");
}
