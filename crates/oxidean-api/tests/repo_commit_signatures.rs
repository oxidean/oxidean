//! Forge-signed seed commits must report `signature_status=valid` over RPC.
//! Regression: `apply_verified_policy` used to downgrade crypto-valid web-flow
//! signatures to `unknown` because the forge committer resolves to no user.

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

#[tokio::test]
async fn repo_commits_reports_seed_commit_signature_valid() {
    let _env_guard = support::lock_admin_env().await;
    let dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("OXIDEAN_SSH_HOST_KEY_DIR", dir.path().join("ssh"));
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("sig.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "sigowner@ex.com", "sigowner").await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"repo.create","input":{"name":"signed-seed","visibility":"public","license_id":"MIT"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "repo.create must seed a signed commit — {v}");

    let commits = app
        .oneshot(rpc_req(
            r#"{"procedure":"repo.commits","input":{"owner":"sigowner","name":"signed-seed","ref":"main"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(commits.status(), StatusCode::OK);
    let bytes = commits.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let commit = &v["data"]["commits"][0];
    assert_eq!(
        commit["signature_kind"], "ssh",
        "seed commit must be ssh-signed — {v}"
    );
    assert_eq!(
        commit["signature_status"], "valid",
        "forge-signed seed commit must verify — {v}"
    );
}
