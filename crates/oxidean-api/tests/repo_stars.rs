//! SOC-01 Wave 0 stubs → green in 21-01 / 21-02.
//! Covers: star/unstar idempotent, anonymous rejected, private not_found,
//! RepoPublic.star_count / viewer_has_starred, user.listStarred.

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
async fn repo_stars_star_unstar_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("stars.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, v) = signup_and_login(&app, "star@ex.com", "starown").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "hello", "public").await;

    let star = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.star","input":{"owner":"starown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(star["ok"], true, "{star}");
    assert_eq!(star["data"]["star_count"], 1);
    assert_eq!(star["data"]["viewer_has_starred"], true);

    let star2 = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.star","input":{"owner":"starown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(star2["ok"], true, "{star2}");
    assert_eq!(star2["data"]["star_count"], 1);

    let unstar = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.unstar","input":{"owner":"starown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(unstar["ok"], true, "{unstar}");
    assert_eq!(unstar["data"]["star_count"], 0);
    assert_eq!(unstar["data"]["viewer_has_starred"], false);
}

#[tokio::test]
async fn repo_stars_anonymous_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("stars_anon.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, v) = signup_and_login(&app, "own@ex.com", "ownuser").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "pub", "public").await;

    let star = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.star","input":{"owner":"ownuser","name":"pub"}}"#,
    )
    .await;
    assert_eq!(star["ok"], false, "{star}");
}

#[tokio::test]
async fn repo_stars_private_without_read_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("stars_priv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_c, owner_v) = signup_and_login(&app, "priv@ex.com", "privown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &owner_c, "secret", "private").await;

    let (other_c, other_v) = signup_and_login(&app, "other@ex.com", "otheru").await;
    verify_user(&db, other_v["data"]["id"].as_str().unwrap()).await;

    let star = rpc_json(
        &app,
        Some(&other_c),
        r#"{"procedure":"repo.star","input":{"owner":"privown","name":"secret"}}"#,
    )
    .await;
    assert_eq!(star["ok"], false, "{star}");
    let err = star["error"]["code"].as_str().unwrap_or("");
    assert!(
        err.contains("not_found") || err == "repo.not_found",
        "expected not_found, got {star}"
    );
}

#[tokio::test]
async fn repo_stars_list_starred_pagination() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("stars_list.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, v) = signup_and_login(&app, "list@ex.com", "listown").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "a", "public").await;
    create_repo(&app, &cookie, "b", "public").await;

    let _ = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.star","input":{"owner":"listown","name":"a"}}"#,
    )
    .await;
    let _ = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.star","input":{"owner":"listown","name":"b"}}"#,
    )
    .await;

    let listed = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"user.listStarred","input":{"offset":0,"limit":10}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    assert!(listed["data"]["repos"].as_array().unwrap().len() >= 2);
}
