//! PR-05 merge strategies + closing keywords (D-PR-17..23).

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

async fn seed_repo_with_feature(
    app: &axum::Router,
    db: &Database,
    email: &str,
    user: &str,
    repo: &str,
) -> String {
    let (cookie, login_v) = signup_and_login(app, email, user).await;
    verify_user(db, login_v["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{repo}","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
        ),
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.branchCreate","input":{{"owner":"{user}","name":"{repo}","branch":"feature","start":"main"}}}}"#
        ),
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    cookie
}

#[tokio::test]
async fn pull_merge_merge_commit_and_closes_keyword_issue() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_merge.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = seed_repo_with_feature(&app, &db, "m@ex.com", "mown", "core").await;

    let issue = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"mown","name":"core","title":"bug"}}"#,
    )
    .await;
    assert_eq!(issue["ok"], true, "{issue}");
    assert_eq!(issue["data"]["number"], 1);

    let pr = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"mown","name":"core","title":"Fix","body":"fixes #1","base_ref":"main","head_ref":"feature"}}"#,
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
    assert_eq!(merge["ok"], true, "merge — {merge}");
    assert_eq!(merge["data"]["pull"]["state"], "merged");
    assert!(merge["data"]["merge_commit_sha"].as_str().unwrap().len() >= 7);

    let closed = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.get","input":{"owner":"mown","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(closed["ok"], true, "{closed}");
    assert_eq!(
        closed["data"]["state"], "closed",
        "closing keyword on default merge"
    );
}

#[tokio::test]
async fn pull_merge_settings_disable_merge_commit() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_ms.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = seed_repo_with_feature(&app, &db, "s@ex.com", "sown", "core").await;

    let upd = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.mergeSettings.update","input":{"owner":"sown","name":"core","allow_merge_commit":false}}"#,
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["allow_merge_commit"], false);

    let pr = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"sown","name":"core","title":"PR","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();

    let merge = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"pull.merge","input":{{"owner":"sown","name":"core","number":{n},"method":"merge"}}}}"#
        ),
    )
    .await;
    assert_eq!(merge["ok"], false, "{merge}");
    assert_eq!(merge["error"]["code"], "pull.merge_disabled");
}
