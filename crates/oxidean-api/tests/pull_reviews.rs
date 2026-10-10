//! PR-04 reviews: approve / changes / comment / dismiss + author ACL.

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

#[tokio::test]
async fn pull_reviews_submit_dismiss_author_cannot_approve() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_reviews.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, owner_v) = signup_and_login(&app, "o@ex.com", "rown").await;
    verify_user(&db, owner_v["data"]["id"].as_str().unwrap()).await;
    let (rev_cookie, rev_v) = signup_and_login(&app, "r@ex.com", "reviewer").await;
    verify_user(&db, rev_v["data"]["id"].as_str().unwrap()).await;

    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","description":"","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");
    // Collaborator write for reviewer
    let collab = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"rown","name":"core","username":"reviewer","permission":"write"}}"#,
    )
    .await;
    assert_eq!(collab["ok"], true, "{collab}");

    let br = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"rown","name":"core","branch":"feature","start":"main"}}"#,
    )
    .await;
    assert_eq!(br["ok"], true, "{br}");

    let pr = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"pull.create","input":{"owner":"rown","name":"core","title":"PR","base_ref":"main","head_ref":"feature"}}"#,
    )
    .await;
    assert_eq!(pr["ok"], true, "{pr}");
    let n = pr["data"]["number"].as_i64().unwrap();

    let author_approve = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"rown","name":"core","number":{n},"state":"approved"}}}}"#
        ),
    )
    .await;
    assert_eq!(author_approve["ok"], false, "{author_approve}");
    assert_eq!(
        author_approve["error"]["code"],
        "pull.author_cannot_approve"
    );

    let author_comment = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"rown","name":"core","number":{n},"state":"commented","body":"self note"}}}}"#
        ),
    )
    .await;
    assert_eq!(author_comment["ok"], true, "{author_comment}");

    let approve = rpc_json(
        &app,
        &rev_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"rown","name":"core","number":{n},"state":"approved","body":"lgtm"}}}}"#
        ),
    )
    .await;
    assert_eq!(approve["ok"], true, "{approve}");
    assert_eq!(approve["data"]["state"], "approved");
    let rid = approve["data"]["id"].as_str().unwrap();

    let changes = rpc_json(
        &app,
        &rev_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.submit","input":{{"owner":"rown","name":"core","number":{n},"state":"changes_requested","body":"nit"}}}}"#
        ),
    )
    .await;
    assert_eq!(changes["ok"], true, "{changes}");
    assert_eq!(changes["data"]["state"], "changes_requested");

    let list = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.list","input":{{"owner":"rown","name":"core","number":{n}}}}}"#
        ),
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");
    let reviews = list["data"]["reviews"].as_array().unwrap();
    assert!(reviews.len() >= 3);

    let dismiss = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviews.dismiss","input":{{"owner":"rown","name":"core","number":{n},"review_id":"{rid}","reason":"stale"}}}}"#
        ),
    )
    .await;
    assert_eq!(dismiss["ok"], true, "{dismiss}");
    assert_eq!(dismiss["data"]["state"], "dismissed");

    let req_add = rpc_json(
        &app,
        &owner_cookie,
        &format!(
            r#"{{"procedure":"pull.reviewRequests.add","input":{{"owner":"rown","name":"core","number":{n},"username":"reviewer"}}}}"#
        ),
    )
    .await;
    assert_eq!(req_add["ok"], true, "{req_add}");
    assert!(req_add["data"]["usernames"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u.as_str() == Some("reviewer")));
}
