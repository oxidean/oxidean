//! COL-04 — `issue.pin` / `issue.unpin`: ordered pinned box (≤3), Write+
//! only, `pinned` on `IssuePublic` and `issue.list` for the unfiltered open
//! first page.

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

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
    );
    let v = rpc_json(app, Some(cookie), &body).await;
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

async fn create_issue(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    repo: &str,
    title: &str,
) -> i64 {
    let body = format!(
        r#"{{"procedure":"issue.create","input":{{"owner":"{owner}","name":"{repo}","title":"{title}","body":""}}}}"#
    );
    let v = rpc_json(app, Some(cookie), &body).await;
    assert_eq!(v["ok"], true, "issue.create {title} — {v}");
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
        r#"{{"procedure":"issue.pin","input":{{"owner":"{owner}","name":"{repo}","number":{n}}}}}"#
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
        r#"{{"procedure":"issue.unpin","input":{{"owner":"{owner}","name":"{repo}","number":{n}}}}}"#
    );
    rpc_json(app, Some(cookie), &body).await
}

async fn issue_list(
    app: &axum::Router,
    cookie: Option<&str>,
    owner: &str,
    repo: &str,
    extra: &str,
) -> serde_json::Value {
    let body = format!(
        r#"{{"procedure":"issue.list","input":{{"owner":"{owner}","name":"{repo}"{extra}}}}}"#
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
async fn issue_pin_round_trip_order_cap_and_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("issue_pin_rt.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "ip@ex.com", "ipown").await;
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "ipbox", "public").await;
    let n1 = create_issue(&app, &cookie, "ipown", "ipbox", "one").await;
    let n2 = create_issue(&app, &cookie, "ipown", "ipbox", "two").await;
    let n3 = create_issue(&app, &cookie, "ipown", "ipbox", "three").await;
    let n4 = create_issue(&app, &cookie, "ipown", "ipbox", "four").await;

    // Pin order = call order, not issue order.
    for n in [n3, n1, n2] {
        let v = pin(&app, &cookie, "ipown", "ipbox", n).await;
        assert_eq!(v["ok"], true, "pin #{n} — {v}");
        assert_eq!(v["data"]["pinned"], true, "pin #{n} — {v}");
    }

    // Re-pin is idempotent (position unchanged).
    let v = pin(&app, &cookie, "ipown", "ipbox", n1).await;
    assert_eq!(v["ok"], true, "{v}");

    // issue.get exposes the flag.
    let body = format!(
        r#"{{"procedure":"issue.get","input":{{"owner":"ipown","name":"ipbox","number":{n3}}}}}"#
    );
    let v = rpc_json(&app, None, &body).await;
    assert_eq!(v["data"]["pinned"], true, "{v}");

    // Default open list carries the pinned box in pin order.
    let v = issue_list(&app, None, "ipown", "ipbox", "").await;
    assert_eq!(pinned_numbers(&v), vec![n3, n1, n2], "{v}");
    let titles: Vec<&str> = v["data"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles.len(), 4, "regular list unchanged — {titles:?}");

    // Cap is three (GitHub parity).
    let v = pin(&app, &cookie, "ipown", "ipbox", n4).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "issue.too_many_pins", "{v}");

    // Unpin renumbers the survivors contiguously.
    let v = unpin(&app, &cookie, "ipown", "ipbox", n1).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["pinned"], false, "{v}");
    let v = issue_list(&app, None, "ipown", "ipbox", "").await;
    assert_eq!(pinned_numbers(&v), vec![n3, n2], "{v}");

    // Fourth slot freed — pin now succeeds.
    let v = pin(&app, &cookie, "ipown", "ipbox", n4).await;
    assert_eq!(v["ok"], true, "{v}");
    let v = issue_list(&app, None, "ipown", "ipbox", "").await;
    assert_eq!(pinned_numbers(&v), vec![n3, n2, n4], "{v}");

    // Unpin is idempotent.
    let v = unpin(&app, &cookie, "ipown", "ipbox", n1).await;
    assert_eq!(v["ok"], true, "{v}");
}

#[tokio::test]
async fn issue_list_pinned_only_on_default_view() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("issue_pin_view.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "iv@ex.com", "ivown").await;
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "ivbox", "public").await;
    let n1 = create_issue(&app, &cookie, "ivown", "ivbox", "needle").await;
    pin(&app, &cookie, "ivown", "ivbox", n1).await;

    // State filter hides the box.
    let v = issue_list(&app, None, "ivown", "ivbox", r#","state":"closed""#).await;
    assert!(pinned_numbers(&v).is_empty(), "{v}");

    // Text filter hides the box.
    let v = issue_list(&app, None, "ivown", "ivbox", r#","q":"needle""#).await;
    assert!(pinned_numbers(&v).is_empty(), "{v}");

    // Author filter hides the box.
    let v = issue_list(&app, None, "ivown", "ivbox", r#","author":"ivown""#).await;
    assert!(pinned_numbers(&v).is_empty(), "{v}");

    // Pagination hides the box.
    let v = issue_list(&app, None, "ivown", "ivbox", r#","offset":1"#).await;
    assert!(pinned_numbers(&v).is_empty(), "{v}");
}

#[tokio::test]
async fn issue_pin_requires_write_capability() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("issue_pin_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "iw@ex.com", "iwown").await;
    verify_user(&db, &user_id).await;
    create_repo(&app, &cookie, "iwbox", "public").await;
    let n1 = create_issue(&app, &cookie, "iwown", "iwbox", "first").await;

    // Anonymous → auth gate.
    let body = format!(
        r#"{{"procedure":"issue.pin","input":{{"owner":"iwown","name":"iwbox","number":{n1}}}}}"#
    );
    let v = rpc_json(&app, None, &body).await;
    assert_eq!(v["ok"], false, "{v}");

    // Verified reader without Write → soft not_found (no capability leak).
    let (reader_cookie, reader_id) = signup_and_login(&app, "ir@ex.com", "ireader").await;
    verify_user(&db, &reader_id).await;
    let v = pin(&app, &reader_cookie, "iwown", "iwbox", n1).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "repo.not_found", "{v}");

    // Issue author without Write still cannot pin (pin is maintainer-only).
    let v = unpin(&app, &reader_cookie, "iwown", "iwbox", n1).await;
    assert_eq!(v["error"]["code"], "repo.not_found", "{v}");
}
