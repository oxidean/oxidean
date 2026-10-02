//! COL-13 — per-repo unit toggles for Issues / Pull requests.
//!
//! `repo.issues.setEnabled` / `repo.pulls.setEnabled` (Admin-only) flip
//! `repositories.issues_enabled` / `pulls_enabled`. While a unit is off every
//! RPC under its surface (`issue.*` / `pull.*` — comments, reviews, labels,
//! links, merges included) rejects with the stable `repo.<unit>.disabled`
//! code. Data is never deleted; re-enabling restores the unit. Repo-level
//! surfaces (`repo.get`, `label.*`, `repo.mergeSettings.*`) and git data are
//! unaffected.

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

async fn rpc_json_anon(app: &axum::Router, body: &str) -> serde_json::Value {
    let res = app.clone().oneshot(rpc_req(body)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"public","description":""}}}}"#
    );
    let v = rpc_json(app, cookie, &body).await;
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

fn set_enabled_body(unit: &str, owner: &str, name: &str, enabled: bool) -> String {
    format!(
        r#"{{"procedure":"repo.{unit}.setEnabled","input":{{"owner":"{owner}","name":"{name}","enabled":{enabled}}}}}"#
    )
}

fn get_enabled_body(unit: &str, owner: &str, name: &str) -> String {
    format!(
        r#"{{"procedure":"repo.{unit}.getEnabled","input":{{"owner":"{owner}","name":"{name}"}}}}"#
    )
}

/// Fresh repo defaults to both units enabled; `repo.get` carries the flags;
/// `setEnabled` persists and `repo.get` + `getEnabled` reflect the change.
#[tokio::test]
async fn unit_toggles_default_on_and_persist() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("units_default.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "unitown@ex.com", "unitown").await;
    verify_user(&db, login_v["data"]["id"].as_str().expect("id")).await;
    create_repo(&app, &cookie, "units").await;

    let get = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.get","input":{"owner":"unitown","name":"units"}}"#,
    )
    .await;
    assert_eq!(get["ok"], true, "{get}");
    assert_eq!(
        get["data"]["issues_enabled"], true,
        "issues default enabled — {get}"
    );
    assert_eq!(
        get["data"]["pulls_enabled"], true,
        "pulls default enabled — {get}"
    );

    // Read+ toggle readback.
    let ge = rpc_json(
        &app,
        &cookie,
        &get_enabled_body("issues", "unitown", "units"),
    )
    .await;
    assert_eq!(ge["ok"], true, "{ge}");
    assert_eq!(ge["data"]["enabled"], true);

    let se = rpc_json(
        &app,
        &cookie,
        &set_enabled_body("issues", "unitown", "units", false),
    )
    .await;
    assert_eq!(se["ok"], true, "issues.setEnabled — {se}");
    assert_eq!(se["data"]["enabled"], false);

    // Persists across repo.get + getEnabled.
    let get = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.get","input":{"owner":"unitown","name":"units"}}"#,
    )
    .await;
    assert_eq!(get["data"]["issues_enabled"], false, "persisted — {get}");
    assert_eq!(
        get["data"]["pulls_enabled"], true,
        "pulls untouched — {get}"
    );
    let ge = rpc_json(
        &app,
        &cookie,
        &get_enabled_body("issues", "unitown", "units"),
    )
    .await;
    assert_eq!(ge["data"]["enabled"], false);

    // Anonymous Read on public repo can also read the toggle.
    let ge = rpc_json_anon(&app, &get_enabled_body("pulls", "unitown", "units")).await;
    assert_eq!(ge["ok"], true, "anon getEnabled — {ge}");
    assert_eq!(ge["data"]["enabled"], true);
}

