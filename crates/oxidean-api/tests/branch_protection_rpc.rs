//! ORG-05 / D-03 / D-26: Admin branch protection rule CRUD.

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
    res.headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

async fn signup_and_login(
    app: &axum::Router,
    email: &str,
    username: &str,
) -> (String, serde_json::Value) {
    let signup = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;
    let login = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (cookie, v)
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now).await.expect("verify");
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

/// Admin can create/list/update/delete a rule (ORG-05, D-03, D-26).
#[tokio::test]
async fn branch_protection_rpc_admin_crud() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("bp_crud.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (cookie, login) = signup_and_login(&app, "a@ex.com", "adminu").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let created = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"adminu","name":"core","pattern":"release/*","require_reviews":true,"required_approving_review_count":2,"required_status_contexts":["ci"],"enforce_admins":true}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["data"]["pattern"], "release/*");
    assert_eq!(created["data"]["required_approving_review_count"], 2);
    // GIT-22: field round-trips (default off).
    assert_eq!(created["data"]["require_signed_commits"], false);
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let listed = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.branchProtection.list","input":{"owner":"adminu","name":"core"}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    assert_eq!(listed["data"]["rules"].as_array().unwrap().len(), 1);

    let updated = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.branchProtection.update","input":{{"owner":"adminu","name":"core","id":"{id}","pattern":"main","require_reviews":true,"required_approving_review_count":1,"lock_branch":true,"require_signed_commits":true}}}}"#
        ),
    )
    .await;
    assert_eq!(updated["ok"], true, "{updated}");
    assert_eq!(updated["data"]["pattern"], "main");
    assert_eq!(updated["data"]["lock_branch"], true);
    assert_eq!(updated["data"]["require_signed_commits"], true);

    let deleted = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.branchProtection.delete","input":{{"owner":"adminu","name":"core","id":"{id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(deleted["ok"], true, "{deleted}");
}

/// Non-Admin mutate is soft-denied (Admin gate D-03).
#[tokio::test]
async fn branch_protection_rpc_non_admin_denied() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("bp_deny.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (owner_cookie, owner_login) = signup_and_login(&app, "o@ex.com", "ownu").await;
    verify_user(&db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let (writer_cookie, writer_login) = signup_and_login(&app, "w@ex.com", "writeu").await;
    verify_user(&db, writer_login["data"]["id"].as_str().unwrap()).await;
    let add = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"ownu","name":"core","username":"writeu","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");

    let denied = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"ownu","name":"core","pattern":"main","require_reviews":true}}"#,
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found");
}
