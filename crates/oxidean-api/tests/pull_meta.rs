//! WI-0001: pull.labels.set / pull.assignees.set / pull.files.setViewed /
//! pull.files.listViewed over the RPC shell.

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

fn session_cookie(res: &axum::http::Response<Body>) -> String {
    res.headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req(body, Some(cookie)))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn commit_on_branch(bare: &std::path::Path, branch: &str, files: &[(&str, &str)]) {
    let wt = tempfile::tempdir().unwrap();
    let bare_s = bare.to_str().unwrap();
    let wt_s = wt.path().to_str().unwrap();
    let status = std::process::Command::new("git")
        .args(["clone", "--branch", branch, bare_s, wt_s])
        .status()
        .unwrap();
    assert!(status.success(), "clone");
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
        vec!["-C", wt_s, "commit", "-m", "c"],
        vec!["-C", wt_s, "push", "origin", "HEAD"],
    ] {
        let status = std::process::Command::new("git")
            .args(&args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }
}

/// Repo + branch + PR #1 owned by `pown`; returns (cookie, user_id).
async fn seed_pr(app: &axum::Router, db: &Database, repos: &std::path::Path) -> (String, String) {
    let signup = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"p@ex.com","username":"pown","password":"password1"}}"#,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;
    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"p@ex.com","password":"password1","remember_me":false}}"#,
            None,
        ))
        .await
        .unwrap();
    let cookie = session_cookie(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let create = rpc_json(
        app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        app,
        &cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"pown","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    commit_on_branch(
        &repos.join("pown").join("core.git"),
        "feature",
        &[("NOTE.md", "hello\n")],
    )
    .await;
    let pr = rpc_json(
        app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"pown","name":"core","title":"T","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    (cookie, user_id)
}

#[tokio::test]
async fn pull_labels_set_round_trip_and_validation() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_labels.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _uid) = seed_pr(&app, &db, &repos).await;

    let label = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.create","input":{"scope":"repo","owner":"pown","repo":"core","name":"bug","color":"ff0000"}}"#,
    )
    .await;
    assert_eq!(label["ok"], true, "{label}");
    let label_id = label["data"]["id"].as_str().unwrap();

    let set = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.labels.set","input":{{"owner":"pown","name":"core","number":1,"labelIds":["{label_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(set["ok"], true, "{set}");
    let labels = set["data"]["labels"].as_array().unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0]["name"], "bug");

    // Unknown label id rejected.
    let bad = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.labels.set","input":{"owner":"pown","name":"core","number":1,"labelIds":["missing"]}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["error"]["code"], "rpc.bad_input");

    // Clearing works.
    let clear = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.labels.set","input":{"owner":"pown","name":"core","number":1,"labelIds":[]}}"#,
    )
    .await;
    assert_eq!(clear["ok"], true, "{clear}");
    assert_eq!(clear["data"]["labels"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn pull_assignees_set_round_trip_and_validation() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_assignees.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, user_id) = seed_pr(&app, &db, &repos).await;

    let set = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.assignees.set","input":{{"owner":"pown","name":"core","number":1,"userIds":["{user_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(set["ok"], true, "{set}");
    let assignees = set["data"]["assignees"].as_array().unwrap();
    assert_eq!(assignees.len(), 1);
    assert_eq!(assignees[0]["user_id"], user_id);

    let bad = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.assignees.set","input":{"owner":"pown","name":"core","number":1,"userIds":["missing"]}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["error"]["code"], "rpc.bad_input");
}

#[tokio::test]
async fn pull_file_views_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_views.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _uid) = seed_pr(&app, &db, &repos).await;

    let empty = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.files.listViewed","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(empty["ok"], true, "{empty}");
    assert_eq!(empty["data"]["viewed_paths"].as_array().unwrap().len(), 0);

    let set = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.files.setViewed","input":{"owner":"pown","name":"core","number":1,"path":"NOTE.md","viewed":true}}"#,
    )
    .await;
    assert_eq!(set["ok"], true, "{set}");
    assert_eq!(set["data"]["viewed_paths"], serde_json::json!(["NOTE.md"]));

    let list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.files.listViewed","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(list["data"]["viewed_paths"], serde_json::json!(["NOTE.md"]));

    let unmark = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.files.setViewed","input":{"owner":"pown","name":"core","number":1,"path":"NOTE.md","viewed":false}}"#,
    )
    .await;
    assert_eq!(unmark["ok"], true, "{unmark}");
    assert_eq!(unmark["data"]["viewed_paths"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn pull_review_submit_links_pending_comments() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("pull_review_link.db").display()
    );
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _uid) = seed_pr(&app, &db, &repos).await;

    let comment = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"nit","path":"NOTE.md","side":"RIGHT","line":1}}"#,
    )
    .await;
    assert_eq!(comment["ok"], true, "{comment}");
    let comment_id = comment["data"]["id"].as_str().unwrap();

    // Author cannot approve their own PR, so submit a "commented" review.
    let review = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"pown","name":"core","number":1,"state":"commented","comment_ids":["{comment_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(review["ok"], true, "{review}");
    let review_id = review["data"]["id"].as_str().unwrap();

    let list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.list","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    let comments = list["data"]["comments"].as_array().unwrap();
    assert_eq!(comments[0]["review_id"], review_id);

    // A comment owned by nobody on this pull is rejected.
    let bad = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.reviews.submit","input":{"owner":"pown","name":"core","number":1,"state":"commented","comment_ids":["missing"]}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["error"]["code"], "rpc.bad_input");
}

#[tokio::test]
async fn pull_file_views_require_session() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_views_auth.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let res = app
        .oneshot(rpc_req(
            r#"{"procedure":"pull.files.listViewed","input":{"owner":"pown","name":"core","number":1}}"#,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
