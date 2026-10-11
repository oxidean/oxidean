//! End-to-end dispatch tests — parse argv, gate on a mocked manifest, and
//! verify the wire calls (single RPCs and multi-call chains like `pr merge`
//! strategy resolution and `release upload`'s multipart POST).

use std::path::PathBuf;

use oxidean_cli::args;
use oxidean_cli::commands::{self, Out, Runtime};
use oxidean_cli::config::Config;
use serde_json::{Value, json};
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

/// Procedures these tests exercise — listed rather than wildcarded, matching
/// the real manifest's shape.
const MANIFEST: &[&str] = &[
    "issue.create",
    "issue.labels.set",
    "label.listForRepo",
    "issue.assigneeCandidates",
    "issue.update",
    "issue.get",
    "pull.merge",
    "repo.mergeSettings.get",
    "release.get",
    "release.deleteAsset",
    "notification.markAllRead",
    "notification.markRead",
];

async fn mount_manifest(server: &MockServer, procs: &[&str]) {
    let procedures: Value = procs
        .iter()
        .map(|p| (p.to_string(), Value::Bool(true)))
        .collect::<serde_json::Map<String, Value>>()
        .into();
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(json!({"procedure": "system.manifest", "input": {}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"procedures": procedures, "protocol_version": 1,
                     "server_version": "9.9.9-test"}
        })))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn issue_create_sends_title_and_body() {
    let server = MockServer::start().await;
    mount_manifest(&server, MANIFEST).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(json!({
            "procedure": "issue.create",
            "input": {"owner": "o", "name": "n", "title": "t", "body": "b"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"ok": true, "data": {"number": 4, "title": "t", "state": "open"}}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(
        &server,
        &["issue", "create", "o/n", "--title", "t", "--body", "b"],
    );
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    assert!(matches!(out, Out::Rpc { .. }));
    server.verify().await;
}

#[tokio::test]
async fn issue_edit_resolves_label_names() {
    let server = MockServer::start().await;
    mount_manifest(&server, MANIFEST).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "label.listForRepo", "input": {"owner": "o", "name": "n"}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"labels": [{"id": "l1", "name": "bug"}, {"id": "l2", "name": "ui"}]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(json!({
            "procedure": "issue.labels.set",
            "input": {"owner": "o", "name": "n", "number": 4, "labelIds": ["l1"]}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {}})))
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["issue", "edit", "o/n", "4", "--label", "bug"]);
    commands::dispatch(&inv, &runtime()).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn pr_merge_defaults_to_first_allowed_strategy() {
    let server = MockServer::start().await;
    mount_manifest(&server, MANIFEST).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "repo.mergeSettings.get", "input": {"owner": "o", "name": "n"}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"settings": {"allow_merge_commit": false, "allow_squash_merge": true,
                                  "allow_rebase_merge": false}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(json!({
            "procedure": "pull.merge",
            "input": {"owner": "o", "name": "n", "number": 7, "method": "squash"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {}})))
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["pr", "merge", "o/n", "7"]);
    commands::dispatch(&inv, &runtime()).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn release_upload_posts_multipart() {
    let server = MockServer::start().await;
    mount_manifest(&server, MANIFEST).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(json!({
            "procedure": "release.get",
            "input": {"owner": "o", "name": "n", "tag_name": "v1"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"id": "rel1", "tag_name": "v1"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/repos/o/n/releases/rel1/assets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "data": {"asset": {"id": "a1", "filename": "bin.txt"}}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let tmp = std::env::temp_dir().join("ox-e2e-upload.txt");
    std::fs::write(&tmp, "hello").unwrap();
    let inv = invocation(
        &server,
        &[
            "release",
            "upload",
            "o/n",
            "v1",
            tmp.to_str().unwrap(),
        ],
    );
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    match out {
        Out::Report { text, .. } => assert!(text.contains("uploaded"), "{text}"),
        _ => panic!("expected report"),
    }
    server.verify().await;
    let _ = std::fs::remove_file(&tmp);
}

#[tokio::test]
async fn release_download_writes_files() {
    let server = MockServer::start().await;
    mount_manifest(&server, MANIFEST).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(json!({
            "procedure": "release.get",
            "input": {"owner": "o", "name": "n", "tag_name": "v1"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"id": "rel1", "tag_name": "v1",
                     "assets": [{"id": "a1", "filename": "ox-linux"}]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/releases/assets/a1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-disposition", "attachment; filename=\"ox-linux\"")
                .set_body_bytes(b"binary!"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let dir = std::env::temp_dir().join(format!("ox-e2e-dl-{}", std::process::id()));
    let inv = invocation(
        &server,
        &["release", "download", "o/n", "v1", "--dir", dir.to_str().unwrap()],
    );
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    match out {
        Out::Report { .. } => {}
        _ => panic!("expected report"),
    }
    assert_eq!(std::fs::read(dir.join("ox-linux")).unwrap(), b"binary!");
    server.verify().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn notification_read_all_hits_mark_all() {
    let server = MockServer::start().await;
    mount_manifest(&server, MANIFEST).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "notification.markAllRead", "input": {}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {}})))
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["notification", "read", "--all"]);
    commands::dispatch(&inv, &runtime()).await.unwrap();
    server.verify().await;
}
