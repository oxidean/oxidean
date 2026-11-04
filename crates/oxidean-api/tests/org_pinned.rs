//! COL-04 — `org.listPinnedRepos` / `org.setPinnedRepos`: ordered pins on
//! `/{org}` (≤6), Admin+ only, org-owned repos only, anonymous viewers see
//! public pins only.

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

async fn create_org(app: &axum::Router, cookie: &str, slug: &str) -> serde_json::Value {
    let body = format!(r#"{{"procedure":"org.create","input":{{"slug":"{slug}"}}}}"#);
    let v = rpc_json(app, Some(cookie), &body).await;
    assert_eq!(v["ok"], true, "org.create — {v}");
    v
}

async fn create_org_repo(
    app: &axum::Router,
    cookie: &str,
    org: &str,
    name: &str,
    visibility: &str,
) -> String {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"owner":"{org}","name":"{name}","visibility":"{visibility}","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
    );
    let v = rpc_json(app, Some(cookie), &body).await;
    assert_eq!(v["ok"], true, "repo.create {name} — {v}");
    v["data"]["id"].as_str().unwrap().to_string()
}

async fn set_pins(
    app: &axum::Router,
    cookie: &str,
    slug: &str,
    ids: &[String],
) -> serde_json::Value {
    let ids_json = ids
        .iter()
        .map(|i| format!(r#""{i}""#))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        r#"{{"procedure":"org.setPinnedRepos","input":{{"slug":"{slug}","repo_ids":[{ids_json}]}}}}"#
    );
    rpc_json(app, Some(cookie), &body).await
}

async fn list_pins(app: &axum::Router, cookie: Option<&str>, slug: &str) -> serde_json::Value {
    let body = format!(r#"{{"procedure":"org.listPinnedRepos","input":{{"slug":"{slug}"}}}}"#);
    rpc_json(app, cookie, &body).await
}

#[tokio::test]
async fn org_pinned_round_trip_order_and_admin_gate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("org_pins_rt.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "oo@ex.com", "owneru").await;
    verify_user(&db, &user_id).await;
    create_org(&app, &cookie, "pin-org").await;
    let org = db
        .find_organization_by_slug("pin-org")
        .await
        .expect("find org")
        .expect("org row");

    let a = create_org_repo(&app, &cookie, "pin-org", "alpha", "public").await;
    let b = create_org_repo(&app, &cookie, "pin-org", "beta", "public").await;
    let _c = create_org_repo(&app, &cookie, "pin-org", "gamma", "public").await;

    // A plain member cannot edit pins.
    let (member_cookie, member_id) = signup_and_login(&app, "mm@ex.com", "memberu").await;
    verify_user(&db, &member_id).await;
    db.insert_org_member(&org.id, &member_id, "member")
        .await
        .expect("insert member");
    let v = set_pins(&app, &member_cookie, "pin-org", std::slice::from_ref(&a)).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "org.forbidden", "{v}");

    // An admin member can.
    let (admin_cookie, admin_id) = signup_and_login(&app, "aa@ex.com", "adminu").await;
    verify_user(&db, &admin_id).await;
    db.insert_org_member(&org.id, &admin_id, "admin")
        .await
        .expect("insert admin");
    let v = set_pins(&app, &admin_cookie, "pin-org", &[b.clone(), a.clone()]).await;
    assert_eq!(v["ok"], true, "{v}");
    let names: Vec<&str> = v["data"]["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["beta", "alpha"], "{v}");

    // Anonymous read sees the ordered list.
    let v = list_pins(&app, None, "pin-org").await;
    assert_eq!(v["ok"], true, "{v}");
    let names: Vec<&str> = v["data"]["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["beta", "alpha"], "{v}");

    // Re-set replaces atomically.
    let v = set_pins(&app, &cookie, "pin-org", std::slice::from_ref(&a)).await;
    assert_eq!(v["data"]["repos"].as_array().unwrap().len(), 1, "{v}");
}

#[tokio::test]
async fn org_pinned_validation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("org_pins_val.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "vo@ex.com", "vownu").await;
    verify_user(&db, &user_id).await;
    create_org(&app, &cookie, "val-org").await;
    let own = create_org_repo(&app, &cookie, "val-org", "orgrepo", "public").await;

    // Cap is six (same as user pins).
    let mut many = Vec::new();
    for i in 0..7 {
        many.push(create_org_repo(&app, &cookie, "val-org", &format!("r{i}"), "public").await);
    }
    let v = set_pins(&app, &cookie, "val-org", &many).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "org.too_many_pins", "{v}");

    // Repos owned by someone else (not the org) cannot be pinned.
    let (other_cookie, other_id) = signup_and_login(&app, "ou@ex.com", "otheru").await;
    verify_user(&db, &other_id).await;
    let foreign = {
        let body = r#"{"procedure":"repo.create","input":{"name":"foreign","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#;
        let v = rpc_json(&app, Some(&other_cookie), body).await;
        assert_eq!(v["ok"], true, "{v}");
        v["data"]["id"].as_str().unwrap().to_string()
    };
    let v = set_pins(&app, &cookie, "val-org", &[own.clone(), foreign]).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "repo.forbidden", "{v}");

    // Unknown repo id → repo.not_found.
    let v = set_pins(
        &app,
        &cookie,
        "val-org",
        &[
            own.clone(),
            "00000000-0000-0000-0000-000000000000".to_string(),
        ],
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "repo.not_found", "{v}");

    // Unknown org → org.not_found (list and set).
    let v = list_pins(&app, None, "no-org").await;
    assert_eq!(v["error"]["code"], "org.not_found", "{v}");
    let v = set_pins(&app, &cookie, "no-org", &[own]).await;
    assert_eq!(v["error"]["code"], "org.not_found", "{v}");
}

#[tokio::test]
async fn org_pinned_private_hidden_from_outsiders() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("org_pins_priv.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, user_id) = signup_and_login(&app, "po@ex.com", "pownu").await;
    verify_user(&db, &user_id).await;
    create_org(&app, &cookie, "priv-org").await;
    let public_repo = create_org_repo(&app, &cookie, "priv-org", "pubpin", "public").await;
    let private_repo = create_org_repo(&app, &cookie, "priv-org", "privpin", "private").await;
    let v = set_pins(&app, &cookie, "priv-org", &[public_repo, private_repo]).await;
    assert_eq!(v["ok"], true, "{v}");

    // Owner sees both pins.
    let v = list_pins(&app, Some(&cookie), "priv-org").await;
    assert_eq!(v["data"]["repos"].as_array().unwrap().len(), 2, "{v}");

    // Anonymous sees only the public pin.
    let v = list_pins(&app, None, "priv-org").await;
    let repos = v["data"]["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 1, "{v}");
    assert_eq!(repos[0]["name"], "pubpin", "{v}");
}
