//! DEBT-03 `search.global` integration tests — sitewide grouped search with
//! ACL filtering (public + owned/collaborator/org-readable repos only).

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

async fn verify_email(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify email");
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

async fn create_repo(
    app: &axum::Router,
    cookie: &str,
    name: &str,
    visibility: &str,
    description: &str,
) {
    let res = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":"{description}"}}}}"#
        ),
        Some(cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
}

async fn seed_file(
    repos: &std::path::Path,
    owner: &str,
    repo: &str,
    message: &str,
    path: &str,
    content: &str,
) {
    let bare = repos.join(owner).join(format!("{repo}.git"));
    CliGitBackend::new()
        .seed_commit(
            &bare,
            "main",
            message,
            &[(path.to_string(), content.as_bytes().to_vec())],
        )
        .await
        .expect("seed commit");
}

fn hit_names(resp: &serde_json::Value, group: &str, field: &str) -> Vec<String> {
    resp["data"][group]["hits"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|h| h[field].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Public repos + issues surface for anonymous viewers; private ones never do.
#[tokio::test]
async fn search_global_anonymous_public_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("search_global_anon.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "gown@ex.com", "gsown").await;
    verify_email(&db, v["data"]["id"].as_str().unwrap()).await;

    create_repo(&app, &cookie, "public-widgets", "public", "GSRCH_TOKEN pub").await;
    create_repo(
        &app,
        &cookie,
        "private-widgets",
        "private",
        "GSRCH_TOKEN priv",
    )
    .await;
    seed_file(
        &repos,
        "gsown",
        "public-widgets",
        "GSRCH_TOKEN commit msg",
        "a.txt",
        "GSRCH_TOKEN body",
    )
    .await;
    seed_file(
        &repos,
        "gsown",
        "private-widgets",
        "GSRCH_TOKEN commit msg",
        "a.txt",
        "GSRCH_TOKEN body",
    )
    .await;
    let iss = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"gsown","name":"public-widgets","title":"GSRCH_TOKEN issue","body":""}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(iss["ok"], true, "{iss}");
    let iss_priv = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"gsown","name":"private-widgets","title":"GSRCH_TOKEN issue","body":""}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(iss_priv["ok"], true, "{iss_priv}");

    let res = rpc_json(
        &app,
        r#"{"procedure":"search.global","input":{"q":"GSRCH_TOKEN"}}"#,
        None,
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");

    let repo_names = hit_names(&res, "repositories", "name");
    assert!(repo_names.contains(&"public-widgets".to_string()), "{res}");
    assert!(
        !repo_names.contains(&"private-widgets".to_string()),
        "anonymous must not see private repos — {res}"
    );
    assert_eq!(res["data"]["repositories"]["total"].as_i64().unwrap(), 1);

    // Issues from private repos are invisible to anonymous viewers.
    let issue_repos = hit_names(&res, "issues", "repo_name");
    assert_eq!(issue_repos, vec!["public-widgets".to_string()], "{res}");

    // Commits/code scans stay inside the same ACL boundary.
    let code_repos = hit_names(&res, "code", "repo_name");
    assert_eq!(code_repos, vec!["public-widgets".to_string()], "{res}");
    let commit_repos = hit_names(&res, "commits", "repo_name");
    assert_eq!(commit_repos, vec!["public-widgets".to_string()], "{res}");

    // Anonymous viewers cannot enumerate users at all.
    assert_eq!(
        res["data"]["users"]["hits"].as_array().unwrap().len(),
        0,
        "anonymous user directory must be empty — {res}"
    );
    assert_eq!(res["data"]["users"]["total"].as_i64().unwrap(), 0);
}

/// Owners (and collaborators) see their private repos/issues; strangers don't.
#[tokio::test]
async fn search_global_private_acl() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("search_global_acl.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "gacl-o@ex.com", "gaclo").await;
    verify_email(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    let (collab_cookie, collab_v) = signup_and_login(&app, "gacl-c@ex.com", "gaclc").await;
    verify_email(&db, collab_v["data"]["id"].as_str().unwrap()).await;
    let (stranger_cookie, stranger_v) = signup_and_login(&app, "gacl-s@ex.com", "gacls").await;
    verify_email(&db, stranger_v["data"]["id"].as_str().unwrap()).await;

    create_repo(&app, &owner_cookie, "vault", "private", "GACL_TOKEN vault").await;
    seed_file(
        &repos,
        "gaclo",
        "vault",
        "GACL_TOKEN msg",
        "a.txt",
        "GACL_TOKEN code",
    )
    .await;
    let iss = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"gaclo","name":"vault","title":"GACL_TOKEN issue","body":""}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(iss["ok"], true, "{iss}");

    // Grant the collaborator read on the private repo.
    let grant = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"gaclo","name":"vault","username":"gaclc","permission":"read"}}"#,
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(grant["ok"], true, "{grant}");

    for (who, cookie, expect_hit) in [
        ("owner", &owner_cookie, true),
        ("collaborator", &collab_cookie, true),
        ("stranger", &stranger_cookie, false),
    ] {
        let res = rpc_json(
            &app,
            r#"{"procedure":"search.global","input":{"q":"GACL_TOKEN"}}"#,
            Some(cookie.as_str()),
        )
        .await;
        assert_eq!(res["ok"], true, "{who}: {res}");
        let names = hit_names(&res, "repositories", "name");
        assert_eq!(
            names.contains(&"vault".to_string()),
            expect_hit,
            "{who} repo visibility — {res}"
        );
        let issue_repos = hit_names(&res, "issues", "repo_name");
        assert_eq!(
            issue_repos.contains(&"vault".to_string()),
            expect_hit,
            "{who} issue visibility — {res}"
        );
        let code_repos = hit_names(&res, "code", "repo_name");
        assert_eq!(
            code_repos.contains(&"vault".to_string()),
            expect_hit,
            "{who} code visibility — {res}"
        );
    }
}

