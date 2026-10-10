//! GIT-24 `pull.branchStatus` + `pull.updateBranch`: GitHub "Update branch"
//! parity — merge the base branch into the PR head branch (same-repo and
//! fork-head PRs), with ACL + head-branch protection enforcement.

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

fn bare_tip(bare: &std::path::Path, rev: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["-C", bare.to_str().unwrap(), "rev-parse", rev])
        .output()
        .unwrap();
    assert!(out.status.success(), "rev-parse {rev}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Signed-up + verified user who owns `repo` with a `feature` branch holding
/// one commit, and an open PR feature→main. Returns `(cookie, pr_number)`.
async fn same_repo_pr(
    app: &axum::Router,
    db: &Database,
    repos: &std::path::Path,
    email: &str,
    user: &str,
    repo: &str,
) -> (String, i64) {
    let (cookie, v) = signup_and_login(app, email, user).await;
    verify_user(db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(app, &cookie, repo, "public").await;
    let br = rpc_json(
        app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.branchCreate","input":{{"owner":"{user}","name":"{repo}","branch":"feature","start":"main"}}}}"#
        ),
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    commit_on_branch(
        &repos.join(user).join(format!("{repo}.git")),
        "feature",
        "feature work",
        &[("FEAT.md", "f\n")],
    )
    .await;
    let pr = rpc_json(
        app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.create","input":{{"owner":"{user}","name":"{repo}","title":"feat","base_ref":"main","head_ref":"feature"}}}}"#
        ),
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    (cookie, pr["data"]["number"].as_i64().unwrap())
}

#[tokio::test]
async fn pull_update_branch_merges_base_into_head_same_repo() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ub_same.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, n) = same_repo_pr(&app, &db, &repos, "o@ex.com", "oown", "core").await;

    // Base advances → head is behind.
    let bare = repos.join("oown").join("core.git");
    commit_on_branch(&bare, "main", "base work", &[("BASE.md", "b\n")]).await;
    let base_tip = bare_tip(&bare, "main");

    let status = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.branchStatus","input":{{"owner":"oown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["data"]["status"], "behind");
    assert_eq!(status["data"]["behind_count"], 1);
    assert_eq!(status["data"]["ahead_count"], 1);
    assert_eq!(status["data"]["base_sha"], base_tip);
    assert_eq!(status["data"]["can_update"], true);

    let upd = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.updateBranch","input":{{"owner":"oown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["status"], "updated");
    let merge_sha = upd["data"]["merge_commit_sha"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(upd["data"]["pull"]["head_sha"], merge_sha);
    assert_eq!(bare_tip(&bare, "feature"), merge_sha);

    // Now up to date.
    let status = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.branchStatus","input":{{"owner":"oown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(status["data"]["status"], "up_to_date");
    assert_eq!(status["data"]["behind_count"], 0);
}

#[tokio::test]
async fn pull_update_branch_noop_when_up_to_date() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ub_utd.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, n) = same_repo_pr(&app, &db, &repos, "o@ex.com", "oown", "core").await;

    let status = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.branchStatus","input":{{"owner":"oown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(status["data"]["status"], "up_to_date", "{status}");

    let upd = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.updateBranch","input":{{"owner":"oown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["status"], "up_to_date");
    assert!(upd["data"]["merge_commit_sha"].is_null());
}

