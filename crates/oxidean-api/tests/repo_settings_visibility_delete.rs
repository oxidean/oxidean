//! D-26 / D-35: owner visibility toggle + soft-delete (confirm name); non-owner → not_found.

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

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("rpc json body")
}

async fn seed_owner_repo(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
    repo: &str,
    visibility: &str,
) -> String {
    let (cookie, login_v) = signup_and_login(app, email, username).await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let create = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{repo}","visibility":"{visibility}","description":""}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "seed create — {create}");
    cookie
}

/// Owner flips private→public and reverse (D-26).
#[tokio::test]
async fn repo_settings_owner_toggles_visibility() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("repo_vis_toggle.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = seed_owner_repo(&app, &db, "vis@ex.com", "visowner", "toggle-me", "private").await;

    let to_public = rpc_json(
        &app,
        r#"{"procedure":"repo.updateVisibility","input":{"owner":"visowner","name":"toggle-me","visibility":"public"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(
        to_public["ok"], true,
        "owner private→public must succeed — {to_public}"
    );
    assert_eq!(to_public["data"]["visibility"], "public");

    let get_public = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"visowner","name":"toggle-me"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(get_public["ok"], true);
    assert_eq!(get_public["data"]["visibility"], "public");

    let to_private = rpc_json(
        &app,
        r#"{"procedure":"repo.updateVisibility","input":{"owner":"visowner","name":"toggle-me","visibility":"private"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(
        to_private["ok"], true,
        "owner public→private must succeed — {to_private}"
    );
    assert_eq!(to_private["data"]["visibility"], "private");
}

/// Non-owner visibility update → identical not_found (D-26 / T-07-23).
#[tokio::test]
async fn repo_settings_non_owner_update_visibility_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_vis_non_owner.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let _owner = seed_owner_repo(&app, &db, "own@ex.com", "repoown", "shared-pub", "public").await;

    let (stranger_cookie, stranger_v) =
        signup_and_login(&app, "stranger@ex.com", "stranger1").await;
    let stranger_id = stranger_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&stranger_id, &now)
        .await
        .expect("verify stranger");

    let update = rpc_json(
        &app,
        r#"{"procedure":"repo.updateVisibility","input":{"owner":"repoown","name":"shared-pub","visibility":"private"}}"#,
        &stranger_cookie,
    )
    .await;
    assert_eq!(update["ok"], false, "non-owner must fail — {update}");
    assert_eq!(
        update["error"]["code"], "repo.not_found",
        "unified not_found — {update}"
    );
}

/// Soft-delete sets deleted_at; get/list omit; bare dir remains (D-35).
#[tokio::test]
async fn repo_settings_soft_delete_hides_row_keeps_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("repo_soft_delete.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let cookie = seed_owner_repo(&app, &db, "del@ex.com", "delowner", "goner", "public").await;

    let bare = repos.join("delowner").join("goner.git");
    assert!(
        bare.exists(),
        "bare repo must exist before soft-delete at {}",
        bare.display()
    );

    let wrong = rpc_json(
        &app,
        r#"{"procedure":"repo.softDelete","input":{"owner":"delowner","name":"goner","confirmName":"wrong-name"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(wrong["ok"], false, "mismatched confirm must fail — {wrong}");
    assert_eq!(
        wrong["error"]["code"], "repo.confirm_mismatch",
        "stable confirm mismatch code — {wrong}"
    );

    let deleted = rpc_json(
        &app,
        r#"{"procedure":"repo.softDelete","input":{"owner":"delowner","name":"goner","confirmName":"goner"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(
        deleted["ok"], true,
        "owner soft-delete with matching name — {deleted}"
    );

    let get = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"delowner","name":"goner"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(get["ok"], false, "get must omit soft-deleted — {get}");
    assert_eq!(get["error"]["code"], "repo.not_found");

    let list = rpc_json(&app, r#"{"procedure":"repo.listMine","input":{}}"#, &cookie).await;
    assert_eq!(list["ok"], true, "listMine — {list}");
    let names: Vec<&str> = list["data"]["repos"]
        .as_array()
        .expect("repos")
        .iter()
        .filter_map(|r| r["name"].as_str())
        .collect();
    assert!(
        !names.contains(&"goner"),
        "listMine must omit soft-deleted — {names:?}"
    );

    assert!(
        bare.exists(),
        "disk bare repo must remain after soft-delete (purge deferred) at {}",
        bare.display()
    );
}
