//! Issue #23 — pathLastCommits, commitCount, contributors.list.

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
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
    );
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

async fn rpc_json(app: &axum::Router, cookie: Option<&str>, body: &str) -> serde_json::Value {
    let res = match cookie {
        Some(c) => app
            .clone()
            .oneshot(rpc_req_with_cookie(body, c))
            .await
            .unwrap(),
        None => app.clone().oneshot(rpc_req(body)).await.unwrap(),
    };
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn path_last_commits_and_count_and_contributors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("enrich.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, v) = signup_and_login(&app, "hist@example.com", "histuser").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "histrepo", "public").await;

    let tip = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.commits","input":{"owner":"histuser","name":"histrepo","ref":"main","limit":1}}"#,
    )
    .await;
    assert_eq!(tip["ok"], true, "{tip}");
    assert!(
        tip["data"]["commits"].as_array().unwrap().len() >= 1,
        "expected at least one commit from stack seed"
    );

    let count = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.commitCount","input":{"owner":"histuser","name":"histrepo","ref":"main"}}"#,
    )
    .await;
    assert_eq!(count["ok"], true, "{count}");
    assert!(count["data"]["count"].as_u64().unwrap() >= 1);

    let last = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.pathLastCommits","input":{"owner":"histuser","name":"histrepo","ref":"main","path":""}}"#,
    )
    .await;
    assert_eq!(last["ok"], true, "{last}");
    let commits = last["data"]["commits"].as_object().unwrap();
    assert!(
        !commits.is_empty(),
        "expected last-commit map entries: {commits:?}"
    );

    let contrib = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.contributors.list","input":{"owner":"histuser","name":"histrepo","limit":10}}"#,
    )
    .await;
    assert_eq!(contrib["ok"], true, "{contrib}");
    assert!(
        !contrib["data"]["contributors"]
            .as_array()
            .unwrap()
            .is_empty(),
        "expected contributors"
    );

    let langs = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.languages","input":{"owner":"histuser","name":"histrepo"}}"#,
    )
    .await;
    assert_eq!(langs["ok"], true, "{langs}");
    // Seeded stack repos typically include programming-language sources.
    assert!(
        langs["data"]["languages"].as_array().is_some(),
        "expected languages array: {langs}"
    );

    create_repo(&app, &cookie, "secret", "private").await;
    let (cookie2, v2) = signup_and_login(&app, "other@example.com", "otheruser").await;
    verify_user(&db, v2["data"]["id"].as_str().unwrap()).await;
    let denied = rpc_json(
        &app,
        Some(&cookie2),
        r#"{"procedure":"repo.pathLastCommits","input":{"owner":"histuser","name":"secret","ref":"main"}}"#,
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found");
}
