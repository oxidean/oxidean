//! packages.list / packages.deleteVersion RPC (PKG-05).

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

async fn setup() -> (axum::Router, Database, String, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("p.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = router_with_state(
        AppState::new(
            db.clone(),
            Arc::new(LogSink) as Arc<dyn EmailSender>,
            "development",
        )
        .with_packages_dir(dir.path().join("pkg")),
        build_cors("development", None).unwrap(),
    );
    let _ = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"auth.signup","input":{"email":"rpc@ex.com","username":"rpcown","password":"password1"}}"#,
            None,
        ))
        .await;
    let login = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"auth.login","input":{"identifier":"rpc@ex.com","password":"password1","remember_me":false}}"#,
            None,
        ))
        .await
        .unwrap();
    let cookie = login
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let uid = v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&uid, &now).await.unwrap();
    (app, db, cookie, uid, dir)
}

fn rpc(body: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1");
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    b.body(Body::from(body.to_owned())).unwrap()
}

async fn seed_package(db: &Database, uid: &str, name: &str, version: &str) -> String {
    let id = Uuid::new_v4().to_string();
    db.insert_package(&id, "user", uid, name, "generic", "public", None, "")
        .await
        .unwrap();
    let vid = Uuid::new_v4().to_string();
    db.insert_package_version(&vid, &id, version, None, "{}", Some(uid))
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn package_rpc_list_by_owner() {
    let (app, db, cookie, uid, _dir) = setup().await;
    seed_package(&db, &uid, "tool", "1.0.0").await;
    let res = app
        .oneshot(rpc(
            r#"{"procedure":"packages.list","input":{"owner":"rpcown"}}"#,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(
        v["data"]["packages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "tool"),
        "expected packages.list by owner — {v}"
    );
}

#[tokio::test]
async fn package_rpc_list_by_repo_link() {
    let (app, db, cookie, uid, _dir) = setup().await;
    // create repo via RPC
    let create = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"repo.create","input":{"name":"linked","visibility":"public","description":""}}"#,
            Some(&cookie),
        ))
        .await
        .unwrap();
    let cv: serde_json::Value =
        serde_json::from_slice(&create.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let repo_id = cv["data"]["id"].as_str().unwrap();
    let id = Uuid::new_v4().to_string();
    db.insert_package(
        &id,
        "user",
        &uid,
        "linkedpkg",
        "npm",
        "public",
        Some(repo_id),
        "",
    )
    .await
    .unwrap();
    let vid = Uuid::new_v4().to_string();
    db.insert_package_version(&vid, &id, "1.0.0", None, "{}", Some(&uid))
        .await
        .unwrap();
    let res = app
        .clone()
        .oneshot(rpc(
            &format!(r#"{{"procedure":"packages.list","input":{{"repository_id":"{repo_id}"}}}}"#),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(
        v["data"]["packages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "linkedpkg"),
        "expected packages.list filtered by repository_id"
    );

    // Anonymous viewers of a public repo must still see linked public packages (About sidebar).
    let anon = app
        .oneshot(rpc(
            &format!(r#"{{"procedure":"packages.list","input":{{"repository_id":"{repo_id}"}}}}"#),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(anon.status(), StatusCode::OK);
    let av: serde_json::Value =
        serde_json::from_slice(&anon.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(av["ok"], true, "{av}");
    assert!(
        av["data"]["packages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "linkedpkg"),
        "anonymous packages.list by repository_id — {av}"
    );
}

#[tokio::test]
async fn package_rpc_delete_version_admin_confirm() {
    let (app, db, cookie, uid, _dir) = setup().await;
    let pkg_id = seed_package(&db, &uid, "tool", "1.0.0").await;
    let res = app
        .oneshot(rpc(
            &format!(
                r#"{{"procedure":"packages.deleteVersion","input":{{"package_id":"{pkg_id}","version":"1.0.0","confirm":"tool@1.0.0"}}}}"#
            ),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["data"]["ok"], true);
}

#[tokio::test]
async fn package_rpc_delete_version_bad_confirm_rejected() {
    let (app, db, cookie, uid, _dir) = setup().await;
    let pkg_id = seed_package(&db, &uid, "tool", "1.0.0").await;
    let res = app
        .oneshot(rpc(
            &format!(
                r#"{{"procedure":"packages.deleteVersion","input":{{"package_id":"{pkg_id}","version":"1.0.0","confirm":"wrong"}}}}"#
            ),
            Some(&cookie),
        ))
        .await
        .unwrap();
    let status = res.status();
    let v: serde_json::Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(
        status == StatusCode::BAD_REQUEST
            || v["error"]["code"] == "packages.confirm_mismatch"
            || v["ok"] == false,
        "expected reject when confirm != name@version — status={status} {v}"
    );
}

#[tokio::test]
async fn package_rpc_admin_usage_and_set_quota() {
    use oxidean_api::auth::hash_password_str;

    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("a.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let hash = hash_password_str("password1").unwrap();
    let id = Uuid::new_v4().to_string();
    db.create_user(
        &id,
        "adm@ex.com",
        "pkgadmin",
        Some(&hash),
        "Admin",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&id, &now).await.unwrap();

    let app = router_with_state(
        AppState::new(
            db.clone(),
            Arc::new(LogSink) as Arc<dyn EmailSender>,
            "development",
        )
        .with_packages_dir(dir.path().join("pkg")),
        build_cors("development", None).unwrap(),
    );
    let login = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"auth.login","input":{"identifier":"adm@ex.com","password":"password1","remember_me":false}}"#,
            None,
        ))
        .await
        .unwrap();
    let cookie = login
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let set = app
        .clone()
        .oneshot(rpc(
            r#"{"procedure":"packages.adminSetQuota","input":{"owner":"pkgadmin","max_bytes":12345}}"#,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(set.status(), StatusCode::OK);
    let usage = app
        .oneshot(rpc(
            r#"{"procedure":"packages.adminUsage","input":{"owner":"pkgadmin"}}"#,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(usage.status(), StatusCode::OK);
    let v: serde_json::Value =
        serde_json::from_slice(&usage.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["data"]["quota_bytes"], 12345);
}

#[tokio::test]
async fn package_rpc_admin_usage_forbidden_for_normal_user() {
    let (app, _db, cookie, _uid, _dir) = setup().await;
    let res = app
        .oneshot(rpc(
            r#"{"procedure":"packages.adminUsage","input":{"owner":"rpcown"}}"#,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
