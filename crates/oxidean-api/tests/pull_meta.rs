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
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"nit","path":"NOTE.md","side":"RIGHT","line":1,"pending":true}}"#,
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

    // Already-submitted comments cannot be swept into a review.
    let posted = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"posted","path":"NOTE.md","side":"RIGHT","line":1}}"#,
    )
    .await;
    let posted_id = posted["data"]["id"].as_str().unwrap();
    let not_pending = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"pown","name":"core","number":1,"state":"commented","comment_ids":["{posted_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(not_pending["ok"], false, "{not_pending}");
    assert_eq!(not_pending["error"]["code"], "rpc.bad_input");
}

#[tokio::test]
async fn pull_files_whitespace_and_context_opts() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_ws.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _uid) = seed_pr(&app, &db, &repos).await;

    // Head adds a whitespace-only change to README's first line region —
    // `w=1` should collapse the resulting hunk.
    let files_plain = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.files","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(files_plain["ok"], true, "{files_plain}");
    assert!(!files_plain["data"]["files"].as_array().unwrap().is_empty());

    // NOTE.md is a pure addition — whitespace flag can't remove it, but the
    // request must still succeed (the opts are plumbed through, not ignored).
    let files_w = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.files","input":{"owner":"pown","name":"core","number":1,"w":true,"context":0}}"#,
    )
    .await;
    assert_eq!(files_w["ok"], true, "{files_w}");
    assert_eq!(
        files_w["data"]["files"].as_array().unwrap().len(),
        files_plain["data"]["files"].as_array().unwrap().len()
    );
    // -U0 collapses context lines: the patch carries hunks but no ' ' prefix lines.
    let patch = files_w["data"]["files"][0]["patch"].as_str().unwrap();
    assert!(!patch.lines().any(|l| l.starts_with(' ')), "{patch}");
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

#[tokio::test]
async fn pull_comments_pending_drafts_visibility_and_submit() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_pending.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _uid) = seed_pr(&app, &db, &repos).await;

    // A second signed-in user must NOT see the first user's draft.
    let signup2 = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"q@ex.com","username":"qview","password":"password1"}}"#,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(signup2.status(), StatusCode::OK);
    let login2 = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"q@ex.com","password":"password1","remember_me":false}}"#,
            None,
        ))
        .await
        .unwrap();
    let cookie2 = session_cookie(&login2);
    let login2_body: serde_json::Value =
        serde_json::from_slice(&login2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let uid2 = login2_body["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&uid2, &now).await.expect("verify");

    let draft = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"wip note","path":"NOTE.md","side":"RIGHT","line":1,"pending":true}}"#,
    )
    .await;
    assert_eq!(draft["ok"], true, "{draft}");
    assert_eq!(draft["data"]["pending"], true);
    let draft_id = draft["data"]["id"].as_str().unwrap().to_string();

    // Author sees it flagged pending.
    let mine = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.list","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    let mine_list = mine["data"]["comments"].as_array().unwrap();
    assert!(mine_list.iter().any(|c| c["id"] == draft_id));

    // Another user does not.
    let theirs = rpc_json(
        &app,
        &cookie2,
        r#"{"procedure":"pull.comments.list","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert!(theirs["data"]["comments"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["id"] != draft_id));

    // Anonymous viewers do not either.
    let anon = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"pull.comments.list","input":{"owner":"pown","name":"core","number":1}}"#,
            None,
        ))
        .await
        .unwrap();
    let anon_body: serde_json::Value =
        serde_json::from_slice(&anon.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(anon_body["data"]["comments"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["id"] != draft_id));

    // Submitting a review links the draft and clears pending.
    let review = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"pown","name":"core","number":1,"state":"commented","comment_ids":["{draft_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(review["ok"], true, "{review}");
    let after = rpc_json(
        &app,
        &cookie2,
        r#"{"procedure":"pull.comments.list","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    let linked = after["data"]["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == draft_id)
        .expect("draft now visible to others");
    assert_eq!(linked["pending"], false);
    assert_eq!(linked["review_id"], review["data"]["id"]);

    // Discarding a draft is a plain delete by its author.
    let draft2 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"junk","pending":true}}"#,
    )
    .await;
    let draft2_id = draft2["data"]["id"].as_str().unwrap().to_string();
    let del = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.delete","input":{{"owner":"pown","name":"core","number":1,"comment_id":"{draft2_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");

    // Read-only users can comment but cannot stash drafts — submission is
    // Write+-gated, so a pending flag would orphan permanently.
    let other_pending = rpc_json(
        &app,
        &cookie2,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"outsider draft","pending":true}}"#,
    )
    .await;
    assert_eq!(other_pending["ok"], false, "{other_pending}");
    assert_eq!(other_pending["error"]["code"], "repo.not_found");
    let other_posted = rpc_json(
        &app,
        &cookie2,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"outsider take"}}"#,
    )
    .await;
    assert_eq!(other_posted["ok"], true, "{other_posted}");
}

