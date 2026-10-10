//! GIT-24 `repo.forkStatus` + `repo.syncFork`: divergence reporting and
//! bring-up-to-date (fast-forward / merge) for forks.

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

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":"src","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
    );
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Clone the bare repo, commit files on `branch`, push back (local-path push).
async fn commit_on_branch(
    bare: &std::path::Path,
    branch: &str,
    message: &str,
    files: &[(&str, &str)],
) {
    let wt = tempfile::tempdir().unwrap();
    let bare_s = bare.to_str().unwrap();
    let wt_s = wt.path().to_str().unwrap();
    let status = std::process::Command::new("git")
        .args(["clone", "--branch", branch, bare_s, wt_s])
        .status()
        .unwrap();
    assert!(status.success(), "clone {bare_s}#{branch}");
    for (path, content) in files {
        let dest = wt.path().join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&dest, content.as_bytes()).unwrap();
    }
    for args in [
        vec!["-C", wt_s, "config", "user.email", "t@ex.com"],
        vec!["-C", wt_s, "config", "user.name", "Test"],
        vec!["-C", wt_s, "add", "-A"],
        vec!["-C", wt_s, "commit", "-m", message],
        vec!["-C", wt_s, "push", "origin", "HEAD"],
    ] {
        let status = std::process::Command::new("git")
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }
}

/// `git rev-parse` a ref inside a bare repo.
fn bare_tip(bare: &std::path::Path, rev: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["-C", bare.to_str().unwrap(), "rev-parse", rev])
        .output()
        .unwrap();
    assert!(out.status.success(), "rev-parse {rev}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// `git rev-list --parents -n 1` → the commit + its parent list.
fn bare_parents(bare: &std::path::Path, rev: &str) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args([
            "-C",
            bare.to_str().unwrap(),
            "rev-list",
            "--parents",
            "-n",
            "1",
            rev,
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "rev-list {rev}");
    String::from_utf8(out.stdout)
        .unwrap()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Owner creates `repo`, `forker` forks it. Returns the two cookies.
async fn fork_fixture(
    app: &axum::Router,
    db: &Database,
    owner: (&str, &str),
    forker: (&str, &str),
    repo: &str,
) -> (String, String) {
    let (owner_c, owner_v) = signup_and_login(app, owner.0, owner.1).await;
    verify_user(db, owner_v["data"]["id"].as_str().unwrap()).await;
    create_repo(app, &owner_c, repo, "public").await;

    let (fork_c, fork_v) = signup_and_login(app, forker.0, forker.1).await;
    verify_user(db, fork_v["data"]["id"].as_str().unwrap()).await;
    let forked = rpc_json(
        app,
        &fork_c,
        &format!(
            r#"{{"procedure":"repo.fork","input":{{"owner":"{}","name":"{repo}"}}}}"#,
            owner.1
        ),
    )
    .await;
    assert_eq!(forked["ok"], true, "{forked}");
    (owner_c, fork_c)
}

#[tokio::test]
async fn fork_status_behind_then_sync_fast_forwards() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("sync_ff.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (_owner_c, fork_c) = fork_fixture(
        &app,
        &db,
        ("up@ex.com", "upown"),
        ("fk@ex.com", "fkown"),
        "core",
    )
    .await;

    // Upstream advances after the fork.
    let up_bare = repos.join("upown").join("core.git");
    commit_on_branch(&up_bare, "main", "upstream work", &[("NEW.md", "new\n")]).await;
    let upstream_tip = bare_tip(&up_bare, "main");
    let fork_bare = repos.join("fkown").join("core.git");
    let fork_tip_before = bare_tip(&fork_bare, "main");

    let status = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.forkStatus","input":{"owner":"fkown","name":"core"}}"#,
    )
    .await;
    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["data"]["status"], "behind");
    assert_eq!(status["data"]["behind_count"], 1);
    assert_eq!(status["data"]["ahead_count"], 0);
    assert_eq!(status["data"]["upstream_owner"], "upown");
    assert_eq!(status["data"]["upstream_branch"], "main");

    let sync = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.syncFork","input":{"owner":"fkown","name":"core"}}"#,
    )
    .await;
    assert_eq!(sync["ok"], true, "{sync}");
    assert_eq!(sync["data"]["status"], "fast_forwarded");
    assert_eq!(sync["data"]["before_sha"], fork_tip_before);
    assert_eq!(sync["data"]["after_sha"], upstream_tip);
    assert_eq!(bare_tip(&fork_bare, "main"), upstream_tip);
}

