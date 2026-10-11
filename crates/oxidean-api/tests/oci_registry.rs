//! OCI Distribution Spec — PKG-01 / D-PKG-15.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::packages::store::digest_of_bytes;
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(db: Database, packages_dir: std::path::PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_packages_dir(packages_dir);
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

fn encode_b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn basic_header(user: &str, password: &str) -> String {
    format!(
        "Basic {}",
        encode_b64(format!("{user}:{password}").as_bytes())
    )
}

async fn signup_login_pat(app: &axum::Router, db: &Database) -> (String, String) {
    let signup = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"oci@ex.com","username":"ociowner","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.login","input":{"identifier":"oci@ex.com","password":"password1","remember_me":false}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now).await.unwrap();

    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createFineGrained","input":{"name":"oci","repo_access":"all","contents":"read","packages":"write"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    let bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v["data"]["token"].as_str().unwrap().to_string();
    (cookie, token)
}

async fn push_blob(app: &axum::Router, token: &str, name: &str, bytes: &[u8]) -> String {
    let digest = digest_of_bytes(bytes);
    let start = Request::builder()
        .method("POST")
        .uri(format!("/v2/{name}/blobs/uploads/"))
        .header(header::AUTHORIZATION, basic_header("ociowner", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(start).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let loc = res
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let patch = Request::builder()
        .method("PATCH")
        .uri(&loc)
        .header(header::AUTHORIZATION, basic_header("ociowner", token))
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(bytes.to_vec()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(patch).await.unwrap().status(),
        StatusCode::ACCEPTED
    );

    let put = Request::builder()
        .method("PUT")
        .uri(format!("{loc}?digest={digest}"))
        .header(header::AUTHORIZATION, basic_header("ociowner", token))
        .body(Body::empty())
        .unwrap();
    let put_res = app.clone().oneshot(put).await.unwrap();
    assert_eq!(put_res.status(), StatusCode::CREATED);
    digest
}

async fn put_manifest(
    app: &axum::Router,
    token: &str,
    name: &str,
    reference: &str,
    body: &[u8],
) -> StatusCode {
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v2/{name}/manifests/{reference}"))
        .header(header::AUTHORIZATION, basic_header("ociowner", token))
        .header(
            header::CONTENT_TYPE,
            "application/vnd.oci.image.manifest.v1+json",
        )
        .body(Body::from(body.to_vec()))
        .unwrap();
    app.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn oci_registry_v2_discovery() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("d.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db, dir.path().join("pkg")).await;
    let res = app
        .oneshot(Request::builder().uri("/v2/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res
        .headers()
        .get("docker-distribution-api-version")
        .is_some());
}

#[tokio::test]
async fn oci_registry_anonymous_public_pull() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "http://localhost");
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("d.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("pkg")).await;
    let (_cookie, token) = signup_login_pat(&app, &db).await;
    let name = "ociowner/hello";
    let blob = b"layer-bytes";
    let _digest = push_blob(&app, &token, name, blob).await;
    let manifest = br#"{"schemaVersion":2,"layers":[]}"#;
    assert_eq!(
        put_manifest(&app, &token, name, "latest", manifest).await,
        StatusCode::CREATED
    );

    let get = Request::builder()
        .uri(format!("/v2/{name}/manifests/latest"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(get).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "expected anonymous GET blob/manifest for public package to succeed (D-PKG-05)"
    );
}

#[tokio::test]
async fn oci_registry_auth_push_manifest_blob() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "http://localhost");
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("d.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("pkg")).await;
    let (cookie, token) = signup_login_pat(&app, &db).await;
    let name = "ociowner/pushme";
    let blob = b"blob-data";
    let digest = push_blob(&app, &token, name, blob).await;
    let manifest = format!(r#"{{"schemaVersion":2,"config":{{"digest":"{digest}"}}}}"#);
    assert_eq!(
        put_manifest(&app, &token, name, "v1", manifest.as_bytes()).await,
        StatusCode::CREATED
    );

    // Cookie alone must not push
    let bad = Request::builder()
        .method("POST")
        .uri(format!("/v2/{name}/blobs/uploads/"))
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(bad).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "expected PAT Basic/Bearer push; Cookie header ignored"
    );
}

#[tokio::test]
async fn oci_registry_tags_list() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "http://localhost");
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("d.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("pkg")).await;
    let (_c, token) = signup_login_pat(&app, &db).await;
    let name = "ociowner/tags";
    let m = br#"{"schemaVersion":2}"#;
    assert_eq!(
        put_manifest(&app, &token, name, "a", m).await,
        StatusCode::CREATED
    );
    assert_eq!(
        put_manifest(&app, &token, name, "b", m).await,
        StatusCode::CREATED
    );
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/v2/{name}/tags/list"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let tags = v["tags"].as_array().unwrap();
    assert!(tags.iter().any(|t| t == "a") && tags.iter().any(|t| t == "b"));
}

#[tokio::test]
async fn oci_registry_digest_immutable_conflict() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "http://localhost");
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("d.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("pkg")).await;
    let (_c, token) = signup_login_pat(&app, &db).await;
    let name = "ociowner/immut";
    let body1 = br#"{"schemaVersion":2,"v":1}"#;
    let digest = digest_of_bytes(body1);
    assert_eq!(
        put_manifest(&app, &token, name, &digest, body1).await,
        StatusCode::CREATED
    );
    let body2 = br#"{"schemaVersion":2,"v":2}"#;
    // Wrong digest in path vs body → 400; put same digest path with different bytes after storing under digest key
    let status = put_manifest(&app, &token, name, &digest, body2).await;
    assert!(
        status == StatusCode::CONFLICT || status == StatusCode::BAD_REQUEST,
        "expected digest put conflict when bytes differ (D-PKG-10) — got {status}"
    );
}

#[tokio::test]
async fn oci_registry_tag_retarget_allowed() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "http://localhost");
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("d.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("pkg")).await;
    let (_c, token) = signup_login_pat(&app, &db).await;
    let name = "ociowner/retag";
    assert_eq!(
        put_manifest(&app, &token, name, "latest", br#"{"v":1}"#).await,
        StatusCode::CREATED
    );
    assert_eq!(
        put_manifest(&app, &token, name, "latest", br#"{"v":2}"#).await,
        StatusCode::CREATED,
        "expected tag PUT to retarget digest (D-PKG-10)"
    );
}

#[tokio::test]
async fn oci_registry_manifest_delete_admin() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "http://localhost");
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("d.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("pkg")).await;
    let (_c, token) = signup_login_pat(&app, &db).await;
    let name = "ociowner/del";
    assert_eq!(
        put_manifest(&app, &token, name, "gone", br#"{"x":1}"#).await,
        StatusCode::CREATED
    );
    let del = Request::builder()
        .method("DELETE")
        .uri(format!("/v2/{name}/manifests/gone"))
        .header(header::AUTHORIZATION, basic_header("ociowner", &token))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(del).await.unwrap();
    assert!(
        res.status() == StatusCode::ACCEPTED || res.status() == StatusCode::NO_CONTENT,
        "expected DELETE manifest only for Admin + package:write (D-PKG-06) — {}",
        res.status()
    );
}

#[tokio::test]
async fn oci_registry_cookie_header_ignored() {
    std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "http://localhost");
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("d.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("pkg")).await;
    let (cookie, _token) = signup_login_pat(&app, &db).await;
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v2/ociowner/x/blobs/uploads/")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "expected Cookie alone to be ignored on /v2 (Phase 8 D-12 lesson)"
    );
    assert!(res.headers().get(header::WWW_AUTHENTICATE).is_some());
}
