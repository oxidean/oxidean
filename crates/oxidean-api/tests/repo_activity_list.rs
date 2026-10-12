//! `repo.activity.list` — empty feed + ACL (public anyone / private Read+).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;
use uuid::Uuid;

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

async fn rpc_json(app: &axum::Router, cookie: Option<&str>, body: &str) -> serde_json::Value {
    let res = match cookie {
        Some(c) => app
            .clone()
            .oneshot(rpc_req_with_cookie(body, c))
            .await
            .unwrap(),
        None => app.clone().oneshot(rpc_req(body)).await.unwrap(),
    };
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn activity_list_empty_and_acl() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("activity-list.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).unwrap();
    let app = test_app(db.clone(), repos_dir).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "actowner@ex.com", "actown").await;
    let owner_id = owner_login["data"]["id"].as_str().unwrap();
    verify_user(&db, owner_id).await;
    create_repo(&app, &owner_cookie, "pubrepo", "public").await;
    create_repo(&app, &owner_cookie, "privrepo", "private").await;

    // Empty public feed is readable anonymously.
    let empty = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.activity.list","input":{"owner":"actown","name":"pubrepo"}}"#,
    )
    .await;
    assert_eq!(empty["ok"], true, "{empty}");
    assert_eq!(empty["data"]["total"], 0);
    assert_eq!(empty["data"]["items"].as_array().unwrap().len(), 0);

    // Private repo: anonymous → soft not_found.
    let anon_priv = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.activity.list","input":{"owner":"actown","name":"privrepo"}}"#,
    )
    .await;
    assert_eq!(anon_priv["ok"], false, "{anon_priv}");
    assert_eq!(anon_priv["error"]["code"], "repo.not_found");

    // Stranger (no access) → soft not_found on private.
    let (stranger_cookie, stranger_login) =
        signup_and_login(&app, "stranger@ex.com", "stranger").await;
    let stranger_id = stranger_login["data"]["id"].as_str().unwrap();
    verify_user(&db, stranger_id).await;
    let stranger_priv = rpc_json(
        &app,
        Some(&stranger_cookie),
        r#"{"procedure":"repo.activity.list","input":{"owner":"actown","name":"privrepo"}}"#,
    )
    .await;
    assert_eq!(stranger_priv["ok"], false, "{stranger_priv}");
    assert_eq!(stranger_priv["error"]["code"], "repo.not_found");

    // Owner can read private empty feed.
    let owner_priv = rpc_json(
        &app,
        Some(&owner_cookie),
        r#"{"procedure":"repo.activity.list","input":{"owner":"actown","name":"privrepo"}}"#,
    )
    .await;
    assert_eq!(owner_priv["ok"], true, "{owner_priv}");
    assert_eq!(owner_priv["data"]["total"], 0);

    // Insert a push row and list it.
    let repo = db
        .find_repository_by_owner_name(owner_id, "pubrepo")
        .await
        .expect("find")
        .expect("pubrepo");
    let event_id = Uuid::new_v4().to_string();
    db.insert_repo_activity(
        &event_id,
        &repo.id,
        owner_id,
        "push",
        "refs/heads/main",
        "0000000000000000000000000000000000000000",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        2,
        Some("Initial commit"),
        None,
    )
    .await
    .expect("insert activity");

    let listed = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.activity.list","input":{"owner":"actown","name":"pubrepo","limit":10}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    assert_eq!(listed["data"]["total"], 1);
    let item = &listed["data"]["items"][0];
    assert_eq!(item["push_type"], "push");
    assert_eq!(item["ref_short"], "main");
    assert_eq!(item["commits_count"], 2);
    assert_eq!(item["commit_message"], "Initial commit");
    assert_eq!(item["pusher"]["login"], "actown");
}

#[tokio::test]
async fn activity_list_rejects_invalid_since() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("activity-since.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).unwrap();
    let app = test_app(db.clone(), repos_dir).await;

    let (cookie, login) = signup_and_login(&app, "since@ex.com", "sinceown").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "hello", "public").await;

    let bad = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.activity.list","input":{"owner":"sinceown","name":"hello","since":"not-a-timestamp"}}"#,
    )
    .await;
    assert_eq!(bad["ok"], false, "{bad}");
    assert_eq!(bad["error"]["code"], "rpc.bad_input");

    let ok = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.activity.list","input":{"owner":"sinceown","name":"hello","since":"2020-01-01T00:00:00Z"}}"#,
    )
    .await;
    assert_eq!(ok["ok"], true, "{ok}");
}

#[tokio::test]
async fn activity_recorded_from_branch_create_rename_delete() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("activity-write.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let repos_dir = dir.path().join("repos");
    std::fs::create_dir_all(&repos_dir).unwrap();
    let app = test_app(db.clone(), repos_dir).await;

    let (cookie, login) = signup_and_login(&app, "write@ex.com", "writeown").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    create_repo(&app, &cookie, "flow", "public").await;

    let create = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.branchCreate","input":{"owner":"writeown","name":"flow","branch":"feature-a","start":"main"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let rename = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.branchRename","input":{"owner":"writeown","name":"flow","from":"feature-a","to":"feature-b"}}"#,
    )
    .await;
    assert_eq!(rename["ok"], true, "{rename}");

    let delete = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.branchDelete","input":{"owner":"writeown","name":"flow","branch":"feature-b"}}"#,
    )
    .await;
    assert_eq!(delete["ok"], true, "{delete}");

    let listed = rpc_json(
        &app,
        None,
        r#"{"procedure":"repo.activity.list","input":{"owner":"writeown","name":"flow","limit":20}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    assert!(listed["data"]["total"].as_i64().unwrap() >= 3, "{listed}");
    let types: Vec<&str> = listed["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["push_type"].as_str())
        .collect();
    assert!(
        types.contains(&"branch_creation"),
        "missing branch_creation — {types:?}"
    );
    assert!(
        types.contains(&"branch_rename"),
        "missing branch_rename — {types:?}"
    );
    assert!(
        types.contains(&"branch_deletion"),
        "missing branch_deletion — {types:?}"
    );

    let rename_item = listed["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["push_type"] == "branch_rename")
        .expect("rename row");
    assert_eq!(rename_item["ref_short"], "feature-b");
    assert_eq!(rename_item["commit_message"], "feature-a");
}
