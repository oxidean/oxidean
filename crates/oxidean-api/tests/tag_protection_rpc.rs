//! GIT-21: Admin tag protection ruleset CRUD (`repo.tagProtection.*`).

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

/// Admin can create/list/update/delete a tag protection rule (GIT-21).
#[tokio::test]
async fn tag_protection_rpc_admin_crud() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("tp_crud.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (cookie, login) = signup_and_login(&app, "a@ex.com", "tadmin").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let created = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.tagProtection.create","input":{"owner":"tadmin","name":"core","pattern":"v*","allow_create":true,"enforce_admins":true}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["data"]["pattern"], "v*");
    assert_eq!(created["data"]["allow_create"], true);
    assert_eq!(created["data"]["allow_delete"], false);
    assert_eq!(created["data"]["enforce_admins"], true);
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let listed = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.tagProtection.list","input":{"owner":"tadmin","name":"core"}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    assert_eq!(listed["data"]["rules"].as_array().unwrap().len(), 1);

    let updated = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.tagProtection.update","input":{{"owner":"tadmin","name":"core","id":"{id}","pattern":"release-*","allow_delete":true}}}}"#
        ),
    )
    .await;
    assert_eq!(updated["ok"], true, "{updated}");
    assert_eq!(updated["data"]["pattern"], "release-*");
    assert_eq!(updated["data"]["allow_delete"], true);
    assert_eq!(updated["data"]["allow_create"], false);

    let deleted = rpc_json(
        &app,
        &cookie,
        &format!(
            r#"{{"procedure":"repo.tagProtection.delete","input":{{"owner":"tadmin","name":"core","id":"{id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(deleted["ok"], true, "{deleted}");

    let listed = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.tagProtection.list","input":{"owner":"tadmin","name":"core"}}"#,
    )
    .await;
    assert_eq!(listed["data"]["rules"].as_array().unwrap().len(), 0);
}

/// Bad patterns rejected (GIT-21).
#[tokio::test]
async fn tag_protection_rpc_rejects_bad_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("tp_bad.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (cookie, login) = signup_and_login(&app, "b@ex.com", "tbad").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    for pattern in ["", "a..b"] {
        let body = format!(
            r#"{{"procedure":"repo.tagProtection.create","input":{{"owner":"tbad","name":"core","pattern":"{pattern}"}}}}"#
        );
        let res = rpc_json(&app, &cookie, &body).await;
        assert_eq!(res["ok"], false, "pattern {pattern:?} must fail — {res}");
        assert_eq!(res["error"]["code"], "rpc.bad_input");
    }
}

/// Non-Admin mutate is soft-denied (same Admin gate as branch rules, GIT-21).
#[tokio::test]
async fn tag_protection_rpc_non_admin_denied() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("tp_deny.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (owner_cookie, owner_login) = signup_and_login(&app, "o@ex.com", "townu").await;
    verify_user(&db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let (writer_cookie, writer_login) = signup_and_login(&app, "w@ex.com", "twriteu").await;
    verify_user(&db, writer_login["data"]["id"].as_str().unwrap()).await;
    let add = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"townu","name":"core","username":"twriteu","permission":"write"}}"#,
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");

    let denied = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"repo.tagProtection.create","input":{"owner":"townu","name":"core","pattern":"v*"}}"#,
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found");

    let denied_list = rpc_json(
        &app,
        &writer_cookie,
        r#"{"procedure":"repo.tagProtection.list","input":{"owner":"townu","name":"core"}}"#,
    )
    .await;
    assert_eq!(denied_list["ok"], false, "{denied_list}");
    assert_eq!(denied_list["error"]["code"], "repo.not_found");
}

/// Gitea REST surface for tag protections — `/repos/{o}/{r}/tag_protections`
/// CRUD with `{name_pattern}` bodies and `{id, name_pattern, whitelist_*}` rows.
#[tokio::test]
async fn tag_protection_rest_crud() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("tp_rest.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (cookie, login) = signup_and_login(&app, "rt@ex.com", "rtadmin").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"tagged","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let (s, v) = rest_all(
        &app,
        "GET",
        "/api/v1/repos/rtadmin/tagged/tag_protections",
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v.as_array().unwrap().len(), 0);

    let (s, v) = rest_all(
        &app,
        "POST",
        "/api/v1/repos/rtadmin/tagged/tag_protections",
        &cookie,
        Some(r#"{"name_pattern":"v*"}"#.into()),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["name_pattern"], "v*");
    let id = v["id"].as_str().unwrap().to_string();

    let (s, v) = rest_all(
        &app,
        "GET",
        &format!("/api/v1/repos/rtadmin/tagged/tag_protections/{id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["id"], id);
    let (s, _) = rest_all(
        &app,
        "GET",
        "/api/v1/repos/rtadmin/tagged/tag_protections/nope",
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // PATCH merges — pattern change, flags retained.
    let (s, v) = rest_all(
        &app,
        "PATCH",
        &format!("/api/v1/repos/rtadmin/tagged/tag_protections/{id}"),
        &cookie,
        Some(r#"{"name_pattern":"release-*","allow_delete":true}"#.into()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["name_pattern"], "release-*");
    assert_eq!(v["allow_delete"], true);

    let (s, _) = rest_all(
        &app,
        "DELETE",
        &format!("/api/v1/repos/rtadmin/tagged/tag_protections/{id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = rest_all(
        &app,
        "GET",
        "/api/v1/repos/rtadmin/tagged/tag_protections",
        &cookie,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v.as_array().unwrap().len(), 0);
}

/// GitHub `/repos/{o}/{r}/activity` — the push feed with `activity_type`
/// (our `push_type`) and `time_period` filters projected to GitHub rows.
#[tokio::test]
async fn rest_repo_activity() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("act_rest.db").display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;
    let (cookie, login) = signup_and_login(&app, "ra@ex.com", "raown").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"feed","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    // Activity rows come from real pushes — create a branch so a
    // branch_creation row lands in the feed.
    let branched = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.branchCreate","input":{"owner":"raown","name":"feed","branch":"dev"}}"#,
    )
    .await;
    assert_eq!(branched["ok"], true, "{branched}");

    let (s, v) = rest_get(&app, "/api/v1/repos/raown/feed/activity", &cookie).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let items = v.as_array().unwrap();
    assert!(
        !items.is_empty(),
        "expected at least the branch_creation row"
    );
    let creation = items
        .iter()
        .find(|a| a["activity_type"] == "branch_creation")
        .expect("branch_creation row");
    assert_eq!(creation["ref"], "refs/heads/dev");
    assert_eq!(creation["actor"]["login"], "raown");
    assert!(creation["timestamp"].is_string());

    // Filter to a type that cannot exist yet → empty.
    let (s, v) = rest_get(
        &app,
        "/api/v1/repos/raown/feed/activity?activity_type=force_push",
        &cookie,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v.as_array().unwrap().len(), 0);
}

async fn rest_all(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    body: Option<String>,
) -> (StatusCode, serde_json::Value) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie);
    if body.is_some() {
        b = b.header("content-type", "application/json");
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::from(body.unwrap_or_default())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

async fn rest_get(app: &axum::Router, uri: &str, cookie: &str) -> (StatusCode, serde_json::Value) {
    rest_all(app, "GET", uri, cookie, None).await
}
