//! HTTP-boundary tests for `rpc::Client` — verify the wire envelope, version
//! header, and Bearer auth against a local mock server.

use oxidean_cli::rpc::{error_exit_code, CallError, Client};
use oxidean_core::RpcResponse;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn call_posts_envelope_with_bearer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(header("oxidean-rpc-version", "1"))
        .and(header("authorization", "Bearer oxidean_pat_test"))
        .and(body_json(
            json!({"procedure": "system.health", "input": {}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"ok": true, "data": {"status": "ok", "version": "1.0.0", "database": "skipped"}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), Some("oxidean_pat_test".into()));
    let resp = client.call("system.health", json!({})).await.unwrap();
    match resp {
        RpcResponse::Ok { data, .. } => assert_eq!(data["status"], "ok"),
        other => panic!("expected ok envelope, got {other:?}"),
    }
    server.verify().await;
}

#[tokio::test]
async fn call_returns_error_envelope_verbatim() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({"ok": false, "error": {"code": "auth.unauthenticated", "message": "not authenticated"}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), None);
    let resp = client.call("repo.listMine", json!({})).await.unwrap();
    match &resp {
        RpcResponse::Err { error, .. } => {
            assert_eq!(error.code, "auth.unauthenticated");
            assert_eq!(error_exit_code(error), 3);
        }
        other => panic!("expected error envelope, got {other:?}"),
    }
    assert!(serde_json::to_string(&resp)
        .unwrap()
        .contains("\"ok\":false"));
    server.verify().await;
}

#[tokio::test]
async fn non_envelope_http_error_is_http_call_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), None);
    let err = client.call("system.health", json!({})).await.unwrap_err();
    match err {
        CallError::Http { status, .. } => assert_eq!(status, 403),
        other => panic!("expected http error, got {other:?}"),
    }
    assert_eq!(err.exit_code(), 3);
    server.verify().await;
}
