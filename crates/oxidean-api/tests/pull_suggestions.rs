//! COL-08 — `pull.suggestions.apply` / `pull.suggestions.apply_batch`:
//! ````suggestion` blocks in diff-anchored review comments apply as a signed
//! commit on the PR head branch (GitHub parity).

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

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn blob_content(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    name: &str,
    ref_name: &str,
    path: &str,
) -> serde_json::Value {
    let body = format!(
        r#"{{"procedure":"repo.blob","input":{{"owner":"{owner}","name":"{name}","ref":"{ref_name}","path":"{path}"}}}}"#
    );
    rpc_json(app, cookie, &body).await
}

/// Owner-owned repo with `docs/guide.md` (5 known lines) on `main`, a
/// `feature` branch at the same tip, and an open PR `feature` → `main`.
/// Returns the PR number.
async fn setup_pr(app: &axum::Router, cookie: &str, owner: &str, name: &str) -> i64 {
    let create = rpc_json(
        app,
        cookie,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
        ),
    )
    .await;
    assert_eq!(create["ok"], true, "repo.create — {create}");
    let file = rpc_json(
        app,
        cookie,
        &format!(
            r#"{{"procedure":"repo.file.create","input":{{"owner":"{owner}","name":"{name}","path":"docs/guide.md","content":"line one\nline two\nline three\nline four\nline five\n","message":"Add guide"}}}}"#
        ),
    )
    .await;
    assert_eq!(file["ok"], true, "file.create — {file}");
    let branch = rpc_json(
        app,
        cookie,
        &format!(
            r#"{{"procedure":"repo.branchCreate","input":{{"owner":"{owner}","name":"{name}","branch":"feature","start":"main"}}}}"#
        ),
    )
    .await;
    assert_eq!(branch["ok"], true, "branchCreate — {branch}");
    let pr = rpc_json(
        app,
        cookie,
        &format!(
            r#"{{"procedure":"pull.create","input":{{"owner":"{owner}","name":"{name}","title":"Change","base_ref":"main","head_ref":"feature"}}}}"#
        ),
    )
    .await;
    assert_eq!(pr["ok"], true, "pull.create — {pr}");
    pr["data"]["number"].as_i64().expect("pr number")
}

/// Plant a RIGHT-side line comment carrying one ````suggestion` fence;
/// returns the comment id.
#[expect(clippy::too_many_arguments)]
async fn plant_suggestion(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    name: &str,
    number: i64,
    line: i64,
    start_line: Option<i64>,
    suggestion: &str,
) -> String {
    plant_comment_body(
        app,
        cookie,
        owner,
        name,
        number,
        "docs/guide.md",
        "RIGHT",
        line,
        start_line,
        &format!("Please fix:\n```suggestion\n{suggestion}\n```\n"),
        false,
    )
    .await
}

#[expect(clippy::too_many_arguments)]
async fn plant_comment_body(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    name: &str,
    number: i64,
    path: &str,
    side: &str,
    line: i64,
    start_line: Option<i64>,
    body: &str,
    pending: bool,
) -> String {
    let body_json = serde_json::to_string(body).unwrap();
    let start = start_line
        .map(|l| format!(r#""start_line":{l},"#))
        .unwrap_or_default();
    let req = format!(
        r#"{{"procedure":"pull.comments.create","input":{{"owner":"{owner}","name":"{name}","number":{number},"body":{body_json},"path":"{path}","side":"{side}",{start}"line":{line},"pending":{pending}}}}}"#
    );
    let res = rpc_json(app, cookie, &req).await;
    assert_eq!(res["ok"], true, "comment plant — {res}");
    res["data"]["id"].as_str().expect("comment id").to_string()
}

#[tokio::test]
async fn apply_single_line_suggestion_commits_to_head() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("sugg1.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "own1@ex.com", "ownone").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let number = setup_pr(&app, &cookie, "ownone", "suggone").await;
    let head_before = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.get","input":{"owner":"ownone","name":"suggone","number":1}}"#,
    )
    .await;
    assert_eq!(number, 1);
    let head_sha_before = head_before["data"]["head_sha"]
        .as_str()
        .unwrap()
        .to_string();

    // Suggestion authored by a different (verified, Write) user so the
    // co-author trailer can be asserted.
    let (rev_cookie, rev_v) = signup_and_login(&app, "rev1@ex.com", "revone").await;
    verify_user(&db, rev_v["data"]["id"].as_str().unwrap()).await;
    let grant = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"ownone","name":"suggone","username":"revone","permission":"write"}}"#,
    )
    .await;
    assert_eq!(grant["ok"], true, "collab add — {grant}");
    let comment_id = plant_suggestion(
        &app,
        &rev_cookie,
        "ownone",
        "suggone",
        number,
        2,
        None,
        "line TWO",
    )
    .await;

    let apply = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownone","name":"suggone","number":{number},"comment_id":"{comment_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(apply["ok"], true, "apply — {apply}");
    let sha = apply["data"]["commit_sha"].as_str().expect("commit_sha");
    assert_eq!(apply["data"]["branch"], "feature");

    let blob = blob_content(
        &app,
        &cookie,
        "ownone",
        "suggone",
        "feature",
        "docs/guide.md",
    )
    .await;
    assert_eq!(
        blob["data"]["content"],
        "line one\nline TWO\nline three\nline four\nline five\n"
    );

    // head_sha advanced + commit message carries GitHub-style default and the
    // suggestion author's co-author trailer.
    let head_after = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.get","input":{"owner":"ownone","name":"suggone","number":1}}"#,
    )
    .await;
    let head_sha_after = head_after["data"]["head_sha"].as_str().unwrap();
    assert_ne!(head_sha_after, head_sha_before);
    assert_eq!(head_sha_after, sha);

    let commit = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.commit","input":{{"owner":"ownone","name":"suggone","sha":"{sha}"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        commit["data"]["subject"],
        "Apply suggestion from code review"
    );
    let body = commit["data"]["body"].as_str().unwrap_or("");
    assert!(
        body.contains("Co-authored-by: revone <"),
        "co-author trailer — {body}"
    );
}

