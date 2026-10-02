//! Phase 18: HOOK-02/03 delivery + HMAC for issues path (D-HOOK-08 / D-HOOK-12 / D-HOOK-13).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::webhook::deliver::hmac_sha256_hex;
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use oxidean_git::CliGitBackend;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
    let set_cookie = res.headers().get("set-cookie").expect("Set-Cookie").to_str().unwrap();
    set_cookie.split(';').next().unwrap().trim().to_string()
}

async fn signup_and_login(app: &axum::Router, email: &str, username: &str) -> (String, serde_json::Value) {
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
    let res = app.clone().oneshot(rpc_req_with_cookie(body, cookie)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("rpc json body")
}

async fn verified_owner(app: &axum::Router, db: &Database, email: &str, username: &str) -> String {
    let (cookie, login_v) = signup_and_login(app, email, username).await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now).await.expect("verify");
    cookie
}

#[tokio::test]
async fn webhook_issues_deliver_opened() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1..)
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("deliv_open.db").display());
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

    let hook_url = format!("{}/hook", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"delown","name":"demo","url":"{hook_url}","secret":"hooksecret","events":["issues"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let issue = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"delown","name":"demo","title":"Opened via hook","body":"body"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(issue["ok"], true, "{issue}");

    // Wait for async delivery
    let mut attempt_status = None;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 10).await.expect("list");
        if let Some(d) = deliveries.first() {
            if d.status == "success" || d.attempt_count > 0 {
                let latest = db
                    .latest_webhook_delivery_attempt(&d.id)
                    .await
                    .expect("attempt");
                attempt_status = latest.and_then(|a| a.http_status);
                if attempt_status == Some(200) {
                    break;
                }
            }
        }
    }
    assert_eq!(attempt_status, Some(200), "expected successful delivery attempt");

    let requests = sink.received_requests().await.expect("received requests");
    assert!(!requests.is_empty());
    let req = &requests[0];
    assert_eq!(
        req.headers.get("x-github-event").map(|v| v.to_str().unwrap()),
        Some("issues")
    );
    assert!(req.headers.get("x-hub-signature-256").is_some());
    assert!(req.headers.get("x-github-delivery").is_some());
    let body = String::from_utf8_lossy(&req.body);
    assert!(body.contains("\"action\":\"opened\""));
    assert!(body.contains("Opened via hook"));
}

#[tokio::test]
async fn webhook_hmac_signature() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sig"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("hmac.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "hmac@ex.com", "hmacown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/sig", sink.uri());
    let secret = "signmeplease";
    let _ = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"hmacown","name":"demo","url":"{hook_url}","secret":"{secret}","events":["issues"]}}}}"#
        ),
        &cookie,
    )
    .await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"hmacown","name":"demo","title":"Sig","body":""}}"#,
        &cookie,
    )
    .await;

    let mut got = None;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let requests = sink.received_requests().await.unwrap_or_default();
        if let Some(req) = requests.first() {
            got = Some(req.clone());
            break;
        }
    }
    let req = got.expect("delivery POST");
    let header = req
        .headers
        .get("x-hub-signature-256")
        .expect("sig header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(header.starts_with("sha256="));
    let expected = format!("sha256={}", hmac_sha256_hex(secret.as_bytes(), &req.body));
    assert_eq!(header, expected);
    // No SHA-1 header
    assert!(req.headers.get("x-hub-signature").is_none());
}

#[tokio::test]
async fn webhook_issues_edited_closed_reopened() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/life"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("life.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "life@ex.com", "lifeown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/life", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"lifeown","name":"demo","url":"{hook_url}","secret":"s","events":["issues"]}}}}"#
        ),
        &cookie,
    )
    .await;
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let issue = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"lifeown","name":"demo","title":"T1","body":"b"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(issue["ok"], true);
    let _ = rpc_json(
        &app,
        r#"{"procedure":"issue.update","input":{"owner":"lifeown","name":"demo","number":1,"title":"T2"}}"#,
        &cookie,
    )
    .await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"issue.close","input":{"owner":"lifeown","name":"demo","number":1}}"#,
        &cookie,
    )
    .await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"issue.reopen","input":{"owner":"lifeown","name":"demo","number":1}}"#,
        &cookie,
    )
    .await;

    let mut actions = std::collections::HashSet::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
        for d in deliveries {
            actions.insert(d.action);
        }
        if actions.contains("opened")
            && actions.contains("edited")
            && actions.contains("closed")
            && actions.contains("reopened")
        {
            break;
        }
    }
    assert!(actions.contains("opened"), "{actions:?}");
    assert!(actions.contains("edited"), "{actions:?}");
    assert!(actions.contains("closed"), "{actions:?}");
    assert!(actions.contains("reopened"), "{actions:?}");
}


