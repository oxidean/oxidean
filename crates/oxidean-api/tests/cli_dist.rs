//! CLI distribution — `/cli/install.sh`, `/cli/latest`, `/cli/bin/*`.
//! Unauthenticated surface used by `ox self-update` + `curl | sh` installs.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{AppState, build_cors, router_with_state};
use oxidean_db::Database;
use tower::ServiceExt;

const FAKE_BINARY: &[u8] = b"#!/bin/sh\necho fake-ox\n";

/// One dist dir for the whole file — `OXIDEAN_CLI_DIST_DIR` is process env, so
/// parallel tests must share it rather than race to repoint it.
static DIST: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();

fn dist() -> &'static tempfile::TempDir {
    DIST.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ox-x86_64-unknown-linux-gnu"), FAKE_BINARY).unwrap();
        std::fs::write(dir.path().join("ox-aarch64-apple-darwin"), b"darwin-binary").unwrap();
        // Not served — wrong name shape.
        std::fs::write(dir.path().join("README.txt"), b"ignore me").unwrap();
        std::env::set_var("OXIDEAN_CLI_DIST_DIR", dir.path());
        std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "https://forge.example.com");
        dir
    })
}

async fn setup() -> axum::Router {
    dist();
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", tmp.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    );
    router_with_state(state, build_cors("development", None).unwrap())
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String, axum::http::HeaderMap) {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned(), headers)
}

#[tokio::test]
async fn install_script_serves_origin_substituted_shell() {
    let app = setup().await;
    let (status, body, headers) = get(&app, "/cli/install.sh").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/x-shellscript; charset=utf-8"
    );
    assert!(body.starts_with("#!/bin/sh"));
    assert!(body.contains("BASE=\"https://forge.example.com\""));
    assert!(!body.contains("@OXIDEAN_ORIGIN@"));
    assert!(body.contains("/cli/bin/ox-"));
}

#[tokio::test]
async fn latest_lists_targets_with_digests() {
    let app = setup().await;
    let (status, body, _) = get(&app, "/cli/latest").await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["version"].as_str().unwrap().len() >= 3);
    let targets = json["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 2, "{body}");
    let names: Vec<&str> = targets
        .iter()
        .map(|t| t["target"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"x86_64-unknown-linux-gnu"));
    assert!(names.contains(&"aarch64-apple-darwin"));
    // README.txt was excluded by the name filter.
    assert!(!names.contains(&"README.txt"));
    for t in targets {
        assert_eq!(t["sha256"].as_str().unwrap().len(), 64);
        assert!(t["bytes"].as_u64().unwrap() > 0);
    }
}

#[tokio::test]
async fn binary_serves_artifact_and_matching_digest() {
    let app = setup().await;

    let (status, body, headers) = get(&app, "/cli/bin/ox-x86_64-unknown-linux-gnu").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/octet-stream"
    );
    assert_eq!(body.as_bytes(), FAKE_BINARY);

    let (status, digest, _) = get(&app, "/cli/bin/ox-x86_64-unknown-linux-gnu.sha256").await;
    assert_eq!(status, StatusCode::OK);
    let hex = digest.split_whitespace().next().unwrap();
    assert_eq!(hex.len(), 64);
    assert!(digest.trim_end().ends_with("ox-x86_64-unknown-linux-gnu"));
}

#[tokio::test]
async fn binary_rejects_missing_and_traversal_names() {
    let app = setup().await;

    let (status, _, _) = get(&app, "/cli/bin/ox-riscv64-unknown-linux-gnu").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = get(&app, "/cli/bin/..%2F..%2Fetc%2Fpasswd").await;
    assert!(matches!(
        status,
        StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST
    ));

    let (status, body, _) = get(&app, "/cli/bin/README.txt").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