#[tokio::test]
async fn apply_range_and_empty_suggestion() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("sugg2.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "own2@ex.com", "owntwo").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let number = setup_pr(&app, &cookie, "owntwo", "suggtwo").await;

    // Multi-line range: lines 2..4 replaced by two suggestion lines.
    let c1 = plant_suggestion(
        &app,
        &cookie,
        "owntwo",
        "suggtwo",
        number,
        4,
        Some(2),
        "new A\nnew B",
    )
    .await;
    let apply = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"owntwo","name":"suggtwo","number":{number},"comment_id":"{c1}"}}}}"#
        ),
    )
    .await;
    assert_eq!(apply["ok"], true, "range apply — {apply}");
    let blob = blob_content(
        &app,
        &cookie,
        "owntwo",
        "suggtwo",
        "feature",
        "docs/guide.md",
    )
    .await;
    assert_eq!(
        blob["data"]["content"],
        "line one\nnew A\nnew B\nline five\n"
    );

    // Empty suggestion deletes the range. Comment anchored on the *original*
    // line numbering is not what a fresh author sees — re-anchor at current
    // content: file is now 4 lines; delete lines 3..4 via a range comment.
    let c2 = plant_suggestion(&app, &cookie, "owntwo", "suggtwo", number, 4, Some(3), "").await;
    let apply2 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"owntwo","name":"suggtwo","number":{number},"comment_id":"{c2}"}}}}"#
        ),
    )
    .await;
    assert_eq!(apply2["ok"], true, "delete apply — {apply2}");
    let blob2 = blob_content(
        &app,
        &cookie,
        "owntwo",
        "suggtwo",
        "feature",
        "docs/guide.md",
    )
    .await;
    assert_eq!(blob2["data"]["content"], "line one\nnew A\n");
}

