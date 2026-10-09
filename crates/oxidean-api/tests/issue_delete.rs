//! ISS-01 Admin hard-delete with confirmNumber; no #N reuse (D-ISS-02 / D-ISS-20).
//!
//! Threat: T-11-08 — Admin-only hard-delete + typed confirm.

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
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"{visibility}","description":""}}}}"#
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

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Repo Admin hard-deletes with typed `confirmNumber` matching `#N` (D-ISS-02).
#[tokio::test]
async fn issue_delete_admin_hard_delete_requires_confirm_number() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_del_confirm.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "delown@ex.com", "delown").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "gone", "public").await;

    let created = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"delown","name":"gone","title":"Delete me"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["data"]["number"], 1);

    let mismatch = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.delete","input":{"owner":"delown","name":"gone","number":1,"confirmNumber":99}}"#,
    )
    .await;
    assert_eq!(mismatch["ok"], false, "wrong confirm — {mismatch}");
    assert_eq!(
        mismatch["error"]["code"], "issue.confirm_mismatch",
        "{mismatch}"
    );

    let still = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.get","input":{"owner":"delown","name":"gone","number":1}}"#,
    )
    .await;
    assert_eq!(still["ok"], true, "still present after mismatch — {still}");

    let deleted = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.delete","input":{"owner":"delown","name":"gone","number":1,"confirmNumber":1}}"#,
    )
    .await;
    assert_eq!(deleted["ok"], true, "admin delete — {deleted}");
    assert_eq!(deleted["data"]["number"], 1);

    let missing = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.get","input":{"owner":"delown","name":"gone","number":1}}"#,
    )
    .await;
    assert_eq!(missing["ok"], false, "gone after delete — {missing}");
    assert_eq!(missing["error"]["code"], "issue.not_found");
}

/// Non-Admin Write cannot hard-delete (D-ISS-20 / T-11-08).
#[tokio::test]
async fn issue_delete_write_role_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("issue_del_write.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "deladmin@ex.com", "deladmin").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "guarded", "public").await;

    let created = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.create","input":{"owner":"deladmin","name":"guarded","title":"Keep"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");

    let (writer_cookie, writer_v) = signup_and_login(&app, "delwrite@ex.com", "delwrite").await;
    let writer_id = writer_v["data"]["id"].as_str().expect("id");
    verify_user(&db, writer_id).await;
    let add = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"deladmin","name":"guarded","username":"delwrite","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "add write collab — {add}");

    let denied = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"issue.delete","input":{"owner":"deladmin","name":"guarded","number":1,"confirmNumber":1}}"#,
    )
    .await;
    assert_eq!(denied["ok"], false, "write cannot delete — {denied}");
    assert_eq!(
        denied["error"]["code"], "repo.not_found",
        "soft deny — {denied}"
    );

    let still = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.get","input":{"owner":"deladmin","name":"guarded","number":1}}"#,
    )
    .await;
    assert_eq!(still["ok"], true, "issue remains — {still}");
}

/// After hard-delete, `#N` is never reused; next create continues counters (D-ISS-01 / D-ISS-02).
#[tokio::test]
async fn issue_delete_number_not_reused_after_hard_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("issue_del_reuse.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "noreuse@ex.com", "noreuse").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "nums", "public").await;

    let c1 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"noreuse","name":"nums","title":"One"}}"#,
    )
    .await;
    assert_eq!(c1["data"]["number"], 1);

    let c2 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"noreuse","name":"nums","title":"Two"}}"#,
    )
    .await;
    assert_eq!(c2["data"]["number"], 2);

    let deleted = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.delete","input":{"owner":"noreuse","name":"nums","number":1,"confirmNumber":1}}"#,
    )
    .await;
    assert_eq!(deleted["ok"], true, "{deleted}");

    let c3 = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"noreuse","name":"nums","title":"Three"}}"#,
    )
    .await;
    assert_eq!(c3["ok"], true, "{c3}");
    assert_eq!(
        c3["data"]["number"], 3,
        "must not reclaim #1 after hard-delete — {c3}"
    );
}
