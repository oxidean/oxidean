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

/// Issue #102 / GitHub parity: diff-anchored PR comments emit
/// `pull_request_review_comment` (`created` / `edited` / `deleted`) while
/// conversation comments stay on `issue_comment`.
#[tokio::test]
async fn webhook_pull_request_review_comment_lifecycle() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/prc"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("prc.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let cookie = verified_owner(&app, &db, "prc@ex.com", "prcown").await;
    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    let br = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"prcown","name":"demo","branch":"feature","start":"main"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");

    let bare = repos.join("prcown").join("demo.git");
    commit_on_branch(&bare, "feature", "note", &[("NOTE.md", "line1\n")]).await;

    let pr = rpc_json(
        &app,
        r#"{"procedure":"pull.create","input":{"owner":"prcown","name":"demo","title":"PR","base_ref":"main","head_ref":"feature"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();

    let hook_url = format!("{}/prc", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"prcown","name":"demo","url":"{hook_url}","secret":"s","events":["pull_request_review_comment"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    // Conversation comments must not emit `pull_request_review_comment` —
    // delivery rows are inserted synchronously before the RPC returns.
    let general = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"prcown","name":"demo","number":{n},"body":"conversation note"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(general["ok"], true, "{general}");
    assert!(
        db.list_webhook_deliveries(&hook_id, 50)
            .await
            .expect("list")
            .is_empty(),
        "conversation comment must not emit pull_request_review_comment"
    );

    let line = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pull.comments.create","input":{{"owner":"prcown","name":"demo","number":{n},"body":"nit","path":"NOTE.md","side":"RIGHT","line":1}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(line["ok"], true, "{line}");
    let comment_id = line["data"]["id"].as_str().unwrap().to_string();

    let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
    assert_eq!(deliveries.len(), 1, "{deliveries:?}");
    let created_row = &deliveries[0];
    assert_eq!(created_row.event, "pull_request_review_comment");
    assert_eq!(created_row.action, "created");
    assert!(created_row.payload_json.contains("\"path\":\"NOTE.md\""));
    assert!(created_row.payload_json.contains("\"line\":1"));
    assert!(created_row.payload_json.contains("\"commit_id\""));
    assert!(created_row.payload_json.contains("\"created_at\""));
    assert!(created_row.payload_json.contains("\"pull_request\""));
    assert!(created_row.payload_json.contains("\"sender\""));

    let edit = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pull.comments.update","input":{{"owner":"prcown","name":"demo","number":{n},"commentId":"{comment_id}","body":"nit revised"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(edit["ok"], true, "{edit}");

    let del = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pull.comments.delete","input":{{"owner":"prcown","name":"demo","number":{n},"commentId":"{comment_id}"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");

    let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
    let prc: Vec<_> = deliveries
        .iter()
        .filter(|d| d.event == "pull_request_review_comment")
        .collect();
    assert_eq!(prc.len(), 3, "{deliveries:?}");
    let edited = prc
        .iter()
        .find(|d| d.action == "edited")
        .expect("edited delivery");
    assert!(edited.payload_json.contains("nit revised"));
    assert!(edited.payload_json.contains("\"changes\""));
    assert!(
        edited.payload_json.contains("\"from\":\"nit\""),
        "{}",
        edited.payload_json
    );
    let deleted = prc
        .iter()
        .find(|d| d.action == "deleted")
        .expect("deleted delivery");
    assert!(deleted.payload_json.contains("\"comment\""));
    assert!(deleted.payload_json.contains("\"path\":\"NOTE.md\""));

    // Wait for the async HTTP posts and check the GitHub event header.
    let mut requests = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        requests = sink.received_requests().await.expect("received requests");
        if requests.len() >= 3 {
            break;
        }
    }
    assert_eq!(requests.len(), 3, "{requests:?}");
    for req in &requests {
        assert_eq!(
            req.headers.get("x-github-event").map(|v| v.to_str().unwrap()),
            Some("pull_request_review_comment")
        );
    }
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

// ---------------------------------------------------------------------------
// API-04: broader event catalog — release / star / fork / create / delete /
// workflow_run / registry_package.
// ---------------------------------------------------------------------------

/// `webhook.create` accepts every new API-04 event name.
#[tokio::test]
async fn webhook_create_accepts_api04_events() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("api04.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "api04@ex.com", "api04own").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let created = rpc_json(
        &app,
        r#"{"procedure":"webhook.create","input":{"owner":"api04own","name":"demo","url":"https://example.com/hook","secret":"s","events":["release","star","fork","create","delete","workflow_run","registry_package"]}}"#,
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let events: Vec<&str> = created["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
        .collect();
    for want in [
        "release",
        "star",
        "fork",
        "create",
        "delete",
        "workflow_run",
        "registry_package",
    ] {
        assert!(events.contains(&want), "{events:?}");
    }
}

/// `release` fires `published` on create, `edited` on update, `deleted` on delete.
#[tokio::test]
async fn webhook_release_lifecycle() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rel"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("rel.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;
    let cookie = verified_owner(&app, &db, "rel@ex.com", "relown").await;
    let created_repo = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(created_repo["ok"], true, "{created_repo}");

    // release.create requires the tag to exist on the bare repo.
    let bare = repos.join("relown").join("demo.git");
    let status = std::process::Command::new("git")
        .args([
            "-C",
            bare.to_str().unwrap(),
            "tag",
            "v1.0.0",
            "refs/heads/main",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "git tag");

    let hook_url = format!("{}/rel", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"relown","name":"demo","url":"{hook_url}","secret":"s","events":["release"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let rel = rpc_json(
        &app,
        r#"{"procedure":"release.create","input":{"owner":"relown","name":"demo","tag_name":"v1.0.0","title":"First","body":"notes"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(rel["ok"], true, "{rel}");
    let upd = rpc_json(
        &app,
        r#"{"procedure":"release.update","input":{"owner":"relown","name":"demo","tag_name":"v1.0.0","title":"First (edited)"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    let del = rpc_json(
        &app,
        r#"{"procedure":"release.delete","input":{"owner":"relown","name":"demo","tag_name":"v1.0.0"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");

    let mut actions = std::collections::HashSet::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
        for d in deliveries.iter().filter(|d| d.event == "release") {
            actions.insert(d.action.clone());
        }
        if actions.contains("published") && actions.contains("edited") && actions.contains("deleted")
        {
            break;
        }
    }
    assert!(actions.contains("published"), "{actions:?}");
    assert!(actions.contains("edited"), "{actions:?}");
    assert!(actions.contains("deleted"), "{actions:?}");

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
            Some("release")
        );
    }
    let bodies: Vec<String> = requests
        .iter()
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .collect();
    assert!(
        bodies.iter().any(|b| b.contains("\"action\":\"published\"")
            && b.contains("\"tag_name\":\"v1.0.0\"")
            && b.contains("\"login\":\"relown\"")),
        "{bodies:?}"
    );
    assert!(
        bodies.iter().any(|b| b.contains("\"action\":\"deleted\"")),
        "{bodies:?}"
    );
}

/// `star` fires `created` on star and `deleted` on unstar; the idempotent
/// second `repo.star` does not re-emit.
#[tokio::test]
async fn webhook_star_created_deleted() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/star"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("star.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "star@ex.com", "starown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/star", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"starown","name":"demo","url":"{hook_url}","secret":"s","events":["star"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    for _ in 0..2 {
        let st = rpc_json(
            &app,
            r#"{"procedure":"repo.star","input":{"owner":"starown","name":"demo"}}"#,
            &cookie,
        )
        .await;
        assert_eq!(st["ok"], true, "{st}");
    }
    let un = rpc_json(
        &app,
        r#"{"procedure":"repo.unstar","input":{"owner":"starown","name":"demo"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(un["ok"], true, "{un}");

    let mut actions = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
        actions = deliveries
            .iter()
            .filter(|d| d.event == "star")
            .map(|d| d.action.clone())
            .collect();
        if actions.contains(&"deleted".to_string()) {
            break;
        }
    }
    // Exactly one `created` (idempotent re-star is suppressed) + one `deleted`.
    assert_eq!(
        actions.iter().filter(|a| a.as_str() == "created").count(),
        1,
        "{actions:?}"
    );
    assert_eq!(
        actions.iter().filter(|a| a.as_str() == "deleted").count(),
        1,
        "{actions:?}"
    );

    let mut bodies: Vec<String> = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let requests = sink.received_requests().await.expect("received requests");
        bodies = requests
            .iter()
            .map(|r| String::from_utf8_lossy(&r.body).into_owned())
            .collect();
        if bodies.len() >= 2 {
            break;
        }
    }
    assert!(
        bodies.iter().any(|b| b.contains("\"action\":\"created\"")
            && b.contains("\"full_name\":\"starown/demo\"")),
        "{bodies:?}"
    );
}

/// `fork` fires on the source repository when another user forks it; the
/// payload's `forkee` identifies the new repo.
#[tokio::test]
async fn webhook_fork_created() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/fork"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("fork.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let src_cookie = verified_owner(&app, &db, "forksrc@ex.com", "forksrc").await;
    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"upstream","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
        &src_cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let hook_url = format!("{}/fork", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"forksrc","name":"upstream","url":"{hook_url}","secret":"s","events":["fork"]}}}}"#
        ),
        &src_cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let fork_cookie = verified_owner(&app, &db, "forker@ex.com", "forker").await;
    let forked = rpc_json(
        &app,
        r#"{"procedure":"repo.fork","input":{"owner":"forksrc","name":"upstream"}}"#,
        &fork_cookie,
    )
    .await;
    assert_eq!(forked["ok"], true, "{forked}");

    let mut deliveries = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        deliveries = db
            .list_webhook_deliveries(&hook_id, 50)
            .await
            .expect("list")
            .into_iter()
            .filter(|d| d.event == "fork")
            .collect();
        if !deliveries.is_empty() {
            break;
        }
    }
    assert_eq!(deliveries.len(), 1, "{deliveries:?}");
    let payload = &deliveries[0].payload_json;
    assert!(payload.contains("\"forkee\""), "{payload}");
    assert!(payload.contains("forker/upstream"), "{payload}");
    assert!(payload.contains("\"login\":\"forker\""), "{payload}");

    let mut requests = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        requests = sink.received_requests().await.expect("received requests");
        if !requests.is_empty() {
            break;
        }
    }
    assert!(!requests.is_empty());
    assert_eq!(
        requests[0]
            .headers
            .get("x-github-event")
            .and_then(|v| v.to_str().ok()),
        Some("fork")
    );
}