#[tokio::test]
async fn apply_rejects_invalid_targets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("sugg3.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "own3@ex.com", "ownthree").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let number = setup_pr(&app, &cookie, "ownthree", "suggthree").await;

    // LEFT-side anchor.
    let left = plant_comment_body(
        &app,
        &cookie,
        "ownthree",
        "suggthree",
        number,
        "docs/guide.md",
        "LEFT",
        2,
        None,
        "```suggestion\nx\n```\n",
        false,
    )
    .await;
    let r = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownthree","name":"suggthree","number":{number},"comment_id":"{left}"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], false, "LEFT — {r}");
    assert_eq!(r["error"]["code"], "pull.suggestion_invalid");

    // File-level comment (path only, no line) — plant via raw create.
    let file_level = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"ownthree","name":"suggthree","number":{number},"body":"```suggestion\nx\n```\n","path":"docs/guide.md"}}}}"#
        ),
    )
    .await;
    assert_eq!(file_level["ok"], true, "{file_level}");
    let fl_id = file_level["data"]["id"].as_str().unwrap();
    let r = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownthree","name":"suggthree","number":{number},"comment_id":"{fl_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], false, "file-level — {r}");
    assert_eq!(r["error"]["code"], "pull.suggestion_invalid");

    // Pending review draft.
    let pending = plant_comment_body(
        &app,
        &cookie,
        "ownthree",
        "suggthree",
        number,
        "docs/guide.md",
        "RIGHT",
        2,
        None,
        "```suggestion\nx\n```\n",
        true,
    )
    .await;
    let r = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownthree","name":"suggthree","number":{number},"comment_id":"{pending}"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], false, "pending — {r}");
    assert_eq!(r["error"]["code"], "pull.suggestion_invalid");

    // Comment with no suggestion fence at all.
    let plain = plant_comment_body(
        &app,
        &cookie,
        "ownthree",
        "suggthree",
        number,
        "docs/guide.md",
        "RIGHT",
        3,
        None,
        "just a note",
        false,
    )
    .await;
    let r = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownthree","name":"suggthree","number":{number},"comment_id":"{plain}"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], false, "no fence — {r}");
    assert_eq!(r["error"]["code"], "pull.suggestion_not_found");

    // suggestion_index out of range.
    let one_fence =
        plant_suggestion(&app, &cookie, "ownthree", "suggthree", number, 3, None, "y").await;
    let r = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownthree","name":"suggthree","number":{number},"comment_id":"{one_fence}","suggestion_index":4}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], false, "bad index — {r}");
    assert_eq!(r["error"]["code"], "pull.suggestion_not_found");

    // Line beyond EOF.
    let beyond = plant_suggestion(
        &app,
        &cookie,
        "ownthree",
        "suggthree",
        number,
        99,
        None,
        "z",
    )
    .await;
    let r = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownthree","name":"suggthree","number":{number},"comment_id":"{beyond}"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], false, "beyond EOF — {r}");
    assert_eq!(r["error"]["code"], "pull.suggestion_out_of_range");

    // Closed PR.
    let close = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.close","input":{{"owner":"ownthree","name":"suggthree","number":{number}}}}}"#
        ),
    )
    .await;
    assert_eq!(close["ok"], true);
    let r = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownthree","name":"suggthree","number":{number},"comment_id":"{one_fence}"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], false, "closed — {r}");
    assert_eq!(r["error"]["code"], "pull.invalid_state");
}

#[tokio::test]
async fn apply_denied_without_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("sugg4.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "own4@ex.com", "ownfour").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let number = setup_pr(&app, &cookie, "ownfour", "suggfour").await;
    let comment_id = plant_suggestion(
        &app, &cookie, "ownfour", "suggfour", number, 2, None, "line TWO",
    )
    .await;

    // Verified stranger: Read on the public base repo, no write anywhere.
    let (stranger, stranger_v) = signup_and_login(&app, "s4@ex.com", "strfour").await;
    verify_user(&db, stranger_v["data"]["id"].as_str().unwrap()).await;
    let r = rpc_json(
        &app,
        &stranger,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownfour","name":"suggfour","number":{number},"comment_id":"{comment_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], false, "stranger apply — {r}");
    assert_eq!(r["error"]["code"], "repo.not_found");
}

#[tokio::test]
async fn apply_batch_commits_once_across_ranges() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("sugg5.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "own5@ex.com", "ownfive").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let number = setup_pr(&app, &cookie, "ownfive", "suggfive").await;

    let c1 = plant_suggestion(&app, &cookie, "ownfive", "suggfive", number, 1, None, "ONE").await;
    let c2 = plant_suggestion(
        &app,
        &cookie,
        "ownfive",
        "suggfive",
        number,
        4,
        Some(3),
        "THREE",
    )
    .await;
    let c3 = plant_suggestion(
        &app, &cookie, "ownfive", "suggfive", number, 2, None, "line two",
    )
    .await;

    let apply = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply_batch","input":{{"owner":"ownfive","name":"suggfive","number":{number},"comment_ids":["{c1}","{c2}","{c3}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(apply["ok"], true, "batch — {apply}");
    assert_eq!(apply["data"]["applied"], 2);
    let sha = apply["data"]["commit_sha"].as_str().unwrap();

    let blob = blob_content(
        &app,
        &cookie,
        "ownfive",
        "suggfive",
        "feature",
        "docs/guide.md",
    )
    .await;
    assert_eq!(blob["data"]["content"], "ONE\nline two\nTHREE\nline five\n");

    // Exactly one commit subject with the plural default message.
    let commit = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.commit","input":{{"owner":"ownfive","name":"suggfive","sha":"{sha}"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        commit["data"]["subject"],
        "Apply suggestions from code review"
    );

    // Overlapping ranges reject the whole batch.
    let o1 = plant_suggestion(
        &app,
        &cookie,
        "ownfive",
        "suggfive",
        number,
        2,
        Some(1),
        "a",
    )
    .await;
    let o2 = plant_suggestion(
        &app,
        &cookie,
        "ownfive",
        "suggfive",
        number,
        3,
        Some(2),
        "b",
    )
    .await;
    let r = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply_batch","input":{{"owner":"ownfive","name":"suggfive","number":{number},"comment_ids":["{o1}","{o2}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(r["ok"], false, "overlap — {r}");
    assert_eq!(r["error"]["code"], "pull.suggestion_overlap");
}