#[tokio::test]
async fn fork_status_up_to_date_and_sync_noop() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("sync_utd.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (_owner_c, fork_c) = fork_fixture(
        &app,
        &db,
        ("u@ex.com", "uown"),
        ("f@ex.com", "fown"),
        "core",
    )
    .await;

    let status = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.forkStatus","input":{"owner":"fown","name":"core"}}"#,
    )
    .await;
    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["data"]["status"], "up_to_date");
    assert_eq!(status["data"]["behind_count"], 0);

    let sync = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.syncFork","input":{"owner":"fown","name":"core"}}"#,
    )
    .await;
    assert_eq!(sync["ok"], true, "{sync}");
    assert_eq!(sync["data"]["status"], "up_to_date");
    assert!(sync["data"]["merge_commit_sha"].is_null());
}

#[tokio::test]
async fn fork_sync_diverged_creates_merge_commit() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("sync_div.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (_owner_c, fork_c) = fork_fixture(
        &app,
        &db,
        ("up@ex.com", "upown"),
        ("fk@ex.com", "fkown"),
        "core",
    )
    .await;

    let up_bare = repos.join("upown").join("core.git");
    let fork_bare = repos.join("fkown").join("core.git");
    commit_on_branch(&up_bare, "main", "upstream", &[("UP.md", "u\n")]).await;
    commit_on_branch(&fork_bare, "main", "fork work", &[("FORK.md", "f\n")]).await;

    let status = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.forkStatus","input":{"owner":"fkown","name":"core"}}"#,
    )
    .await;
    assert_eq!(status["data"]["status"], "diverged", "{status}");
    assert_eq!(status["data"]["ahead_count"], 1);
    assert_eq!(status["data"]["behind_count"], 1);

    let sync = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.syncFork","input":{"owner":"fkown","name":"core"}}"#,
    )
    .await;
    assert_eq!(sync["ok"], true, "{sync}");
    assert_eq!(sync["data"]["status"], "merged");
    let merge_sha = sync["data"]["merge_commit_sha"].as_str().unwrap();
    assert_eq!(bare_tip(&fork_bare, "main"), merge_sha);
    assert_eq!(
        bare_parents(&fork_bare, "main").len(),
        3,
        "merge has 2 parents"
    );
}

#[tokio::test]
async fn fork_sync_diverged_denied_when_merge_commits_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("sync_nodiv.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (_owner_c, fork_c) = fork_fixture(
        &app,
        &db,
        ("up@ex.com", "upown"),
        ("fk@ex.com", "fkown"),
        "core",
    )
    .await;

    commit_on_branch(
        &repos.join("upown").join("core.git"),
        "main",
        "upstream",
        &[("UP.md", "u\n")],
    )
    .await;
    commit_on_branch(
        &repos.join("fkown").join("core.git"),
        "main",
        "fork work",
        &[("FORK.md", "f\n")],
    )
    .await;

    let off = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.mergeSettings.update","input":{"owner":"fkown","name":"core","allow_merge_commit":false}}"#,
    )
    .await;
    assert_eq!(off["ok"], true, "{off}");

    let sync = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.syncFork","input":{"owner":"fkown","name":"core"}}"#,
    )
    .await;
    assert_eq!(sync["ok"], false, "{sync}");
    assert_eq!(sync["error"]["code"], "repo.sync_diverged");
}