#[tokio::test]
async fn webhook_issue_comment_lifecycle() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ic"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ic.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "ic@ex.com", "icown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/ic", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"icown","name":"demo","url":"{hook_url}","secret":"s","events":["issue_comment"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let issue = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"icown","name":"demo","title":"T","body":"b"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(issue["ok"], true);
    let comment = rpc_json(
        &app,
        r#"{"procedure":"issue.comments.create","input":{"owner":"icown","name":"demo","number":1,"body":"first take"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(comment["ok"], true, "{comment}");
    let comment_id = comment["data"]["id"].as_str().unwrap();
    let edit = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"issue.comments.update","input":{{"owner":"icown","name":"demo","number":1,"commentId":"{comment_id}","body":"second take"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(edit["ok"], true, "{edit}");
    let del = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"issue.comments.delete","input":{{"owner":"icown","name":"demo","number":1,"commentId":"{comment_id}"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");

    let mut actions = std::collections::HashSet::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
        for d in deliveries.iter().filter(|d| d.event == "issue_comment") {
            actions.insert(d.action.clone());
        }
        if actions.contains("created")
            && actions.contains("edited")
            && actions.contains("deleted")
        {
            break;
        }
    }
    assert!(actions.contains("created"), "{actions:?}");
    assert!(actions.contains("edited"), "{actions:?}");
    assert!(actions.contains("deleted"), "{actions:?}");

    // Delivery rows land inside `emit` before the spawned HTTP POST completes;
    // wait for the sink to observe all three requests as well.
    let mut requests = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        requests = sink.received_requests().await.expect("received requests");
        if requests.len() >= 3 {
            break;
        }
    }
    assert!(!requests.is_empty());
    for req in &requests {
        assert_eq!(
            req.headers.get("x-github-event").map(|v| v.to_str().unwrap()),
            Some("issue_comment")
        );
    }
    let bodies: Vec<String> = requests
        .iter()
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .collect();
    assert!(
        bodies
            .iter()
            .any(|b| b.contains("\"action\":\"created\"") && b.contains("first take")),
        "{bodies:?}"
    );
    assert!(
        bodies
            .iter()
            .any(|b| b.contains("\"action\":\"edited\"")
                && b.contains("\"changes\"")
                && b.contains("first take")),
        "{bodies:?}"
    );
}

/// GitHub parity: `issue_comment` fires for PR conversation comments too
/// (`issue.pull_request` marker); line-anchored comments do not emit it (DEBT-04).
#[tokio::test]
async fn webhook_issue_comment_pull_conversation() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/icpr"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("icpr.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let cookie = verified_owner(&app, &db, "icpr@ex.com", "icprown").await;
    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"icprown","name":"demo","branch":"feature","start":"main"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");

    let bare = repos.join("icprown").join("demo.git");
    commit_on_branch(&bare, "feature", "note", &[("NOTE.md", "line1\n")]).await;

    let pr = rpc_json(
        &app,
        r#"{"procedure":"pull.create","input":{"owner":"icprown","name":"demo","title":"PR","base_ref":"main","head_ref":"feature"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();

    let hook_url = format!("{}/icpr", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"icprown","name":"demo","url":"{hook_url}","secret":"s","events":["issue_comment"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let general = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"icprown","name":"demo","number":{n},"body":"conversation note"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(general["ok"], true, "{general}");

    let line = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"icprown","name":"demo","number":{n},"body":"nit","path":"NOTE.md","side":"RIGHT","line":1}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(line["ok"], true, "{line}");

    // Delivery rows are inserted inside `emit` before the RPC returns, so the
    // line-anchored comment must not have produced a second `issue_comment` row.
    let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
    let ic: Vec<_> = deliveries
        .iter()
        .filter(|d| d.event == "issue_comment")
        .collect();
    assert_eq!(ic.len(), 1, "{deliveries:?}");
    assert_eq!(ic[0].action, "created");
    assert!(ic[0].payload_json.contains("conversation note"));
    assert!(ic[0].payload_json.contains("\"pull_request\""));
}

async fn commit_on_branch(
    bare: &std::path::Path,
    branch: &str,
    message: &str,
    files: &[(&str, &str)],
) {
    let wt = tempfile::tempdir().unwrap();
    let bare_s = bare.to_str().unwrap();
    let wt_s = wt.path().to_str().unwrap();
    let status = std::process::Command::new("git")
        .args(["clone", "--branch", branch, bare_s, wt_s])
        .status()
        .unwrap();
    assert!(status.success(), "clone");
    for (path, content) in files {
        let dest = wt.path().join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&dest, content.as_bytes()).unwrap();
    }
    for args in [
        vec!["-C", wt_s, "config", "user.email", "t@ex.com"],
        vec!["-C", wt_s, "config", "user.name", "Test"],
        vec!["-C", wt_s, "add", "-A"],
        vec!["-C", wt_s, "commit", "-m", message],
        vec!["-C", wt_s, "push", "origin", "HEAD"],
    ] {
        let status = std::process::Command::new("git").args(&args).status().unwrap();
        assert!(status.success(), "git {args:?}");
    }
}

#[tokio::test]
async fn webhook_ssrf_rejects_unsafe_url() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("ssrf.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "ssrf@ex.com", "ssrfown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let bad = rpc_json(
        &app,
        r#"{"procedure":"webhook.create","input":{"owner":"ssrfown","name":"demo","url":"https://169.254.169.254/latest","secret":"s","events":["issues"]}}"#,
        &cookie,
    )
    .await;
    assert_eq!(bad["ok"], false, "{bad}");
    assert_eq!(bad["error"]["code"], "webhook.invalid_url");
}

#[tokio::test]
async fn webhook_timeout_records_error() {
    // Unreachable blackhole port — connection/timeout error path
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("timeout.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    std::env::set_var("OXIDEAN_WEBHOOK_TIMEOUT_SECS", "1");
    std::env::set_var("OXIDEAN_WEBHOOK_MAX_ATTEMPTS", "1");
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "to@ex.com", "toown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let created = rpc_json(
        &app,
        r#"{"procedure":"webhook.create","input":{"owner":"toown","name":"demo","url":"http://127.0.0.1:1/hook","secret":"s","events":["issues"]}}"#,
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();
    let _ = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"toown","name":"demo","title":"t","body":""}}"#,
        &cookie,
    )
    .await;
    let mut saw_error = false;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 10).await.expect("list");
        if let Some(d) = deliveries.first() {
            if let Ok(Some(a)) = db.latest_webhook_delivery_attempt(&d.id).await {
                if a.error_message.is_some() {
                    saw_error = true;
                    break;
                }
            }
        }
    }
    assert!(saw_error, "expected timeout/connection error recorded");
}

