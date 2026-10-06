//! PR-03 pull comments: general + line + resolve + outdated.

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

async fn signup_and_login(
    app: &axum::Router,
    email: &str,
    username: &str,
) -> (String, serde_json::Value) {
    let signup = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;
    let login = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (cookie, v)
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now).await.expect("verify");
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
        vec!["-C", wt_s, "commit", "-m", message],
        vec!["-C", wt_s, "push", "origin", "HEAD"],
    ] {
        let status = std::process::Command::new("git").args(&args).status().unwrap();
        assert!(status.success(), "git {args:?}");
    }
}

#[tokio::test]
async fn pull_comments_general_line_resolve_outdated() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_comments.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "c@ex.com", "cown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;

    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"cown","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");

    let bare = repos.join("cown").join("core.git");
    commit_on_branch(&bare, "feature", "note", &[("NOTE.md", "line1\n")]).await;

    let pr = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"cown","name":"core","title":"PR","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();

    let general = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"cown","name":"core","number":{n},"body":"LGTM overall"}}}}"#
        ),
    )
    .await;
    assert_eq!(general["ok"], true, "{general}");
    assert!(general["data"]["path"].is_null());

    let line = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"cown","name":"core","number":{n},"body":"nit","path":"NOTE.md","side":"RIGHT","line":1}}}}"#
        ),
    )
    .await;
    assert_eq!(line["ok"], true, "{line}");
    assert_eq!(line["data"]["path"], "NOTE.md");
    assert_eq!(line["data"]["outdated"], false);
    let line_id = line["data"]["id"].as_str().unwrap();

    let resolve = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.resolve","input":{{"owner":"cown","name":"core","number":{n},"comment_id":"{line_id}","resolved":true}}}}"#
        ),
    )
    .await;
    assert_eq!(resolve["ok"], true, "{resolve}");
    assert_eq!(resolve["data"]["resolved"], true);

    commit_on_branch(&bare, "feature", "rewrite", &[("NOTE.md", "changed\n")]).await;

    let upd = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.update","input":{{"owner":"cown","name":"core","number":{n},"title":"PR refreshed"}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");

    let list = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.list","input":{{"owner":"cown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");
    let comments = list["data"]["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2);
    let line_c = comments
        .iter()
        .find(|c| c["id"].as_str() == Some(line_id))
        .unwrap();
    assert_eq!(
        line_c["outdated"], true,
        "line comment outdated after head change"
    );
    let gen_c = comments.iter().find(|c| c["path"].is_null()).unwrap();
    assert_eq!(gen_c["outdated"], false, "general comments stay current");
}

/// Repo + branch + one-commit head + PR; returns pull number.
async fn setup_pull(
    app: &axum::Router,
    repos: &std::path::Path,
    cookie: &str,
    owner: &str,
    repo: &str,
) -> i64 {
    let create = rpc_json(
        app,
        cookie,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{repo}","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
        ),
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        app,
        cookie,
        &format!(
            r#"{{"procedure":"repo.branchCreate","input":{{"owner":"{owner}","name":"{repo}","branch":"feature","start":"main"}}}}"#
        ),
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    let bare = repos.join(owner).join(format!("{repo}.git"));
    commit_on_branch(&bare, "feature", "note", &[("NOTE.md", "line1\n")]).await;
    let pr = rpc_json(
        app,
        cookie,
        &format!(
            r#"{{"procedure":"pull.create","input":{{"owner":"{owner}","name":"{repo}","title":"PR","base_ref":"main","head_ref":"feature"}}}}"#
        ),
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    pr["data"]["number"].as_i64().unwrap()
}

/// Author edit stores prior bodies in `pull.comments.history` oldest-first
/// (#113 / D-ISS-09 / D-ISS-12 parity).
#[tokio::test]
async fn pull_comments_author_edit_history() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_edit.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, login_v) = signup_and_login(&app, "pe@ex.com", "peown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    let n = setup_pull(&app, &repos, &cookie, "peown", "core").await;

    let created = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"peown","name":"core","number":{n},"body":"v1","path":"NOTE.md","side":"RIGHT","line":1}}}}"#
        ),
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let comment_id = created["data"]["id"].as_str().unwrap();

    let upd = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.update","input":{{"owner":"peown","name":"core","number":{n},"commentId":"{comment_id}","body":"v2"}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["body"], "v2");

    let upd2 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.update","input":{{"owner":"peown","name":"core","number":{n},"commentId":"{comment_id}","body":"v3"}}}}"#
        ),
    )
    .await;
    assert_eq!(upd2["ok"], true, "{upd2}");

    let hist = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.comments.history","input":{{"owner":"peown","name":"core","number":{n},"commentId":"{comment_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(hist["ok"], true, "{hist}");
    let revs = hist["data"]["revisions"].as_array().unwrap();
    assert_eq!(revs.len(), 2, "{hist}");
    assert_eq!(revs[0]["body"], "v1", "oldest revision first");
    assert_eq!(revs[1]["body"], "v2");
    assert_eq!(revs[0]["editor_username"], "peown");
}

