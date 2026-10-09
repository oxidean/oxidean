//! D-11 / D-12 / D-13: Classic commit status create/list.

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
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
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

/// Write+ upserts status by (sha, context); list returns latest-wins (D-11..13).
#[tokio::test]
async fn commit_status_rpc_create_list_latest_wins() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("cs.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (cookie, login) = signup_and_login(&app, "s@ex.com", "stown").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let pending = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.commitStatus.create","input":{{"owner":"stown","name":"core","sha":"{sha}","context":"ci/test","state":"pending"}}}}"#
        ),
    )
    .await;
    assert_eq!(pending["ok"], true, "{pending}");
    assert_eq!(pending["data"]["state"], "pending");

    let success = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.commitStatus.create","input":{{"owner":"stown","name":"core","sha":"{sha}","context":"ci/test","state":"success","description":"ok"}}}}"#
        ),
    )
    .await;
    assert_eq!(success["ok"], true, "{success}");
    assert_eq!(success["data"]["state"], "success");

    let listed = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.commitStatus.list","input":{{"owner":"stown","name":"core","sha":"{sha}"}}}}"#
        ),
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    let statuses = listed["data"]["statuses"].as_array().unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0]["state"], "success");
}

/// Read+ can list; insufficient cannot create (D-13).
#[tokio::test]
async fn commit_status_rpc_acl_gates() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("cs_acl.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (owner_cookie, owner_login) = signup_and_login(&app, "o@ex.com", "csown").await;
    verify_user(&db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"priv","visibility":"private","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let (reader_cookie, reader_login) = signup_and_login(&app, "r@ex.com", "csread").await;
    verify_user(&db, reader_login["data"]["id"].as_str().unwrap()).await;
    let add = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"csown","name":"priv","username":"csread","permission":"read"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");

    let sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let denied = rpc_json(
        &app,
        &reader_cookie,
        &format!(
            r#"{{"procedure":"repo.commitStatus.create","input":{{"owner":"csown","name":"priv","sha":"{sha}","context":"ci","state":"success"}}}}"#
        ),
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found");
}
