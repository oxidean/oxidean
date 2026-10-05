//! COL-02 `repo.templates.list` — issue/PR templates read from the repo tree.

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

async fn signup_and_login(app: &axum::Router, email: &str, username: &str) -> (String, String) {
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
    let user_id = v["data"]["id"].as_str().expect("id").to_string();
    (cookie, user_id)
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
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("rpc json")
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str) {
    let res = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":""}}}}"#
        ),
        Some(cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
}

const BUG_TEMPLATE: &str = "---\nname: Bug report\nabout: File a bug to help us improve\ntitle: \"[BUG] \"\nlabels: bug, triage\n---\n\n**Steps to reproduce**\n\n1. …\n";
const FEATURE_TEMPLATE: &str =
    "---\nname: Feature request\nabout: Suggest an idea\n---\n\n## Summary\n\nDescribe it.\n";
const PR_TEMPLATE: &str = "## What changed\n\n- \n\n## Testing\n\n- [ ] Tested\n";

/// Seed repo with `.github/ISSUE_TEMPLATE/` + `PULL_REQUEST_TEMPLATE.md`.
async fn seed_templates(repos: &std::path::Path, owner: &str, repo: &str) {
    let bare = repos.join(owner).join(format!("{repo}.git"));
    let git = CliGitBackend::new();
    git.seed_commit(
        &bare,
        "main",
        "seed templates",
        &[
            (
                ".github/ISSUE_TEMPLATE/bug.md".into(),
                BUG_TEMPLATE.as_bytes().to_vec(),
            ),
            (
                ".github/ISSUE_TEMPLATE/feature.md".into(),
                FEATURE_TEMPLATE.as_bytes().to_vec(),
            ),
            // Non-markdown config must not surface as a template.
            (
                ".github/ISSUE_TEMPLATE/config.yml".into(),
                b"blank_issues_enabled: false\n".to_vec(),
            ),
            ("README.md".into(), b"# seeded\n".to_vec()),
            (
                "PULL_REQUEST_TEMPLATE.md".into(),
                PR_TEMPLATE.as_bytes().to_vec(),
            ),
        ],
    )
    .await
    .expect("seed templates");
}

