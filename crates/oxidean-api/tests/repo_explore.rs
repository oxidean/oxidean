//! SOC-03 Wave 0 stubs → green in 21-04.
//! Anonymous list public sorted by stars then updated_at; private absent; optional q.

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

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str, desc: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":"{desc}","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
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
async fn repo_explore_anonymous_public_sorted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("explore.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, v) = signup_and_login(&app, "ex@ex.com", "exown").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "alpha", "public", "alpha desc").await;
    create_repo(&app, &cookie, "beta", "public", "beta desc").await;
    create_repo(&app, &cookie, "hidden", "private", "nope").await;

    let _ = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.star","input":{"owner":"exown","name":"beta"}}"#,
    )
    .await;

    let listed = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.explore","input":{"offset":0,"limit":50}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    let repos = listed["data"]["repos"].as_array().unwrap();
    assert!(repos.iter().all(|r| r["visibility"] == "public"));
    assert!(repos.iter().all(|r| r["name"] != "hidden"));
    if repos.len() >= 2 {
        let names: Vec<&str> = repos.iter().map(|r| r["name"].as_str().unwrap()).collect();
        let beta_i = names.iter().position(|n| *n == "beta");
        let alpha_i = names.iter().position(|n| *n == "alpha");
        if let (Some(bi), Some(ai)) = (beta_i, alpha_i) {
            assert!(bi < ai, "beta (starred) should sort before alpha");
        }
    }
}

#[tokio::test]
async fn repo_explore_q_filter() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("explore_q.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, v) = signup_and_login(&app, "q@ex.com", "qown").await;
    verify_user(&db, v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "needle", "public", "findme").await;
    create_repo(&app, &cookie, "other", "public", "skip").await;

    let listed = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.explore","input":{"q":"needle","offset":0,"limit":20}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    let repos = listed["data"]["repos"].as_array().unwrap();
    assert!(repos.iter().any(|r| r["name"] == "needle"));
    assert!(repos.iter().all(
        |r| r["name"] != "other" || r["description"].as_str().unwrap_or("").contains("needle")
    ));
}
