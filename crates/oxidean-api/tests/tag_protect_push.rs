//! GIT-21: Protected tag rulesets — create/update/delete on `refs/tags/*` denied
//! for non-bypass actors, allowed when the matching rule permits the action.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::git::bare_repo_path;
use oxidean_api::protection::{
    check_ref_update, evaluate_tag_push, hooks_installed, union_tag_rules, TagProtectionIntent,
    ZERO_SHA,
};
use oxidean_api::repo::Capability;
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::{Database, TagProtectionRuleRow};
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

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn deny_reasons(err: &oxidean_core::AppError) -> Vec<String> {
    err.data.as_ref().unwrap()["reasons"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| r.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// GIT-21: `v*` rule with no allows → Write create denied (reason `create`),
/// Admin bypasses while enforce_admins=false.
#[tokio::test]
async fn tag_protect_push_denies_create_for_write() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let db_path = dir.path().join("tp_push.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "owner@ex.com", "tpown").await;
    verify_user(&db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let bare = bare_repo_path(&repos, "tpown", "core").expect("bare path");
    assert!(
        hooks_installed(&bare).await,
        "init_bare must install protection hooks (D-19)"
    );

    let rule = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.tagProtection.create","input":{"owner":"tpown","name":"core","pattern":"v*"}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "create tag rule — {rule}");

    // Write create on matching tag denied.
    let err = check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/tags/v1.0.0",
        ZERO_SHA,
        SHA_A,
        Capability::Write,
    )
    .await
    .expect_err("Write create must be denied on protected tag");
    assert_eq!(err.code, "repo.tag_protection");
    assert!(
        deny_reasons(&err).iter().any(|r| r == "create"),
        "expected create reason — {err:?}"
    );

    // Write update (retarget) denied.
    let err = check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/tags/v1.0.0",
        SHA_A,
        SHA_B,
        Capability::Write,
    )
    .await
    .expect_err("Write update must be denied on protected tag");
    assert!(deny_reasons(&err).iter().any(|r| r == "update"), "{err:?}");

    // Write delete denied.
    let err = check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/tags/v1.0.0",
        SHA_A,
        ZERO_SHA,
        Capability::Write,
    )
    .await
    .expect_err("Write delete must be denied on protected tag");
    assert!(deny_reasons(&err).iter().any(|r| r == "delete"), "{err:?}");

    // Admin bypass when enforce_admins=false (default).
    check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/tags/v1.0.0",
        ZERO_SHA,
        SHA_A,
        Capability::Admin,
    )
    .await
    .expect("Admin may create when enforce_admins=false");

    // Non-matching tag is unaffected.
    check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/tags/nightly-1",
        ZERO_SHA,
        SHA_A,
        Capability::Write,
    )
    .await
    .expect("non-matching tag pattern must allow create");
}

/// GIT-21: `allow_create` on the rule lets Write create while delete stays denied.
#[tokio::test]
async fn tag_protect_push_allows_create_when_rule_permits() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let db_path = dir.path().join("tp_allow.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "o2@ex.com", "tpown2").await;
    verify_user(&db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let rule = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.tagProtection.create","input":{"owner":"tpown2","name":"core","pattern":"v*","allow_create":true}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "create tag rule — {rule}");

    let bare = bare_repo_path(&repos, "tpown2", "core").expect("bare path");

    check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/tags/v2.0",
        ZERO_SHA,
        SHA_A,
        Capability::Write,
    )
    .await
    .expect("allow_create must permit Write create");
    let err = check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/tags/v2.0",
        SHA_A,
        ZERO_SHA,
        Capability::Write,
    )
    .await
    .expect_err("delete still denied when allow_delete=false");
    assert!(deny_reasons(&err).iter().any(|r| r == "delete"), "{err:?}");
    let err = check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/tags/v2.0",
        SHA_A,
        SHA_B,
        Capability::Write,
    )
    .await
    .expect_err("update still denied when allow_update=false");
    assert!(deny_reasons(&err).iter().any(|r| r == "update"), "{err:?}");
}

/// GIT-21: `enforce_admins` removes the Admin bypass.
#[tokio::test]
async fn tag_protect_push_enforce_admins_blocks_admin() {
    let dir = tempfile::tempdir().unwrap();
    let repos = dir.path().join("repos");
    let db_path = dir.path().join("tp_admin.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos.clone()).await;

    let (owner_cookie, owner_login) = signup_and_login(&app, "o3@ex.com", "tpown3").await;
    verify_user(&db, owner_login["data"]["id"].as_str().unwrap()).await;
    let create = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.create","input":{"name":"core","visibility":"public","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let rule = rpc_json(
        &app,
        &owner_cookie,
        r#"{"procedure":"repo.tagProtection.create","input":{"owner":"tpown3","name":"core","pattern":"v*","enforce_admins":true}}"#,
    )
    .await;
    assert_eq!(rule["ok"], true, "create tag rule — {rule}");

    let bare = bare_repo_path(&repos, "tpown3", "core").expect("bare path");
    let err = check_ref_update(
        &db,
        &repos,
        &bare,
        "refs/tags/v9",
        ZERO_SHA,
        SHA_A,
        Capability::Admin,
    )
    .await
    .expect_err("enforce_admins must deny Admin create");
    assert_eq!(err.code, "repo.tag_protection");
}

/// Union semantics: most restrictive wins across overlapping rules (GIT-21).
#[test]
fn tag_union_most_restrictive_wins() {
    let base = TagProtectionRuleRow {
        id: "1".into(),
        repo_id: "r".into(),
        pattern: "v*".into(),
        allow_create: true,
        allow_update: true,
        allow_delete: true,
        enforce_admins: false,
        created_at: String::new(),
        updated_at: String::new(),
    };
    let mut strict = base.clone();
    strict.id = "2".into();
    strict.pattern = "v1*".into();
    strict.allow_delete = false;
    strict.enforce_admins = true;

    let eff = union_tag_rules(&[base, strict], "v1.2");
    assert!(eff.matched);
    assert!(eff.allow_create);
    assert!(eff.allow_update);
    assert!(!eff.allow_delete, "any disallow must win in the union");
    assert!(eff.enforce_admins);

    let err = evaluate_tag_push(&eff, TagProtectionIntent::Delete, Some(Capability::Admin))
        .expect_err("enforce_admins + no allow_delete denies admin delete");
    assert_eq!(err.code, "repo.tag_protection");

    let unmatched = union_tag_rules(&[], "v1.2");
    assert!(!unmatched.matched);
    evaluate_tag_push(
        &unmatched,
        TagProtectionIntent::Delete,
        Some(Capability::Read),
    )
    .expect("no matching rule → allowed");
}
