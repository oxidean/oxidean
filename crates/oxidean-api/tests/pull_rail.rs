//! WI-0001 pr-new-ui rail parity: `pull.milestone.set`,
//! `pull.development.list`, `pull.projects.list`, `pull.deployments.list`
//! over the RPC shell.

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

/// Repo + branch + PR #1 owned by `pown`; returns (cookie, pull_id).
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
    let pull_id = pr["data"]["id"].as_str().unwrap().to_string();
    (cookie, pull_id)
}

#[tokio::test]
async fn pull_milestone_set_round_trip_and_clear() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_rail.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _pull_id) = seed_pr(&app, &db, &repos).await;

    let ms = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"milestone.create","input":{"owner":"pown","name":"core","title":"v1","description":"","due_on":null}}"#,
    )
    .await;
    assert_eq!(ms["ok"], true, "{ms}");
    let ms_number = ms["data"]["number"].as_i64().unwrap();

    // Unknown milestone number → not_found.
    let bad = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.milestone.set","input":{"owner":"pown","name":"core","number":1,"milestone":999}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false, "{bad}");
    assert_eq!(bad["error"]["code"], "milestone.not_found");

    let set = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.milestone.set","input":{{"owner":"pown","name":"core","number":1,"milestone":{ms_number}}}}}"#
        ),
    )
    .await;
    assert_eq!(set["ok"], true, "{set}");
    assert_eq!(set["data"]["milestone"]["number"], ms_number);

    let got = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.get","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(got["data"]["milestone"]["title"], "v1", "{got}");

    let clear = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.milestone.set","input":{"owner":"pown","name":"core","number":1,"milestone":null}}"#,
    )
    .await;
    assert_eq!(clear["ok"], true, "{clear}");
    assert!(clear["data"]["milestone"].is_null(), "{clear}");
}

#[tokio::test]
async fn pull_development_list_finds_closing_refs() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_dev.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _pull_id) = seed_pr(&app, &db, &repos).await;

    let issue = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"pown","name":"core","title":"Bug","body":""}}"#,
    )
    .await;
    assert_eq!(issue["ok"], true, "{issue}");
    let issue_number = issue["data"]["number"].as_i64().unwrap();

    // No refs yet → empty.
    let empty = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.development.list","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(empty["ok"], true, "{empty}");
    assert_eq!(empty["data"]["items"].as_array().unwrap().len(), 0);

    let upd = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.update","input":{{"owner":"pown","name":"core","number":1,"body":"This closes #{issue_number} and fixes #{issue_number}."}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");

    let dev = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.development.list","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(dev["ok"], true, "{dev}");
    let items = dev["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{dev}");
    assert_eq!(items[0]["relation"], "closes");
    assert_eq!(items[0]["issue"]["number"], issue_number);
    assert_eq!(items[0]["repo_owner"], "pown");
    assert_eq!(items[0]["repo_name"], "core");
}

#[tokio::test]
async fn pull_development_list_covers_keyword_inflections_and_qualified_refs() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_dev_kw.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _pull_id) = seed_pr(&app, &db, &repos).await;

    let issue = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"pown","name":"core","title":"Bug","body":""}}"#,
    )
    .await;
    assert_eq!(issue["ok"], true, "{issue}");
    let n = issue["data"]["number"].as_i64().unwrap();

    // Inflected keywords + repo-qualified ref all resolve; bogus forms don't.
    let upd = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.update","input":{{"owner":"pown","name":"core","number":1,"body":"close #{n} fixed pown/core#{n} resolved #99999 notakeyword #{n}x"}}}}"#
        ),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");

    let dev = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.development.list","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(dev["ok"], true, "{dev}");
    let items = dev["data"]["items"].as_array().unwrap();
    // `close #n`, `fixed pown/core#n`, `resolved #99999` (missing issue →
    // skipped); `#{n}x` must not match.
    let relations: Vec<&str> = items
        .iter()
        .map(|i| i["relation"].as_str().unwrap())
        .collect();
    assert_eq!(relations, vec!["close", "fixed"], "{dev}");
    for item in items {
        assert_eq!(item["repo_owner"], "pown");
        assert_eq!(item["repo_name"], "core");
        assert_eq!(item["issue"]["number"], n);
    }
}

#[tokio::test]
async fn pull_projects_list_returns_linked_card() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_proj.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, pull_id) = seed_pr(&app, &db, &repos).await;

    let proj = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"projects.create","input":{"owner":"pown","name":"core","title":"Roadmap","body":""}}"#,
    )
    .await;
    assert_eq!(proj["ok"], true, "{proj}");
    let proj_number = proj["data"]["number"].as_i64().unwrap();

    let col = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.columns.create","input":{{"owner":"pown","name":"core","project":{proj_number},"column_name":"Todo"}}}}"#
        ),
    )
    .await;
    assert_eq!(col["ok"], true, "{col}");
    let col_id = col["data"]["id"].as_str().unwrap().to_string();

    let card = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"projects.cards.create","input":{{"owner":"pown","name":"core","column":"{col_id}","content_type":"pull_request","content_id":"{pull_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(card["ok"], true, "{card}");

    let links = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.projects.list","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(links["ok"], true, "{links}");
    let rows = links["data"]["links"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{links}");
    assert_eq!(rows[0]["project_number"], proj_number);
    assert_eq!(rows[0]["project_title"], "Roadmap");
    assert!(
        rows[0]["project_url"]
            .as_str()
            .unwrap()
            .contains("/projects/"),
        "{links}"
    );
}

#[tokio::test]
async fn pull_deployments_list_empty_is_ok() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_dep.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let (cookie, _pull_id) = seed_pr(&app, &db, &repos).await;

    let res = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.deployments.list","input":{"owner":"pown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    assert_eq!(
        res["data"]["deployments"].as_array().unwrap().len(),
        0,
        "{res}"
    );
}