#[tokio::test]
async fn pull_update_branch_into_fork_head() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ub_fork.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    // srcown creates upstream + feature branch.
    let (src_c, src_v) = signup_and_login(&app, "src@ex.com", "srcown").await;
    verify_user(&db, src_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &src_c, "upstream", "public").await;
    let br = rpc_json(
        &app,
        &src_c,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"srcown","name":"upstream","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");

    // fkown forks, commits on the fork's feature branch, opens the PR.
    let (fk_c, fk_v) = signup_and_login(&app, "fk@ex.com", "fkown").await;
    verify_user(&db, fk_v["data"]["id"].as_str().unwrap()).await;
    let forked = rpc_json(
        &app,
        &fk_c,
        r#"{"procedure":"repo.fork","input":{"owner":"srcown","name":"upstream"}}"#,
    )
    .await;
    assert_eq!(forked["ok"], true, "{forked}");

    let fork_bare = repos.join("fkown").join("upstream.git");
    commit_on_branch(
        &fork_bare,
        "feature",
        "fork feature work",
        &[("FEAT.md", "from fork\n")],
    )
    .await;

    // Fork-head PRs need Write on the base repo (same gate as pull_lifecycle).
    let collab = rpc_json(
        &app,
        &src_c,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"srcown","name":"upstream","username":"fkown","permission":"write"}}"#,
    )
    .await;
    assert_eq!(collab["ok"], true, "{collab}");

    let pr = rpc_json(
        &app,
        &fk_c,
        r#"{"procedure":"pull.create","input":{"owner":"srcown","name":"upstream","title":"From fork","base_ref":"main","head_ref":"feature","head_owner":"fkown","head_name":"upstream"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();

    // Upstream main advances → fork head is behind.
    let src_bare = repos.join("srcown").join("upstream.git");
    commit_on_branch(&src_bare, "main", "base moved", &[("BASE.md", "b\n")]).await;

    let status = rpc_json(
        &app,
        &fk_c,
        &format!(
            r#"{{"procedure":"pull.branchStatus","input":{{"owner":"srcown","name":"upstream","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(status["data"]["status"], "behind", "{status}");
    assert_eq!(status["data"]["can_update"], true);

    // The fork owner (head-repo Write) updates the head branch.
    let upd = rpc_json(
        &app,
        &fk_c,
        &format!(
            r#"{{"procedure":"pull.updateBranch","input":{{"owner":"srcown","name":"upstream","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["status"], "updated");
    let merge_sha = upd["data"]["merge_commit_sha"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(upd["data"]["pull"]["head_sha"], merge_sha);
    // The merge commit lives on the FORK's feature branch.
    assert_eq!(bare_tip(&fork_bare, "feature"), merge_sha);
    assert_eq!(upd["data"]["pull"]["head_owner"], "fkown");
}

#[tokio::test]
async fn pull_update_branch_denied_for_read_only_actor() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ub_acl.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (_cookie, n) = same_repo_pr(&app, &db, &repos, "o@ex.com", "oown", "core").await;
    commit_on_branch(
        &repos.join("oown").join("core.git"),
        "main",
        "base moved",
        &[("BASE.md", "b\n")],
    )
    .await;

    let (other_c, other_v) = signup_and_login(&app, "r@ex.com", "rando").await;
    verify_user(&db, other_v["data"]["id"].as_str().unwrap()).await;

    // Read-only outsider: can read status (can_update=false), cannot update.
    let status = rpc_json(
        &app,
        &other_c,
        &format!(
            r#"{{"procedure":"pull.branchStatus","input":{{"owner":"oown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["data"]["can_update"], false);

    let upd = rpc_json(
        &app,
        &other_c,
        &format!(
            r#"{{"procedure":"pull.updateBranch","input":{{"owner":"oown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], false, "{upd}");
    assert_eq!(upd["error"]["code"], "repo.not_found");
}

#[tokio::test]
async fn pull_update_branch_denied_by_head_protection() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ub_bp.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, n) = same_repo_pr(&app, &db, &repos, "o@ex.com", "oown", "core").await;
    commit_on_branch(
        &repos.join("oown").join("core.git"),
        "main",
        "base moved",
        &[("BASE.md", "b\n")],
    )
    .await;

    // Protect the head branch — required reviews block Write-level updates.
    let rule = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"oown","name":"core","pattern":"feature","require_reviews":true,"required_approving_review_count":1}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "{rule}");

    // A Write collaborator (not the repo owner/Admin) hits the protection gate.
    let (collab_c, collab_v) = signup_and_login(&app, "c@ex.com", "collab").await;
    verify_user(&db, collab_v["data"]["id"].as_str().unwrap()).await;
    let add = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"oown","name":"core","username":"collab","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");

    let upd = rpc_json(
        &app,
        &collab_c,
        &format!(
            r#"{{"procedure":"pull.updateBranch","input":{{"owner":"oown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], false, "{upd}");
    assert_eq!(upd["error"]["code"], "repo.branch_protection");
}

#[tokio::test]
async fn pull_update_branch_conflict_reports_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ub_conf.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "o@ex.com", "oown").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "core", "public").await;
    let bare = repos.join("oown").join("core.git");

    // Seed a file on main before branching so both sides can conflict on it.
    commit_on_branch(&bare, "main", "seed", &[("SAME.md", "base\n")]).await;
    let br = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"oown","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    commit_on_branch(&bare, "feature", "feature", &[("SAME.md", "feature\n")]).await;

    let pr = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"oown","name":"core","title":"feat","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    let n = pr["data"]["number"].as_i64().unwrap();

    commit_on_branch(&bare, "main", "conflict", &[("SAME.md", "main\n")]).await;

    let upd = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.updateBranch","input":{{"owner":"oown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], false, "{upd}");
    assert_eq!(upd["error"]["code"], "pull.update_conflict");
}