/// Edit is author-only; delete is author or Write+ moderation (#113 /
/// D-ISS-09 parity — denied calls soft-fail as `repo.not_found`).
#[tokio::test]
async fn pull_comments_update_delete_acl() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_acl.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "pa@ex.com", "paown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    let n = setup_pull(&app, &repos, &owner_cookie, "paown", "core").await;

    let (writer_cookie, writer_v) = signup_and_login(&app, "pw@ex.com", "pwrite").await;
    verify_user(&db, writer_v["data"]["id"].as_str().unwrap()).await;
    let add_w = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"paown","name":"core","username":"pwrite","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add_w["ok"], true, "{add_w}");

    let (reader_cookie, reader_v) = signup_and_login(&app, "pr@ex.com", "pread").await;
    verify_user(&db, reader_v["data"]["id"].as_str().unwrap()).await;
    let add_r = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"paown","name":"core","username":"pread","permission":"read"}}"#,
    )
    .await;
    assert_eq!(add_r["ok"], true, "{add_r}");

    // Comment authored by the writer collaborator.
    let created = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"paown","name":"core","number":{n},"body":"mine"}}}}"#
        ),
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let comment_id = created["data"]["id"].as_str().unwrap();

    // Non-author (repo owner, Write+) cannot edit.
    let denied_edit = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.comments.update","input":{{"owner":"paown","name":"core","number":{n},"commentId":"{comment_id}","body":"hijack"}}}}"#
        ),
    )
    .await;
    assert_eq!(denied_edit["ok"], false, "{denied_edit}");
    assert_eq!(denied_edit["error"]["code"], "repo.not_found", "{denied_edit}");

    // Read collaborator cannot delete.
    let denied_del = rpc_json(
        &app,
        &reader_cookie,
        &format!(
            r#"{{"procedure":"pull.comments.delete","input":{{"owner":"paown","name":"core","number":{n},"commentId":"{comment_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(denied_del["ok"], false, "{denied_del}");
    assert_eq!(denied_del["error"]["code"], "repo.not_found", "{denied_del}");

    // Write+ moderator (owner) can delete another user's comment.
    let moderated = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.comments.delete","input":{{"owner":"paown","name":"core","number":{n},"commentId":"{comment_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(moderated["ok"], true, "{moderated}");

    // Author can delete their own comment.
    let own = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"paown","name":"core","number":{n},"body":"mine again"}}}}"#
        ),
    )
    .await;
    assert_eq!(own["ok"], true, "{own}");
    let own_id = own["data"]["id"].as_str().unwrap();
    let own_del = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"pull.comments.delete","input":{{"owner":"paown","name":"core","number":{n},"commentId":"{own_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(own_del["ok"], true, "{own_del}");

    let list = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.comments.list","input":{{"owner":"paown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(list["data"]["comments"].as_array().unwrap().len(), 0);
}