/// Disabled issues reject every `issue.*` surface with the stable code;
/// `pull.*` stays live; re-enabling restores the unit end-to-end.
#[tokio::test]
async fn disabled_issues_unit_rejects_issue_rpcs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("units_issues.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "offown@ex.com", "offown").await;
    verify_user(&db, login_v["data"]["id"].as_str().expect("id")).await;
    create_repo(&app, &cookie, "box").await;

    // Seed an issue while enabled.
    let i = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.create","input":{"owner":"offown","name":"box","title":"seed"}}"#,
    )
    .await;
    assert_eq!(i["ok"], true, "seed issue — {i}");

    let off = rpc_json(
        &app,
        &cookie,
        &set_enabled_body("issues", "offown", "box", false),
    )
    .await;
    assert_eq!(off["ok"], true, "{off}");

    // Read + write + comment surfaces all reject with the stable code.
    for (label, body) in [
        (
            "issue.list",
            r#"{"procedure":"issue.list","input":{"owner":"offown","name":"box"}}"#,
        ),
        (
            "issue.get",
            r#"{"procedure":"issue.get","input":{"owner":"offown","name":"box","number":1}}"#,
        ),
        (
            "issue.create",
            r#"{"procedure":"issue.create","input":{"owner":"offown","name":"box","title":"nope"}}"#,
        ),
        (
            "issue.comments.list",
            r#"{"procedure":"issue.comments.list","input":{"owner":"offown","name":"box","number":1}}"#,
        ),
        (
            "issue.labels.set",
            r#"{"procedure":"issue.labels.set","input":{"owner":"offown","name":"box","number":1,"labelIds":[]}}"#,
        ),
        (
            "issue.links.list",
            r#"{"procedure":"issue.links.list","input":{"owner":"offown","name":"box","number":1}}"#,
        ),
    ] {
        let v = rpc_json(&app, &cookie, body).await;
        assert_eq!(v["ok"], false, "{label} must fail — {v}");
        assert_eq!(
            v["error"]["code"], "repo.issues.disabled",
            "{label} stable code — {v}"
        );
    }

    // Cross-unit isolation: pulls stay enabled.
    let pl = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.list","input":{"owner":"offown","name":"box"}}"#,
    )
    .await;
    assert_eq!(pl["ok"], true, "pull.list unaffected — {pl}");

    // Repo metadata surfaces stay live (labels are shared repo metadata).
    let ll = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"label.listForRepo","input":{"owner":"offown","name":"box"}}"#,
    )
    .await;
    assert_eq!(ll["ok"], true, "label.listForRepo unaffected — {ll}");

    // Re-enable restores the unit — seeded issue still there (no data loss).
    let on = rpc_json(
        &app,
        &cookie,
        &set_enabled_body("issues", "offown", "box", true),
    )
    .await;
    assert_eq!(on["ok"], true, "{on}");
    let l = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.list","input":{"owner":"offown","name":"box"}}"#,
    )
    .await;
    assert_eq!(l["ok"], true, "issue.list back — {l}");
    assert_eq!(l["data"]["total"], 1, "seeded issue preserved — {l}");
}

