//! D-ISS-11: Eight reaction content values on issue and comment.
//!
//! Write+ may react (D-ISS-20). Unknown content → rpc.bad_input. Toggle is
//! idempotent per user/content (on → off).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

const EIGHT_CONTENTS: &[&str] = &[
    "+1", "-1", "laugh", "confused", "heart", "hooray", "rocket", "eyes",
];

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

/// Owner (Write+) + Read collaborator on a private org repo with issue #1.
async fn setup_react_fixture(app: &axum::Router, db: &Database) -> (String, String) {
    let (owner_cookie, owner_v) = signup_and_login(app, "rxown@ex.com", "rxown").await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id");
    verify_user(db, owner_id).await;

    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"org.create","input":{"slug":"rx-org"}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"repo.create","input":{"name":"core","visibility":"private","owner":"rx-org"}}"#,
        )
        .await["ok"],
        true
    );
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"issue.create","input":{"owner":"rx-org","name":"core","title":"React me","body":""}}"#,
        )
        .await["ok"],
        true
    );

    let (reader_cookie, reader_v) = signup_and_login(app, "rxread@ex.com", "rxread").await;
    let reader_id = reader_v["data"]["id"].as_str().expect("id");
    verify_user(db, reader_id).await;
    assert_eq!(
        rpc_json(
            app,
            &owner_cookie,
            r#"{"procedure":"repo.collaborators.add","input":{"owner":"rx-org","name":"core","username":"rxread","permission":"read"}}"#,
        )
        .await["ok"],
        true
    );

    (owner_cookie, reader_cookie)
}

fn group_count(reactions: &serde_json::Value, content: &str) -> i64 {
    reactions
        .as_array()
        .expect("reactions array")
        .iter()
        .find(|g| g["content"].as_str() == Some(content))
        .and_then(|g| g["count"].as_i64())
        .unwrap_or(0)
}

fn viewer_reacted(reactions: &serde_json::Value, content: &str) -> bool {
    reactions
        .as_array()
        .expect("reactions array")
        .iter()
        .find(|g| g["content"].as_str() == Some(content))
        .and_then(|g| g["viewerHasReacted"].as_bool().or_else(|| g["viewer_has_reacted"].as_bool()))
        .unwrap_or(false)
}

/// Eight content values toggle on an issue (D-ISS-11).
#[tokio::test]
async fn issue_reactions_eight_content_on_issue() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_reactions_issue.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, reader_cookie) = setup_react_fixture(&app, &db).await;

    for content in EIGHT_CONTENTS {
        let toggle = rpc_json(
            &app,
            &owner_cookie,
            &format!(
                r#"{{"procedure":"issue.reactions.toggle","input":{{"owner":"rx-org","name":"core","number":1,"target":"issue","content":"{content}"}}}}"#
            ),
        )
        .await;
        assert_eq!(
            toggle["ok"], true,
            "Write+ toggle {content} on issue — {toggle}"
        );
        assert!(
            viewer_reacted(&toggle["data"]["reactions"], content),
            "viewerHasReacted for {content} — {toggle}"
        );
        assert_eq!(
            group_count(&toggle["data"]["reactions"], content),
            1,
            "count for {content}"
        );
    }

    let got = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.get","input":{"owner":"rx-org","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(got["ok"], true, "issue.get — {got}");
    for content in EIGHT_CONTENTS {
        assert_eq!(
            group_count(&got["data"]["reactions"], content),
            1,
            "issue.get has {content}"
        );
    }

    let bad = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.reactions.toggle","input":{"owner":"rx-org","name":"core","number":1,"target":"issue","content":"thumbs"}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false, "unknown content — {bad}");
    assert_eq!(bad["error"]["code"], "rpc.bad_input");

    // Read collaborator participates: reactions are Read+ verified (D-ISS-20).
    let allowed = rpc_json(
        &app,
        &reader_cookie,
        r#"{"procedure":"issue.reactions.toggle","input":{"owner":"rx-org","name":"core","number":1,"target":"issue","content":"+1"}}"#,
    )
    .await;
    assert_eq!(allowed["ok"], true, "Read can react — {allowed}");
    assert!(viewer_reacted(&allowed["data"]["reactions"], "+1"), "{allowed}");

    // Verified stranger with no access to the private repo is denied.
    let (stranger_cookie, stranger_v) =
        signup_and_login(&app, "rxstranger@ex.com", "rxstranger").await;
    let stranger_id = stranger_v["data"]["id"].as_str().expect("id");
    verify_user(&db, stranger_id).await;
    let denied = rpc_json(
        &app,
        &stranger_cookie,
        r#"{"procedure":"issue.reactions.toggle","input":{"owner":"rx-org","name":"core","number":1,"target":"issue","content":"+1"}}"#,
    )
    .await;
    assert_eq!(denied["ok"], false, "no-access stranger denied — {denied}");
    assert_eq!(
        denied["error"]["code"], "repo.not_found",
        "soft deny for private — {denied}"
    );
}

/// Same eight content values toggle on a comment (D-ISS-11).
#[tokio::test]
async fn issue_reactions_eight_content_on_comment() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_reactions_comment.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, _) = setup_react_fixture(&app, &db).await;

    let created = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.comments.create","input":{"owner":"rx-org","name":"core","number":1,"body":"hello"}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "create comment — {created}");
    let comment_id = created["data"]["id"].as_str().expect("comment id");

    for content in EIGHT_CONTENTS {
        let toggle = rpc_json(
            &app,
            &owner_cookie,
            &format!(
                r#"{{"procedure":"issue.reactions.toggle","input":{{"owner":"rx-org","name":"core","number":1,"target":"comment","commentId":"{comment_id}","content":"{content}"}}}}"#
            ),
        )
        .await;
        assert_eq!(
            toggle["ok"], true,
            "Write+ toggle {content} on comment — {toggle}"
        );
        assert!(
            viewer_reacted(&toggle["data"]["reactions"], content),
            "viewerHasReacted for {content} — {toggle}"
        );
    }

    let listed = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.comments.list","input":{"owner":"rx-org","name":"core","number":1}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "comments.list — {listed}");
    let comments = listed["data"]["comments"].as_array().expect("comments");
    assert_eq!(comments.len(), 1);
    for content in EIGHT_CONTENTS {
        assert_eq!(
            group_count(&comments[0]["reactions"], content),
            1,
            "comment list has {content}"
        );
    }
}

/// Toggle off removes the caller's reaction (D-ISS-11).
#[tokio::test]
async fn issue_reactions_toggle_off() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!(
        "sqlite:{}",
        dir.path().join("issue_reactions_off.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (owner_cookie, _) = setup_react_fixture(&app, &db).await;

    let on = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.reactions.toggle","input":{"owner":"rx-org","name":"core","number":1,"target":"issue","content":"heart"}}"#,
    )
    .await;
    assert_eq!(on["ok"], true, "toggle on — {on}");
    assert!(viewer_reacted(&on["data"]["reactions"], "heart"));
    assert_eq!(group_count(&on["data"]["reactions"], "heart"), 1);
    assert_eq!(on["data"]["reacted"], true);

    let off = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"issue.reactions.toggle","input":{"owner":"rx-org","name":"core","number":1,"target":"issue","content":"heart"}}"#,
    )
    .await;
    assert_eq!(off["ok"], true, "toggle off — {off}");
    assert!(!viewer_reacted(&off["data"]["reactions"], "heart"));
    assert_eq!(group_count(&off["data"]["reactions"], "heart"), 0);
    assert_eq!(off["data"]["reacted"], false);
}
