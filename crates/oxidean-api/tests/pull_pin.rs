//! COL-04 — `pull.pin` / `pull.unpin`: ordered pinned box (≤3), Write+ only,
//! `pinned` on `PullPublic` and `pull.list` for the unfiltered open first page.

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

async fn signup_and_login(app: &axum::Router, email: &str, username: &str) -> (String, String) {
    let signup_body = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password": "password1"}}}}"#
    );
    let signup = app.clone().oneshot(rpc_req(&signup_body)).await.unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login_body = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password": "password1","remember_me":false}}}}"#
    );
    let login = app.clone().oneshot(rpc_req(&login_body)).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (cookie, v["data"]["id"].as_str().unwrap().to_string())
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

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
    );
    let v = rpc_json(app, Some(cookie), &body).await;
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

async fn create_pull(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    repo: &str,
    title: &str,
    branch: &str,
) -> i64 {
    let br = rpc_json(
        app,
        Some(cookie),
        &format!(
            r#"{{"procedure":"repo.branchCreate","input":{{"owner":"{owner}","name":"{repo}","branch":"{branch}","start":"main"}}}}"#
        ),
    )
    .await;
    assert_eq!(br["ok"], true, "branchCreate {branch} — {br}");
    let v = rpc_json(
        app,
        Some(cookie),
        &format!(
            r#"{{"procedure":"pull.create","input":{{"owner":"{owner}","name":"{repo}","title":"{title}","body":"","base_ref":"main","head_ref":"{branch}"}}}}"#
        ),
    )
    .await;
    assert_eq!(v["ok"], true, "pull.create {title} — {v}");
    v["data"]["number"].as_i64().unwrap()
}

async fn pin(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    repo: &str,
    n: i64,
) -> serde_json::Value {
    let body = format!(
        r#"{{"procedure":"pull.pin","input":{{"owner":"{owner}","name":"{repo}","number":{n}}}}}"#
    );
    rpc_json(app, Some(cookie), &body).await
}

async fn unpin(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    repo: &str,
    n: i64,
) -> serde_json::Value {
    let body = format!(
        r#"{{"procedure":"pull.unpin","input":{{"owner":"{owner}","name":"{repo}","number":{n}}}}}"#
    );
    rpc_json(app, Some(cookie), &body).await
}

async fn pull_list(
    app: &axum::Router,
    cookie: Option<&str>,
    owner: &str,
    repo: &str,
    extra: &str,
) -> serde_json::Value {
    let body = format!(
        r#"{{"procedure":"pull.list","input":{{"owner":"{owner}","name":"{repo}"{extra}}}}}"#
    );
    rpc_json(app, cookie, &body).await
}

fn pinned_numbers(v: &serde_json::Value) -> Vec<i64> {
    v["data"]["pinned"]
        .as_array()
        .map(|a| a.iter().map(|i| i["number"].as_i64().unwrap()).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn pull_pin_round_trip_order_cap_and_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pull_pin_rt.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "pp@ex.com", "ppown").await;
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "ppbox").await;
    let n1 = create_pull(&app, &cookie, "ppown", "ppbox", "one", "f1").await;
    let n2 = create_pull(&app, &cookie, "ppown", "ppbox", "two", "f2").await;
    let n3 = create_pull(&app, &cookie, "ppown", "ppbox", "three", "f3").await;
    let n4 = create_pull(&app, &cookie, "ppown", "ppbox", "four", "f4").await;

    for n in [n3, n1, n2] {
        let v = pin(&app, &cookie, "ppown", "ppbox", n).await;
        assert_eq!(v["ok"], true, "pin #{n} — {v}");
        assert_eq!(v["data"]["pinned"], true, "pin #{n} — {v}");
    }

    // pull.get exposes the flag.
    let body = format!(
        r#"{{"procedure":"pull.get","input":{{"owner":"ppown","name":"ppbox","number":{n3}}}}}"#
    );
    let v = rpc_json(&app, None, &body).await;
    assert_eq!(v["data"]["pinned"], true, "{v}");

    // Default open list carries the pinned box in pin order.
    let v = pull_list(&app, None, "ppown", "ppbox", "").await;
    assert_eq!(pinned_numbers(&v), vec![n3, n1, n2], "{v}");
    assert_eq!(v["data"]["pulls"].as_array().unwrap().len(), 4, "{v}");

    // Cap is three (GitHub parity).
    let v = pin(&app, &cookie, "ppown", "ppbox", n4).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "pull.too_many_pins", "{v}");

    // Unpin renumbers the survivors.
    let v = unpin(&app, &cookie, "ppown", "ppbox", n1).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["pinned"], false, "{v}");
    let v = pull_list(&app, None, "ppown", "ppbox", "").await;
    assert_eq!(pinned_numbers(&v), vec![n3, n2], "{v}");

    let v = pin(&app, &cookie, "ppown", "ppbox", n4).await;
    assert_eq!(v["ok"], true, "{v}");
    let v = pull_list(&app, None, "ppown", "ppbox", "").await;
    assert_eq!(pinned_numbers(&v), vec![n3, n2, n4], "{v}");
}

#[tokio::test]
async fn pull_list_pinned_only_on_default_view() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pull_pin_view.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "pv@ex.com", "pvown").await;
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "pvbox").await;
    let n1 = create_pull(&app, &cookie, "pvown", "pvbox", "needle", "p1").await;
    pin(&app, &cookie, "pvown", "pvbox", n1).await;

    let v = pull_list(&app, None, "pvown", "pvbox", r#","state":"closed""#).await;
    assert!(pinned_numbers(&v).is_empty(), "{v}");

    let v = pull_list(&app, None, "pvown", "pvbox", r#","q":"needle""#).await;
    assert!(pinned_numbers(&v).is_empty(), "{v}");

    let v = pull_list(&app, None, "pvown", "pvbox", r#","author":"pvown""#).await;
    assert!(pinned_numbers(&v).is_empty(), "{v}");
}

#[tokio::test]
async fn pull_pin_requires_write_capability() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pull_pin_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "pw@ex.com", "pwown").await;
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "pwbox").await;
    let n1 = create_pull(&app, &cookie, "pwown", "pwbox", "first", "w1").await;

    // Anonymous → auth gate.
    let body = format!(
        r#"{{"procedure":"pull.pin","input":{{"owner":"pwown","name":"pwbox","number":{n1}}}}}"#
    );
    let v = rpc_json(&app, None, &body).await;
    assert_eq!(v["ok"], false, "{v}");

    // Verified reader (PR author, no Write) → soft not_found.
    let (reader_cookie, reader_id) = signup_and_login(&app, "pr@ex.com", "preader").await;
    verify_user(&db, &reader_id).await;
    let v = pin(&app, &reader_cookie, "pwown", "pwbox", n1).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "repo.not_found", "{v}");
}
