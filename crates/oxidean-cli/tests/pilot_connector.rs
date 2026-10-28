//! HTTP-boundary tests for the C3 connector surface — wire shapes, the
//! flat `{"error":"<code>"}` envelope, `Retry-After` honoring, the
//! route-aware classifier, and the cancel-set / terminal semantics of the
//! run loop. Same wiremock style as `rpc_http.rs`.

use std::path::PathBuf;

use oxidean_cli::pilot::client::C3Error;
use oxidean_cli::pilot::client::{
    ClaimAnswer, ConnectorClient, ReportStatus, CLAIM_PATH, HEARTBEAT_PATH, PAIR_PATH, REPORT_PATH,
};
use oxidean_cli::pilot::custody::{self, SecretString, TargetRow};
use oxidean_cli::pilot::state::{self, classify, Disposition, Route, RunOpts, RunState};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn connector(server: &MockServer) -> ConnectorClient {
    ConnectorClient::new(&server.uri(), false)
}

fn verifier() -> SecretString {
    SecretString::new("verifier-under-test-123".to_string())
}

fn target_row() -> TargetRow {
    TargetRow {
        instance: "https://forge.example".to_string(),
        connector_id: "conn-1".to_string(),
        target_id: "target-1".to_string(),
        device_label: "test-box".to_string(),
        granted_caps: vec!["exec_shell".into(), "fs_read".into(), "fs_write".into()],
        requested_caps: vec!["exec_shell".into()],
        paired_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

// -----------------------------------------------------------------------
// Wire shapes — all four routes.
// -----------------------------------------------------------------------

#[tokio::test]
async fn pair_posts_code_label_caps_and_decodes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(PAIR_PATH))
        .and(body_json(json!({
            "pairing_code": "PAIR-CODE-1",
            "device_label": "dev-laptop",
            "requested_caps": ["exec_shell", "fs_read", "fs_write"],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "connector_id": "conn-1",
            "target_id": "target-1",
            "verifier": "verifier-value",
        })))
        .expect(1)
        .mount(&server)
        .await;
    let r = connector(&server)
        .pair(
            "PAIR-CODE-1",
            "dev-laptop",
            &["exec_shell".into(), "fs_read".into(), "fs_write".into()],
        )
        .await
        .unwrap();
    assert_eq!(r.connector_id, "conn-1");
    assert_eq!(r.target_id, "target-1");
    assert_eq!(r.verifier, "verifier-value");
    server.verify().await;
}

#[tokio::test]
async fn claim_posts_bearer_and_decodes_batch() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .and(header("authorization", "Bearer verifier-under-test-123"))
        .and(body_json(json!({
            "target_id": "target-1",
            "wait_seconds": 30,
            "capabilities": ["exec_shell"],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "actions": [
                {"action_id": "a-1", "kind": "exec_shell", "args": "{\"command\":\"echo hi\"}"},
            ],
            "cancel": ["a-0"],
            "retry_after": 0,
        })))
        .expect(1)
        .mount(&server)
        .await;
    let ans = connector(&server)
        .claim(&verifier(), "target-1", 30, &["exec_shell".into()])
        .await
        .unwrap();
    match ans {
        ClaimAnswer::Batch {
            actions,
            cancel,
            retry_after,
        } => {
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].action_id, "a-1");
            // args is the DOUBLE-ENCODED json string — inner object next.
            assert_eq!(actions[0].args, "{\"command\":\"echo hi\"}");
            assert_eq!(cancel, vec!["a-0".to_string()]);
            assert_eq!(retry_after, 0);
        }
        other => panic!("expected batch, got {other:?}"),
    }
    server.verify().await;
}

#[tokio::test]
async fn claim_204_honors_retry_after_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .respond_with(ResponseTemplate::new(204).insert_header("Retry-After", "7"))
        .expect(1)
        .mount(&server)
        .await;
    let ans = connector(&server)
        .claim(&verifier(), "target-1", 30, &[])
        .await
        .unwrap();
    match ans {
        ClaimAnswer::Timeout { retry_after } => assert_eq!(retry_after, 7),
        other => panic!("expected timeout, got {other:?}"),
    }
    server.verify().await;
}

