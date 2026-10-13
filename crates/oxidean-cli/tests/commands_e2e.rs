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

// ── admin auth ─────────────────────────────────────────────────────

fn auth_settings(allow_signup: bool) -> Value {
    json!({
        "provider_mode": "local",
        "email_provider": "log",
        "from_address": null,
        "oidc_issuer": null,
        "oidc_client_id": null,
        "workos_client_id": null,
        "allow_signup": allow_signup,
        "default_visibility": "public"
    })
}

#[tokio::test]
async fn admin_auth_get_calls_get_settings() {
    let server = MockServer::start().await;
    mount_manifest(&server, &["admin.auth.get_settings"]).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "admin.auth.get_settings", "input": {}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"ok": true, "data": auth_settings(true)}),
        ))
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["admin", "auth"]);
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    assert!(matches!(out, Out::Rpc { .. }));
    server.verify().await;
}

#[tokio::test]
async fn admin_auth_set_merges_flags_over_current() {
    let server = MockServer::start().await;
    mount_manifest(
        &server,
        &["admin.auth.get_settings", "admin.auth.update_settings"],
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "admin.auth.get_settings", "input": {}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"ok": true, "data": auth_settings(true)}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    // The update payload is the full settings object — only --no-signup
    // flips; every other field carries the server's current values over.
    let mut merged = auth_settings(true);
    merged["allow_signup"] = json!(false);
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(
            json!({"procedure": "admin.auth.update_settings", "input": merged}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true, "data": {}})))
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["admin", "auth", "set", "--no-signup"]);
    commands::dispatch(&inv, &runtime()).await.unwrap();
    server.verify().await;
}

// ── run watch ──────────────────────────────────────────────────────

fn get_run_body(status: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "ok": true,
        "data": {"run": {"id": "r1", "status": status, "title": "ci"}, "jobs": []}
    }))
}

#[tokio::test]
async fn run_watch_polls_to_terminal_then_renders() {
    let server = MockServer::start().await;
    mount_manifest(&server, &["repo.actions.getRun"]).await;
    // First-mounted wins: the capped in_progress mock serves the first two
    // polls, then the success mock takes over once its quota is spent.
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(json!({
            "procedure": "repo.actions.getRun",
            "input": {"owner": "o", "name": "n", "run_id": "r1"}
        })))
        .respond_with(get_run_body("in_progress"))
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .and(body_json(json!({
            "procedure": "repo.actions.getRun",
            "input": {"owner": "o", "name": "n", "run_id": "r1"}
        })))
        .respond_with(get_run_body("success"))
        .expect(2) // terminal poll + final render invoke
        .mount(&server)
        .await;

    let inv = invocation(&server, &["run", "watch", "o/n", "r1", "--interval", "1"]);
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    match out {
        Out::Rpc { pretty, .. } => assert!(pretty.contains("success"), "{pretty}"),
        _ => panic!("expected rpc"),
    }
    server.verify().await;
}

#[tokio::test]
async fn run_watch_timeout_is_usage_error() {
    let server = MockServer::start().await;
    mount_manifest(&server, &["repo.actions.getRun"]).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .respond_with(get_run_body("in_progress"))
        .mount(&server)
        .await;

    let inv = invocation(
        &server,
        &["run", "watch", "o/n", "r1", "--interval", "1", "--timeout", "1"],
    );
    let err = commands::dispatch(&inv, &runtime()).await.err().unwrap();
    assert!(format!("{err}").contains("timed out"), "{err}");
}

#[tokio::test]
async fn run_watch_exit_status_fails_on_failure() {
    let server = MockServer::start().await;
    mount_manifest(&server, &["repo.actions.getRun"]).await;
    Mock::given(method("POST"))
        .and(path("/api/rpc"))
        .respond_with(get_run_body("failure"))
        .mount(&server)
        .await;

    let inv = invocation(&server, &["run", "watch", "o/n", "r1", "--exit-status"]);
    let err = commands::dispatch(&inv, &runtime()).await.err().unwrap();
    assert!(format!("{err}").contains("failure"), "{err}");
}

// ── local commands: config + auth switch ────────────────────────────

fn parse_argv(argv: &[&str]) -> args::Invocation {
    let full: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    args::parse(&full).unwrap()
}

fn runtime_with(
    dir: &tempfile::TempDir,
    instances: &[&str],
    default: Option<&str>,
) -> Runtime {
    let mut map = std::collections::BTreeMap::new();
    for u in instances {
        map.insert(
            u.to_string(),
            oxidean_cli::config::InstanceEntry {
                token: Some("secret-pat-value".into()),
            },
        );
    }
    Runtime {
        config: Config {
            default_instance: default.map(String::from),
            instances: map,
        },
        config_path: dir.path().join("config.json"),
    }
}

#[tokio::test]
async fn config_set_get_list_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let rt = runtime_with(&dir, &["https://a.example.com"], None);

    commands::dispatch(
        &parse_argv(&[
            "config",
            "set",
            "default_instance",
            "https://a.example.com/",
        ]),
        &rt,
    )
    .await
    .unwrap();
    let saved = oxidean_cli::config::load(&rt.config_path).unwrap();
    assert_eq!(
        saved.default_instance.as_deref(),
        Some("https://a.example.com") // normalized: trailing slash stripped
    );

    let rt = runtime_with(&dir, &["https://a.example.com"], Some("https://a.example.com"));
    match commands::dispatch(&parse_argv(&["config", "get", "default_instance"]), &rt)
        .await
        .unwrap()
    {
        Out::Report { text, .. } => assert_eq!(text, "https://a.example.com"),
        _ => panic!("expected report"),
    }
    match commands::dispatch(&parse_argv(&["config", "list"]), &rt)
        .await
        .unwrap()
    {
        Out::Report { text, json } => {
            assert!(text.contains("token=set"), "{text}");
            assert!(!text.contains("secret-pat-value"), "{text}");
            assert!(!json.to_string().contains("secret-pat-value"));
        }
        _ => panic!("expected report"),
    }
}

