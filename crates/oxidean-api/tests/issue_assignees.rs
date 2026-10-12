//! ISS-03: multi-assignee + Read+ eligibility (D-ISS-06 / D-ISS-08).
//!
//! Write+ assign per D-ISS-07 / D-ISS-20. Threat: T-11-12.

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

/// Private org repo with Write collaborator + Read collaborator + outsider.
async fn setup_private_assign_fixture(
    app: &axum::Router,
    db: &Database,
) -> (String, String, String, String) {
    let (owner_cookie, owner_v) = signup_and_login(app, "asgown@ex.com", "asgown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(db, owner_id).await;

    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"org.create","input":{"slug":"asg-org"}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"repo.create","input":{"name":"core","visibility":"private","owner":"asg-org"}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"issue.create","input":{"owner":"asg-org","name":"core","title":"Assign me","body":""}}"#,
        )
        .await["ok"],
        true
    );

    let (writer_cookie, writer_v) = signup_and_login(app, "asgwrite@ex.com", "asgwrite").await;
    let writer_id = writer_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(db, &writer_id).await;
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"asg-org","name":"core","username":"asgwrite","permission":"write"}}"#,
        )
        .await["ok"],
        true
    );

    let (_reader_cookie, reader_v) = signup_and_login(app, "asgread@ex.com", "asgread").await;
    let reader_id = reader_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(db, &reader_id).await;
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"asg-org","name":"core","username":"asgread","permission":"read"}}"#,
        )
        .await["ok"],
        true
    );

    let (_out_cookie, out_v) = signup_and_login(app, "asgout@ex.com", "asgout").await;
    let outsider_id = out_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(db, &outsider_id).await;

    (owner_cookie, writer_cookie, reader_id, outsider_id)
}

/// Write+ can assign multiple users to an issue (ISS-03 / D-ISS-06).
#[tokio::test]
async fn issue_assignees_multi_assign() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_assignees_multi.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, writer_cookie, reader_id, _) = setup_private_assign_fixture(&app, &db).await;

    let owner_id = rpc_json(&app, &owner_cookie, r#"{"procedure":"auth.me","input":{}}"#).await
        ["data"]["id"]
        .as_str()
        .expect("owner id")
        .to_string();

    let set_ok = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"issue.assignees.set","input":{{"owner":"asg-org","name":"core","number":1,"userIds":["{owner_id}","{reader_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(set_ok["ok"], true, "Write+ multi-assign — {set_ok}");
    let assignees = set_ok["data"]["assignees"].as_array().expect("assignees");
    assert_eq!(assignees.len(), 2, "two assignees — {assignees:?}");
    let ids: Vec<&str> = assignees
        .iter()
        .filter_map(|a| a["user_id"].as_str())
        .collect();
    assert!(ids.contains(&owner_id.as_str()));
    assert!(ids.contains(&reader_id.as_str()));

    let got = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"issue.get","input":{"owner":"asg-org","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(got["ok"], true, "{got}");
    assert_eq!(
        got["data"]["assignees"].as_array().unwrap().len(),
        2,
        "persisted on get"
    );

    let cands = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"issue.assigneeCandidates","input":{"owner":"asg-org","name":"core"}}"#,
    )
    .await;
    assert_eq!(cands["ok"], true, "assigneeCandidates — {cands}");
    let users = cands["data"]["users"].as_array().expect("users");
    let usernames: Vec<&str> = users
        .iter()
        .filter_map(|u| u["username"].as_str())
        .collect();
    assert!(
        usernames.contains(&"asgown")
            && usernames.contains(&"asgwrite")
            && usernames.contains(&"asgread"),
        "eligible Read+ profiles listed — {usernames:?}"
    );
    assert!(
        !usernames.contains(&"asgout"),
        "outsider without Read+ excluded — {usernames:?}"
    );
}

/// Assignee eligibility = Read+ on the repo; ineligible user rejected (ISS-03 / D-ISS-08).
#[tokio::test]
async fn issue_assignees_reject_without_read_access() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_assignees_reject.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (_owner_cookie, writer_cookie, _reader_id, outsider_id) =
        setup_private_assign_fixture(&app, &db).await;

    let denied = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"issue.assignees.set","input":{{"owner":"asg-org","name":"core","number":1,"userIds":["{outsider_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(
        denied["ok"], false,
        "outsider without Read+ rejected — {denied}"
    );
    assert_eq!(denied["error"]["code"], "rpc.bad_input");
}

/// Write+ can unassign (ISS-03 / D-ISS-07).
#[tokio::test]
async fn issue_assignees_write_unassign() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_assignees_unassign.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (_owner_cookie, writer_cookie, reader_id, _) =
        setup_private_assign_fixture(&app, &db).await;

    let set_ok = rpc_json(
        &app,
        &writer_cookie,
        &format!(
            r#"{{"procedure":"issue.assignees.set","input":{{"owner":"asg-org","name":"core","number":1,"userIds":["{reader_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(set_ok["ok"], true, "assign first — {set_ok}");
    assert_eq!(set_ok["data"]["assignees"].as_array().unwrap().len(), 1);

    let clear = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"issue.assignees.set","input":{"owner":"asg-org","name":"core","number":1,"userIds":[]}}"#,
    )
    .await;
    assert_eq!(clear["ok"], true, "clear assignees — {clear}");
    assert_eq!(clear["data"]["assignees"].as_array().unwrap().len(), 0);

    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"asgread@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let reader_cookie = session_cookie_from_response(&login);
    let _ = login.into_body().collect().await;

    let reader_denied = rpc_json(
        &app,
        &reader_cookie,
        &format!(
            r#"{{"procedure":"issue.assignees.set","input":{{"owner":"asg-org","name":"core","number":1,"userIds":["{reader_id}"]}}}}"#
        ),
    )
    .await;
    assert_eq!(
        reader_denied["ok"], false,
        "Read cannot set assignees — {reader_denied}"
    );
    assert_eq!(reader_denied["error"]["code"], "repo.not_found");
}