/// Happy path: two issue templates parsed with frontmatter + one PR template.
#[tokio::test]
async fn repo_templates_list_github_dir_and_legacy_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("templates.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, user_id) = signup_and_login(&app, "t@ex.com", "tplowner").await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    create_repo(&app, &cookie, "templated", "public").await;
    seed_templates(&repos, "tplowner", "templated").await;

    let res = rpc_json(
        &app,
        r#"{"procedure":"repo.templates.list","input":{"owner":"tplowner","name":"templated"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");

    let issues = res["data"]["issues"].as_array().expect("issues");
    assert_eq!(issues.len(), 2, "{res}");
    // Sorted by filename within the winning location.
    assert_eq!(issues[0]["filename"], ".github/ISSUE_TEMPLATE/bug.md");
    assert_eq!(issues[0]["name"], "Bug report");
    assert_eq!(issues[0]["description"], "File a bug to help us improve");
    assert_eq!(issues[0]["title"], "[BUG] ");
    assert_eq!(issues[0]["labels"], serde_json::json!(["bug", "triage"]));
    assert!(
        issues[0]["body"]
            .as_str()
            .unwrap_or("")
            .contains("Steps to reproduce"),
        "{res}"
    );
    assert!(
        !issues[0]["body"].as_str().unwrap_or("").contains("name:"),
        "body must not retain frontmatter — {res}"
    );
    assert_eq!(issues[1]["name"], "Feature request");
    assert!(issues[1]["title"].is_null() || issues[1].get("title").is_none());

    let pulls = res["data"]["pulls"].as_array().expect("pulls");
    assert_eq!(pulls.len(), 1, "{res}");
    assert_eq!(pulls[0]["filename"], "PULL_REQUEST_TEMPLATE.md");
    assert_eq!(pulls[0]["name"], "PULL_REQUEST_TEMPLATE");
    assert!(
        pulls[0]["body"]
            .as_str()
            .unwrap_or("")
            .contains("## What changed"),
        "{res}"
    );
}

/// Ordered candidates: `.github/ISSUE_TEMPLATE/` beats `ISSUE_TEMPLATE/`; the
/// single `ISSUE_TEMPLATE.md` file is used only when no dir location matches.
#[tokio::test]
async fn repo_templates_candidate_precedence_and_single_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("templates_prec.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, user_id) = signup_and_login(&app, "p@ex.com", "precown").await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    create_repo(&app, &cookie, "multi", "public").await;
    let bare = repos.join("precown").join("multi.git");
    let git = CliGitBackend::new();
    git.seed_commit(
        &bare,
        "main",
        "seed competing locations",
        &[
            (
                ".github/ISSUE_TEMPLATE/winner.md".into(),
                "---\nname: Winner\n---\nfrom github dir\n"
                    .as_bytes()
                    .to_vec(),
            ),
            (
                "ISSUE_TEMPLATE/loser.md".into(),
                "---\nname: Root dir loser\n---\nroot\n".as_bytes().to_vec(),
            ),
            (
                "ISSUE_TEMPLATE.md".into(),
                b"legacy single issue template\n".to_vec(),
            ),
        ],
    )
    .await
    .expect("seed");

    let res = rpc_json(
        &app,
        r#"{"procedure":"repo.templates.list","input":{"owner":"precown","name":"multi"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    let issues = res["data"]["issues"].as_array().expect("issues");
    assert_eq!(issues.len(), 1, "first non-empty location wins — {res}");
    assert_eq!(issues[0]["name"], "Winner");

    // Second repo: only the legacy single file → still surfaces.
    create_repo(&app, &cookie, "legacy", "public").await;
    let bare = repos.join("precown").join("legacy.git");
    git.seed_commit(
        &bare,
        "main",
        "seed legacy single file",
        &[(
            "ISSUE_TEMPLATE.md".into(),
            b"legacy single issue template\n".to_vec(),
        )],
    )
    .await
    .expect("seed legacy");

    let res = rpc_json(
        &app,
        r#"{"procedure":"repo.templates.list","input":{"owner":"precown","name":"legacy"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    let issues = res["data"]["issues"].as_array().expect("issues");
    assert_eq!(issues.len(), 1, "{res}");
    assert_eq!(issues[0]["filename"], "ISSUE_TEMPLATE.md");
    assert_eq!(issues[0]["name"], "ISSUE_TEMPLATE");
    assert!(
        issues[0]["body"]
            .as_str()
            .unwrap_or("")
            .contains("legacy single issue template"),
        "{res}"
    );
}

/// Template-less / empty repos → empty lists, not errors. Anonymous on public OK.
#[tokio::test]
async fn repo_templates_empty_when_absent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("templates_empty.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, user_id) = signup_and_login(&app, "e@ex.com", "emptytpl").await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    create_repo(&app, &cookie, "plain", "public").await;
    let bare = repos.join("emptytpl").join("plain.git");
    CliGitBackend::new()
        .seed_commit(
            &bare,
            "main",
            "readme only",
            &[("README.md".into(), b"hi\n".to_vec())],
        )
        .await
        .expect("seed");

    // Anonymous read on a public repo.
    let res = rpc_json(
        &app,
        r#"{"procedure":"repo.templates.list","input":{"owner":"emptytpl","name":"plain"}}"#,
        None,
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    assert_eq!(res["data"]["issues"].as_array().unwrap().len(), 0);
    assert_eq!(res["data"]["pulls"].as_array().unwrap().len(), 0);
}

/// ACL: private repo templates hidden behind soft `repo.not_found`.
#[tokio::test]
async fn repo_templates_private_acl() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("templates_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (owner_cookie, owner_id) = signup_and_login(&app, "o@ex.com", "tplpriv").await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&owner_id, &now)
        .await
        .expect("verify");

    create_repo(&app, &owner_cookie, "secret", "private").await;
    seed_templates(&repos, "tplpriv", "secret").await;

    let (stranger_cookie, _stranger_id) = signup_and_login(&app, "s@ex.com", "tplstr").await;

    let denied = rpc_json(
        &app,
        r#"{"procedure":"repo.templates.list","input":{"owner":"tplpriv","name":"secret"}}"#,
        Some(&stranger_cookie),
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found", "{denied}");

    let anon = rpc_json(
        &app,
        r#"{"procedure":"repo.templates.list","input":{"owner":"tplpriv","name":"secret"}}"#,
        None,
    )
    .await;
    assert_eq!(anon["ok"], false, "{anon}");
    assert_eq!(anon["error"]["code"], "repo.not_found", "{anon}");

    // Owner still sees them.
    let ok = rpc_json(
        &app,
        r#"{"procedure":"repo.templates.list","input":{"owner":"tplpriv","name":"secret"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(ok["ok"], true, "{ok}");
    assert_eq!(ok["data"]["issues"].as_array().unwrap().len(), 2);
}