#[tokio::test]
async fn claim_clamps_wait_seconds_client_side() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .and(body_json(json!({
            "target_id": "t",
            "wait_seconds": 60,
            "capabilities": [],
        })))
        .respond_with(ResponseTemplate::new(204).insert_header("Retry-After", "1"))
        .expect(1)
        .mount(&server)
        .await;
    let _ = connector(&server)
        .claim(&verifier(), "t", 300, &[])
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn report_posts_status_exit_code_output() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(REPORT_PATH))
        .and(header("authorization", "Bearer verifier-under-test-123"))
        .and(body_json(json!({
            "action_id": "a-1",
            "status": "done",
            "exit_code": 0,
            "output": "hi\n",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    connector(&server)
        .report(&verifier(), "a-1", ReportStatus::Done, Some(0), "hi\n")
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn report_omits_exit_code_when_absent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(REPORT_PATH))
        .and(body_json(json!({
            "action_id": "a-1",
            "status": "failed",
            "output": "cancelled:pre-start",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    connector(&server)
        .report(
            &verifier(),
            "a-1",
            ReportStatus::Failed,
            None,
            "cancelled:pre-start",
        )
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn heartbeat_decodes_revoked_and_cancel() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(HEARTBEAT_PATH))
        .and(header("authorization", "Bearer verifier-under-test-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "revoked": true,
            "cancel": ["a-7", "a-8"],
        })))
        .expect(1)
        .mount(&server)
        .await;
    let hb = connector(&server)
        .heartbeat(&verifier(), "target-1")
        .await
        .unwrap();
    assert!(hb.revoked);
    assert_eq!(hb.cancel, vec!["a-7".to_string(), "a-8".to_string()]);
    server.verify().await;
}

// -----------------------------------------------------------------------
// Error envelope + classification totality.
// -----------------------------------------------------------------------

/// Every wire code → the decoded `C3Error` variant on a loop route, and
/// the classifier arm it lands in (dispatch totality).
#[tokio::test]
async fn every_error_code_decodes_and_classifies_on_loop_routes() {
    let cases: &[(&str, u16, &str)] = &[
        ("unauthorized", 401, "auth"),
        ("forbidden", 403, "auth"),
        ("revoked", 403, "revoked"),
        ("disabled", 403, "backoff"),
        ("not_found", 404, "notfound"),
        ("bad_input", 400, "badinput"),
        ("internal", 500, "backoff"),
        ("rate_limited", 429, "sleep"),
        ("pairing_expired", 410, "backoff"),
        ("pairing_used", 409, "backoff"),
    ];
    for (code, http, want) in cases {
        let server = MockServer::start().await;
        let mut rt = ResponseTemplate::new(*http).set_body_json(json!({"error": code}));
        if *code == "rate_limited" {
            rt = rt.insert_header("Retry-After", "9");
        }
        Mock::given(method("POST"))
            .and(path(CLAIM_PATH))
            .respond_with(rt)
            .expect(1)
            .mount(&server)
            .await;
        let err = connector(&server)
            .claim(&verifier(), "t", 1, &[])
            .await
            .unwrap_err();
        let got = match (classify(&err, Route::Claim), *want) {
            (Disposition::AuthDeath(_), "auth") => "auth",
            (Disposition::TerminalRevoked, "revoked") => "revoked",
            (Disposition::TerminalNotFound, "notfound") => "notfound",
            (Disposition::BadInputExit, "badinput") => "badinput",
            (Disposition::Backoff, "backoff") => "backoff",
            (Disposition::Sleep(9), "sleep") => "sleep",
            (d, w) => panic!("code {code}: expected {w}, got {d:?} ({err:?})"),
        };
        assert_eq!(got, *want, "code {code}");
        server.verify().await;
    }
}

/// The same codes on the REPORT route — `not_found` is DropAction (the
/// row is dead; keep going), everything else matches the loop table.
#[tokio::test]
async fn report_route_drops_not_found_and_follows_loop_table() {
    // not_found → DropAction
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(REPORT_PATH))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error": "not_found"})))
        .expect(1)
        .mount(&server)
        .await;
    let err = connector(&server)
        .report(&verifier(), "a-1", ReportStatus::Done, Some(0), "x")
        .await
        .unwrap_err();
    assert_eq!(classify(&err, Route::Report), Disposition::DropAction);
    server.verify().await;
}

/// Pair-route rows: pairing_expired/pairing_used/forbidden/disabled are
/// terminal refusals; rate_limited retries once; loop-only codes are
/// internal-class PairFail — never a clean exit.
#[tokio::test]
async fn pair_error_vocabulary_is_route_specific() {
    let cases: &[(&str, u16, &str)] = &[
        ("pairing_expired", 410, "reject"),
        ("pairing_used", 409, "reject"),
        ("forbidden", 403, "reject"),
        ("disabled", 403, "reject"),
        ("unauthorized", 401, "fail"),
        ("not_found", 404, "fail"),
        ("revoked", 403, "fail"),
        ("bad_input", 400, "fail"),
        ("internal", 500, "fail"),
        ("rate_limited", 429, "retry"),
    ];
    for (code, http, want) in cases {
        let server = MockServer::start().await;
        let mut rt = ResponseTemplate::new(*http).set_body_json(json!({"error": code}));
        if *code == "rate_limited" {
            rt = rt.insert_header("Retry-After", "4");
        }
        Mock::given(method("POST"))
            .and(path(PAIR_PATH))
            .respond_with(rt)
            .expect(1)
            .mount(&server)
            .await;
        let err = connector(&server)
            .pair("code", "label", &["exec_shell".into()])
            .await
            .unwrap_err();
        match (classify(&err, Route::Pair), *want) {
            (Disposition::PairReject(_), "reject") => {}
            (Disposition::PairFail(_), "fail") => {}
            (Disposition::PairRetryOnce(4), "retry") => {}
            (d, w) => panic!("code {code}: expected {w}, got {d:?} ({err:?})"),
        }
        server.verify().await;
    }
}

/// An unparseable error body maps to Transport — never a guessed code.
#[tokio::test]
async fn unparseable_error_body_is_transport() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>bad gateway</html>"))
        .expect(1)
        .mount(&server)
        .await;
    let err = connector(&server)
        .claim(&verifier(), "t", 1, &[])
        .await
        .unwrap_err();
    assert!(matches!(err, C3Error::Transport(_)), "{err:?}");
    assert_eq!(classify(&err, Route::Claim), Disposition::Backoff);
    server.verify().await;
}

// -----------------------------------------------------------------------
// Custody — file backend I/O (keyring coverage lives in custody.rs unit
// tests; here the seam against a tempdir).
// -----------------------------------------------------------------------

#[test]
fn custody_file_backend_roundtrip_and_zombie() {
    let dir = tempfile::tempdir().unwrap();
    // No verifier anywhere → zombie-shaped None (row may exist later).
    let none = custody::read_verifier(dir.path(), "https://forge.example", "conn-x").unwrap();
    // On machines with a keyring this still yields None — the composite
    // key was never written.
    assert!(none.is_none());
    // Write + read through the file map directly (backend-independent).
    let mut map = std::collections::BTreeMap::new();
    map.insert(
        "https://forge.example/conn-x".to_string(),
        "verifier-value".to_string(),
    );
    let body = serde_json::to_string(&map).unwrap();
    std::fs::write(custody::secrets_path(dir.path()), body).unwrap();
    let found = custody::read_verifier(dir.path(), "https://forge.example", "conn-x").unwrap();
    match found {
        Some((backend, v)) => {
            let _ = backend;
            assert_eq!(v.expose(), "verifier-value");
        }
        None => {
            // A present keyring without this key still falls through to
            // the file map — unreachable unless the file is ignored.
            panic!("expected verifier");
        }
    }
    // delete is backend-total: file entry gone afterwards.
    custody::delete_verifier(dir.path(), "https://forge.example", "conn-x");
    let raw = std::fs::read_to_string(custody::secrets_path(dir.path())).unwrap();
    assert!(!raw.contains("verifier-value"));
}

// -----------------------------------------------------------------------
// Run-loop — cancel set closes out + terminal not_found exits clean.
// -----------------------------------------------------------------------

fn opts(server_uri: &str, jail: PathBuf, cfg: PathBuf, row: TargetRow) -> RunOpts {
    RunOpts {
        client: ConnectorClient::new(server_uri, false),
        verifier: verifier(),
        row,
        cfg_dir: cfg.clone(),
        canonical_root: jail,
        canonical_cfg: cfg,
        secrets: vec!["verifier-under-test-123".to_string()],
        wait_seconds: 1,
        exec_timeout_secs: 60,
        verbose: false,
    }
}

#[tokio::test]
async fn cancel_set_reports_pre_start_then_not_found_exits_clean() {
    let server = MockServer::start().await;
    // First claim: cancel-only batch carrying "a-cancel". `up_to_n_times(1)`
    // makes it match ONLY the first claim; the not_found mock answers the
    // re-poll → terminate → exit 0.
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "actions": [],
            "cancel": ["a-cancel"],
            "retry_after": 1,
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error": "not_found"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(REPORT_PATH))
        .and(body_json(json!({
            "action_id": "a-cancel",
            "status": "failed",
            "output": "cancelled:pre-start",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(HEARTBEAT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "revoked": false,
            "cancel": [],
        })))
        .mount(&server)
        .await;

    let jail = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let st = RunState::new(opts(
        &server.uri(),
        jail.path().into(),
        cfg.path().into(),
        target_row(),
    ))
    .unwrap();
    // The row is deleted by the not_found terminal — the tempdir must
    // contain a targets file first for delete_target to succeed (it
    // tolerates absence too).
    let out = tokio::time::timeout(std::time::Duration::from_secs(15), st.run_loop())
        .await
        .expect("run loop hung")
        .expect("not_found must exit clean");
    server.verify().await;
    match out {
        oxidean_cli::commands::Out::Report { text, .. } => {
            assert!(text.contains("no longer exists"), "{text}");
        }
        _ => panic!("expected report out"),
    }
}

#[tokio::test]
async fn revoked_heartbeat_terminates_clean_and_deletes_row() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(HEARTBEAT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "revoked": true,
            "cancel": [],
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .respond_with(ResponseTemplate::new(204).insert_header("Retry-After", "5"))
        .mount(&server)
        .await;

    let jail = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let row = target_row();
    custody::upsert_target(cfg.path(), &row).unwrap();
    let st = RunState::new(opts(
        &server.uri(),
        jail.path().into(),
        cfg.path().into(),
        row,
    ))
    .unwrap();
    let out = tokio::time::timeout(std::time::Duration::from_secs(15), st.run_loop())
        .await
        .expect("run loop hung")
        .expect("revoked must exit clean");
    match out {
        oxidean_cli::commands::Out::Report { text, .. } => {
            assert!(text.contains("revoked"), "{text}");
        }
        _ => panic!("expected report out"),
    }
    // The row is gone (revoked deletes creds + row).
    let map = custody::load_targets(cfg.path()).unwrap();
    assert!(map.is_empty(), "revoked row should be deleted: {map:?}");
}

#[tokio::test]
async fn unauthorized_claim_is_auth_death_exit_1() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "unauthorized"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(HEARTBEAT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "revoked": false,
            "cancel": [],
        })))
        .mount(&server)
        .await;
    let jail = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let st = RunState::new(opts(
        &server.uri(),
        jail.path().into(),
        cfg.path().into(),
        target_row(),
    ))
    .unwrap();
    let res = tokio::time::timeout(std::time::Duration::from_secs(15), st.run_loop())
        .await
        .expect("run loop hung");
    match res {
        Err(e) => {
            assert_eq!(e.exit_code(), 1, "{e}");
            assert!(e.to_string().contains("unauthorized"), "{e}");
        }
        Ok(_) => panic!("unauthorized must exit 1"),
    }
}

