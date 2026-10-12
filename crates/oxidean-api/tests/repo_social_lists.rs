//! `repo.stargazers.list` / `repo.watchers.list` / `repo.forks.list` ACL + empty + sort.

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
async fn social_lists_empty_and_acl() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("social-lists.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).unwrap();
    let app = test_app(db.clone(), repos_dir).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "owner@ex.com", "listown").await;
    let owner_id = owner_login["data"]["id"].as_str().unwrap();
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "hello", "public").await;

    let got = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.get","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(got["ok"], true, "anonymous repo.get public — {got}");

    let (outsider_cookie, outsider_login) = signup_and_login(&app, "out@ex.com", "listout").await;
    let outsider_id = outsider_login["data"]["id"].as_str().unwrap();
    verify_user(&db, outsider_id).await;

    // Empty watchers / forks for anyone who can see public repo.
    let empty_watch = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.watchers.list","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(empty_watch["ok"], true, "{empty_watch}");
    assert_eq!(empty_watch["data"]["total"], 0);
    assert!(empty_watch["data"]["watchers"]
        .as_array()
        .unwrap()
        .is_empty());

    let empty_forks = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.forks.list","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(empty_forks["ok"], true, "{empty_forks}");
    assert_eq!(empty_forks["data"]["total"], 0);

    // Stargazers: anonymous / Read-only → soft not_found (Write+ gate).
    let anon_stars = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.stargazers.list","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(anon_stars["ok"], false);
    assert_eq!(anon_stars["error"]["code"], "repo.not_found");

    let out_stars = rpc_json(
        &app,
        Some(&outsider_cookie),
        r#"{"procedure":"repo.stargazers.list","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(out_stars["ok"], false);
    assert_eq!(out_stars["error"]["code"], "repo.not_found");

    // Owner (Write+) can list empty stargazers.
    let owner_stars = rpc_json(
        &app,
        Some(&owner_cookie),
        r#"{"procedure":"repo.stargazers.list","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(owner_stars["ok"], true, "{owner_stars}");
    assert_eq!(owner_stars["data"]["total"], 0);

    // Populate: outsider stars + watches; then forks.
    let star = rpc_json(
        &app,
        Some(&outsider_cookie),
        r#"{"procedure":"repo.star","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(star["ok"], true, "{star}");
    let watch = rpc_json(
        &app,
        Some(&outsider_cookie),
        r#"{"procedure":"repo.watch","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(watch["ok"], true, "{watch}");

    let listed_stars = rpc_json(
        &app,
        Some(&owner_cookie),
        r#"{"procedure":"repo.stargazers.list","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(listed_stars["ok"], true, "{listed_stars}");
    assert_eq!(listed_stars["data"]["total"], 1);
    assert_eq!(listed_stars["data"]["stargazers"][0]["username"], "listout");

    let listed_watch = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.watchers.list","input":{"owner":"listown","name":"hello","q":"listout"}}"#,
    )
    .await;
    assert_eq!(listed_watch["ok"], true, "{listed_watch}");
    assert_eq!(listed_watch["data"]["total"], 1);

    let no_match = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.watchers.list","input":{"owner":"listown","name":"hello","q":"zzz-nomatch"}}"#,
    )
    .await;
    assert_eq!(no_match["ok"], true);
    assert_eq!(no_match["data"]["total"], 0);

    let fork = rpc_json(
        &app,
        Some(&outsider_cookie),
        r#"{"procedure":"repo.fork","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(fork["ok"], true, "{fork}");

    let forks_stars = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.forks.list","input":{"owner":"listown","name":"hello","sort":"stars"}}"#,
    )
    .await;
    assert_eq!(forks_stars["ok"], true, "{forks_stars}");
    assert_eq!(forks_stars["data"]["total"], 1);
    assert_eq!(forks_stars["data"]["forks"][0]["owner_username"], "listout");

    let forks_created = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.forks.list","input":{"owner":"listown","name":"hello","sort":"created"}}"#,
    )
    .await;
    assert_eq!(forks_created["ok"], true, "{forks_created}");
    assert_eq!(forks_created["data"]["total"], 1);

    // Private fork must not appear on the public forks list (ACL leak guard).
    let hide = rpc_json(
        &app,
        Some(&outsider_cookie),
        r#"{"procedure":"repo.updateVisibility","input":{"owner":"listout","name":"hello","visibility":"private"}}"#,
    )
    .await;
    assert_eq!(hide["ok"], true, "{hide}");
    let forks_after_private = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.forks.list","input":{"owner":"listown","name":"hello"}}"#,
    )
    .await;
    assert_eq!(forks_after_private["ok"], true, "{forks_after_private}");
    assert_eq!(forks_after_private["data"]["total"], 0);
    assert!(forks_after_private["data"]["forks"]
        .as_array()
        .unwrap()
        .is_empty());

    // Private: anonymous cannot list watchers.
    create_repo(&app, &owner_cookie, "secret", "private").await;
    let priv_watch = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.watchers.list","input":{"owner":"listown","name":"secret"}}"#,
    )
    .await;
    assert_eq!(priv_watch["ok"], false);
    assert_eq!(priv_watch["error"]["code"], "repo.not_found");
}
