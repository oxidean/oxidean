//! Phase 18: HOOK-01 Admin webhook CRUD RPC (D-HOOK-02 / D-HOOK-03 / D-HOOK-15 / D-HOOK-16).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::CliGitBackend;
use tower::ServiceExt;

async fn test_app(db: Database, repos_dir: std::path::PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir)
        .with_git(Arc::new(CliGitBackend::new()));
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

async fn verified_owner(app: &axum::Router, db: &Database, email: &str, username: &str) -> String {
    let (cookie, login_v) = signup_and_login(app, email, username).await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    cookie
}

#[tokio::test]
async fn webhook_create_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("hook_create.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "own@ex.com", "hookown").await;

    let create_repo = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    assert_eq!(create_repo["ok"], true, "{create_repo}");

    let created = rpc_json(
        &app,
        r#"{"procedure":"webhook.create","input":{"owner":"hookown","name":"demo","url":"https://example.com/hook","secret":"supersecret","events":["issues"],"active":true,"description":"test"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["data"]["url"], "https://example.com/hook");
    assert_eq!(created["data"]["secret"], "supersecret");
    assert!(created["data"]["secret_masked"]
        .as_str()
        .unwrap()
        .contains("****"));
    assert_eq!(created["data"]["events"][0], "issues");
    assert_eq!(created["data"]["active"], true);
}

#[tokio::test]
async fn webhook_list_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("hook_list.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "list@ex.com", "listown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"webhook.create","input":{"owner":"listown","name":"demo","url":"https://example.com/a","secret":"sekrit1","events":["push"]}}"#,
        &cookie,
    )
    .await;

    let listed = rpc_json(
        &app,
        r#"{"procedure":"webhook.list","input":{"owner":"listown","name":"demo"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    let hooks = listed["data"]["webhooks"].as_array().unwrap();
    assert_eq!(hooks.len(), 1);
    assert!(
        hooks[0]["secret"].is_null()
            || !hooks[0]
                .get("secret")
                .map(|s| s.is_string())
                .unwrap_or(false)
    );
    assert_ne!(hooks[0]["secret_masked"], "sekrit1");
}

#[tokio::test]
async fn webhook_admin_denial() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("hook_deny.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let owner = verified_owner(&app, &db, "adm@ex.com", "admown").await;
    let writer = verified_owner(&app, &db, "wri@ex.com", "wriuser").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"locked","visibility":"public","description":""}}"#,
        &owner,
    )
    .await;
    let add = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"admown","name":"locked","username":"wriuser","permission":"write"}}"#,
        &owner,
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");

    let denied = rpc_json(
        &app,
        r#"{"procedure":"webhook.create","input":{"owner":"admown","name":"locked","url":"https://example.com/x","secret":"x","events":["issues"]}}"#,
        &writer,
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    // Anti-enumeration: soft not_found
    assert_eq!(denied["error"]["code"], "repo.not_found");
}