/// `create` / `delete` fire for `repo.branchCreate` / `repo.branchDelete`.
#[tokio::test]
async fn webhook_create_delete_branch_rpc() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/refs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("refs.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "refs@ex.com", "refsown").await;
    let create = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let hook_url = format!("{}/refs", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"refsown","name":"demo","url":"{hook_url}","secret":"s","events":["create","delete"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let br = rpc_json(
        &app,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"refsown","name":"demo","branch":"topic","start":"main"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");
    let bd = rpc_json(
        &app,
        r#"{"procedure":"repo.branchDelete","input":{"owner":"refsown","name":"demo","branch":"topic"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(bd["ok"], true, "{bd}");

    let mut events = std::collections::HashSet::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
        for d in deliveries {
            events.insert(d.event);
        }
        if events.contains("create") && events.contains("delete") {
            break;
        }
    }
    assert!(events.contains("create"), "{events:?}");
    assert!(events.contains("delete"), "{events:?}");

    let mut bodies: Vec<String> = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let requests = sink.received_requests().await.expect("received requests");
        bodies = requests
            .iter()
            .map(|r| String::from_utf8_lossy(&r.body).into_owned())
            .collect();
        if bodies.len() >= 2 {
            break;
        }
    }
    assert!(
        bodies.iter().any(|b| b.contains("\"ref\":\"topic\"")
            && b.contains("\"ref_type\":\"branch\"")),
        "{bodies:?}"
    );
}

/// The receive-pack notify path emits `create` / `delete` for branch and tag
/// refs beside `push` (covers the smart-HTTP / SSH update triples).
#[tokio::test]
async fn webhook_ref_events_receive_pack() {
    use oxidean_api::webhook::dispatch;

    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/refevents"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("refevents.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "rp@ex.com", "rpown").await;
    let _ = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    let hook_url = format!("{}/refevents", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"rpown","name":"demo","url":"{hook_url}","secret":"s","events":["create","delete"]}}}}"#
        ),
        &cookie,
    )
    .await;
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();
    let repo_id = created["data"]["repo_id"].as_str().unwrap().to_string();

    let zero = "0".repeat(40);
    let one = "1".repeat(40);
    let two = "2".repeat(40);
    let three = "3".repeat(40);
    // Same triple shape both receive-pack notify sites consume.
    let updates = vec![
        (zero.clone(), one.clone(), "refs/heads/feature".to_string()), // branch create
        (zero.clone(), two.clone(), "refs/tags/v2.0.0".to_string()),   // tag create
        (three.clone(), zero.clone(), "refs/heads/old".to_string()),   // branch delete
        (one.clone(), two.clone(), "refs/heads/main".to_string()),     // push-only update
    ];
    dispatch::notify_ref_events(
        &db,
        &repo_id,
        "rpown",
        "demo",
        "rpown",
        "uid",
        &updates,
        "development",
    )
    .await;

    let mut deliveries = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
        if deliveries.len() >= 3 {
            break;
        }
    }
    assert_eq!(deliveries.len(), 3, "{deliveries:?}");
    let creates: Vec<_> = deliveries.iter().filter(|d| d.event == "create").collect();
    let deletes: Vec<_> = deliveries.iter().filter(|d| d.event == "delete").collect();
    assert_eq!(creates.len(), 2, "{deliveries:?}");
    assert_eq!(deletes.len(), 1, "{deliveries:?}");
    assert!(
        creates
            .iter()
            .any(|d| d.payload_json.contains("\"ref\":\"feature\"")
                && d.payload_json.contains("\"ref_type\":\"branch\"")),
        "{creates:?}"
    );
    assert!(
        creates
            .iter()
            .any(|d| d.payload_json.contains("\"ref\":\"v2.0.0\"")
                && d.payload_json.contains("\"ref_type\":\"tag\"")),
        "{creates:?}"
    );
    assert!(deletes[0].payload_json.contains("\"ref\":\"old\""));
    // `create`/`delete` carry no action (GitHub parity — the event is the action).
    assert!(deliveries.iter().all(|d| d.action.is_empty()), "{deliveries:?}");
}

