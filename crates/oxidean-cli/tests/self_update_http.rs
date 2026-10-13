//! HTTP lockstep coverage for `ox self-update`: same advertised version but a
//! different per-target sha256 must download and replace the local binary.

use std::fs;
use std::path::PathBuf;

use oxidean_cli::args::Global;
use oxidean_cli::commands::Runtime;
use oxidean_cli::config::Config;
use oxidean_cli::self_update::{self, current_target, sha256_hex};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn runtime() -> Runtime {
    Runtime {
        config: Config::default(),
        config_path: PathBuf::from("unused-config.json"),
    }
}

fn global(instance: &str) -> Global {
    Global {
        instance: Some(instance.to_string()),
        json: true,
        help: false,
        version: false,
    }
}

#[tokio::test]
async fn self_update_replaces_when_sha_differs_same_version() {
    let server = MockServer::start().await;
    let target = current_target();
    let new_bytes = b"ox-self-update-http-fixture-v2";
    let new_sha = sha256_hex(new_bytes);
    let version = env!("CARGO_PKG_VERSION");

    Mock::given(method("GET"))
        .and(path("/cli/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "version": version,
            "targets": [{
                "target": target,
                "sha256": new_sha,
                "size": new_bytes.len(),
            }],
            "dist_commit": "abc123",
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/cli/bin/ox-{target}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(new_bytes.as_slice()))
        .expect(1)
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("ox");
    fs::write(&exe, b"old-local-ox-bytes").unwrap();

    let out = self_update::run_with_exe(
        &global(&server.uri()),
        &runtime(),
        &exe,
        true,
        true,
    )
    .await
    .expect("self-update should succeed");

    assert_eq!(fs::read(&exe).unwrap(), new_bytes.as_slice());
    match out {
        oxidean_cli::commands::Out::Report { json, .. } => {
            assert_eq!(json["ok"], true);
            assert_eq!(json["data"]["updated"], true);
            assert_eq!(json["data"]["sha256"], new_sha);
            assert_eq!(json["data"]["to"], version);
        }
        _ => panic!("unexpected Out variant (expected Report)"),
    }
    server.verify().await;
}

#[tokio::test]
async fn self_update_skips_when_version_and_sha_match() {
    let server = MockServer::start().await;
    let target = current_target();
    let local_bytes = b"already-current-ox";
    let local_sha = sha256_hex(local_bytes);
    let version = env!("CARGO_PKG_VERSION");

    Mock::given(method("GET"))
        .and(path("/cli/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "version": version,
            "targets": [{
                "target": target,
                "sha256": local_sha,
                "size": local_bytes.len(),
            }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    // Must not download the binary when already current.
    Mock::given(method("GET"))
        .and(path(format!("/cli/bin/ox-{target}")))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("ox");
    fs::write(&exe, local_bytes).unwrap();

    let out = self_update::run_with_exe(
        &global(&server.uri()),
        &runtime(),
        &exe,
        true,
        true,
    )
    .await
    .expect("self-update should report up to date");

    assert_eq!(fs::read(&exe).unwrap(), local_bytes);
    match out {
        oxidean_cli::commands::Out::Report { json, .. } => {
            assert_eq!(json["ok"], true);
            assert_eq!(json["data"]["updated"], false);
        }
        _ => panic!("unexpected Out variant (expected Report)"),
    }
    server.verify().await;
}