#[tokio::test]
async fn webhook_update_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("hook_upd.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "upd@ex.com", "updown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let created = rpc_json(
        &app,
        r#"{"procedure":"webhook.create","input":{"owner":"updown","name":"demo","url":"https://example.com/old","secret":"oldsecret","events":["issues"]}}"#,
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let id = created["data"]["id"].as_str().unwrap();

    let updated = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.update","input":{{"owner":"updown","name":"demo","id":"{id}","url":"https://example.com/new","secret":"newsecret","events":["push","issues"],"active":false}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(updated["ok"], true, "{updated}");
    assert_eq!(updated["data"]["url"], "https://example.com/new");
    assert_eq!(updated["data"]["secret"], "newsecret");
    assert_eq!(updated["data"]["active"], false);
    assert!(updated["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e == "push"));
}

#[tokio::test]
async fn webhook_delete_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("hook_del.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "del@ex.com", "delown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let created = rpc_json(
        &app,
        r#"{"procedure":"webhook.create","input":{"owner":"delown","name":"demo","url":"https://example.com/h","secret":"s","events":["issues"]}}"#,
        &cookie,
    )
    .await;
    let id = created["data"]["id"].as_str().unwrap();
    let deleted = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.delete","input":{{"owner":"delown","name":"demo","id":"{id}"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(deleted["ok"], true, "{deleted}");
    let listed = rpc_json(
        &app,
        r#"{"procedure":"webhook.list","input":{"owner":"delown","name":"demo"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(listed["data"]["webhooks"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn webhook_inactive_skips_enqueue() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("hook_inactive.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "ina@ex.com", "inaown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let created = rpc_json(
        &app,
        r#"{"procedure":"webhook.create","input":{"owner":"inaown","name":"demo","url":"https://example.com/h","secret":"s","events":["issues"],"active":false}}"#,
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let issue = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"inaown","name":"demo","title":"Hello","body":""}}"#,
        &cookie,
    )
    .await;
    assert_eq!(issue["ok"], true, "{issue}");

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let deliveries = db
        .list_webhook_deliveries(&hook_id, 10)
        .await
        .expect("list");
    assert!(deliveries.is_empty(), "inactive hook must not enqueue");
}

#[tokio::test]
async fn webhook_deliveries_list() {
    let sink = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .mount(&sink)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("deliv_list.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "dl@ex.com", "dlown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/h", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"dlown","name":"demo","url":"{hook_url}","secret":"s","events":["issues"]}}}}"#
        ),
        &cookie,
    )
    .await;
    let hook_id = created["data"]["id"].as_str().unwrap();
    let _ = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"dlown","name":"demo","title":"t","body":""}}"#,
        &cookie,
    )
    .await;
    let mut listed_ok = false;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let listed = rpc_json(
            &app,
            &format!(
                r#"{{"procedure":"webhook.deliveries.list","input":{{"owner":"dlown","name":"demo","webhook_id":"{hook_id}","limit":25}}}}"#
            ),
            &cookie,
        )
        .await;
        assert_eq!(listed["ok"], true, "{listed}");
        let arr = listed["data"]["deliveries"].as_array().unwrap();
        if !arr.is_empty() {
            assert!(arr[0]["status"].as_str().is_some());
            listed_ok = true;
            break;
        }
    }
    assert!(listed_ok);
}

#[tokio::test]
async fn webhook_ping() {
    let sink = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .mount(&sink)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ping.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "ping@ex.com", "pingown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/p", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"pingown","name":"demo","url":"{hook_url}","secret":"s","events":["issues"]}}}}"#
        ),
        &cookie,
    )
    .await;
    let hook_id = created["data"]["id"].as_str().unwrap();
    let pinged = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.ping","input":{{"owner":"pingown","name":"demo","id":"{hook_id}"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(pinged["ok"], true, "{pinged}");
    assert!(pinged["data"]["delivery_id"].as_str().is_some());
}

#[tokio::test]
async fn webhook_redeliver() {
    let sink = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .mount(&sink)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("redel.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "re@ex.com", "reown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/r", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"reown","name":"demo","url":"{hook_url}","secret":"s","events":["issues"]}}}}"#
        ),
        &cookie,
    )
    .await;
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();
    let _ = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"reown","name":"demo","title":"t","body":""}}"#,
        &cookie,
    )
    .await;
    let mut delivery_id = None;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 10).await.unwrap();
        if let Some(d) = deliveries.first() {
            delivery_id = Some(d.id.clone());
            break;
        }
    }
    let delivery_id = delivery_id.expect("delivery");
    let red = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.redeliver","input":{{"owner":"reown","name":"demo","webhook_id":"{hook_id}","delivery_id":"{delivery_id}"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(red["ok"], true, "{red}");
    assert_ne!(red["data"]["delivery_id"].as_str().unwrap(), delivery_id);
}