/// `workflow_run` fires `requested` when a run is enqueued and `completed`
/// (conclusion `cancelled`) via `repo.actions.cancelRun`.
#[tokio::test]
async fn webhook_workflow_run_requested_and_completed() {
    use oxidean_api::actions::{enqueue_run, parse_workflow_yaml};

    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/wf"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("wf.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let cookie = verified_owner(&app, &db, "wf@ex.com", "wfown").await;
    let created_repo = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    assert_eq!(created_repo["ok"], true, "{created_repo}");
    let repo_id = created_repo["data"]["id"].as_str().unwrap().to_string();

    let hook_url = format!("{}/wf", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"wfown","name":"demo","url":"{hook_url}","secret":"s","events":["workflow_run"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    let doc = parse_workflow_yaml(
        br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
    )
    .unwrap();
    let (run_id, _jobs) = enqueue_run(
        &db,
        &repo_id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "abc1234deadbeef",
        "refs/heads/main",
        None,
    )
    .await
    .unwrap();

    let cancel = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.actions.cancelRun","input":{{"owner":"wfown","name":"demo","run_id":"{run_id}"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(cancel["ok"], true, "{cancel}");

    let mut actions = std::collections::HashSet::new();
    let mut completed_payload = String::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
        for d in deliveries.iter().filter(|d| d.event == "workflow_run") {
            actions.insert(d.action.clone());
            if d.action == "completed" {
                completed_payload = d.payload_json.clone();
            }
        }
        if actions.contains("requested") && actions.contains("completed") {
            break;
        }
    }
    assert!(actions.contains("requested"), "{actions:?}");
    assert!(actions.contains("completed"), "{actions:?}");
    assert!(completed_payload.contains("\"conclusion\":\"cancelled\""), "{completed_payload}");
    assert!(completed_payload.contains("\"status\":\"completed\""), "{completed_payload}");
    assert!(completed_payload.contains("\"name\":\"CI\""), "{completed_payload}");
}

/// `registry_package` fires for generic-registry publishes on repo-linked
/// packages (`published` for a new version, `updated` when a file is added to
/// an existing version).
#[tokio::test]
async fn webhook_registry_package_generic_publish() {
    let sink = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/pkg"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&sink)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let packages = dir.path().join("packages");
    let url = format!("sqlite:{}", dir.path().join("pkg.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let state = AppState::new(db.clone(), Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos)
        .with_packages_dir(packages)
        .with_git(Arc::new(CliGitBackend::new()));
    let cors = build_cors("development", None).expect("cors");
    let app = router_with_state(state, cors);

    let cookie = verified_owner(&app, &db, "pkg@ex.com", "pkgown").await;
    let created_repo = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"demo","visibility":"public","description":""}}"#,
        &cookie,
    )
    .await;
    assert_eq!(created_repo["ok"], true, "{created_repo}");
    let repo_id = created_repo["data"]["id"].as_str().unwrap().to_string();

    let hook_url = format!("{}/pkg", sink.uri());
    let created = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"webhook.create","input":{{"owner":"pkgown","name":"demo","url":"{hook_url}","secret":"s","events":["registry_package"]}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let hook_id = created["data"]["id"].as_str().unwrap().to_string();

    // packages:write PAT authenticates the registry PUT (Basic user:token).
    let pat_res = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createFineGrained","input":{"name":"pkg-write","repo_access":"all","contents":"read","packages":"write"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let pat_bytes = pat_res.into_body().collect().await.unwrap().to_bytes();
    let pat_v: serde_json::Value = serde_json::from_slice(&pat_bytes).unwrap();
    let token = pat_v["data"]["token"].as_str().expect("token").to_string();

    for (file, bytes) in [("a.bin", &b"one"[..]), ("b.bin", &b"two"[..])] {
        let put = Request::builder()
            .method("PUT")
            .uri(format!(
                "/generic/pkgown/tool/1.0.0/{file}?repository_id={repo_id}"
            ))
            .header(
                axum::http::header::AUTHORIZATION,
                basic_auth("pkgown", &token),
            )
            .body(Body::from(bytes))
            .unwrap();
        let res = app.clone().oneshot(put).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED, "PUT {file}");
    }

    let mut actions = std::collections::HashSet::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let deliveries = db.list_webhook_deliveries(&hook_id, 50).await.expect("list");
        for d in deliveries.iter().filter(|d| d.event == "registry_package") {
            actions.insert(d.action.clone());
        }
        if actions.contains("published") && actions.contains("updated") {
            break;
        }
    }
    assert!(actions.contains("published"), "{actions:?}");
    assert!(actions.contains("updated"), "{actions:?}");

    let mut bodies: Vec<String> = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let requests = sink.received_requests().await.expect("received requests");
        bodies = requests
            .iter()
            .map(|r| String::from_utf8_lossy(&r.body).into_owned())
            .collect();
        if bodies.len() >= 2 {
            break;
        }
    }
    assert!(
        bodies.iter().any(|b| b.contains("\"name\":\"tool\"")
            && b.contains("\"package_type\":\"generic\"")
            && b.contains("\"version\":\"1.0.0\"")),
        "{bodies:?}"
    );
}

fn basic_auth(user: &str, password: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let raw = format!("{user}:{password}");
    let input = raw.as_bytes();
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    format!("Basic {out}")
}