/// Grouped response shape: every entity kind populated from one call.
#[tokio::test]
async fn search_global_grouped_hits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("search_global_grouped.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "gg@ex.com", "ggrpuser").await;
    verify_email(&db, v["data"]["id"].as_str().unwrap()).await;

    create_repo(&app, &cookie, "gizmo", "public", "GGRP gizmo repo").await;
    seed_file(
        &repos,
        "ggrpuser",
        "gizmo",
        "GGRP commit subject",
        "src/g.txt",
        "GGRP code body\n",
    )
    .await;
    let iss = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"ggrpuser","name":"gizmo","title":"GGRP issue","body":""}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(iss["ok"], true, "{iss}");
    let br = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"ggrpuser","name":"gizmo","branch":"feat","start":"main"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    let pr = rpc_json(
        &app,
        r#"{"procedure":"pull.create","input":{"owner":"ggrpuser","name":"gizmo","title":"GGRP pull","body":"","base_ref":"main","head_ref":"feat"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let org = rpc_json(
        &app,
        r#"{"procedure":"org.create","input":{"slug":"ggrp-org","display_name":"GGRP Org"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(org["ok"], true, "{org}");

    let res = rpc_json(
        &app,
        r#"{"procedure":"search.global","input":{"q":"GGRP"}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    assert_eq!(res["data"]["q"], "GGRP");

    assert!(
        hit_names(&res, "repositories", "name").contains(&"gizmo".to_string()),
        "{res}"
    );
    assert!(
        hit_names(&res, "users", "username").contains(&"ggrpuser".to_string()),
        "{res}"
    );
    assert!(
        hit_names(&res, "organizations", "slug").contains(&"ggrp-org".to_string()),
        "{res}"
    );
    let issues = hit_names(&res, "issues", "title");
    assert_eq!(issues, vec!["GGRP issue".to_string()], "{res}");
    let pulls = hit_names(&res, "pulls", "title");
    assert_eq!(pulls, vec!["GGRP pull".to_string()], "{res}");

    let commits = res["data"]["commits"]["hits"].as_array().expect("commits");
    assert!(
        commits
            .iter()
            .any(|c| c["subject"].as_str() == Some("GGRP commit subject")),
        "{res}"
    );
    let code = res["data"]["code"]["hits"].as_array().expect("code");
    assert!(
        code.iter().any(|c| c["path"].as_str() == Some("src/g.txt")),
        "{res}"
    );
}

/// `types` scopes hit population; database groups still report totals.
#[tokio::test]
async fn search_global_types_filter() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("search_global_types.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "gt@ex.com", "gtuser").await;
    verify_email(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "gtk", "public", "GTK repo").await;
    let iss = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"gtuser","name":"gtk","title":"GTK issue","body":""}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(iss["ok"], true, "{iss}");

    let res = rpc_json(
        &app,
        r#"{"procedure":"search.global","input":{"q":"GTK","types":["issues"]}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    assert_eq!(res["data"]["issues"]["hits"].as_array().unwrap().len(), 1);
    // Unrequested groups: empty hits but real totals for tab badges.
    assert_eq!(
        res["data"]["repositories"]["hits"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        res["data"]["repositories"]["total"].as_i64().unwrap(),
        1,
        "{res}"
    );
    assert_eq!(
        res["data"]["commits"]["total"].as_i64().unwrap(),
        0,
        "{res}"
    );
}

/// `is:` / `author:` qualifiers apply to issue/pull/commit groups.
#[tokio::test]
async fn search_global_qualifiers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("search_global_qual.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, v) = signup_and_login(&app, "gq@ex.com", "gquser").await;
    verify_email(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "gq", "public", "").await;
    seed_file(&repos, "gquser", "gq", "GQUAL commit", "a.txt", "x\n").await;
    let open = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"gquser","name":"gq","title":"GQUAL open","body":""}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(open["ok"], true, "{open}");
    let closed = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"gquser","name":"gq","title":"GQUAL closed","body":""}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(closed["ok"], true, "{closed}");
    let upd = rpc_json(
        &app,
        r#"{"procedure":"issue.close","input":{"owner":"gquser","name":"gq","number":2}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");

    // is:closed only matches the closed issue.
    let res = rpc_json(
        &app,
        r#"{"procedure":"search.global","input":{"q":"GQUAL is:closed","types":["issues","commits"]}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    let titles = hit_names(&res, "issues", "title");
    assert_eq!(titles, vec!["GQUAL closed".to_string()], "{res}");
    // Commits still match the bare keyword (is: doesn't apply to git log).
    let commits = hit_names(&res, "commits", "subject");
    assert!(commits.contains(&"GQUAL commit".to_string()), "{res}");

    // author:Oxidean narrows commits by git author name.
    let res = rpc_json(
        &app,
        r#"{"procedure":"search.global","input":{"q":"author:Oxidean","types":["commits"]}}"#,
        Some(&cookie),
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    let commits = hit_names(&res, "commits", "subject");
    assert!(commits.contains(&"GQUAL commit".to_string()), "{res}");
}

/// Empty/whitespace queries return empty groups, not errors.
#[tokio::test]
async fn search_global_empty_query() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("search_global_empty.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let res = rpc_json(
        &app,
        r#"{"procedure":"search.global","input":{"q":"   "}}"#,
        None,
    )
    .await;
    assert_eq!(res["ok"], true, "{res}");
    for group in [
        "repositories",
        "users",
        "organizations",
        "issues",
        "pulls",
        "commits",
        "code",
    ] {
        assert_eq!(
            res["data"][group]["total"].as_i64().unwrap_or(-1),
            0,
            "{group} should be empty — {res}"
        );
        assert!(
            res["data"][group]["hits"].as_array().unwrap().is_empty(),
            "{res}"
        );
    }
}
