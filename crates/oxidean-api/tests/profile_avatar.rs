//! AUTH-08: profile update + avatar multipart round-trip + traversal/size rejects.

mod support;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(db: Database, uploads: &std::path::Path) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_uploads_dir(uploads.to_path_buf());
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
    let pair = set_cookie.split(';').next().unwrap().trim();
    assert!(
        pair.starts_with("oxidean_session="),
        "unexpected Set-Cookie: {set_cookie}"
    );
    pair.to_string()
}

/// Minimal valid 1×1 RGB PNG.
fn tiny_png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xC9, 0xFE, 0x92, 0xEF, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

fn multipart_avatar(png: &[u8], boundary: &str) -> (String, Vec<u8>) {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"avatar\"; filename=\"photo.png\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[tokio::test]
async fn profile_update_and_avatar_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let uploads = dir.path().join("uploads");
    let url = format!("sqlite:{}", dir.path().join("profile.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db, &uploads).await;

    let signup = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"p@ex.com","username":"profiler","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&signup);
    let signup_bytes = signup.into_body().collect().await.unwrap().to_bytes();
    let signup_v: serde_json::Value = serde_json::from_slice(&signup_bytes).unwrap();
    let user_id = signup_v["data"]["id"].as_str().expect("id").to_string();

    let update = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"user.update_profile","input":{"display_name":"Pat Profile","username":"profiler","bio":"hello bio"}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::OK);
    let update_bytes = update.into_body().collect().await.unwrap().to_bytes();
    let update_v: serde_json::Value = serde_json::from_slice(&update_bytes).unwrap();
    assert_eq!(update_v["ok"], true);
    assert_eq!(update_v["data"]["display_name"], "Pat Profile");
    assert_eq!(update_v["data"]["bio"], "hello bio");

    let get = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"user.get_profile","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let get_bytes = get.into_body().collect().await.unwrap().to_bytes();
    let get_v: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert_eq!(get_v["data"]["display_name"], "Pat Profile");

    let png = tiny_png();
    let boundary = "----oxideanboundary";
    let (ct, body) = multipart_avatar(&png, boundary);
    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/avatar")
                .header("content-type", ct)
                .header("cookie", &cookie)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let upload_status = upload.status();
    let upload_bytes = upload.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        upload_status,
        StatusCode::OK,
        "avatar upload failed: {upload_status} body={}",
        String::from_utf8_lossy(&upload_bytes)
    );
    let upload_v: serde_json::Value = serde_json::from_slice(&upload_bytes).unwrap();
    let expected_url = format!("/uploads/avatars/{user_id}.webp");
    assert_eq!(upload_v["avatar_url"], expected_url);

    let get_avatar = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&expected_url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_avatar.status(), StatusCode::OK);
    let ct = get_avatar
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(ct, "image/webp");
    let avatar_bytes = get_avatar.into_body().collect().await.unwrap().to_bytes();
    assert!(!avatar_bytes.is_empty());
}

#[tokio::test]
async fn avatar_delete_clears_profile_and_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let uploads = dir.path().join("uploads");
    let url = format!("sqlite:{}", dir.path().join("del.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), &uploads).await;

    let signup = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"del@ex.com","username":"deluser","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    let cookie = session_cookie_from_response(&signup);
    let signup_bytes = signup.into_body().collect().await.unwrap().to_bytes();
    let signup_v: serde_json::Value = serde_json::from_slice(&signup_bytes).unwrap();
    let user_id = signup_v["data"]["id"].as_str().expect("id").to_string();

    let png = tiny_png();
    let boundary = "----delboundary";
    let (ct, body) = multipart_avatar(&png, boundary);
    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/avatar")
                .header("content-type", ct)
                .header("cookie", &cookie)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::OK);
    let expected_url = format!("/uploads/avatars/{user_id}.webp");
    let on_disk = uploads.join("avatars").join(format!("{user_id}.webp"));
    assert!(on_disk.is_file(), "avatar file should exist after upload");

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/user/avatar")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let delete_status = delete.status();
    let delete_bytes = delete.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        delete_status,
        StatusCode::OK,
        "avatar delete failed: {delete_status} body={}",
        String::from_utf8_lossy(&delete_bytes)
    );
    let delete_v: serde_json::Value = serde_json::from_slice(&delete_bytes).unwrap();
    assert_eq!(delete_v["ok"], true);
    assert!(!on_disk.exists(), "avatar file should be removed");

    let get = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            r#"{"procedure":"user.get_profile","input":{}}"#,
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let get_bytes = get.into_body().collect().await.unwrap().to_bytes();
    let get_v: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert!(
        get_v["data"]["avatar_url"].is_null(),
        "profile avatar_url should be cleared, got {}",
        get_v["data"]["avatar_url"]
    );

    let serve = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&expected_url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(serve.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn avatar_rejects_oversized_upload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let uploads = dir.path().join("uploads");
    let url = format!("sqlite:{}", dir.path().join("big.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db, &uploads).await;

    let signup = app
        .clone()
        .oneshot(rpc_req(
            r#"{"procedure":"auth.signup","input":{"email":"big@ex.com","username":"biguser","password":"password1"}}"#,
        ))
        .await
        .unwrap();
    let cookie = session_cookie_from_response(&signup);

    // Payload just over 2 MiB (raw field body, not a real image — limit checked first).
    let oversized = vec![0u8; 2 * 1024 * 1024 + 1];
    let boundary = "----bigboundary";
    let (ct, body) = multipart_avatar(&oversized, boundary);

    let upload = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/avatar")
                .header("content-type", ct)
                .header("cookie", &cookie)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        upload.status() == StatusCode::PAYLOAD_TOO_LARGE
            || upload.status() == StatusCode::BAD_REQUEST,
        "expected reject, got {}",
        upload.status()
    );
}

#[tokio::test]
async fn avatar_serve_rejects_path_traversal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let uploads = dir.path().join("uploads");
    std::fs::create_dir_all(uploads.join("avatars")).unwrap();
    let url = format!("sqlite:{}", dir.path().join("trav.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db, &uploads).await;

    for uri in [
        "/uploads/avatars/../Cargo.toml",
        "/uploads/avatars/..%2F..%2Fetc%2Fpasswd",
        "/uploads/avatars/foo/bar.webp",
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "expected 404 for {uri}, got {}",
            res.status()
        );
    }
}
