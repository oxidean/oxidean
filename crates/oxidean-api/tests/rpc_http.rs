use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::{build_cors, router};
use oxidean_db::Database;
use tower::ServiceExt;

fn test_app() -> axum::Router {
    let cors = build_cors("development", None).expect("cors");
    router(Database::skipped(), cors)
}

#[tokio::test]
async fn health_ok() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn system_health_requires_version() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"procedure":"system.health","input":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "rpc.version_mismatch");
}

#[tokio::test]
async fn system_health_ok() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(r#"{"procedure":"system.health","input":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["status"], "ok");
    assert_eq!(v["data"]["database"], "skipped");
}

#[tokio::test]
async fn system_echo_ok() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(
                    r#"{"procedure":"system.echo","input":{"message":"hi"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["data"]["message"], "hi");
}

#[tokio::test]
async fn system_manifest_shape() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(
                    r#"{"procedure":"system.manifest","input":{}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    let data = &v["data"];
    assert_eq!(data["protocol_version"], 1);
    assert_eq!(
        data["server_version"].as_str().unwrap(),
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(data["min_cli_version"].as_str().unwrap(), "0.1.0");
    // Every entry is `true`, and known procedures are advertised.
    let procedures = data["procedures"].as_object().unwrap();
    assert!(procedures.len() > 100, "procedures: {}", procedures.len());
    for proc in [
        "system.health",
        "system.manifest",
        "repo.listMine",
        "issue.create",
        "pull.get",
        "packages.list",
    ] {
        assert_eq!(procedures.get(proc), Some(&serde_json::json!(true)), "{proc}");
    }
    assert!(procedures.values().all(|v| *v == serde_json::json!(true)));
    let caps = &data["capabilities"];
    assert_eq!(caps["mcp"], false);
    assert_eq!(caps["rest"], false);
    assert_eq!(caps["oauth"], false);
}

#[tokio::test]
async fn system_manifest_allowed_during_needs_setup() {
    // Empty-instance lock must not hide the manifest — the CLI probes it
    // before (and while) setup completes.
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("oxidean-setup.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.expect("connect sqlite");
    db.migrate().await.expect("migrate sqlite");

    let cors = build_cors("development", None).expect("cors");
    let app = router(db, cors);
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(
                    r#"{"procedure":"system.manifest","input":{}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["protocol_version"], 1);
}
