//! Phase 16 / GIT-18 `repo.search` integration tests.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::{CliGitBackend, GitBackend};
use tower::ServiceExt;

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

async fn rpc_json(app: &axum::Router, body: &str, cookie: Option<&str>) -> serde_json::Value {
    let res = match cookie {
        Some(c) => app
            .clone()
            .oneshot(rpc_req_with_cookie(body, c))
            .await
            .unwrap(),
        None => app.clone().oneshot(rpc_req(body)).await.unwrap(),
    };
    // Soft `repo.not_found` is HTTP 404 with RPC error body (anti-enumeration).
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("rpc json")
}

async fn create_and_seed(
    app: &axum::Router,
    repos: &std::path::Path,
    cookie: &str,
    owner: &str,
    repo: &str,
    visibility: &str,
    needle: &str,
) {
    let create = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{repo}","visibility":"{visibility}","description":""}}}}"#
        ),
        Some(cookie),
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let bare = repos.join(owner).join(format!("{repo}.git"));
    let git = CliGitBackend::new();
    let content = format!("hello\n{needle}\nworld\n");
    git.seed_commit(
        &bare,
        "main",
        "seed search",
        &[("src/needle.txt".into(), content.into_bytes())],
    )
    .await
    .expect("seed");
}

/// D-SRCH-06 / D-SRCH-14: Read user finds seeded text on default branch via type=code.
#[tokio::test]
async fn repo_search_code() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_search_code.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "code@ex.com", "codesrch").await;
    let owner_id = v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify");

    let needle = "OXIDEAN_SRCH_UNIQUE_CODE_42";
    create_and_seed(
        &app, &repos, &cookie, "codesrch", "widgets", "public", needle,
    )
    .await;

    let res = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.search","input":{{"owner":"codesrch","name":"widgets","type":"code","q":"{needle}"}}}}"#
        ),
        Some(&cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    let hits = res["data"]["hits"].as_array().expect("hits");
    assert!(!hits.is_empty(), "expected code hits — {res}");
    assert_eq!(hits[0]["kind"], "code");
    assert_eq!(hits[0]["path"], "src/needle.txt");
    let content = hits[0]["content"].as_str().unwrap_or("");
    assert!(
        content.contains(needle),
        "hit content should include needle — {res}"
    );

    // No matches → empty hits, not error.
    let empty = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"codesrch","name":"widgets","type":"code","q":"zzz_no_such_token_zzz"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(empty["ok"], true, "{empty}");
    assert_eq!(empty["data"]["hits"].as_array().unwrap().len(), 0);
}

/// D-SRCH-07 / D-SRCH-12: type=commits matches message grep and author:login.
#[tokio::test]
async fn repo_search_commits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_search_commits.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "cmt@ex.com", "cmtsrch").await;
    let owner_id = v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify");

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"hist","visibility":"public","description":""}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let bare = repos.join("cmtsrch").join("hist.git");
    let git = CliGitBackend::new();
    git.seed_commit(
        &bare,
        "main",
        "UNIQUE_COMMIT_SRCH_MSG",
        &[("a.txt".into(), b"one\n".to_vec())],
    )
    .await
    .expect("seed");

    let by_msg = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"cmtsrch","name":"hist","type":"commits","q":"UNIQUE_COMMIT_SRCH_MSG"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(by_msg["ok"], true, "{by_msg}");
    let hits = by_msg["data"]["hits"].as_array().expect("hits");
    assert!(!hits.is_empty(), "{by_msg}");
    assert_eq!(hits[0]["kind"], "commit");
    assert!(
        hits[0]["subject"]
            .as_str()
            .unwrap_or("")
            .contains("UNIQUE_COMMIT_SRCH_MSG"),
        "{by_msg}"
    );

    let by_author = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"cmtsrch","name":"hist","type":"commits","q":"author:Oxidean"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(by_author["ok"], true, "{by_author}");
    assert!(
        !by_author["data"]["hits"].as_array().unwrap().is_empty(),
        "author: qualifier should find seeded commit — {by_author}"
    );
}

/// D-SRCH-09 / D-SRCH-12: type=issues matches title/body with is:open/is:closed.
#[tokio::test]
async fn repo_search_issues() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_search_issues.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "iss@ex.com", "isssrch").await;
    let owner_id = v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify");

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"bugs","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let issue = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"isssrch","name":"bugs","title":"UNIQUE_ISSUE_SRCH_TITLE","body":"details here"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(issue["ok"], true, "{issue}");

    let found = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"isssrch","name":"bugs","type":"issues","q":"UNIQUE_ISSUE_SRCH_TITLE is:open"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(found["ok"], true, "{found}");
    let hits = found["data"]["hits"].as_array().expect("hits");
    assert!(!hits.is_empty(), "{found}");
    assert_eq!(hits[0]["kind"], "issue");
    assert!(
        hits[0]["title"]
            .as_str()
            .unwrap_or("")
            .contains("UNIQUE_ISSUE_SRCH_TITLE"),
        "{found}"
    );

    // Issues tab must not return pull rows (create a PR and ensure type=issues ignores it).
    let branch = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"isssrch","name":"bugs","branch":"feature","start":"main"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(branch["ok"], true, "{branch}");
    let pr = rpc_json(
        &app,
        r#"{"procedure":"pull.create","input":{"owner":"isssrch","name":"bugs","title":"UNIQUE_ISSUE_SRCH_TITLE pr twin","body":"","base_ref":"main","head_ref":"feature"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");

    let issues_only = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"isssrch","name":"bugs","type":"issues","q":"UNIQUE_ISSUE_SRCH_TITLE"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(issues_only["ok"], true, "{issues_only}");
    for hit in issues_only["data"]["hits"].as_array().unwrap() {
        assert_eq!(
            hit["kind"], "issue",
            "issues tab must not mix PRs — {issues_only}"
        );
    }
}

