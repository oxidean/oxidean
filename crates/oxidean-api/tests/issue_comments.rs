//! ISS-02 comment CRUD, author edit/delete, Write+ moderate, history.
//!
//! Covers D-ISS-09, D-ISS-12; Write+ create/comment per D-ISS-20.

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

async fn create_issue(app: &axum::Router, cookie: &str, owner: &str, name: &str) {
    let body = format!(
        r#"{{"procedure":"issue.create","input":{{"owner":"{owner}","name":"{name}","title":"Talk","body":"root"}}}}"#
    );
    let create = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    let bytes = create.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "issue.create — {v}");
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

/// Write+ can create a comment on an issue (ISS-02 / D-ISS-20).
#[tokio::test]
async fn issue_comments_create() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_comments_create.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "cmtown@ex.com", "cmtown").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "talk", "public").await;
    create_issue(&app, &cookie, "cmtown", "talk").await;

    let created = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.comments.create","input":{"owner":"cmtown","name":"talk","number":1,"body":"Hello thread"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "comment.create — {created}");
    assert_eq!(created["data"]["body"], "Hello thread");
    assert_eq!(created["data"]["author_username"], "cmtown");
    let comment_id = created["data"]["id"].as_str().expect("id");

    let listed = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.comments.list","input":{"owner":"cmtown","name":"talk","number":1}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "comment.list — {listed}");
    assert_eq!(listed["data"]["comments"].as_array().unwrap().len(), 1);
    assert_eq!(listed["data"]["comments"][0]["id"], comment_id);
}

/// Author can edit own comment body (ISS-02 / D-ISS-09).
#[tokio::test]
async fn issue_comments_author_edit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_comments_edit.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "edcmt@ex.com", "edcmt").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "edits", "public").await;
    create_issue(&app, &owner_cookie, "edcmt", "edits").await;

    let (writer_cookie, writer_v) = signup_and_login(&app, "edwriter@ex.com", "edwriter").await;
    let writer_id = writer_v["data"]["id"].as_str().expect("id");
    verify_user(&db, writer_id).await;
    let add = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"edcmt","name":"edits","username":"edwriter","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "add write collab — {add}");

    let created = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"issue.comments.create","input":{"owner":"edcmt","name":"edits","number":1,"body":"v1"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let comment_id = created["data"]["id"].as_str().expect("id");

    let updated = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"issue.comments.update","input":{{"owner":"edcmt","name":"edits","number":1,"commentId":"{comment_id}","body":"v2"}}}}"#
        ),
    )
    .await;
    assert_eq!(updated["ok"], true, "author can edit — {updated}");
    assert_eq!(updated["data"]["body"], "v2");

    // Owner (Write+) cannot edit another's comment (D-ISS-09 — edit is author-only).
    let denied = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"issue.comments.update","input":{{"owner":"edcmt","name":"edits","number":1,"commentId":"{comment_id}","body":"hijack"}}}}"#
        ),
    )
    .await;
    assert_eq!(denied["ok"], false, "non-author cannot edit — {denied}");
    assert_eq!(
        denied["error"]["code"], "repo.not_found",
        "soft deny — {denied}"
    );
}

/// Author can delete own comment (ISS-02 / D-ISS-09).
#[tokio::test]
async fn issue_comments_author_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_comments_adel.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "delown@ex.com", "delown").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "gone", "public").await;
    create_issue(&app, &cookie, "delown", "gone").await;

    let created = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.comments.create","input":{"owner":"delown","name":"gone","number":1,"body":"temp"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let comment_id = created["data"]["id"].as_str().expect("id");

    let deleted = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"issue.comments.delete","input":{{"owner":"delown","name":"gone","number":1,"commentId":"{comment_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(deleted["ok"], true, "author delete — {deleted}");

    let listed = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.comments.list","input":{"owner":"delown","name":"gone","number":1}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    assert_eq!(listed["data"]["comments"].as_array().unwrap().len(), 0);
}

/// Write+ can moderate-delete another user's comment (ISS-02 / D-ISS-09 / T-11-10).
#[tokio::test]
async fn issue_comments_write_moderate_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_comments_mod.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "modown@ex.com", "modown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "mod", "public").await;
    create_issue(&app, &owner_cookie, "modown", "mod").await;

    let (writer_cookie, writer_v) = signup_and_login(&app, "modwrite@ex.com", "modwrite").await;
    let writer_id = writer_v["data"]["id"].as_str().expect("id");
    verify_user(&db, writer_id).await;
    let add_w = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"modown","name":"mod","username":"modwrite","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add_w["ok"], true, "{add_w}");

    let (reader_cookie, reader_v) = signup_and_login(&app, "modread@ex.com", "modread").await;
    let reader_id = reader_v["data"]["id"].as_str().expect("id");
    verify_user(&db, reader_id).await;
    let add_r = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"modown","name":"mod","username":"modread","permission":"read"}}"#,
    )
    .await;
    assert_eq!(add_r["ok"], true, "{add_r}");

    let created = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"issue.comments.create","input":{"owner":"modown","name":"mod","number":1,"body":"please remove"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let comment_id = created["data"]["id"].as_str().expect("id");

    // Read cannot moderate-delete.
    let read_denied = rpc_json(
        &app,
        &reader_cookie,
        &format!(
            r#"{{"procedure":"issue.comments.delete","input":{{"owner":"modown","name":"mod","number":1,"commentId":"{comment_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        read_denied["ok"], false,
        "read cannot delete — {read_denied}"
    );
    assert_eq!(
        read_denied["error"]["code"], "repo.not_found",
        "soft deny — {read_denied}"
    );

    // Owner (Write+) can moderate-delete.
    let moderated = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"issue.comments.delete","input":{{"owner":"modown","name":"mod","number":1,"commentId":"{comment_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(
        moderated["ok"], true,
        "Write+ moderate delete — {moderated}"
    );

    let listed = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.comments.list","input":{"owner":"modown","name":"mod","number":1}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    assert_eq!(listed["data"]["comments"].as_array().unwrap().len(), 0);
}

/// Full edit history on comments (ISS-02 / D-ISS-12).
#[tokio::test]
async fn issue_comments_edit_history_trail() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_comments_hist.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "histcmt@ex.com", "histcmt").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "trail", "public").await;
    create_issue(&app, &cookie, "histcmt", "trail").await;

    let created = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.comments.create","input":{"owner":"histcmt","name":"trail","number":1,"body":"first"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let comment_id = created["data"]["id"].as_str().expect("id");

    let u1 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"issue.comments.update","input":{{"owner":"histcmt","name":"trail","number":1,"commentId":"{comment_id}","body":"second"}}}}"#
        ),
    )
    .await;
    assert_eq!(u1["ok"], true, "{u1}");

    let u2 = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"issue.comments.update","input":{{"owner":"histcmt","name":"trail","number":1,"commentId":"{comment_id}","body":"third"}}}}"#
        ),
    )
    .await;
    assert_eq!(u2["ok"], true, "{u2}");
    assert_eq!(u2["data"]["body"], "third");

    let hist = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"issue.comments.history","input":{{"owner":"histcmt","name":"trail","number":1,"commentId":"{comment_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(hist["ok"], true, "comment history — {hist}");
    let revs = hist["data"]["revisions"].as_array().expect("revisions");
    assert_eq!(revs.len(), 2, "two prior bodies — {hist}");
    assert_eq!(revs[0]["body"], "first");
    assert_eq!(revs[1]["body"], "second");
    assert_eq!(revs[0]["editor_username"], "histcmt");
}
