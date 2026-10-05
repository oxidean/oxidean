//! Manifest gating tests (CLI-02): commands feature-gate on the instance's
//! `system.manifest` procedures map, failing open when the manifest can't be
//! fetched (pre-manifest server, transport error).

use std::path::PathBuf;

use oxidean_cli::args;
use oxidean_cli::commands::{self, CliError, Out, Runtime};
use oxidean_cli::config::Config;
use oxidean_core::RpcResponse;
use serde_json::{json, Value};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn runtime() -> Runtime {
    Runtime {
        config: Config::default(),
        config_path: PathBuf::from("unused-config.json"),
    }
}

fn invocation(server: &MockServer, argv: &[&str]) -> args::Invocation {
    let mut full = vec!["--instance".to_string(), server.uri()];
    full.extend(argv.iter().map(|s| s.to_string()));
    args::parse(&full).unwrap()
}

fn manifest_body(procedures: Value) -> Value {
    json!({
        "ok": true,
        "data": {
            "protocol_version": 1,
            "server_version": "9.9.9-test",
            "procedures": procedures,
            "capabilities": {"mcp": false, "rest": false, "oauth": false},
            "min_cli_version": "0.1.0"
        }
    })
}

async fn mount_manifest(server: &MockServer, procedures: Value) {
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "system.manifest", "input": {}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(manifest_body(procedures)))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn gated_command_fails_when_procedure_missing() {
    let server = MockServer::start().await;
    mount_manifest(
        &server,
        json!({"system.manifest": true, "system.health": true}),
    )
    .await;
    // The gated call must never reach the wire.
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "repo.listMine", "input": {}}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {"repos": []}})),
        )
        .expect(0)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["repo", "list"]);
    let err = commands::dispatch(&inv, &runtime()).await.err().unwrap();
    assert!(matches!(err, CliError::Unsupported(_)), "{err:?}");
    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("missing repo.listMine"), "{err}");
    server.verify().await;
}

#[tokio::test]
async fn manifest_listed_procedure_runs() {
    let server = MockServer::start().await;
    mount_manifest(
        &server,
        json!({"system.manifest": true, "repo.listMine": true}),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "repo.listMine", "input": {}}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {"repos": []}})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["repo", "list"]);
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    match out {
        Out::Rpc { envelope, .. } => match envelope {
            RpcResponse::Ok { data, .. } => assert_eq!(data["repos"], json!([])),
            other => panic!("expected ok envelope, got {other:?}"),
        },
        Out::Report { .. } => panic!("expected rpc out"),
    }
    server.verify().await;
}

#[tokio::test]
async fn pre_manifest_server_fails_open() {
    let server = MockServer::start().await;
    // Old server: no system.manifest → unknown_procedure → run ungated.
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "system.manifest", "input": {}}),
        ))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "ok": false,
            "error": {"code": "rpc.unknown_procedure", "message": "unknown procedure: system.manifest"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "repo.listMine", "input": {}}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {"repos": []}})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["repo", "list"]);
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    assert!(matches!(out, Out::Rpc { .. }));
    server.verify().await;
}

#[tokio::test]
async fn manifest_http_failure_fails_open() {
    let server = MockServer::start().await;
    // Manifest endpoint behind a broken proxy → non-envelope 500 → ungated.
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "system.manifest", "input": {}}),
        ))
        .respond_with(ResponseTemplate::new(500).set_body_string("proxy exploded"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "repo.listMine", "input": {}}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {"repos": []}})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["repo", "list"]);
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    assert!(matches!(out, Out::Rpc { .. }));
    server.verify().await;
}

#[tokio::test]
async fn malformed_manifest_fails_open() {
    let server = MockServer::start().await;
    // Manifest endpoint returns a payload without a procedures map.
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "system.manifest", "input": {}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "data": {"protocol_version": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "repo.listMine", "input": {}}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {"repos": []}})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["repo", "list"]);
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    assert!(matches!(out, Out::Rpc { .. }));
    server.verify().await;
}

#[tokio::test]
async fn auth_status_gated_on_auth_me() {
    let server = MockServer::start().await;
    mount_manifest(&server, json!({"system.manifest": true})).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(json!({"procedure": "auth.me", "input": {}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {}})))
        .expect(0)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["auth", "status"]);
    let err = commands::dispatch(&inv, &runtime()).await.err().unwrap();
    assert!(matches!(err, CliError::Unsupported(_)), "{err:?}");
    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("missing auth.me"), "{err}");
    server.verify().await;
}

#[tokio::test]
async fn api_escape_hatch_also_gated() {
    let server = MockServer::start().await;
    mount_manifest(&server, json!({"system.manifest": true})).await;

    let inv = invocation(&server, &["api", "pull.merge", "-f", "owner=o"]);
    let err = commands::dispatch(&inv, &runtime()).await.err().unwrap();
    assert!(matches!(err, CliError::Unsupported(_)), "{err:?}");
    assert!(err.to_string().contains("missing pull.merge"), "{err}");
    server.verify().await;
}

#[tokio::test]
async fn pr_checks_requires_both_procedures() {
    let server = MockServer::start().await;
    // pull.get present, repo.commitStatus.list absent → gate rejects.
    mount_manifest(&server, json!({"system.manifest": true, "pull.get": true})).await;

    let inv = invocation(&server, &["pr", "checks", "o/n", "7"]);
    let err = commands::dispatch(&inv, &runtime()).await.err().unwrap();
    assert!(matches!(err, CliError::Unsupported(_)), "{err:?}");
    assert!(
        err.to_string().contains("missing repo.commitStatus.list"),
        "{err}"
    );
    server.verify().await;
}