/// D-SRCH-10 / D-SRCH-11: type=pulls finds PRs separately from issues.
#[tokio::test]
async fn repo_search_pulls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_search_pulls.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "pr@ex.com", "prsrch").await;
    let owner_id = v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify");

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"app","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let _issue = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"prsrch","name":"app","title":"UNIQUE_PULL_SRCH_TITLE issue twin","body":""}}"#,
        Some(&cookie),
    )
    .await;

    let branch = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"prsrch","name":"app","branch":"feature","start":"main"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(branch["ok"], true, "{branch}");

    let pr = rpc_json(
        &app,
        r#"{"procedure":"pull.create","input":{"owner":"prsrch","name":"app","title":"UNIQUE_PULL_SRCH_TITLE","body":"pr body","base_ref":"main","head_ref":"feature"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");

    let found = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"prsrch","name":"app","type":"pulls","q":"UNIQUE_PULL_SRCH_TITLE is:open"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(found["ok"], true, "{found}");
    let hits = found["data"]["hits"].as_array().expect("hits");
    assert!(!hits.is_empty(), "{found}");
    assert_eq!(hits[0]["kind"], "pull");
    assert!(
        hits[0]["title"]
            .as_str()
            .unwrap_or("")
            .contains("UNIQUE_PULL_SRCH_TITLE"),
        "{found}"
    );
    for hit in hits {
        assert_eq!(
            hit["kind"], "pull",
            "pulls tab must not mix issues — {found}"
        );
    }

    let issues_tab = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"prsrch","name":"app","type":"issues","q":"UNIQUE_PULL_SRCH_TITLE"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(issues_tab["ok"], true, "{issues_tab}");
    for hit in issues_tab["data"]["hits"].as_array().unwrap() {
        assert_eq!(hit["kind"], "issue", "{issues_tab}");
        assert_ne!(
            hit["title"].as_str().unwrap_or(""),
            "UNIQUE_PULL_SRCH_TITLE",
            "exact PR title must not appear in issues tab"
        );
    }
}

/// D-SRCH-04: private unauthorized actor gets soft repo.not_found.
#[tokio::test]
async fn repo_search_acl() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("repo_search_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "owner@ex.com", "srchown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify owner");

    create_and_seed(
        &app,
        &repos,
        &owner_cookie,
        "srchown",
        "secret",
        "private",
        "PRIVATE_NEEDLE",
    )
    .await;

    let (stranger_cookie, stranger_v) = signup_and_login(&app, "stranger@ex.com", "srchstr").await;
    let stranger_id = stranger_v["data"]["id"].as_str().expect("id");
    db.set_email_verified_at(stranger_id, &now)
        .await
        .expect("verify stranger");

    let denied = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"srchown","name":"secret","type":"code","q":"PRIVATE_NEEDLE"}}"#,
        Some(&stranger_cookie),
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(
        denied["error"]["code"], "repo.not_found",
        "private unauthorized → soft not_found — {denied}"
    );

    let anon = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"srchown","name":"secret","type":"code","q":"PRIVATE_NEEDLE"}}"#,
        None,
    )
    .await;
    assert_eq!(anon["ok"], false, "{anon}");
    assert_eq!(anon["error"]["code"], "repo.not_found", "{anon}");
}

/// D-SRCH-08: soft max-matches yield truncated flag.
#[tokio::test]
async fn repo_search_limits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_search_limits.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "lim@ex.com", "srchlim").await;
    let owner_id = v["data"]["id"].as_str().expect("id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(owner_id, &now)
        .await
        .expect("verify");

    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"many","visibility":"public","description":""}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let bare = repos.join("srchlim").join("many.git");
    let git = CliGitBackend::new();
    // Many lines with the same token so soft cap can truncate.
    let mut body = String::new();
    for i in 0..120 {
        body.push_str(&format!("line {i} SRCH_CAP_TOKEN more\n"));
    }
    git.seed_commit(
        &bare,
        "main",
        "many matches",
        &[("cap.txt".into(), body.into_bytes())],
    )
    .await
    .expect("seed");

    let res = rpc_json(
        &app,
        r#"{"procedure":"repo.search","input":{"owner":"srchlim","name":"many","type":"code","q":"SRCH_CAP_TOKEN","limit":30}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    let hits = res["data"]["hits"].as_array().expect("hits");
    assert!(!hits.is_empty(), "{res}");
    assert!(
        res["data"]["truncated"].as_bool().unwrap_or(false),
        "expected truncated when matches exceed page/cap — {res}"
    );
}
