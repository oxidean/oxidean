//! ISS-04: issue↔PR link stubs + manual add/remove (D-ISS-13 / D-ISS-14).
//!
//! Write+ may mutate; Read+ may list on accessible issues (D-ISS-20).
//! Closing keywords are not enforced in Phase 11 (D-ISS-15 → Phase 12).

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

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Owner (Write+) + Read collaborator on a private org repo with issues #1 and #2.
async fn setup_links_fixture(app: &axum::Router, db: &Database) -> (String, String) {
    let (owner_cookie, owner_v) = signup_and_login(app, "lnkown@ex.com", "lnkown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(db, owner_id).await;

    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"org.create","input":{"slug":"lnk-org"}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"repo.create","input":{"name":"core","visibility":"private","owner":"lnk-org"}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"issue.create","input":{"owner":"lnk-org","name":"core","title":"Source","body":""}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"issue.create","input":{"owner":"lnk-org","name":"core","title":"Target","body":""}}"#,
        )
        .await["ok"],
        true
    );

    let (reader_cookie, reader_v) = signup_and_login(app, "lnkread@ex.com", "lnkread").await;
    let reader_id = reader_v["data"]["id"].as_str().expect("id");
    verify_user(db, reader_id).await;
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"lnk-org","name":"core","username":"lnkread","permission":"read"}}"#,
        )
        .await["ok"],
        true
    );

    (owner_cookie, reader_cookie)
}

/// Manual link control writes opaque PR/issue stub rows (ISS-04 / D-ISS-13 / D-ISS-14).
#[tokio::test]
async fn issue_links_manual_add_stub() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("issue_links_add.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, reader_cookie) = setup_links_fixture(&app, &db).await;

    let add_pr = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.links.add","input":{"owner":"lnk-org","name":"core","number":1,"kind":"pr_stub","targetNumber":42,"title":"Stub PR title"}}"#,
    )
    .await;
    assert_eq!(add_pr["ok"], true, "Write+ add pr_stub — {add_pr}");
    let link = &add_pr["data"];
    assert_eq!(link["kind"], "pr_stub");
    assert_eq!(
        link["targetNumber"]
            .as_i64()
            .or_else(|| link["target_number"].as_i64()),
        Some(42)
    );
    assert_eq!(
        link["title"].as_str(),
        Some("Stub PR title"),
        "optional title preserved"
    );
    let link_id = link["id"].as_str().expect("opaque id");
    assert!(!link_id.is_empty(), "stub rows carry opaque id");

    let add_issue = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.links.add","input":{"owner":"lnk-org","name":"core","number":1,"kind":"issue","targetNumber":2}}"#,
    )
    .await;
    assert_eq!(add_issue["ok"], true, "Write+ add issue link — {add_issue}");
    assert_eq!(add_issue["data"]["kind"], "issue");
    assert_eq!(
        add_issue["data"]["targetNumber"]
            .as_i64()
            .or_else(|| add_issue["data"]["target_number"].as_i64()),
        Some(2)
    );

    let denied = rpc_json(
        &app,
        &reader_cookie,
        r#"{"procedure":"issue.links.add","input":{"owner":"lnk-org","name":"core","number":1,"kind":"pr_stub","targetNumber":99}}"#,
    )
    .await;
    assert_eq!(
        denied["ok"], false,
        "Read collaborator cannot add links — {denied}"
    );
}

/// Manual remove deletes a stub link (ISS-04 / D-ISS-14).
#[tokio::test]
async fn issue_links_manual_remove() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_links_remove.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, _) = setup_links_fixture(&app, &db).await;

    let add = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.links.add","input":{"owner":"lnk-org","name":"core","number":1,"kind":"pr_stub","targetNumber":7,"title":"Gone soon"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "add before remove — {add}");
    let link_id = add["data"]["id"].as_str().expect("id");

    let remove = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"issue.links.remove","input":{{"owner":"lnk-org","name":"core","number":1,"linkId":"{link_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(remove["ok"], true, "Write+ remove — {remove}");

    let list = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.links.list","input":{"owner":"lnk-org","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(list["ok"], true, "list after remove — {list}");
    let links = list["data"]["links"].as_array().expect("links array");
    assert!(
        links.iter().all(|l| l["id"].as_str() != Some(link_id)),
        "removed link must not appear — {list}"
    );
}

/// Closing keywords (`fixes` / `closes` `#N`) are not enforced in Phase 11 (D-ISS-15).
#[tokio::test]
async fn issue_links_no_closing_keyword_enforcement() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_links_keywords.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, _) = setup_links_fixture(&app, &db).await;

    let comment = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.comments.create","input":{"owner":"lnk-org","name":"core","number":1,"body":"fixes #2\ncloses #2"}}"#,
    )
    .await;
    assert_eq!(comment["ok"], true, "comment with keywords — {comment}");

    let target = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.get","input":{"owner":"lnk-org","name":"core","number":2}}"#,
    )
    .await;
    assert_eq!(target["ok"], true, "get target — {target}");
    assert_eq!(
        target["data"]["state"], "open",
        "closing keywords must NOT auto-close in Phase 11 — {target}"
    );

    let links = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.links.list","input":{"owner":"lnk-org","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(links["ok"], true, "list after keyword comment — {links}");
    let arr = links["data"]["links"].as_array().expect("links");
    assert!(
        arr.is_empty(),
        "keyword comment must not auto-create links — {links}"
    );
}

/// Linked PRs panel lists stub rows until Phase 12 objects exist (D-ISS-13).
#[tokio::test]
async fn issue_links_list_stubs_for_panel() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_links_list.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, reader_cookie) = setup_links_fixture(&app, &db).await;

    assert_eq!(
        rpc_json(
            &app,
            &owner_cookie,
            r#"{"procedure":"issue.links.add","input":{"owner":"lnk-org","name":"core","number":1,"kind":"pr_stub","targetNumber":3,"title":"Panel stub"}}"#,
        )
        .await["ok"],
        true
    );

    let list = rpc_json(
        &app,
        &reader_cookie,
        r#"{"procedure":"issue.links.list","input":{"owner":"lnk-org","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(list["ok"], true, "Read+ may list — {list}");
    let links = list["data"]["links"].as_array().expect("links");
    assert_eq!(links.len(), 1, "one stub for panel — {list}");
    assert_eq!(links[0]["kind"], "pr_stub");
    assert!(
        links[0]["id"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "opaque id present"
    );
    assert_eq!(
        links[0]["targetNumber"]
            .as_i64()
            .or_else(|| links[0]["target_number"].as_i64()),
        Some(3)
    );
    assert_eq!(links[0]["title"].as_str(), Some("Panel stub"));
}
