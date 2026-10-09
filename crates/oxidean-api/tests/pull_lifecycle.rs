//! PR-01 / PR-06 pull lifecycle (shared #N, close/reopen).

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

async fn create_branch(app: &axum::Router, cookie: &str, owner: &str, name: &str, branch: &str) {
    let body = format!(
        r#"{{"procedure":"repo.branchCreate","input":{{"owner":"{owner}","name":"{name}","branch":"{branch}","start":"main"}}}}"#
    );
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(&body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "branchCreate — {v}");
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

#[tokio::test]
async fn pull_lifecycle_create_list_get_close_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_life.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "prowner@ex.com", "prowner").await;
    let user_id = login_v["data"]["id"].as_str().expect("id");
    verify_user(&db, user_id).await;
    create_repo(&app, &cookie, "hello", "public").await;
    create_branch(&app, &cookie, "prowner", "hello", "feature").await;

    // Shared #N: issue first → PR gets #2
    let issue = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"prowner","name":"hello","title":"Iss"}}"#,
    )
    .await;
    assert_eq!(issue["ok"], true, "{issue}");
    assert_eq!(issue["data"]["number"], 1);

    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"prowner","name":"hello","title":"My PR","body":"hi","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "pull.create — {create}");
    assert_eq!(create["data"]["number"], 2, "shared #N with issues");
    assert_eq!(create["data"]["state"], "open");
    assert_eq!(create["data"]["title"], "My PR");
    assert_eq!(create["data"]["base_ref"], "main");
    assert_eq!(create["data"]["head_ref"], "feature");

    let get = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.get","input":{"owner":"prowner","name":"hello","number":2}}"#,
    )
    .await;
    assert_eq!(get["ok"], true, "{get}");
    assert_eq!(get["data"]["number"], 2);

    let list = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.list","input":{"owner":"prowner","name":"hello"}}"#,
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");
    assert_eq!(list["data"]["total"], 1);
    assert_eq!(list["data"]["pulls"][0]["number"], 2);

    let close = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.close","input":{"owner":"prowner","name":"hello","number":2}}"#,
    )
    .await;
    assert_eq!(close["ok"], true, "{close}");
    assert_eq!(close["data"]["state"], "closed");

    let reopen = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.reopen","input":{"owner":"prowner","name":"hello","number":2}}"#,
    )
    .await;
    assert_eq!(reopen["ok"], true, "{reopen}");
    assert_eq!(reopen["data"]["state"], "open");
}

#[tokio::test]
async fn pull_lifecycle_rejects_identical_refs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_same.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, login_v) = signup_and_login(&app, "same@ex.com", "sameown").await;
    verify_user(&db, login_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "r", "public").await;

    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.create","input":{"owner":"sameown","name":"r","title":"noop","base_ref":"main","head_ref":"main"}}"#,
    )
    .await;
    assert_eq!(create["ok"], false, "{create}");
}

#[tokio::test]
async fn pull_lifecycle_fork_head_pr() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_fork.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "src@ex.com", "srcown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &owner_cookie, "upstream", "public").await;
    create_branch(&app, &owner_cookie, "srcown", "upstream", "feature").await;

    let (fork_cookie, fork_v) = signup_and_login(&app, "fork@ex.com", "forkown").await;
    verify_user(&db, fork_v["data"]["id"].as_str().unwrap()).await;

    let forked = rpc_json(
        &app,
        &fork_cookie,
        r#"{"procedure":"repo.fork","input":{"owner":"srcown","name":"upstream"}}"#,
    )
    .await;
    assert_eq!(forked["ok"], true, "{forked}");
    assert_eq!(forked["data"]["owner_username"], "forkown");
    assert_eq!(forked["data"]["name"], "upstream");

    // No collaborator grant needed: fork head ⇒ Write+ on head + Read on base (D-PR-29).
    let pr = rpc_json(
        &app,
        &fork_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"srcown","name":"upstream","title":"From fork","base_ref":"main","head_ref":"feature","head_owner":"forkown","head_name":"upstream"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    assert_eq!(pr["data"]["head_owner"], "forkown");

    // Author (Read-only on base) edits and closes their own PR.
    let upd = rpc_json(
        &app,
        &fork_cookie,
        r#"{"procedure":"pull.update","input":{"owner":"srcown","name":"upstream","number":1,"title":"Renamed PR"}}"#,
    )
    .await;
    assert_eq!(upd["ok"], true, "author updates own PR — {upd}");
    let close = rpc_json(
        &app,
        &fork_cookie,
        r#"{"procedure":"pull.close","input":{"owner":"srcown","name":"upstream","number":1}}"#,
    )
    .await;
    assert_eq!(close["ok"], true, "author closes own PR — {close}");
    let reopen = rpc_json(
        &app,
        &fork_cookie,
        r#"{"procedure":"pull.reopen","input":{"owner":"srcown","name":"upstream","number":1}}"#,
    )
    .await;
    assert_eq!(reopen["ok"], true, "author reopens own PR — {reopen}");

    // Verified stranger (no collaborator row) can comment on the PR conversation.
    let (stranger_cookie, stranger_v) =
        signup_and_login(&app, "prstranger@ex.com", "prstranger").await;
    verify_user(&db, stranger_v["data"]["id"].as_str().unwrap()).await;
    let comment = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"pull.comments.create","input":{"owner":"srcown","name":"upstream","number":1,"body":"drive-by review note"}}"#,
    )
    .await;
    assert_eq!(comment["ok"], true, "stranger comments — {comment}");

    // Stranger cannot close/update the PR.
    let denied = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"pull.close","input":{"owner":"srcown","name":"upstream","number":1}}"#,
    )
    .await;
    assert_eq!(denied["ok"], false, "stranger cannot close — {denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found");

    // Read-only on base cannot open a same-repo PR from someone else's branch.
    let same_repo_denied = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"srcown","name":"upstream","title":"nope","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(
        same_repo_denied["ok"], false,
        "same-repo head needs Write — {same_repo_denied}"
    );
    assert_eq!(same_repo_denied["error"]["code"], "repo.not_found");
}