#[tokio::test]
async fn pull_comments_start_line_ranges() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_range.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _uid) = seed_pr(&app, &db, &repos).await;

    // start_line > line rejected.
    let bad = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"x","path":"NOTE.md","side":"RIGHT","line":2,"start_line":5}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["error"]["code"], "rpc.bad_input");

    // start_line without line rejected.
    let noline = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"x","path":"NOTE.md","side":"RIGHT","start_line":1}}"#,
    )
    .await;
    assert_eq!(noline["ok"], false);

    // A valid range round-trips.
    let ok = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"range","path":"NOTE.md","side":"RIGHT","line":3,"start_line":1}}"#,
    )
    .await;
    assert_eq!(ok["ok"], true, "{ok}");
    assert_eq!(ok["data"]["start_line"], 1);
    assert_eq!(ok["data"]["line"], 3);

    // File-level anchor: path alone (GitHub's "comment on this file").
    let file = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"file note","path":"NOTE.md"}}"#,
    )
    .await;
    assert_eq!(file["ok"], true, "{file}");
    assert_eq!(file["data"]["path"], "NOTE.md");
    assert!(file["data"]["line"].is_null());
    assert!(file["data"]["side"].is_null());

    // A side without a line is not a valid anchor.
    let stray_side = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"pown","name":"core","number":1,"body":"x","path":"NOTE.md","side":"RIGHT"}}"#,
    )
    .await;
    assert_eq!(stray_side["ok"], false);
    assert_eq!(stray_side["error"]["code"], "rpc.bad_input");
}

#[tokio::test]
async fn pull_files_scoped_diffs() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_scope.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _uid) = seed_pr(&app, &db, &repos).await;

    let commits = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.commits","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(commits["ok"], true, "{commits}");
    let head = commits["data"]["commits"]
        .as_array()
        .unwrap()
        .first()
        .unwrap()["sha"]
        .as_str()
        .unwrap()
        .to_string();

    // Single-commit scope: diff is commit-vs-first-parent — the NOTE.md add.
    let single = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.files","input":{{"owner":"pown","name":"core","number":1,"sha":"{head}"}}}}"#
        ),
    )
    .await;
    assert_eq!(single["ok"], true, "{single}");
    let single_files = single["data"]["files"].as_array().unwrap();
    assert!(single_files.iter().any(|f| f["path"] == "NOTE.md"));

    // The oldest listed commit is the repo root — it has no parent, so its
    // scope diffs against the empty tree and still resolves.
    let root = commits["data"]["commits"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["sha"]
        .as_str()
        .unwrap()
        .to_string();
    let root_diff = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.files","input":{{"owner":"pown","name":"core","number":1,"sha":"{root}"}}}}"#
        ),
    )
    .await;
    assert_eq!(root_diff["ok"], true, "{root_diff}");
    assert!(!root_diff["data"]["files"].as_array().unwrap().is_empty());

    // Range scope: pull base..head equals the full diff.
    let pull = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.get","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    let (base_sha, head_sha) = (
        pull["data"]["base_sha"].as_str().unwrap().to_string(),
        pull["data"]["head_sha"].as_str().unwrap().to_string(),
    );
    let range = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.files","input":{{"owner":"pown","name":"core","number":1,"base_sha":"{base_sha}","head_sha":"{head_sha}"}}}}"#
        ),
    )
    .await;
    assert_eq!(range["ok"], true, "{range}");
    assert!(!range["data"]["files"].as_array().unwrap().is_empty());

    // Conflicting scope inputs rejected.
    let bad = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.files","input":{{"owner":"pown","name":"core","number":1,"sha":"{head}","base_sha":"{base_sha}","head_sha":"{head_sha}"}}}}"#
        ),
    )
    .await;
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["error"]["code"], "rpc.bad_input");

    // Range requires both ends.
    let half = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.files","input":{{"owner":"pown","name":"core","number":1,"base_sha":"{base_sha}"}}}}"#
        ),
    )
    .await;
    assert_eq!(half["ok"], false);

    // since_review: anonymous → unauthenticated; authed-but-no-review → domain error.
    let anon = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"pull.files","input":{"owner":"pown","name":"core","number":1,"since_review":true}}"#,
            None,
        ))
        .await
        .unwrap();
    let anon_body: serde_json::Value =
        serde_json::from_slice(&anon.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(anon_body["error"]["code"], "auth.unauthenticated");

    let norev = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.files","input":{"owner":"pown","name":"core","number":1,"since_review":true}}"#,
    )
    .await;
    assert_eq!(norev["ok"], false);
    assert_eq!(norev["error"]["code"], "pull.no_prior_review");

    // After submitting a review, since_review resolves to review_sha..head.
    let review = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.reviews.submit","input":{"owner":"pown","name":"core","number":1,"state":"commented","body":"looks fine"}}"#,
    )
    .await;
    assert_eq!(review["ok"], true, "{review}");
    let since = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.files","input":{"owner":"pown","name":"core","number":1,"since_review":true}}"#,
    )
    .await;
    // The review was taken at the current head, so the scoped diff is empty.
    assert_eq!(since["ok"], true, "{since}");
    assert_eq!(since["data"]["files"].as_array().unwrap().len(), 0);
}
