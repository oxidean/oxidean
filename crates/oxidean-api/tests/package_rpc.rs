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

/// GitHub-shaped packages REST: `/user/packages`, `/users/{u}/packages`,
/// `/orgs/{o}/packages` — list/get/versions/delete, `container`→`oci`
/// package_type mapping, ACL-aware anonymous reads.
#[tokio::test]
async fn package_rest_github_surface() {
    let (app, db, cookie, uid, _dir) = setup().await;

    // Seed: one generic package (2 versions) + one oci package.
    let pkg_id = seed_package(&db, &uid, "tool", "1.0.0").await;
    let vid2 = Uuid::new_v4().to_string();
    db.insert_package_version(&vid2, &pkg_id, "2.0.0", None, "{}", Some(&uid))
        .await
        .unwrap();
    let oci_id = Uuid::new_v4().to_string();
    db.insert_package(&oci_id, "user", &uid, "img", "oci", "public", None, "")
        .await
        .unwrap();
    let ovid = Uuid::new_v4().to_string();
    db.insert_package_version(&ovid, &oci_id, "sha256:abc", None, "{}", Some(&uid))
        .await
        .unwrap();

    // Org package for the /orgs scope.
    let org = db
        .insert_organization("o-pkgr", "pkgr", "Pkg Org", "read")
        .await
        .unwrap();
    let opkg = Uuid::new_v4().to_string();
    db.insert_package(&opkg, "org", &org.id, "lib", "generic", "public", None, "")
        .await
        .unwrap();
    let opv = Uuid::new_v4().to_string();
    db.insert_package_version(&opv, &opkg, "0.1.0", None, "{}", Some(&uid))
        .await
        .unwrap();

    async fn rest(
        app: &axum::Router,
        method: &str,
        uri: &str,
        cookie: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let mut b = Request::builder().method(method).uri(uri.to_string());
        if let Some(c) = cookie {
            b = b.header("cookie", c.to_string());
        }
        let res = app
            .clone()
            .oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::json!(null));
        (status, v)
    }

    // GET /user/packages — session user; GitHub shape, oci → container.
    let (status, v) = rest(&app, "GET", "/api/v1/user/packages", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 2, "{v}");
    let img = arr.iter().find(|p| p["name"] == "img").expect("img");
    assert_eq!(img["package_type"], "container");
    assert_eq!(img["version_count"], 1);
    assert_eq!(img["visibility"], "public");
    assert_eq!(img["owner"]["login"], "rpcown");
    let tool = arr.iter().find(|p| p["name"] == "tool").expect("tool");
    assert_eq!(tool["package_type"], "generic");
    assert_eq!(tool["version_count"], 2);
    assert!(tool["created_at"].as_str().unwrap().len() > 8);

    // package_type + visibility filters.
    let (status, v) = rest(
        &app,
        "GET",
        "/api/v1/user/packages?package_type=container",
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 1, "{v}");

    // Anonymous /users/{u}/packages sees the public packages.
    let (status, v) = rest(&app, "GET", "/api/v1/users/rpcown/packages", None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v.as_array().unwrap().len(), 2, "{v}");

    // GET single + versions + version detail (version_id = row id).
    let (status, v) = rest(
        &app,
        "GET",
        "/api/v1/user/packages/generic/tool",
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["version_count"], 2);

    let (status, v) = rest(
        &app,
        "GET",
        "/api/v1/user/packages/generic/tool/versions",
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let versions = v.as_array().unwrap();
    assert_eq!(versions.len(), 2);
    let v2 = versions.iter().find(|x| x["name"] == "2.0.0").unwrap();
    let v2_id = v2["id"].as_str().unwrap().to_string();

    let (status, v) = rest(
        &app,
        "GET",
        &format!("/api/v1/user/packages/generic/tool/versions/{v2_id}"),
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "2.0.0");

    // Org scope reads.
    let (status, v) = rest(&app, "GET", "/api/v1/orgs/pkgr/packages", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert!(v.as_array().unwrap().iter().any(|p| p["name"] == "lib"));

    // Missing package → 404.
    let (status, _) = rest(&app, "GET", "/api/v1/user/packages/npm/nope", Some(&cookie)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // DELETE a version by id → 204; the package remains (one version left).
    let (status, _) = rest(
        &app,
        "DELETE",
        &format!("/api/v1/user/packages/generic/tool/versions/{v2_id}"),
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let remaining = db.list_package_versions(&pkg_id).await.unwrap();
    assert_eq!(remaining.len(), 1);

    // DELETE the package → 204; row gone.
    let (status, _) = rest(
        &app,
        "DELETE",
        "/api/v1/user/packages/generic/tool",
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(db.find_package_by_id(&pkg_id).await.unwrap().is_none());

    // Org-scope delete: rpcown isn't a member of `pkgr` → 403; grant org
    // owner and the same call succeeds.
    let (status, _) = rest(
        &app,
        "DELETE",
        "/api/v1/orgs/pkgr/packages/generic/lib",
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "non-member org delete");
    db.insert_org_member(&org.id, &uid, "owner").await.unwrap();
    let (status, _) = rest(
        &app,
        "DELETE",
        "/api/v1/orgs/pkgr/packages/generic/lib",
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "org owner delete");
    assert!(db.find_package_by_id(&opkg).await.unwrap().is_none());

    // A private package stays invisible to anonymous /users reads.
    let priv_id = Uuid::new_v4().to_string();
    db.insert_package(
        &priv_id, "user", &uid, "hidden", "generic", "private", None, "",
    )
    .await
    .unwrap();
    let pvv = Uuid::new_v4().to_string();
    db.insert_package_version(&pvv, &priv_id, "9.9.9", None, "{}", Some(&uid))
        .await
        .unwrap();
    let (status, v) = rest(&app, "GET", "/api/v1/users/rpcown/packages", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!v.as_array().unwrap().iter().any(|p| p["name"] == "hidden"));

    // Unauthenticated /user/packages → 401.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/user/packages")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
