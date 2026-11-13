//! Comment-attachment upload/serve routes: auth, PAT scope gate, quota, GC refs.

mod support;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

/// Upload quota env vars are process-global — serialize every test in this
/// file so the quota case cannot leak `MAX_COUNT=1` into a sibling test.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    set_cookie.split(';').next().unwrap().trim().to_string()
}

async fn signup(app: &axum::Router, email: &str, username: &str) -> (String, String) {
    let res = app
        .clone()
        .oneshot(rpc_req(&format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        )))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&res);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().expect("id").to_string();
    (cookie, user_id)
}

fn upload_req(uri: &str, body: Vec<u8>, auth: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/octet-stream");
    if let Some(auth) = auth {
        b = b.header(header::AUTHORIZATION, auth);
    }
    b.body(Body::from(body)).unwrap()
}

async fn app_with_user(tag: &str) -> (tempfile::TempDir, axum::Router, Database, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let uploads = dir.path().join("uploads");
    let url = format!("sqlite:{}", dir.path().join(format!("{tag}.db")).display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), &uploads).await;
    let (cookie, user_id) = signup(&app, &format!("{tag}@ex.com"), tag).await;
    // PAT minting requires a verified email (require_verified).
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");
    (dir, app, db, cookie, user_id)
}

#[tokio::test]
async fn upload_requires_auth() {
    let _g = SERIAL.lock().await;
    let (_dir, app, _db, _cookie, _uid) = app_with_user("noauth").await;
    let res = app
        .oneshot(upload_req(
            "/api/uploads/comment-attachments?name=a.png&content_type=image/png",
            b"png".to_vec(),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn upload_and_serve_round_trip() {
    let _g = SERIAL.lock().await;
    let (dir, app, _db, cookie, user_id) = app_with_user("roundtrip").await;

    // Cookie session upload.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/uploads/comment-attachments?name=pic.png&content_type=image/png")
                .header("cookie", &cookie)
                .body(Body::from(b"pngbytes".to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    let id = v["data"]["id"].as_str().unwrap().to_string();
    let url = v["data"]["url"].as_str().unwrap().to_string();
    assert_eq!(url, format!("/uploads/comment-attachments/{id}"));

    // Sidecar attributes the upload to the session user.
    let meta_path = dir
        .path()
        .join("uploads/comment-attachments")
        .join(format!("{id}.json"));
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&meta_path).expect("meta file")).unwrap();
    assert_eq!(meta["uploader_id"], user_id);
    assert_eq!(meta["size"], 8);

    // Raster image serves inline.
    let img = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(img.status(), StatusCode::OK);
    assert_eq!(
        img.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("image/png")
    );
    assert_eq!(
        img.headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok()),
        Some("inline")
    );
    assert_eq!(
        img.headers()
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );

    // Non-image serves as an attachment download.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/uploads/comment-attachments?name=notes.txt&content_type=text/plain")
                .header("cookie", &cookie)
                .body(Body::from(b"hello".to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let txt_url = v["data"]["url"].as_str().unwrap().to_string();
    let txt = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&txt_url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(txt.status(), StatusCode::OK);
    let cd = txt
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(cd.starts_with("attachment"), "disposition: {cd}");
    assert!(cd.contains("notes.txt"), "disposition: {cd}");
}

#[tokio::test]
async fn serve_rejects_unknown_traversal_and_bad_ids() {
    let _g = SERIAL.lock().await;
    let (_dir, app, _db, _cookie, _uid) = app_with_user("serve404").await;
    for uri in [
        "/uploads/comment-attachments/not-a-uuid",
        "/uploads/comment-attachments/00000000-0000-0000-0000-000000000000",
        "/uploads/comment-attachments/../secret",
        "/uploads/comment-attachments/..%2F..%2Fetc%2Fpasswd",
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
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "uri {uri}");
    }
}

#[tokio::test]
async fn upload_rejects_oversize() {
    let _g = SERIAL.lock().await;
    let (_dir, app, _db, cookie, _uid) = app_with_user("oversize").await;
    let body = vec![0u8; 10 * 1024 * 1024 + 1];
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/uploads/comment-attachments?name=big.bin")
                .header("cookie", &cookie)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn pat_scope_gate() {
    let _g = SERIAL.lock().await;
    let (_dir, app, _db, cookie, _uid) = app_with_user("patgate").await;

    async fn mint(app: &axum::Router, cookie: &str, body: &str) -> String {
        let res = app
            .clone()
            .oneshot(rpc_req_with_cookie(body, cookie))
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!(
                "mint {status} non-json body={:?}: {e}",
                String::from_utf8_lossy(&bytes)
            )
        });
        assert_eq!(v["ok"], true, "mint failed: {v}");
        v["data"]["token"].as_str().unwrap().to_string()
    }

    let uri = "/api/uploads/comment-attachments?name=p.png&content_type=image/png";

    // Classic PAT without repo/public_repo is rejected.
    let weak = mint(
        &app,
        &cookie,
        r#"{"procedure":"pat.createClassic","input":{"name":"weak","scopes":["notifications"]}}"#,
    )
    .await;
    let res = app
        .clone()
        .oneshot(upload_req(
            uri,
            b"x".to_vec(),
            Some(&format!("Bearer {weak}")),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Classic PAT with repo scope passes.
    let strong = mint(
        &app,
        &cookie,
        r#"{"procedure":"pat.createClassic","input":{"name":"strong","scopes":["repo"]}}"#,
    )
    .await;
    let res = app
        .clone()
        .oneshot(upload_req(
            uri,
            b"x".to_vec(),
            Some(&format!("Bearer {strong}")),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Fine-grained PAT with no comment-domain write is rejected.
    let ro = mint(
        &app,
        &cookie,
        r#"{"procedure":"pat.createFineGrained","input":{"name":"ro","repo_access":"all","contents":"read"}}"#,
    )
    .await;
    let res = app
        .clone()
        .oneshot(upload_req(
            uri,
            b"x".to_vec(),
            Some(&format!("Bearer {ro}")),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Fine-grained PAT with issues:write passes.
    let wr = mint(
        &app,
        &cookie,
        r#"{"procedure":"pat.createFineGrained","input":{"name":"wr","repo_access":"all","contents":"read","perms":{"issues":"write"}}}"#,
    )
    .await;
    let res = app
        .clone()
        .oneshot(upload_req(
            uri,
            b"x".to_vec(),
            Some(&format!("Bearer {wr}")),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn quota_count_enforced() {
    let _g = SERIAL.lock().await;
    let (_dir, app, _db, cookie, _uid) = app_with_user("quota").await;

    std::env::set_var("OXIDEAN_COMMENT_ATTACHMENT_MAX_COUNT_PER_USER", "1");
    let result = async {
        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/uploads/comment-attachments?name=one.png")
                    .header("cookie", &cookie)
                    .body(Body::from(b"1".to_vec()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        let second = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/uploads/comment-attachments?name=two.png")
                    .header("cookie", &cookie)
                    .body(Body::from(b"2".to_vec()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::INSUFFICIENT_STORAGE);
        let bytes = second.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["code"], "upload.quota_exceeded");
    }
    .await;
    std::env::remove_var("OXIDEAN_COMMENT_ATTACHMENT_MAX_COUNT_PER_USER");
    result
}

fn write_attachment_pair(
    uploads: &std::path::Path,
    id: &str,
    created_at: &str,
    meta_json: Option<&str>,
) {
    let dir = uploads.join("comment-attachments");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(id), b"blob").unwrap();
    let meta = meta_json.map(str::to_string).unwrap_or_else(|| {
        format!(
            r#"{{"name":"a.png","content_type":"image/png","size":4,"uploader_id":"u","created_at":"{created_at}"}}"#
        )
    });
    std::fs::write(dir.join(format!("{id}.json")), meta).unwrap();
}

#[tokio::test]
async fn gc_collects_orphans_and_keeps_referenced_recent_and_malformed() {
    let _g = SERIAL.lock().await;
    let (dir, app, db, cookie, _uid) = app_with_user("gc").await;
    let uploads = dir.path().join("uploads");

    let old = chrono::Utc::now() - chrono::Duration::days(30);
    let old_ts = old.to_rfc3339();
    let referenced_id = uuid::Uuid::new_v4().to_string();
    let orphan_id = uuid::Uuid::new_v4().to_string();
    let recent_id = uuid::Uuid::new_v4().to_string();
    let malformed_id = uuid::Uuid::new_v4().to_string();

    // Old + referenced via a saved reply → must survive.
    write_attachment_pair(&uploads, &referenced_id, &old_ts, None);
    // Old + unreferenced → collected.
    write_attachment_pair(&uploads, &orphan_id, &old_ts, None);
    // Fresh + unreferenced → kept by grace.
    write_attachment_pair(&uploads, &recent_id, &chrono::Utc::now().to_rfc3339(), None);
    // Malformed sidecar → kept (ambiguous).
    let mdir = uploads.join("comment-attachments");
    std::fs::create_dir_all(&mdir).unwrap();
    std::fs::write(mdir.join(&malformed_id), b"blob").unwrap();
    std::fs::write(mdir.join(format!("{malformed_id}.json")), b"not json").unwrap();

    // Persist a reference to the old attachment.
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(
            &format!(
                r#"{{"procedure":"savedReplies.create","input":{{"name":"r","body":"see ![x](/uploads/comment-attachments/{referenced_id})"}}}}"#
            ),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true, "{v}");

    let stats = oxidean_api_core::comment_attachments::gc_orphan_comment_attachments(
        &db,
        &uploads,
        std::time::Duration::ZERO,
    )
    .await
    .expect("gc");
    assert_eq!(stats.removed, 1, "{stats:?}");
    assert_eq!(stats.kept_referenced, 1, "{stats:?}");
    assert_eq!(stats.kept_recent, 1, "{stats:?}");
    assert_eq!(stats.errors, 1, "{stats:?}");

    let dir_path = uploads.join("comment-attachments");
    assert!(!dir_path.join(&orphan_id).exists(), "orphan data removed");
    assert!(!dir_path.join(format!("{orphan_id}.json")).exists());
    assert!(dir_path.join(&referenced_id).exists(), "referenced kept");
    assert!(dir_path.join(format!("{referenced_id}.json")).exists());
    assert!(dir_path.join(&recent_id).exists(), "recent kept");
    assert!(dir_path.join(&malformed_id).exists(), "malformed kept");
}
