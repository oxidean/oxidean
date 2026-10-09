//! Generic/raw registry — PKG-03 / D-PKG-16 (tracer + list).

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
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
    let raw = format!("{user}:{password}");
    format!("Basic {}", encode_b64(raw.as_bytes()))
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

async fn mint_package_write_pat(app: &axum::Router, cookie: &str) -> String {
    let create_pat = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"pat.createFineGrained","input":{"name":"pkg-write","repo_access":"all","contents":"read","packages":"write"}}"#,
            cookie,
        ))
        .await
        .unwrap();
    assert_eq!(create_pat.status(), StatusCode::OK);
    let bytes = create_pat.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    v["data"]["token"].as_str().expect("token").to_string()
}

async fn setup_owner() -> (axum::Router, Database, String, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let packages = dir.path().join("packages");
    let url = format!("sqlite:{}", dir.path().join("generic.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), packages).await;
    let (cookie, login_v) = signup_and_login(&app, "pkg@ex.com", "pkgowner").await;
    let user_id = login_v["data"]["id"].as_str().unwrap().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    let token = mint_package_write_pat(&app, &cookie).await;
    (app, db, token, user_id, dir)
}

/// PUT /generic/<owner>/<name>/<version>/<file> uploads a file.
#[tokio::test]
async fn generic_registry_put_file() {
    let (app, _db, token, _uid, _dir) = setup_owner().await;
    let body = b"hello-generic-bytes";
    let req = Request::builder()
        .method("PUT")
        .uri("/generic/pkgowner/tool/1.0.0/app.bin?visibility=public")
        .header(header::AUTHORIZATION, basic_header("pkgowner", &token))
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(body.as_slice()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "expected PUT /generic with PAT auth to create file"
    );
}

/// GET downloads a previously uploaded file.
#[tokio::test]
async fn generic_registry_get_file() {
    let (app, _db, token, _uid, _dir) = setup_owner().await;
    let body = b"download-me";
    let put = Request::builder()
        .method("PUT")
        .uri("/generic/pkgowner/tool/1.0.0/app.bin?visibility=public")
        .header(header::AUTHORIZATION, basic_header("pkgowner", &token))
        .body(Body::from(body.as_slice()))
        .unwrap();
    let put_res = app.clone().oneshot(put).await.unwrap();
    assert_eq!(put_res.status(), StatusCode::CREATED);

    let get = Request::builder()
        .method("GET")
        .uri("/generic/pkgowner/tool/1.0.0/app.bin")
        .body(Body::empty())
        .unwrap();
    let get_res = app.oneshot(get).await.unwrap();
    assert_eq!(get_res.status(), StatusCode::OK);
    let bytes = get_res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], body, "expected GET to return stored bytes");
}

/// DELETE version removes all files for that version id.
#[tokio::test]
async fn generic_registry_delete_version() {
    let (app, _db, token, _uid, _dir) = setup_owner().await;
    let put = Request::builder()
        .method("PUT")
        .uri("/generic/pkgowner/tool/1.0.0/app.bin?visibility=public")
        .header(header::AUTHORIZATION, basic_header("pkgowner", &token))
        .body(Body::from(&b"x"[..]))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(put).await.unwrap().status(),
        StatusCode::CREATED
    );

    let del = Request::builder()
        .method("DELETE")
        .uri("/generic/pkgowner/tool/1.0.0")
        .header(header::AUTHORIZATION, basic_header("pkgowner", &token))
        .body(Body::empty())
        .unwrap();
    let del_res = app.clone().oneshot(del).await.unwrap();
    assert_eq!(
        del_res.status(),
        StatusCode::NO_CONTENT,
        "expected DELETE version to remove files (Admin + package:write)"
    );

    let get = Request::builder()
        .method("GET")
        .uri("/generic/pkgowner/tool/1.0.0/app.bin")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(get).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    // Republish allowed after delete (D-PKG-10).
    let put2 = Request::builder()
        .method("PUT")
        .uri("/generic/pkgowner/tool/1.0.0/app.bin?visibility=public")
        .header(header::AUTHORIZATION, basic_header("pkgowner", &token))
        .body(Body::from(&b"again"[..]))
        .unwrap();
    assert_eq!(
        app.oneshot(put2).await.unwrap().status(),
        StatusCode::CREATED
    );
}

/// Overwrite of an existing version returns 409 (immutable versions).
#[tokio::test]
async fn generic_registry_overwrite_conflict() {
    let (app, _db, token, _uid, _dir) = setup_owner().await;
    let put1 = Request::builder()
        .method("PUT")
        .uri("/generic/pkgowner/tool/1.0.0/app.bin?visibility=public")
        .header(header::AUTHORIZATION, basic_header("pkgowner", &token))
        .body(Body::from(&b"v1"[..]))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(put1).await.unwrap().status(),
        StatusCode::CREATED
    );

    let put2 = Request::builder()
        .method("PUT")
        .uri("/generic/pkgowner/tool/1.0.0/app.bin?visibility=public")
        .header(header::AUTHORIZATION, basic_header("pkgowner", &token))
        .body(Body::from(&b"v2"[..]))
        .unwrap();
    let res = app.oneshot(put2).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::CONFLICT,
        "expected 409 on version/file overwrite while present (D-PKG-10)"
    );
}

/// List versions/files for a generic package.
#[tokio::test]
async fn generic_registry_list() {
    let (app, _db, token, _uid, _dir) = setup_owner().await;
    for (ver, file) in [("1.0.0", "a.bin"), ("1.0.1", "b.bin")] {
        let put = Request::builder()
            .method("PUT")
            .uri(format!(
                "/generic/pkgowner/tool/{ver}/{file}?visibility=public"
            ))
            .header(header::AUTHORIZATION, basic_header("pkgowner", &token))
            .body(Body::from(&b"payload"[..]))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(put).await.unwrap().status(),
            StatusCode::CREATED
        );
    }

    let list = Request::builder()
        .method("GET")
        .uri("/generic/pkgowner/tool")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(list).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let versions = v["versions"].as_array().expect("versions array");
    assert!(
        versions.len() >= 2,
        "expected list of versions/files for owner/name — got {v}"
    );

    // OCI discovery mount must answer from Axum (not SPA).
    let v2 = Request::builder()
        .method("GET")
        .uri("/v2/")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(v2).await.unwrap().status(),
        StatusCode::OK,
        "GET /v2/ discovery"
    );
}