/// Disabled pulls reject `pull.*` surfaces (list/get/create/comments/files/
/// merge) with `repo.pulls.disabled`; issues stay live; repo settings
/// surfaces (`repo.mergeSettings.*`) remain usable.
#[tokio::test]
async fn disabled_pulls_unit_rejects_pull_rpcs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("units_pulls.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (cookie, login_v) = signup_and_login(&app, "poffown@ex.com", "poffown").await;
    verify_user(&db, login_v["data"]["id"].as_str().expect("id")).await;
    create_repo(&app, &cookie, "pbox").await;

    let off = rpc_json(
        &app,
        &cookie,
        &set_enabled_body("pulls", "poffown", "pbox", false),
    )
    .await;
    assert_eq!(off["ok"], true, "{off}");

    for (label, body) in [
        (
            "pull.list",
            r#"{"procedure":"pull.list","input":{"owner":"poffown","name":"pbox"}}"#,
        ),
        (
            "pull.get",
            r#"{"procedure":"pull.get","input":{"owner":"poffown","name":"pbox","number":1}}"#,
        ),
        (
            "pull.create",
            r#"{"procedure":"pull.create","input":{"owner":"poffown","name":"pbox","title":"x","base_ref":"main","head_ref":"main"}}"#,
        ),
        (
            "pull.files",
            r#"{"procedure":"pull.files","input":{"owner":"poffown","name":"pbox","number":1}}"#,
        ),
        (
            "pull.commits",
            r#"{"procedure":"pull.commits","input":{"owner":"poffown","name":"pbox","number":1}}"#,
        ),
        (
            "pull.comments.list",
            r#"{"procedure":"pull.comments.list","input":{"owner":"poffown","name":"pbox","number":1}}"#,
        ),
        (
            "pull.reviews.list",
            r#"{"procedure":"pull.reviews.list","input":{"owner":"poffown","name":"pbox","number":1}}"#,
        ),
        (
            "pull.reviewRequests.list",
            r#"{"procedure":"pull.reviewRequests.list","input":{"owner":"poffown","name":"pbox","number":1}}"#,
        ),
        (
            "pull.merge",
            r#"{"procedure":"pull.merge","input":{"owner":"poffown","name":"pbox","number":1,"method":"merge"}}"#,
        ),
    ] {
        let v = rpc_json(&app, &cookie, body).await;
        assert_eq!(v["ok"], false, "{label} must fail — {v}");
        assert_eq!(
            v["error"]["code"], "repo.pulls.disabled",
            "{label} stable code — {v}"
        );
    }

    // Issues stay live (cross-unit isolation).
    let il = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"issue.list","input":{"owner":"poffown","name":"pbox"}}"#,
    )
    .await;
    assert_eq!(il["ok"], true, "issue.list unaffected — {il}");

    // Settings surfaces stay usable while the unit is off (pre-configuration).
    let ms = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.mergeSettings.get","input":{"owner":"poffown","name":"pbox"}}"#,
    )
    .await;
    assert_eq!(ms["ok"], true, "mergeSettings.get unaffected — {ms}");

    // Re-enable restores list.
    let on = rpc_json(
        &app,
        &cookie,
        &set_enabled_body("pulls", "poffown", "pbox", true),
    )
    .await;
    assert_eq!(on["ok"], true, "{on}");
    let pl = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"pull.list","input":{"owner":"poffown","name":"pbox"}}"#,
    )
    .await;
    assert_eq!(pl["ok"], true, "pull.list back — {pl}");
}

/// Only repo admins may toggle — a verified non-collaborator gets the soft
/// `repo.not_found` (same convention as other Admin-gated repo RPCs).
#[tokio::test]
async fn unit_set_enabled_requires_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("units_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;

    let (owner_cookie, login_v) = signup_and_login(&app, "aclown@ex.com", "aclown").await;
    verify_user(&db, login_v["data"]["id"].as_str().expect("id")).await;
    create_repo(&app, &owner_cookie, "aclbox").await;

    let (reader_cookie, reader_v) = signup_and_login(&app, "aclread@ex.com", "aclread").await;
    verify_user(&db, reader_v["data"]["id"].as_str().expect("id")).await;

    // Non-admin (Read on public repo) cannot toggle — soft not_found.
    let denied = rpc_json(
        &app,
        &reader_cookie,
        &set_enabled_body("issues", "aclown", "aclbox", false),
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found", "{denied}");
    let denied = rpc_json(
        &app,
        &reader_cookie,
        &set_enabled_body("pulls", "aclown", "aclbox", false),
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found", "{denied}");

    // Anonymous cannot toggle either (require_verified → auth.unauthenticated).
    let anon = rpc_json_anon(&app, &set_enabled_body("issues", "aclown", "aclbox", false)).await;
    assert_eq!(anon["ok"], false, "{anon}");
    assert_eq!(anon["error"]["code"], "auth.unauthenticated", "{anon}");

    // Flag unchanged for real.
    let ge = rpc_json(
        &app,
        &owner_cookie,
        &get_enabled_body("issues", "aclown", "aclbox"),
    )
    .await;
    assert_eq!(ge["data"]["enabled"], true, "flag untouched — {ge}");
}
