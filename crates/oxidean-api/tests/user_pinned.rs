//! COL-04 — `user.listPinnedRepos` / `user.setPinnedRepos`: ordered pins,
//! ≤6 cap, owned-repos-only ACL, anonymous viewers see public pins only,
//! unknown/banned users → `user.not_found`.

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

async fn create_repo(app: &axum::Router, cookie: &str, name: &str, visibility: &str) -> String {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
    );
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.create — {v}");
    v["data"]["id"].as_str().unwrap().to_string()
}

async fn set_pins(app: &axum::Router, cookie: &str, ids: &[String]) -> serde_json::Value {
    let ids_json = ids
        .iter()
        .map(|i| format!(r#""{i}""#))
        .collect::<Vec<_>>()
        .join(",");
    let body =
        format!(r#"{{"procedure":"user.setPinnedRepos","input":{{"repo_ids":[{ids_json}]}}}}"#);
    rpc_json(app, Some(cookie), &body).await
}

async fn list_pins(app: &axum::Router, cookie: Option<&str>, username: &str) -> serde_json::Value {
    let body =
        format!(r#"{{"procedure":"user.listPinnedRepos","input":{{"username":"{username}"}}}}"#);
    rpc_json(app, cookie, &body).await
}

#[tokio::test]
async fn pinned_repos_round_trip_and_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pins_rt.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "pin@ex.com", "pinu").await;
    verify_user(&db, &user_id).await;
    let a = create_repo(&app, &cookie, "alpha", "public").await;
    let b = create_repo(&app, &cookie, "beta", "public").await;
    let _c = create_repo(&app, &cookie, "gamma", "public").await;

    // Pin order is caller order, not creation order.
    let v = set_pins(&app, &cookie, &[b.clone(), a.clone()]).await;
    assert_eq!(v["ok"], true, "{v}");
    let names: Vec<&str> = v["data"]["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["beta", "alpha"], "{v}");

    // Anonymous read sees the same ordered list (all public).
    let v = list_pins(&app, None, "pinu").await;
    assert_eq!(v["ok"], true, "{v}");
    let names: Vec<&str> = v["data"]["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["beta", "alpha"], "{v}");

    // Re-set replaces atomically — unpinning all but one.
    let v = set_pins(&app, &cookie, std::slice::from_ref(&a)).await;
    assert_eq!(v["data"]["repos"].as_array().unwrap().len(), 1, "{v}");
    let v = list_pins(&app, None, "pinu").await;
    let names: Vec<&str> = v["data"]["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["alpha"], "{v}");
}

#[tokio::test]
async fn pinned_repos_private_hidden_from_others() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pins_priv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "pv@ex.com", "pvu").await;
    verify_user(&db, &user_id).await;
    let public_repo = create_repo(&app, &cookie, "pubpin", "public").await;
    let private_repo = create_repo(&app, &cookie, "privpin", "private").await;
    let v = set_pins(&app, &cookie, &[public_repo, private_repo]).await;
    assert_eq!(v["ok"], true, "{v}");

    // Owner sees both pins.
    let v = list_pins(&app, Some(&cookie), "pvu").await;
    assert_eq!(v["data"]["repos"].as_array().unwrap().len(), 2, "{v}");

    // Anonymous and another user see the public pin only.
    let v = list_pins(&app, None, "pvu").await;
    let repos = v["data"]["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 1, "{v}");
    assert_eq!(repos[0]["name"], "pubpin", "{v}");

    let (other_cookie, other_id) = signup_and_login(&app, "ot@ex.com", "otu").await;
    verify_user(&db, &other_id).await;
    let v = list_pins(&app, Some(&other_cookie), "pvu").await;
    let repos = v["data"]["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 1, "{v}");
    assert_eq!(repos[0]["name"], "pubpin", "{v}");
}

#[tokio::test]
async fn pinned_repos_validation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pins_val.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "vl@ex.com", "vlu").await;
    verify_user(&db, &user_id).await;
    let (other_cookie, other_id) = signup_and_login(&app, "vo@ex.com", "vou").await;
    verify_user(&db, &other_id).await;
    let own = create_repo(&app, &cookie, "ownpin", "public").await;
    let other = create_repo(&app, &other_cookie, "otherpin", "public").await;

    // Seven distinct ids exceed the GitHub-parity cap.
    let many: Vec<String> = {
        let mut v = Vec::new();
        for i in 0..7 {
            v.push(create_repo(&app, &cookie, &format!("r{i}"), "public").await);
        }
        v
    };
    let v = set_pins(&app, &cookie, &many).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "user.too_many_pins", "{v}");

    // Someone else's repo cannot be pinned.
    let v = set_pins(&app, &cookie, &[own.clone(), other]).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "repo.forbidden", "{v}");

    // Unknown repo id → repo.not_found.
    let v = set_pins(
        &app,
        &cookie,
        &[own.clone(), "00000000-0000-0000-0000-000000000000".into()],
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "repo.not_found", "{v}");

    // Duplicate ids collapse — two pins, not three.
    let v = set_pins(&app, &cookie, &[own.clone(), own.clone()]).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["repos"].as_array().unwrap().len(), 1, "{v}");
}

#[tokio::test]
async fn pinned_repos_unknown_and_banned_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pins_nf.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let v = list_pins(&app, None, "nouser").await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "user.not_found", "{v}");

    let (cookie, user_id) = signup_and_login(&app, "bn@ex.com", "bnu").await;
    verify_user(&db, &user_id).await;
    let repo = create_repo(&app, &cookie, "bnpin", "public").await;
    set_pins(&app, &cookie, &[repo]).await;

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_user_banned_at(&user_id, &now).await.expect("ban");
    let v = list_pins(&app, None, "bnu").await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "user.not_found", "{v}");
}
