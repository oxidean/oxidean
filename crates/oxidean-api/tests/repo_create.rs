//! GIT-01: verified create, unverified gate, duplicate name.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(db: Database, repos_dir: std::path::PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
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

/// Verified session can create a public repository (GIT-01 happy path).
#[tokio::test]
async fn repo_create_verified_happy_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("repo_create_ok.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "owner@ex.com", "owner1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();

    // Mark verified so require_verified passes (D-11).
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let create = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"hello-world","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        create.status(),
        StatusCode::OK,
        "repo.create must succeed for verified owner"
    );
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.create ok=true — {v}");
    assert_eq!(v["data"]["name"], "hello-world", "created repo name");
    assert_eq!(v["data"]["owner_username"], "owner1");
    assert_eq!(v["data"]["visibility"], "public");
    assert_eq!(v["data"]["default_branch"], "main");
}

/// Unverified session must not create repos — auth.email_unverified (D-11 / T-07-05).
#[tokio::test]
async fn repo_create_unverified_email_unverified() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_create_unverified.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db, repos).await;

    let (cookie, login_v) = signup_and_login(&app, "newbie@ex.com", "newbie1").await;
    assert_eq!(login_v["data"]["email_verified"], false);

    let create = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"blocked","visibility":"public","description":""}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        create.status(),
        StatusCode::FORBIDDEN,
        "unverified create must be forbidden"
    );
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        v["error"]["code"], "auth.email_unverified",
        "must surface auth.email_unverified — {v}"
    );
}

/// Second create of same owner+name among non-deleted → stable duplicate error (GIT-01).
#[tokio::test]
async fn repo_create_duplicate_name_stable_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("repo_create_dup.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "dup@ex.com", "dupowner").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let first = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"same-name","visibility":"public"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first_bytes = first.into_body().collect().await.unwrap().to_bytes();
    let first_v: serde_json::Value = serde_json::from_slice(&first_bytes).unwrap();
    assert_eq!(first_v["ok"], true, "first create — {first_v}");

    let second = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"same-name","visibility":"private"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        second.status(),
        StatusCode::BAD_REQUEST,
        "duplicate must not succeed"
    );
    let bytes = second.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        v["error"]["code"], "repo.name_taken",
        "stable duplicate code for inline UI — {v}"
    );
    assert_eq!(
        v["error"]["message"],
        "A repository with this name already exists. Choose a different name.",
        "D-12 inline copy — {v}"
    );
}

/// Template pickers seed a single initial commit on the default branch (ASSUME Q2 / D-02).
#[tokio::test]
async fn repo_create_with_templates_seeds_initial_commit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_create_seed.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "seed@ex.com", "seedowner").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let create = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"templated","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK, "templated create");
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "templated ok — {v}");

    let bare = repos.join("seedowner").join("templated.git");
    assert!(bare.is_dir(), "bare path {}", bare.display());
    let log = std::process::Command::new("git")
        .args(["-C", bare.to_str().unwrap(), "log", "--oneline", "-1"])
        .output()
        .expect("git log");
    assert!(
        log.status.success(),
        "seeded repo must have a commit: {}",
        String::from_utf8_lossy(&log.stderr)
    );
    let log_s = String::from_utf8_lossy(&log.stdout);
    assert!(
        log_s.contains("Initial commit"),
        "expected Initial commit, got {log_s}"
    );
    let ls = std::process::Command::new("git")
        .args([
            "-C",
            bare.to_str().unwrap(),
            "ls-tree",
            "-r",
            "--name-only",
            "HEAD",
        ])
        .output()
        .expect("ls-tree");
    assert!(ls.status.success());
    let tree = String::from_utf8_lossy(&ls.stdout);
    assert!(tree.contains("README.md"), "tree: {tree}");
    assert!(tree.contains("LICENSE"), "tree: {tree}");
    assert!(tree.contains(".gitignore"), "tree: {tree}");
    assert!(tree.contains("Cargo.toml"), "tree: {tree}");
}

/// All-none templates leave bare empty (no commits) — Quick setup path.
#[tokio::test]
async fn repo_create_all_none_templates_leaves_empty_bare() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_create_empty_tpl.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "emptytpl@ex.com", "emptytpl").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let create = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"bare-empty","visibility":"public"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let bare = repos.join("emptytpl").join("bare-empty.git");
    let log = std::process::Command::new("git")
        .args(["-C", bare.to_str().unwrap(), "rev-list", "--count", "--all"])
        .output()
        .expect("rev-list");
    // Empty unborn branch: rev-list may fail or return 0
    let count = String::from_utf8_lossy(&log.stdout).trim().to_string();
    assert!(
        !log.status.success() || count == "0" || count.is_empty(),
        "empty bare must have no commits (status={:?} count={count})",
        log.status.code()
    );
}