#[tokio::test]
async fn config_token_get_reports_state_not_secret() {
    let dir = tempfile::tempdir().unwrap();
    let rt = runtime_with(&dir, &["https://a.example.com"], None);
    for key in [
        "instances.https://a.example.com.token",
        "instances.https://b.example.com.token",
    ] {
        match commands::dispatch(&parse_argv(&["config", "get", key]), &rt)
            .await
            .unwrap()
        {
            Out::Report { text, .. } => {
                assert!(matches!(text.as_str(), "set" | "unset"), "{text}");
            }
            _ => panic!("expected report"),
        }
    }
    // token writes persist without touching other entries
    commands::dispatch(
        &parse_argv(&[
            "config",
            "set",
            "instances.https://b.example.com.token",
            "new-tok",
        ]),
        &rt,
    )
    .await
    .unwrap();
    let saved = oxidean_cli::config::load(&rt.config_path).unwrap();
    assert_eq!(
        saved.instances["https://b.example.com"].token.as_deref(),
        Some("new-tok")
    );
    assert!(saved.instances.contains_key("https://a.example.com"));
    // unknown keys and bad urls are usage errors
    assert!(
        commands::dispatch(&parse_argv(&["config", "get", "bogus.key"]), &rt)
            .await
            .is_err()
    );
    assert!(
        commands::dispatch(
            &parse_argv(&["config", "set", "instances.no-scheme.token", "x"]),
            &rt,
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn auth_switch_sets_default() {
    let dir = tempfile::tempdir().unwrap();
    let rt = runtime_with(
        &dir,
        &["https://a.example.com", "https://b.example.com"],
        Some("https://a.example.com"),
    );
    commands::dispatch(
        &parse_argv(&["auth", "switch", "https://b.example.com"]),
        &rt,
    )
    .await
    .unwrap();
    let saved = oxidean_cli::config::load(&rt.config_path).unwrap();
    assert_eq!(
        saved.default_instance.as_deref(),
        Some("https://b.example.com")
    );
}

#[tokio::test]
async fn auth_switch_rejects_unknown_and_noninteractive() {
    let dir = tempfile::tempdir().unwrap();
    let rt = runtime_with(&dir, &["https://a.example.com"], None);
    let err = commands::dispatch(
        &parse_argv(&["auth", "switch", "https://nope.example.com"]),
        &rt,
    )
    .await
    .err().unwrap();
    assert!(format!("{err}").contains("not logged in"), "{err}");
    // stdin isn't a TTY in tests — no-arg switch is a usage error
    let err = commands::dispatch(&parse_argv(&["auth", "switch"]), &rt)
        .await
        .err().unwrap();
    assert!(format!("{err}").contains("non-interactive"), "{err}");
    // empty instance store gets the login pointer instead
    let rt = runtime_with(&dir, &[], None);
    let err = commands::dispatch(&parse_argv(&["auth", "switch"]), &rt)
        .await
        .err().unwrap();
    assert!(format!("{err}").contains("no logged-in instances"), "{err}");
}

/// `mcp status` probes `POST /api/mcp` directly (initialize + tools/list) —
/// the file-scan half is environment-dependent, so these assert only the
/// live-probe fields.
#[tokio::test]
async fn mcp_status_pings_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/mcp"))
        .and(body_json(json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "ox", "version": env!("CARGO_PKG_VERSION")},
            },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {"protocolVersion": "2025-06-18", "capabilities": {},
                       "serverInfo": {"name": "oxidean", "version": "1.0.0"}},
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/mcp"))
        .and(body_json(
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0", "id": 2,
            "result": {"tools": [{"name": "a"}, {"name": "b"}]},
        })))
        .expect(1)
        .mount(&server)
        .await;

    let inv = invocation(&server, &["mcp", "status"]);
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    match out {
        Out::Report { json, .. } => {
            assert_eq!(json["data"]["probe"]["state"], "reachable");
            assert_eq!(json["data"]["probe"]["server"], "oxidean");
            assert_eq!(json["data"]["probe"]["tools"], 2);
        }
        _ => panic!("expected report"),
    }
}

#[tokio::test]
async fn mcp_status_reports_disabled_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/mcp"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {"code": "mcp.disabled", "message": "MCP is disabled"},
        })))
        .mount(&server)
        .await;

    let inv = invocation(&server, &["mcp", "status"]);
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    match out {
        Out::Report { json, .. } => {
            assert_eq!(json["data"]["probe"]["state"], "disabled")
        }
        _ => panic!("expected report"),
    }
}

#[tokio::test]
async fn mcp_status_reports_auth_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/mcp"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let inv = invocation(&server, &["mcp", "status"]);
    let out = commands::dispatch(&inv, &runtime()).await.unwrap();
    match out {
        Out::Report { json, .. } => {
            assert_eq!(json["data"]["probe"]["state"], "auth-rejected")
        }
        _ => panic!("expected report"),
    }
}
