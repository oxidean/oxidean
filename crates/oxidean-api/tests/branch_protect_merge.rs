//! PR-08 / D-22: PR merge blocked until protection satisfied.

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

async fn seed_protected_repo(app: &axum::Router, db: &Database) -> (String, String) {
    let (owner_cookie, owner_login) = signup_and_login(app, "mown@ex.com", "mown").await;
    verify_user(db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        app,
        &owner_cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"mown","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    let rule = rpc_json(
        app,
        &owner_cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"mown","name":"core","pattern":"main","require_reviews":true,"required_approving_review_count":1,"enforce_admins":true}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "{rule}");
    (
        owner_cookie,
        owner_login["data"]["id"].as_str().unwrap().to_string(),
    )
}

/// Merge into protected base fails without required approvals (PR-08, D-22).
#[tokio::test]
async fn branch_protect_merge_blocked_without_approval() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("bp_merge_block.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (cookie, _) = seed_protected_repo(&app, &db).await;

    let pr = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"mown","name":"core","title":"Fix","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();

    let merge = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"mown","name":"core","number":{n},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(merge["ok"], false, "merge must block — {merge}");
    assert_eq!(merge["error"]["code"], "pull.merge_blocked");
    let reasons = merge["error"]["data"]["reasons"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        reasons.iter().any(|r| r.as_str() == Some("reviews")),
        "expected reviews reason — {merge}"
    );
}

/// After eligible Approve, merge succeeds (PR-08).
#[tokio::test]
async fn branch_protect_merge_succeeds_after_approval() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("bp_merge_ok.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (owner_cookie, _) = seed_protected_repo(&app, &db).await;

    let (reviewer_cookie, reviewer_login) = signup_and_login(&app, "rev@ex.com", "revu").await;
    verify_user(&db, reviewer_login["data"]["id"].as_str().unwrap()).await;
    let add = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"mown","name":"core","username":"revu","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");

    let pr = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"mown","name":"core","title":"Fix","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();

    let review = rpc_json(
        &app,
        &reviewer_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"mown","name":"core","number":{n},"state":"approved","body":"lgtm"}}}}"#
        ),
    )
    .await;
    assert_eq!(review["ok"], true, "{review}");

    let merge = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"mown","name":"core","number":{n},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(merge["ok"], true, "merge after approve — {merge}");
    assert_eq!(merge["data"]["pull"]["state"], "merged");
}

/// Required status context blocks merge until success (D-10) — greened further in 13-04.
#[tokio::test]
async fn branch_protect_merge_requires_status_context() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("bp_merge_ci.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "ci@ex.com", "ciown").await;
    verify_user(&db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"ciown","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    let rule = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"ciown","name":"core","pattern":"main","require_reviews":false,"required_status_contexts":["ci/test"],"enforce_admins":true}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "{rule}");

    let pr = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"ciown","name":"core","title":"CI","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();
    let head = pr["data"]["head_sha"].as_str().unwrap().to_string();

    let merge = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"ciown","name":"core","number":{n},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(merge["ok"], false, "{merge}");
    assert_eq!(merge["error"]["code"], "pull.merge_blocked");

    let st = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"repo.commitStatus.create","input":{{"owner":"ciown","name":"core","sha":"{head}","context":"ci/test","state":"success"}}}}"#
        ),
    )
    .await;
    assert_eq!(st["ok"], true, "{st}");

    let merge2 = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"ciown","name":"core","number":{n},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(merge2["ok"], true, "merge after status — {merge2}");
}
