//! API-01: /api/v1 REST facade — auth (cookie + PAT Bearer), status mapping,
//! and representative resource coverage. Routes dispatch through the same
//! `rpc::dispatch` path as POST /api/rpc; these tests verify the REST
//! translation layer (unwrapped payloads, HTTP statuses, path wiring).

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

fn req(method: &str, uri: &str, body: Option<&str>) -> Request<Body> {
    let b = Request::builder().method(method).uri(uri);
    let b = match body {
        Some(_) => b.header("content-type", "application/json"),
        None => b,
    };
    b.body(Body::from(body.unwrap_or_default().to_owned()))
        .unwrap()
}

fn req_cookie(method: &str, uri: &str, body: Option<&str>, cookie: &str) -> Request<Body> {
    let b = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie);
    let b = match body {
        Some(_) => b.header("content-type", "application/json"),
        None => b,
    };
    b.body(Body::from(body.unwrap_or_default().to_owned()))
        .unwrap()
}

fn req_bearer(method: &str, uri: &str, body: Option<&str>, token: &str) -> Request<Body> {
    let b = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    let b = match body {
        Some(_) => b.header("content-type", "application/json"),
        None => b,
    };
    b.body(Body::from(body.unwrap_or_default().to_owned()))
        .unwrap()
}