#[tokio::test]
async fn webhook_retry_transient() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/retry"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("retry.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    std::env::set_var("OXIDEAN_WEBHOOK_MAX_ATTEMPTS", "3");
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "retry@ex.com", "retryown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/retry", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"retryown","name":"demo","url":"{hook_url}","secret":"s","events":["issues"]}}}}"#
        ),
        &cookie,
    )
    .await;
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();
    let _ = rpc_json(
        &app,
        r#"{"procedure":"issue.create","input":{"owner":"retryown","name":"demo","title":"t","body":""}}"#,
        &cookie,
    )
    .await;

    // First attempt records 503 and stays pending for retry
    let mut pending_with_attempt = false;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 10).await.expect("list");
        if let Some(d) = deliveries.first() {
            if d.attempt_count >= 1 && (d.status == "pending" || d.status == "failed") {
                let latest = db.latest_webhook_delivery_attempt(&d.id).await.unwrap();
                assert_eq!(latest.unwrap().http_status, Some(503));
                pending_with_attempt = true;
                break;
            }
        }
    }
    assert!(pending_with_attempt, "expected 503 attempt recorded");
}


#[tokio::test]
async fn webhook_push_https_receive() {
    use oxidean_api::webhook::dispatch;
    use oxidean_api::webhook::payloads::parse_receive_ref_updates;

    // pkt-line: "40zero 40one refs/heads/main\n"
    let before = "0".repeat(40);
    let after = "1".repeat(40);
    let line = format!("{before} {after} refs/heads/main\n");
    let pkt = format!("{:04x}{}", 4 + line.len(), line);
    let updates = parse_receive_ref_updates(pkt.as_bytes());
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].2, "refs/heads/main");

    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/push"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("push_https.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "ph@ex.com", "phown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/push", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"phown","name":"demo","url":"{hook_url}","secret":"s","events":["push"]}}}}"#
        ),
        &cookie,
    )
    .await;
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();
    let repo_id = created["data"]["repo_id"].as_str().unwrap().to_string();

    // Same notify path Smart HTTP uses after successful receive-pack with ref updates.
    dispatch::notify_push(
        &db,
        &repo_id,
        "phown",
        "demo",
        "phown",
        "user-id",
        &updates,
        "development",
    )
    .await;

    let mut ok = false;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 10).await.unwrap();
        if deliveries.iter().any(|d| d.event == "push") {
            ok = true;
            break;
        }
    }
    assert!(ok, "expected push delivery");
    let reqs = sink.received_requests().await.unwrap_or_default();
    assert!(!reqs.is_empty());
    assert_eq!(
        reqs[0].headers.get("x-github-event").and_then(|v| v.to_str().ok()),
        Some("push")
    );
}