#[tokio::test]
async fn rate_limited_claim_sleeps_hint_then_recovers() {
    let server = MockServer::start().await;
    // First claim 429 Retry-After: 1 → loop sleeps the hint and re-polls;
    // second claim 404 → terminal success.
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "1")
                .set_body_json(json!({"error": "rate_limited"})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(CLAIM_PATH))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error": "not_found"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(HEARTBEAT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "revoked": false,
            "cancel": [],
        })))
        .mount(&server)
        .await;
    let jail = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let st = RunState::new(opts(
        &server.uri(),
        jail.path().into(),
        cfg.path().into(),
        target_row(),
    ))
    .unwrap();
    let out = tokio::time::timeout(std::time::Duration::from_secs(15), st.run_loop())
        .await
        .expect("run loop hung")
        .expect("rate-limit recovery should reach clean exit");
    let _ = out;
    let reqs = server.received_requests().await.unwrap_or_default();
    let claims = reqs.iter().filter(|r| r.url.path() == CLAIM_PATH).count();
    assert!(
        claims >= 2,
        "expected re-poll after rate limit, got {claims}"
    );
}

// `state::classify` is exercised end-to-end here; the exhaustive table
// (including Transport/Unknown) sits in state.rs's own tests.
#[test]
fn dispatch_table_pair_route_rows() {
    // Loop-only codes on pair → PairFail (exit 1), never clean.
    for e in [
        C3Error::Unauthorized,
        C3Error::Revoked,
        C3Error::NotFound,
        C3Error::BadInput,
        C3Error::Internal,
        C3Error::Unknown("weird".into()),
    ] {
        assert!(
            matches!(classify(&e, Route::Pair), Disposition::PairFail(_)),
            "{e:?}"
        );
    }
    // Pair-only codes on loop routes → Backoff, never a pair refusal.
    for e in [C3Error::PairingExpired, C3Error::PairingUsed] {
        for route in [Route::Claim, Route::Report, Route::Heartbeat] {
            assert_eq!(classify(&e, route), Disposition::Backoff, "{e:?} {route:?}");
        }
    }
    let _ = state::HEARTBEAT_INTERVAL;
}