#[tokio::test]
async fn fork_sync_diverged_conflict_reports_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("sync_conf.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (_owner_c, fork_c) = fork_fixture(
        &app,
        &db,
        ("up@ex.com", "upown"),
        ("fk@ex.com", "fkown"),
        "core",
    )
    .await;

    // Same path edited on both sides → merge conflict.
    commit_on_branch(
        &repos.join("upown").join("core.git"),
        "main",
        "upstream",
        &[("SAME.md", "upstream\n")],
    )
    .await;
    commit_on_branch(
        &repos.join("fkown").join("core.git"),
        "main",
        "fork",
        &[("SAME.md", "fork\n")],
    )
    .await;

    let sync = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.syncFork","input":{"owner":"fkown","name":"core"}}"#,
    )
    .await;
    assert_eq!(sync["ok"], false, "{sync}");
    assert_eq!(sync["error"]["code"], "repo.sync_conflict");
}

#[tokio::test]
async fn sync_fork_denied_for_non_writer() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("sync_acl.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (_owner_c, _fork_c) = fork_fixture(
        &app,
        &db,
        ("up@ex.com", "upown"),
        ("fk@ex.com", "fkown"),
        "core",
    )
    .await;

    let (other_c, other_v) = signup_and_login(&app, "rando@ex.com", "rando").await;
    verify_user(&db, other_v["data"]["id"].as_str().unwrap()).await;

    // Read-only outsider can see status on the public fork but cannot sync it.
    let status = rpc_json(
        &app,
        &other_c,
        r#"{"procedure":"repo.forkStatus","input":{"owner":"fkown","name":"core"}}"#,
    )
    .await;
    assert_eq!(status["ok"], true, "{status}");

    let sync = rpc_json(
        &app,
        &other_c,
        r#"{"procedure":"repo.syncFork","input":{"owner":"fkown","name":"core"}}"#,
    )
    .await;
    assert_eq!(sync["ok"], false, "{sync}");
    assert_eq!(sync["error"]["code"], "repo.not_found");
}

#[tokio::test]
async fn sync_fork_denied_by_branch_protection() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("sync_bp.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (_owner_c, fork_c) = fork_fixture(
        &app,
        &db,
        ("up@ex.com", "upown"),
        ("fk@ex.com", "fkown"),
        "core",
    )
    .await;

    commit_on_branch(
        &repos.join("upown").join("core.git"),
        "main",
        "upstream",
        &[("UP.md", "u\n")],
    )
    .await;

    // Protect the fork's default branch — a Write collaborator (non-admin)
    // cannot sync past a required-reviews rule.
    let rule = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"fkown","name":"core","pattern":"main","require_reviews":true,"required_approving_review_count":1}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "{rule}");

    let (collab_c, collab_v) = signup_and_login(&app, "collab@ex.com", "collab").await;
    verify_user(&db, collab_v["data"]["id"].as_str().unwrap()).await;
    let add = rpc_json(
        &app,
        &fork_c,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"fkown","name":"core","username":"collab","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");

    let sync = rpc_json(
        &app,
        &collab_c,
        r#"{"procedure":"repo.syncFork","input":{"owner":"fkown","name":"core"}}"#,
    )
    .await;
    assert_eq!(sync["ok"], false, "{sync}");
    assert_eq!(sync["error"]["code"], "repo.branch_protection");
}

#[tokio::test]
async fn fork_status_errors_for_non_fork() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("sync_nofork.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "solo@ex.com", "solo").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "plain", "public").await;

    for proc in ["repo.forkStatus", "repo.syncFork"] {
        let res = rpc_json(
            &app,
            &cookie,
            &format!(r#"{{"procedure":"{proc}","input":{{"owner":"solo","name":"plain"}}}}"#),
        )
        .await;
        assert_eq!(res["ok"], false, "{proc} — {res}");
        assert_eq!(res["error"]["code"], "repo.not_fork");
    }
}