#[tokio::test]
async fn webhook_push_ssh_receive() {
    use oxidean_api::webhook::dispatch;

    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sshpush"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("push_ssh.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "ps@ex.com", "psown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/sshpush", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"psown","name":"demo","url":"{hook_url}","secret":"s","events":["push"]}}}}"#
        ),
        &cookie,
    )
    .await;
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();
    let repo_id = created["data"]["repo_id"].as_str().unwrap().to_string();

    // SSH path emits with empty updates (generic push payload).
    dispatch::notify_push(
        &db,
        &repo_id,
        "psown",
        "demo",
        "psown",
        "uid",
        &[],
        "development",
    )
    .await;

    let mut ok = false;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 10).await.unwrap();
        if deliveries.iter().any(|d| d.event == "push") {
            ok = true;
            break;
        }
    }
    assert!(ok);
}

#[tokio::test]
async fn webhook_pull_request_lifecycle() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/pr"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pr_hook.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let cookie = verified_owner(&app, &db, "pr@ex.com", "prown").await;
    let created_repo = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(created_repo["ok"], true, "{created_repo}");
    let br = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"prown","name":"demo","branch":"feature","start":"main"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");

    let hook_url = format!("{}/pr", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"prown","name":"demo","url":"{hook_url}","secret":"s","events":["pull_request"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let pr = rpc_json(
        &app,
        r#"{"procedure":"pull.create","input":{"owner":"prown","name":"demo","title":"PR1","body":"b","base_ref":"main","head_ref":"feature"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    assert_eq!(pr["data"]["number"], 1);

    let _ = rpc_json(
        &app,
        r#"{"procedure":"pull.update","input":{"owner":"prown","name":"demo","number":1,"title":"PR1 edited"}}"#,
        &cookie,
    )
    .await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"pull.close","input":{"owner":"prown","name":"demo","number":1}}"#,
        &cookie,
    )
    .await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"pull.reopen","input":{"owner":"prown","name":"demo","number":1}}"#,
        &cookie,
    )
    .await;

    let mut actions = std::collections::HashSet::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.unwrap();
        for d in deliveries {
            if d.event == "pull_request" {
                actions.insert(d.action);
            }
        }
        if actions.contains("opened")
            && actions.contains("edited")
            && actions.contains("closed")
            && actions.contains("reopened")
        {
            break;
        }
    }
    assert!(actions.contains("opened"), "{actions:?}");
    assert!(actions.contains("edited"), "{actions:?}");
    assert!(actions.contains("closed"), "{actions:?}");
    assert!(actions.contains("reopened"), "{actions:?}");
}
