//! Phase 15: GIT-16/17 rename, transfer, redirect tests (D-REL-07..11).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::jobs::orphan_reconcile;
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::CliGitBackend;
use tower::ServiceExt;
use uuid::Uuid;

async fn test_app(db: Database, repos_dir: std::path::PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir)
        .with_git(Arc::new(CliGitBackend::new()));
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

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("rpc json body")
}

async fn setup_empty_repo(app: &axum::Router, cookie: &str, repo: &str) -> serde_json::Value {
    let create = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{repo}","visibility":"public","description":""}}}}"#
        ),
        cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    create
}

/// GIT-16 / D-REL-07 / D-REL-08: Admin rename moves disk+DB and inserts redirect.
#[tokio::test]
async fn repo_rename_admin_moves_disk_and_inserts_redirect() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("rename_ok.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, login_v) = signup_and_login(&app, "ren@ex.com", "renowner").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    setup_empty_repo(&app, &cookie, "oldname").await;
    let old_bare = repos.join("renowner").join("oldname.git");
    assert!(old_bare.exists(), "bare should exist before rename");

    let renamed = rpc_json(
        &app,
        r#"{"procedure":"repo.rename","input":{"owner":"renowner","name":"oldname","newName":"newname"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(renamed["ok"], true, "{renamed}");
    assert_eq!(renamed["data"]["repo"]["name"], "newname");

    let new_bare = repos.join("renowner").join("newname.git");
    assert!(new_bare.exists(), "bare should exist at new path");
    assert!(!old_bare.exists(), "old bare path should be gone");

    let redir = db
        .find_repository_redirect("renowner", "oldname")
        .await
        .expect("find redirect")
        .expect("redirect row");
    assert_eq!(
        redir.repo_id,
        renamed["data"]["repo"]["id"].as_str().unwrap()
    );
}

/// D-REL-07: non-admin rename → soft repo.not_found.
#[tokio::test]
async fn repo_rename_non_admin_soft_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("rename_deny.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, login_v) = signup_and_login(&app, "own@ex.com", "ownadmin").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    setup_empty_repo(&app, &cookie, "hello").await;

    let (writer_cookie, writer_v) = signup_and_login(&app, "w@ex.com", "writer1").await;
    let writer_id = writer_v["data"]["id"].as_str().unwrap().to_string();
    db.set_email_verified_at(&writer_id, &now)
        .await
        .expect("verify writer");
    let add = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"ownadmin","name":"hello","username":"writer1","permission":"write"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");

    let denied = rpc_json(
        &app,
        r#"{"procedure":"repo.rename","input":{"owner":"ownadmin","name":"hello","newName":"nope"}}"#,
        &writer_cookie,
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found");
}

/// D-REL-08: redirect resolve on old path within retention.
#[tokio::test]
async fn redirect_resolve_old_path_within_retention() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("redir_ok.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, login_v) = signup_and_login(&app, "r2@ex.com", "rediruser").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    setup_empty_repo(&app, &cookie, "alpha").await;

    let renamed = rpc_json(
        &app,
        r#"{"procedure":"repo.rename","input":{"owner":"rediruser","name":"alpha","newName":"beta"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(renamed["ok"], true, "{renamed}");

    let old_get = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"rediruser","name":"alpha"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(old_get["ok"], true, "{old_get}");
    assert_eq!(old_get["data"]["name"], "beta");
    assert_eq!(
        old_get["data"]["id"],
        renamed["data"]["repo"]["id"].as_str().unwrap()
    );
}

/// Live repo at old path supersedes redirect.
#[tokio::test]
async fn redirect_supersede_when_new_repo_occupies_old_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("redir_sup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, login_v) = signup_and_login(&app, "s@ex.com", "supuser").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    setup_empty_repo(&app, &cookie, "gamma").await;

    let renamed = rpc_json(
        &app,
        r#"{"procedure":"repo.rename","input":{"owner":"supuser","name":"gamma","newName":"delta"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(renamed["ok"], true, "{renamed}");
    assert!(db
        .find_repository_redirect("supuser", "gamma")
        .await
        .unwrap()
        .is_some());

    let recreated = setup_empty_repo(&app, &cookie, "gamma").await;
    assert!(db
        .find_repository_redirect("supuser", "gamma")
        .await
        .unwrap()
        .is_none());

    let get_old = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"supuser","name":"gamma"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(get_old["ok"], true, "{get_old}");
    assert_eq!(
        get_old["data"]["id"],
        recreated["data"]["id"].as_str().unwrap()
    );
    assert_ne!(
        get_old["data"]["id"],
        renamed["data"]["repo"]["id"].as_str().unwrap()
    );
}

/// Expired redirects purge via orphan reconcile.
#[tokio::test]
async fn redirect_purge_expired_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("redir_purge.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, login_v) = signup_and_login(&app, "p@ex.com", "purgeuser").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    let created = setup_empty_repo(&app, &cookie, "eps").await;
    let repo_id = created["data"]["id"].as_str().unwrap();

    let past = (chrono::Utc::now() - chrono::Duration::days(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.insert_repository_redirect(
        &Uuid::new_v4().to_string(),
        "purgeuser",
        "old-eps",
        repo_id,
        &past,
    )
    .await
    .expect("insert expired redirect");
    assert!(db
        .find_repository_redirect("purgeuser", "old-eps")
        .await
        .unwrap()
        .is_some());

    let stats = orphan_reconcile(&db, &repos).await.expect("reconcile");
    assert!(stats.purged_expired_redirects >= 1);
    assert!(db
        .find_repository_redirect("purgeuser", "old-eps")
        .await
        .unwrap()
        .is_none());
}

/// GIT-17 / D-REL-09 / D-REL-10: Admin transfer to user with type-confirm.
#[tokio::test]
async fn repo_transfer_admin_to_user_or_org_with_confirm() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("xfer_ok.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, login_v) = signup_and_login(&app, "t1@ex.com", "fromuser").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    setup_empty_repo(&app, &cookie, "ship").await;

    let (dest_cookie, dest_v) = signup_and_login(&app, "t2@ex.com", "touser").await;
    let dest_id = dest_v["data"]["id"].as_str().unwrap().to_string();
    db.set_email_verified_at(&dest_id, &now)
        .await
        .expect("verify dest");
    let _ = dest_cookie;

    let xfer = rpc_json(
        &app,
        r#"{"procedure":"repo.transfer","input":{"owner":"fromuser","name":"ship","destOwner":"touser","destOwnerType":"user","confirmName":"ship"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(xfer["ok"], true, "{xfer}");
    assert_eq!(xfer["data"]["repo"]["owner_username"], "touser");
    assert!(repos.join("touser").join("ship.git").exists());
    assert!(!repos.join("fromuser").join("ship.git").exists());
    assert!(db
        .find_repository_redirect("fromuser", "ship")
        .await
        .unwrap()
        .is_some());
    // Former owner becomes admin collaborator.
    let collab = db
        .find_repo_collaborator(xfer["data"]["repo"]["id"].as_str().unwrap(), &user_id)
        .await
        .unwrap();
    assert!(collab.is_some());
}

/// D-REL-10: confirm_name mismatch rejected.
#[tokio::test]
async fn repo_transfer_confirm_mismatch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("xfer_mm.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (cookie, login_v) = signup_and_login(&app, "m1@ex.com", "mmfrom").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    setup_empty_repo(&app, &cookie, "boat").await;
    let (dest_c, dest_v) = signup_and_login(&app, "m2@ex.com", "mmto").await;
    let dest_id = dest_v["data"]["id"].as_str().unwrap().to_string();
    db.set_email_verified_at(&dest_id, &now)
        .await
        .expect("verify");
    let _ = dest_c;

    let bad = rpc_json(
        &app,
        r#"{"procedure":"repo.transfer","input":{"owner":"mmfrom","name":"boat","destOwner":"mmto","destOwnerType":"user","confirmName":"wrong"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(bad["ok"], false, "{bad}");
    assert_eq!(bad["error"]["code"], "repo.confirm_mismatch");
}

/// D-REL-10: issues associations stay on repo_id (when tables exist).
#[tokio::test]
async fn repo_transfer_cascade_issues_lfs_by_repo_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("xfer_cas.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (cookie, login_v) = signup_and_login(&app, "c1@ex.com", "casfrom").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    let created = setup_empty_repo(&app, &cookie, "cargo").await;
    let repo_id = created["data"]["id"].as_str().unwrap().to_string();

    // Create an issue if the RPC exists — ownership rewrite must keep repo_id.
    let issue = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"casfrom","name":"cargo","title":"keep me","body":""}}"#,
        &cookie,
    )
    .await;
    let issue_ok = issue["ok"] == true;
    let issue_number = if issue_ok {
        issue["data"]["number"].as_i64()
    } else {
        None
    };

    let (dest_c, dest_v) = signup_and_login(&app, "c2@ex.com", "casto").await;
    let dest_id = dest_v["data"]["id"].as_str().unwrap().to_string();
    db.set_email_verified_at(&dest_id, &now)
        .await
        .expect("verify");
    let _ = dest_c;

    let xfer = rpc_json(
        &app,
        r#"{"procedure":"repo.transfer","input":{"owner":"casfrom","name":"cargo","destOwner":"casto","destOwnerType":"user","confirmName":"cargo"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(xfer["ok"], true, "{xfer}");
    assert_eq!(xfer["data"]["repo"]["id"], repo_id);

    if let Some(n) = issue_number {
        let got = rpc_json(
            &app,
            &format!(
                r#"{{"procedure":"issue.get","input":{{"owner":"casto","name":"cargo","number":{n}}}}}"#
            ),
            &cookie,
        )
        .await;
        // Former owner is collaborator admin — should still read.
        assert_eq!(got["ok"], true, "{got}");
        assert_eq!(got["data"]["number"], n);
    }
}