/// WR-01: when init_bare fails after insert, soft-delete the row so the name is reusable.
#[tokio::test]
async fn repo_create_git_failure_soft_deletes_row_allows_recreate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_create_git_fail.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "gitfail@ex.com", "gitfail").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    // Block init_bare: destination must be a non-directory file (WR-01 preferred seam).
    let owner_dir = repos.join("gitfail");
    std::fs::create_dir_all(&owner_dir).expect("owner dir");
    let blocked = owner_dir.join("retry-me.git");
    std::fs::write(&blocked, b"not-a-git-dir").expect("block bare path");

    let fail = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"retry-me","visibility":"public"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let fail_bytes = fail.into_body().collect().await.unwrap().to_bytes();
    let fail_v: serde_json::Value = serde_json::from_slice(&fail_bytes).unwrap();
    assert_eq!(
        fail_v["ok"], false,
        "create must fail when init_bare cannot proceed — {fail_v}"
    );
    let code = fail_v["error"]["code"].as_str().unwrap_or("");
    assert!(
        code == "repo.git_init_failed" || code == "repo.git_seed_failed",
        "expected git_*_failed, got {code} — {fail_v}"
    );

    // Compensating soft-delete: no live row should occupy the name.
    let live = db
        .find_repository_by_owner_name(&user_id, "retry-me")
        .await
        .expect("find");
    assert!(
        live.is_none(),
        "WR-01: failed create must not leave a live name-blocking row — found {live:?}"
    );

    // Clear blocker if still present so a retry can succeed on disk
    // (compensate may already have removed the path).
    let _ = std::fs::remove_file(&blocked);
    if blocked.exists() {
        let _ = std::fs::remove_dir_all(&blocked);
    }

    let retry = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"retry-me","visibility":"public"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        retry.status(),
        StatusCode::OK,
        "recreate same name after compensated failure"
    );
    let retry_bytes = retry.into_body().collect().await.unwrap().to_bytes();
    let retry_v: serde_json::Value = serde_json::from_slice(&retry_bytes).unwrap();
    assert_eq!(
        retry_v["ok"], true,
        "recreate must succeed after soft-delete compensate — {retry_v}"
    );
    assert_eq!(retry_v["data"]["name"], "retry-me");
}

/// Org Owner/Admin can create under org slug (A5 / D-ORG-01).
#[tokio::test]
async fn repo_create_under_org_as_owner() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("repo_create_org.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "orgowner@ex.com", "orgowner1").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");

    let org = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"acme-create"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(org.status(), StatusCode::OK);
    let org_bytes = org.into_body().collect().await.unwrap().to_bytes();
    let org_v: serde_json::Value = serde_json::from_slice(&org_bytes).unwrap();
    let org_id = org_v["data"]["id"].as_str().expect("org id").to_string();

    let create = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"widgets","visibility":"public","owner":"acme-create"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(
        create.status(),
        StatusCode::OK,
        "Owner must create under org"
    );
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["name"], "widgets");
    assert_eq!(v["data"]["owner_username"], "acme-create");
    assert_eq!(v["data"]["owner_id"], org_id);
    assert_eq!(v["data"]["owner_type"], "org");

    let bare = repos.join("acme-create").join("widgets.git");
    assert!(
        bare.is_dir(),
        "disk path must use org slug segment (D-ORG-01): {}",
        bare.display()
    );
}

/// Org Member cannot create repos under the org (A5 / T-10-06).
#[tokio::test]
async fn repo_create_under_org_as_member_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_create_member.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "boss@ex.com", "boss2").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify");

    let org = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"org.create","input":{"slug":"member-org"}}"#,
            &owner_cookie,
        ))
        .await
        .unwrap();
    let org_bytes = org.into_body().collect().await.unwrap().to_bytes();
    let org_v: serde_json::Value = serde_json::from_slice(&org_bytes).unwrap();
    let org_id = org_v["data"]["id"].as_str().expect("org id");

    let (member_cookie, member_v) = signup_and_login(&app, "peon@ex.com", "peon1").await;
    let member_id = member_v["data"]["id"].as_str().expect("id");
    db.set_email_verified_at(member_id, &now)
        .await
        .expect("verify");
    db.insert_org_member(org_id, member_id, "member")
        .await
        .expect("add member");

    let create = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"public","owner":"member-org"}}"#,
            &member_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::FORBIDDEN);
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(
        v["error"]["code"], "repo.create_forbidden",
        "Members must not create org repos — {v}"
    );
}

/// Cannot create under another user's username (T-10-06).
#[tokio::test]
async fn repo_create_under_other_user_denied() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_create_other.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (_a_cookie, _) = signup_and_login(&app, "alice@ex.com", "alice1").await;
    let (b_cookie, b_v) = signup_and_login(&app, "bob@ex.com", "bob1").await;
    let bob_id = b_v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(bob_id, &now)
        .await
        .expect("verify");

    let create = app
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"stolen","visibility":"public","owner":"alice1"}}"#,
            &b_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::FORBIDDEN);
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "repo.create_forbidden", "{v}");
}