fn rpc_req(body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", cookie)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn json(app: &axum::Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let res = app.clone().oneshot(request).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({}));
    (status, v)
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

/// Signup + login via RPC; returns session cookie and user id (verified).
async fn signup_verified(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
) -> (String, String) {
    let signup = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
    );
    let res = app.clone().oneshot(rpc_req(&signup, "")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let _ = res.into_body().collect().await;

    let login = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
    );
    let res = app.clone().oneshot(rpc_req(&login, "")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&res);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    (cookie, user_id)
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str) {
    let (status, v) = json(
        app,
        req_cookie(
            "POST",
            "/api/v1/user/repos",
            Some(&format!(r#"{{"name":"{name}","visibility":"public"}}"#)),
            cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create repo — {v}");
    assert_eq!(v["name"], name);
}

/// Anonymous liveness — no envelope, no auth.
#[tokio::test]
async fn rest_health_ok_anonymous() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("rest_health.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let app = test_app(db, dir.path().join("repos")).await;

    let (status, v) = json(&app, req("GET", "/api/v1/health", None)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "ok");
}

/// Cookie session: create repo → get → update metadata → list issues.
#[tokio::test]
async fn rest_repo_flow_with_cookie() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("rest_repo.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let (cookie, _uid) = signup_verified(&app, &db, "a@ex.com", "alice").await;
    create_repo(&app, &cookie, "demo").await;

    // GET /repos/{owner}/{repo} — unwrapped RepoPublic (no {ok,data} envelope).
    let (status, v) = json(&app, req("GET", "/api/v1/repos/alice/demo", None)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "demo");
    assert_eq!(v["owner_username"], "alice");
    assert!(
        v.get("ok").is_none(),
        "REST must not wrap in RPC envelope — {v}"
    );

    // PATCH metadata.
    let (status, v) = json(
        &app,
        req_cookie(
            "PATCH",
            "/api/v1/repos/alice/demo",
            Some(r#"{"description":"rest test","homepage":"https://ex.com"}"#),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["description"], "rest test");

    // GET /user/repos lists it.
    let (status, v) = json(&app, req_cookie("GET", "/api/v1/user/repos", None, &cookie)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["repos"].as_array().unwrap().len(), 1);

    // GET issues — empty list.
    let (status, v) = json(&app, req("GET", "/api/v1/repos/alice/demo/issues", None)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["total"], 0);
}

/// Issue + comment lifecycle over REST.
#[tokio::test]
async fn rest_issue_comment_lifecycle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("rest_issue.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, _uid) = signup_verified(&app, &db, "b@ex.com", "bob").await;
    create_repo(&app, &cookie, "tracker").await;

    // Create issue.
    let (status, v) = json(
        &app,
        req_cookie(
            "POST",
            "/api/v1/repos/bob/tracker/issues",
            Some(r#"{"title":"first","body":"body one"}"#),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    assert_eq!(v["number"], 1);
    assert_eq!(v["state"], "open");

    // Get it.
    let (status, v) = json(&app, req("GET", "/api/v1/repos/bob/tracker/issues/1", None)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["title"], "first");

    // Comment.
    let (status, v) = json(
        &app,
        req_cookie(
            "POST",
            "/api/v1/repos/bob/tracker/issues/1/comments",
            Some(r#"{"body":"hello"}"#),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    let comment_id = v["id"].as_str().expect("comment id").to_string();

    // List comments.
    let (status, v) = json(
        &app,
        req("GET", "/api/v1/repos/bob/tracker/issues/1/comments", None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["comments"].as_array().unwrap().len(), 1);

    // Edit the comment.
    let (status, v) = json(
        &app,
        req_cookie(
            "PATCH",
            &format!("/api/v1/repos/bob/tracker/issues/1/comments/{comment_id}"),
            Some(r#"{"body":"edited"}"#),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["body"], "edited");

    // Close via PATCH state.
    let (status, v) = json(
        &app,
        req_cookie(
            "PATCH",
            "/api/v1/repos/bob/tracker/issues/1",
            Some(r#"{"state":"closed"}"#),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["state"], "closed");

    // state=closed filter.
    let (status, v) = json(
        &app,
        req("GET", "/api/v1/repos/bob/tracker/issues?state=closed", None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["total"], 1);

    // Delete the comment.
    let (status, v) = json(
        &app,
        req_cookie(
            "DELETE",
            &format!("/api/v1/repos/bob/tracker/issues/1/comments/{comment_id}"),
            None,
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");

    let (_, v) = json(
        &app,
        req("GET", "/api/v1/repos/bob/tracker/issues/1/comments", None),
    )
    .await;
    assert_eq!(v["comments"].as_array().unwrap().len(), 0);
}

/// PAT Bearer on REST: `repo` scope reads repos + identity; `package:read`
/// scope is denied on the repo domain (API-02 scope gating reaches REST).
#[tokio::test]
async fn rest_bearer_pat_scope_enforcement() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("rest_pat.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, _uid) = signup_verified(&app, &db, "c@ex.com", "carol").await;
    create_repo(&app, &cookie, "patdemo").await;

    // Mint a repo-scoped classic PAT via RPC (session-only procedure).
    let res = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"pat.createClassic","input":{"name":"ci","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let repo_token = v["data"]["token"].as_str().expect("token").to_string();

    // Mint a package-scoped PAT (no repo scope).
    let res = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"pat.createClassic","input":{"name":"pkgs","scopes":["package:read"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let pkg_token = v["data"]["token"].as_str().expect("token").to_string();

    // repo scope: GET repo OK, GET /user OK (identity read).
    let (status, v) = json(
        &app,
        req_bearer("GET", "/api/v1/repos/carol/patdemo", None, &repo_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "patdemo");

    let (status, v) = json(&app, req_bearer("GET", "/api/v1/user", None, &repo_token)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["username"], "carol");

    // package scope on repo read → 403 auth.pat_scope.
    let (status, v) = json(
        &app,
        req_bearer("GET", "/api/v1/repos/carol/patdemo", None, &pkg_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["code"], "auth.pat_scope");
}

/// No credentials → 401; bad Bearer → 401 + WWW-Authenticate.
#[tokio::test]
async fn rest_unauthenticated_and_bad_bearer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("rest_unauth.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db, dir.path().join("repos")).await;

    let (status, v) = json(&app, req("GET", "/api/v1/user", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["code"], "auth.unauthenticated");

    let res = app
        .clone()
        .oneshot(req_bearer("GET", "/api/v1/user", None, "oxidean_pat_bogus"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(res.headers().get("www-authenticate").is_some());
}

/// Missing repo → mapped 404 (not RPC's default 400).
#[tokio::test]
async fn rest_not_found_maps_404() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("rest_404.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db, dir.path().join("repos")).await;

    let (status, v) = json(&app, req("GET", "/api/v1/repos/ghost/nada", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{v}");
    assert_eq!(v["code"], "repo.not_found");
}

/// Org create + get + members over REST (session cookie).
#[tokio::test]
async fn rest_org_create_get() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("rest_org.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, _uid) = signup_verified(&app, &db, "d@ex.com", "dora").await;

    let (status, v) = json(
        &app,
        req_cookie(
            "POST",
            "/api/v1/orgs",
            Some(r#"{"slug":"acme","display_name":"Acme Inc"}"#),
            &cookie,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    assert_eq!(v["slug"], "acme");

    let (status, v) = json(&app, req("GET", "/api/v1/orgs/acme", None)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["slug"], "acme");

    // Members list requires org membership (RPC-level ACL, not anonymous).
    let (status, v) = json(
        &app,
        req_cookie("GET", "/api/v1/orgs/acme/members", None, &cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["members"].as_array().unwrap().len(), 1);

    // PATs cannot create orgs (session-only procedure → 403 pat_scope).
    let res = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"pat.createClassic","input":{"name":"ci","scopes":["repo"]}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v["data"]["token"].as_str().expect("token").to_string();

    let (status, v) = json(
        &app,
        req_bearer("POST", "/api/v1/orgs", Some(r#"{"slug":"evil"}"#), &token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["code"], "auth.pat_scope");
}
