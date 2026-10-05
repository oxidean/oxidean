//! GIT-25: bare-repo git object size quota — enforcement + RPCs.
//!
//! Enforcement lives in `check_ref_update` (the `hooks/update` helper path that
//! Smart HTTP, SSH, and internal worktree pushes share): a non-delete ref update
//! is denied when the on-disk repo (incl. any quarantined incoming pack) exceeds
//! the effective quota. Deletes stay allowed so over-quota repos can clean up.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::git::bare_repo_path;
use oxidean_api::protection::{check_ref_update, ZERO_SHA};
use oxidean_api::repo::Capability;
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

const A_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

struct Fixture {
    db: Database,
    repos: std::path::PathBuf,
    cookie: String,
    bare: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

async fn setup_repo() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let db_path = dir.path().join("quota.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    // Keep the tempdir alive via the fixture; build the app on a clone.
    let app = test_app(db.clone(), repos.clone()).await;

    let (cookie, login) = signup_and_login(&app, "owner@ex.com", "qowner").await;
    verify_user(&db, login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let bare = bare_repo_path(&repos, "qowner", "core").expect("bare path");
    assert!(bare.exists(), "bare repo must exist on disk");

    Fixture {
        db,
        repos,
        cookie,
        bare,
        _dir: dir,
    }
}

/// Over-quota pushes denied (any ref namespace), under-quota allowed, deletes bypass.
#[tokio::test]
async fn quota_denies_over_allows_under_and_deletes() {
    let fx = setup_repo().await;
    let app = test_app(fx.db.clone(), fx.repos.clone()).await;

    // 1-byte per-repo override → any non-delete update is over quota.
    let set = rpc_json(
        &app,
        &fx.cookie,
        r#"{"procedure":"repo.quota.set","input":{"owner":"qowner","name":"core","size_quota_bytes":1}}"#,
    )
    .await;
    assert_eq!(set["ok"], true, "set quota — {set}");
    assert_eq!(set["data"]["size_quota_bytes"], 1);

    let err = check_ref_update(
        &fx.db,
        &fx.repos,
        &fx.bare,
        "refs/heads/main",
        A_SHA,
        B_SHA,
        Capability::Write,
    )
    .await
    .expect_err("push must be denied when repo is over quota");
    assert_eq!(err.code, "repo.size_quota_exceeded", "{err:?}");

    // Quota applies to non-branch refs too (tags are not a bypass).
    let err = check_ref_update(
        &fx.db,
        &fx.repos,
        &fx.bare,
        "refs/tags/v1",
        ZERO_SHA,
        B_SHA,
        Capability::Write,
    )
    .await
    .expect_err("tag create must be denied when repo is over quota");
    assert_eq!(err.code, "repo.size_quota_exceeded", "{err:?}");

    // Admin capability does not bypass the quota (admins raise the quota, they
    // do not sidestep it).
    let err = check_ref_update(
        &fx.db,
        &fx.repos,
        &fx.bare,
        "refs/heads/main",
        A_SHA,
        B_SHA,
        Capability::Admin,
    )
    .await
    .expect_err("admin push still denied when over quota");
    assert_eq!(err.code, "repo.size_quota_exceeded", "{err:?}");

    // Deletes carry no objects — always allowed so cleanup works.
    check_ref_update(
        &fx.db,
        &fx.repos,
        &fx.bare,
        "refs/heads/feature",
        A_SHA,
        ZERO_SHA,
        Capability::Write,
    )
    .await
    .expect("ref delete must bypass the quota");

    // Raise the override above the current size → pushes allowed again.
    let big = rpc_json(
        &app,
        &fx.cookie,
        r#"{"procedure":"repo.quota.set","input":{"owner":"qowner","name":"core","size_quota_bytes":10737418240}}"#,
    )
    .await;
    assert_eq!(big["ok"], true, "raise quota — {big}");
    check_ref_update(
        &fx.db,
        &fx.repos,
        &fx.bare,
        "refs/heads/main",
        A_SHA,
        B_SHA,
        Capability::Write,
    )
    .await
    .expect("under-quota push allowed");

    // Clearing the override reverts to the (10 GiB built-in) instance default.
    let cleared = rpc_json(
        &app,
        &fx.cookie,
        r#"{"procedure":"repo.quota.set","input":{"owner":"qowner","name":"core","size_quota_bytes":null}}"#,
    )
    .await;
    assert_eq!(cleared["ok"], true, "clear override — {cleared}");
    assert_eq!(cleared["data"]["size_quota_bytes"], serde_json::Value::Null);
    assert_eq!(
        cleared["data"]["effective_quota_bytes"],
        oxidean_api::git::quota::DEFAULT_REPO_QUOTA_BYTES
    );
    check_ref_update(
        &fx.db,
        &fx.repos,
        &fx.bare,
        "refs/heads/main",
        A_SHA,
        B_SHA,
        Capability::Write,
    )
    .await
    .expect("default quota allows push");
}

/// `repo.quota.get` reports measured size + effective quota; instance default applies.
#[tokio::test]
async fn quota_get_reports_usage_and_instance_default_applies() {
    let fx = setup_repo().await;
    let app = test_app(fx.db.clone(), fx.repos.clone()).await;

    // Instance-level override (admin settings row) — a 1-byte default makes the
    // repo over quota with no per-repo override set.
    fx.db
        .update_git_settings(Some(1))
        .await
        .expect("instance override");

    let err = check_ref_update(
        &fx.db,
        &fx.repos,
        &fx.bare,
        "refs/heads/main",
        A_SHA,
        B_SHA,
        Capability::Write,
    )
    .await
    .expect_err("instance default quota must deny");
    assert_eq!(err.code, "repo.size_quota_exceeded", "{err:?}");

    // Per-repo admin override wins over the instance default.
    let set = rpc_json(
        &app,
        &fx.cookie,
        r#"{"procedure":"repo.quota.set","input":{"owner":"qowner","name":"core","size_quota_bytes":10737418240}}"#,
    )
    .await;
    assert_eq!(set["ok"], true, "repo override — {set}");
    check_ref_update(
        &fx.db,
        &fx.repos,
        &fx.bare,
        "refs/heads/main",
        A_SHA,
        B_SHA,
        Capability::Write,
    )
    .await
    .expect("per-repo override above usage allows push");

    let got = rpc_json(
        &app,
        &fx.cookie,
        r#"{"procedure":"repo.quota.get","input":{"owner":"qowner","name":"core"}}"#,
    )
    .await;
    assert_eq!(got["ok"], true, "get quota — {got}");
    assert!(
        got["data"]["size_bytes"].as_i64().unwrap() > 0,
        "size_bytes refreshed live — {got}"
    );
    assert_eq!(got["data"]["size_quota_bytes"], 10737418240i64);
    assert_eq!(got["data"]["instance_quota_bytes"], 1);
    assert_eq!(got["data"]["effective_quota_bytes"], 10737418240i64);

    // Cached column was refreshed by the get.
    let row = fx
        .db
        .find_repository_by_owner_name(
            &fx.db
                .find_user_by_username("qowner")
                .await
                .unwrap()
                .unwrap()
                .id,
            "core",
        )
        .await
        .unwrap()
        .unwrap();
    assert!(row.size_bytes > 0, "size_bytes persisted — {row:?}");
}

/// `repo.quota.set` requires Admin capability on the repo.
#[tokio::test]
async fn quota_set_denied_for_non_admin() {
    let fx = setup_repo().await;
    let app = test_app(fx.db.clone(), fx.repos.clone()).await;

    // Second user with no capability on the repo.
    let (other_cookie, other_login) = signup_and_login(&app, "other@ex.com", "qother").await;
    verify_user(&fx.db, other_login["data"]["id"].as_str().unwrap()).await;

    let denied = rpc_json(
        &app,
        &other_cookie,
        r#"{"procedure":"repo.quota.set","input":{"owner":"qowner","name":"core","size_quota_bytes":1}}"#,
    )
    .await;
    assert_eq!(denied["ok"], false, "non-admin must be denied — {denied}");

    // Read on a public repo still works for non-admins.
    let got = rpc_json(
        &app,
        &other_cookie,
        r#"{"procedure":"repo.quota.get","input":{"owner":"qowner","name":"core"}}"#,
    )
    .await;
    assert_eq!(got["ok"], true, "read quota ok — {got}");

    // `0` stores an explicit unlimited override (repo admin scope).
    let unlimited = rpc_json(
        &app,
        &fx.cookie,
        r#"{"procedure":"repo.quota.set","input":{"owner":"qowner","name":"core","size_quota_bytes":0}}"#,
    )
    .await;
    assert_eq!(unlimited["ok"], true, "unlimited override — {unlimited}");
    assert_eq!(unlimited["data"]["size_quota_bytes"], 0);
    assert_eq!(
        unlimited["data"]["effective_quota_bytes"],
        serde_json::Value::Null
    );
    check_ref_update(
        &fx.db,
        &fx.repos,
        &fx.bare,
        "refs/heads/main",
        A_SHA,
        B_SHA,
        Capability::Write,
    )
    .await
    .expect("unlimited override allows push");
}