#[tokio::test]
async fn apply_multi_fence_selects_index() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("sugg6.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "own6@ex.com", "ownsix").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let number = setup_pr(&app, &cookie, "ownsix", "suggsix").await;

    let cid = plant_comment_body(
        &app,
        &cookie,
        "ownsix",
        "suggsix",
        number,
        "docs/guide.md",
        "RIGHT",
        2,
        None,
        "option one:\n```suggestion\nfirst\n```\nor two:\n```suggestion\nsecond\n```\n",
        false,
    )
    .await;
    let apply = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"ownsix","name":"suggsix","number":{number},"comment_id":"{cid}","suggestion_index":1}}}}"#
        ),
    )
    .await;
    assert_eq!(apply["ok"], true, "index apply — {apply}");
    let blob = blob_content(
        &app,
        &cookie,
        "ownsix",
        "suggsix",
        "feature",
        "docs/guide.md",
    )
    .await;
    assert_eq!(
        blob["data"]["content"],
        "line one\nsecond\nline three\nline four\nline five\n"
    );
}

#[tokio::test]
async fn apply_to_fork_head_as_maintainer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("sugg7.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "up@ex.com", "upown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"up","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let file = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.file.create","input":{"owner":"upown","name":"up","path":"docs/guide.md","content":"one\ntwo\nthree\n","message":"guide"}}"#,
    )
    .await;
    assert_eq!(file["ok"], true, "{file}");

    let (fork_cookie, fork_v) = signup_and_login(&app, "fk@ex.com", "fkown").await;
    verify_user(&db, fork_v["data"]["id"].as_str().unwrap()).await;
    let forked = rpc_json(
        &app,
        &fork_cookie,
        r#"{"procedure":"repo.fork","input":{"owner":"upown","name":"up"}}"#,
    )
    .await;
    assert_eq!(forked["ok"], true, "{forked}");
    let upd = rpc_json(
        &app,
        &fork_cookie,
        r#"{"procedure":"repo.file.update","input":{"owner":"fkown","name":"up","path":"docs/guide.md","content":"one\nTWO-main\nthree\n","message":"edit","branch":"main"}}"#,
    )
    .await;
    assert_eq!(upd["ok"], true, "fork edit — {upd}");
    let pr = rpc_json(
        &app,
        &fork_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"upown","name":"up","title":"fork pr","base_ref":"main","head_ref":"main","head_owner":"fkown","head_name":"up"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "fork pr — {pr}");
    let number = pr["data"]["number"].as_i64().unwrap();

    // Reviewer on the upstream repo leaves a suggestion on the fork-head file.
    let cid = plant_suggestion(&app, &owner_cookie, "upown", "up", number, 2, None, "TWO").await;

    // The base-repo maintainer (Write on base, none on fork) applies it —
    // update-branch parity.
    let apply = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"upown","name":"up","number":{number},"comment_id":"{cid}"}}}}"#
        ),
    )
    .await;
    assert_eq!(apply["ok"], true, "fork apply — {apply}");
    assert_eq!(apply["data"]["branch"], "main");
    let pull_after = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.get","input":{{"owner":"upown","name":"up","number":{number}}}}}"#
        ),
    )
    .await;
    assert_eq!(pull_after["data"]["head_sha"], apply["data"]["commit_sha"]);
    let stale_apply = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.suggestions.apply","input":{{"owner":"upown","name":"up","number":{number},"comment_id":"{cid}"}}}}"#
        ),
    )
    .await;
    assert_eq!(stale_apply["ok"], false, "stale suggestion — {stale_apply}");
    assert_eq!(stale_apply["error"]["code"], "pull.suggestion_invalid");

    // The commit landed on the FORK's main, not upstream's.
    let fblob = blob_content(&app, &fork_cookie, "fkown", "up", "main", "docs/guide.md").await;
    assert_eq!(fblob["data"]["content"], "one\nTWO\nthree\n");
    let ublob = blob_content(&app, &owner_cookie, "upown", "up", "main", "docs/guide.md").await;
    assert_eq!(ublob["data"]["content"], "one\ntwo\nthree\n");
}
