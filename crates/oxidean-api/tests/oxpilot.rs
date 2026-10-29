//! Oxpilot U1 walking-skeleton integration — `oxpilot.*` RPC procs plus the
//! two `/api/oxpilot/...` SSE routes end-to-end over the real router.
//! `OXIDEAN_OXPILOT_ENABLED` is process env — every test holds `ENV_LOCK`
//! for its whole run so enablement resolution can't flake mid-assertion.

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_api_core::oxpilot::{
    disabled_slot, ActiveTurns, BudgetCaps, KeySource, OxpilotRuntime, OxpilotRuntimeSlot,
    ProviderIdentity, SharedOxpilot, StubProvider,
};
use oxidean_db::Database;
use tokio::sync::MutexGuard;
use tower::ServiceExt;

/// Serialize the whole file — enablement env is process-global.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().await
}

fn stub_runtime() -> SharedOxpilot {
    OxpilotRuntime::from_slot(OxpilotRuntimeSlot {
        provider: Some(Arc::new(StubProvider { fail: false })),
        model: Some("stub-1".to_string()),
        disabled_reason: None,
        turns: Arc::new(ActiveTurns::default()),
        built_from: Some(ProviderIdentity {
            provider_id: "stub".to_string(),
            model: Some("stub-1".to_string()),
            base_url: None,
            key_source: KeySource::None,
        }),
        budget_caps: BudgetCaps::default(),
        tools: None,
    })
}

fn empty_runtime() -> SharedOxpilot {
    OxpilotRuntime::from_slot(disabled_slot(
        Arc::new(ActiveTurns::default()),
        "flag_off",
        BudgetCaps::default(),
        None,
    ))
}

async fn test_app(db: Database, slot: Option<SharedOxpilot>) -> axum::Router {
    let mut state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
    if let Some(s) = slot {
        // U2 — the session-service seam installs unconditionally, exactly
        // like `main.rs` (set-once; a second install is a no-op), and the
        // U3 abort hook registers before the delete cascade identically.
        let svc = std::sync::Arc::new(oxidean_api_oxpilot::services::OxpilotSessionService::new(
            state.db.clone(),
        ));
        svc.register_delete_hook(std::sync::Arc::new(
            oxidean_api_oxpilot::runtime::OxpilotTurnAbortHook::new(s.clone()),
        ));
        s.install_sessions(svc);
        state = state.with_oxpilot(s);
    }
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

fn rpc_req(body: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1");
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    b.body(Body::from(body.to_owned())).unwrap()
}

async fn rpc_json(app: &axum::Router, cookie: Option<&str>, body: &str) -> serde_json::Value {
    let res = app.clone().oneshot(rpc_req(body, cookie)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
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

async fn signup_and_login(app: &axum::Router, email: &str, username: &str) -> (String, String) {
    let signup_body = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
    );
    let signup = app
        .clone()
        .oneshot(rpc_req(&signup_body, None))
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login_body = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
    );
    let login = app
        .clone()
        .oneshot(rpc_req(&login_body, None))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let user_id = v["data"]["id"].as_str().unwrap().to_string();
    (cookie, user_id)
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

async fn create_repo(app: &axum::Router, cookie: &str, name: &str) {
    let body = format!(
        r#"{{"procedure":"repo.create","input":{{"name":"{name}","visibility":"public","description":""}}}}"#
    );
    let v = rpc_json(app, Some(cookie), &body).await;
    assert_eq!(v["ok"], true, "repo.create — {v}");
}

async fn create_session(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    name: &str,
    goal: &str,
) -> serde_json::Value {
    let body = format!(
        r#"{{"procedure":"oxpilot.session_create","input":{{"repo_owner":"{owner}","repo_name":"{name}","goal":"{goal}"}}}}"#
    );
    rpc_json(app, Some(cookie), &body).await
}

/// Collect an SSE response body to string (bounded so a hung stream fails
/// the test instead of hanging CI).
async fn sse_text(res: axum::http::Response<Body>) -> String {
    let collected = tokio::time::timeout(Duration::from_secs(15), res.into_body().collect())
        .await
        .expect("SSE body did not finish within 15s");
    let bytes = collected.expect("SSE body collect").to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn sse_frames(text: &str) -> Vec<serde_json::Value> {
    text.split("\n\n")
        .filter_map(|block| {
            let data = block
                .lines()
                .find_map(|l| l.strip_prefix("data:").map(str::trim))?;
            serde_json::from_str(data).ok()
        })
        .collect()
}

fn chat_uri(session_id: &str, suffix: &str) -> String {
    format!("/api/oxpilot/sessions/{session_id}/chat{suffix}")
}

#[tokio::test]
async fn status_flag_off_and_gated_denial() {
    let _g = env_lock().await;
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db, None).await;
    let (cookie, _uid) = signup_and_login(&app, "oxp1@example.com", "oxp1").await;

    let st = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.status","input":{}}"#,
    )
    .await;
    assert_eq!(st["ok"], true, "{st}");
    assert_eq!(st["data"]["enabled"], false);
    assert_eq!(st["data"]["reason"], "flag_off");
    assert_eq!(st["data"]["provider"], serde_json::Value::Null);

    let list = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.session_list","input":{}}"#,
    )
    .await;
    assert_eq!(list["ok"], false);
    assert_eq!(list["error"]["code"], "oxpilot.disabled");

    let create = create_session(&app, &cookie, "oxp1", "r", "g").await;
    assert_eq!(create["ok"], false);
    assert_eq!(create["error"]["code"], "oxpilot.disabled");
}

#[tokio::test]
async fn status_enabled_reports_stub_provider() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db, Some(stub_runtime())).await;
    let (cookie, _uid) = signup_and_login(&app, "oxp2@example.com", "oxp2").await;

    let st = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.status","input":{}}"#,
    )
    .await;
    assert_eq!(st["ok"], true, "{st}");
    assert_eq!(st["data"]["enabled"], true);
    assert_eq!(st["data"]["provider"]["id"], "stub");
    assert_eq!(st["data"]["provider"]["model"], "stub-1");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn session_create_list_and_cross_user_isolation() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie_a, uid_a) = signup_and_login(&app, "oxpa@example.com", "oxpa").await;
    let (cookie_b, uid_b) = signup_and_login(&app, "oxpb@example.com", "oxpb").await;
    verify_user(&db, &uid_a).await;
    verify_user(&db, &uid_b).await;
    create_repo(&app, &cookie_a, "proj").await;

    let create = create_session(&app, &cookie_a, "oxpa", "proj", "fix the flaky test").await;
    assert_eq!(create["ok"], true, "{create}");
    let session_id = create["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(create["data"]["branch"], "main");
    assert_eq!(create["data"]["title"], "fix the flaky test");
    assert_eq!(create["data"]["repo_name"], "proj");

    let list = rpc_json(
        &app,
        Some(&cookie_a),
        r#"{"procedure":"oxpilot.session_list","input":{}}"#,
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");
    assert_eq!(list["data"]["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(list["data"]["sessions"][0]["id"], session_id.as_str());

    // Cross-user: list is empty; session-scoped surfaces answer 404.
    let list_b = rpc_json(
        &app,
        Some(&cookie_b),
        r#"{"procedure":"oxpilot.session_list","input":{}}"#,
    )
    .await;
    assert_eq!(list_b["data"]["sessions"].as_array().unwrap().len(), 0);

    let chat = Request::builder()
        .method("POST")
        .uri(chat_uri(&session_id, ""))
        .header("content-type", "application/json")
        .header("cookie", &cookie_b)
        .body(Body::from(r#"{"content":"hi"}"#))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn pat_credential_denied_session_only() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpp@example.com", "oxpp").await;
    verify_user(&db, &uid).await;

    let pat = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"pat.createClassic","input":{"name":"cli","scopes":["repo"]}}"#,
    )
    .await;
    assert_eq!(pat["ok"], true, "{pat}");
    let token = pat["data"]["token"].as_str().unwrap().to_string();

    // Every oxpilot.* proc refuses PAT-class credentials before dispatch.
    let req = Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(r#"{"procedure":"oxpilot.status","input":{}}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], false, "PAT must be denied on oxpilot.* — {v}");

    // The SSE edge rejects Bearer too.
    let chat = Request::builder()
        .method("POST")
        .uri("/api/oxpilot/sessions/s/chat")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(r#"{"content":"hi"}"#))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn chat_post_streams_appends_then_terminal_done() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpc@example.com", "oxpc").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "chatproj").await;
    let create = create_session(&app, &cookie, "oxpc", "chatproj", "say hi").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    let chat = Request::builder()
        .method("POST")
        .uri(chat_uri(&session_id, ""))
        .header("content-type", "application/json")
        .header("cookie", &cookie)
        .body(Body::from(
            r#"{"content":"hello ox","client_msg_id":"cm-1"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let text = sse_text(res).await;
    let frames = sse_frames(&text);
    let types: Vec<&str> = frames.iter().filter_map(|f| f["type"].as_str()).collect();
    assert!(types.contains(&"append"), "frames: {text}");
    assert_eq!(types.last(), Some(&"terminal"));
    let terminal = frames.iter().find(|f| f["type"] == "terminal").unwrap();
    assert_eq!(terminal["result"], "done");
    let message_id = terminal["message_id"].as_str().unwrap();

    // Persisted transcript: user row + completed assistant row.
    let rows = db.list_oxpilot_messages(&session_id).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].role, "user");
    assert_eq!(rows[0].status, "complete");
    assert_eq!(rows[0].client_msg_id.as_deref(), Some("cm-1"));
    assert_eq!(rows[1].id, message_id);
    assert_eq!(rows[1].status, "complete");
    assert!(rows[1].content.contains("hello ox"));

    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn second_post_while_turn_active_is_409() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let runtime = stub_runtime();
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpd@example.com", "oxpd").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "p2").await;
    let create = create_session(&app, &cookie, "oxpd", "p2", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    // Occupy the turn map directly — a live turn blocks the second POST.
    runtime
        .turns()
        .await
        .claim(&session_id, || {
            let (fanout, _) = tokio::sync::broadcast::channel(256);
            oxidean_api_core::oxpilot::ActiveTurn {
                session_id: session_id.clone(),
                turn_id: "t-blocker".to_string(),
                message_id: "m-blocker".to_string(),
                started_seq: std::sync::atomic::AtomicI64::new(9),
                pending_client_msg_id: None,
                pending_content: String::new(),
                fanout,
                emitted_deltas: tokio::sync::Mutex::new(Vec::new()),
                cancel: tokio_util::sync::CancellationToken::new(),
                admission_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
                terminal_sent: std::sync::atomic::AtomicBool::new(false),
                generation: 0,
            }
        })
        .await
        .expect("claim");

    let chat = Request::builder()
        .method("POST")
        .uri(chat_uri(&session_id, ""))
        .header("content-type", "application/json")
        .header("cookie", &cookie)
        .body(Body::from(r#"{"content":"again"}"#))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "turn_active");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn get_stream_emits_snapshot_then_stays_open() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpe@example.com", "oxpe").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "p3").await;
    let create = create_session(&app, &cookie, "oxpe", "p3", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    // A completed turn first so the snapshot carries a transcript.
    let chat = Request::builder()
        .method("POST")
        .uri(chat_uri(&session_id, ""))
        .header("content-type", "application/json")
        .header("cookie", &cookie)
        .body(Body::from(r#"{"content":"seed"}"#))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    let _ = sse_text(res).await;

    let get = Request::builder()
        .method("GET")
        .uri(chat_uri(&session_id, "/stream"))
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(get).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // Read only the first chunk — the stream stays open on ping cadence.
    let mut body = res.into_body();
    let first = tokio::time::timeout(Duration::from_secs(5), body.frame())
        .await
        .expect("first frame")
        .expect("frame present")
        .expect("frame ok");
    let text = String::from_utf8(first.into_data().unwrap().to_vec()).unwrap();
    let frame: serde_json::Value =
        serde_json::from_str(text.trim().strip_prefix("data:").unwrap().trim()).unwrap();
    assert_eq!(frame["type"], "snapshot");
    assert_eq!(frame["messages"].as_array().unwrap().len(), 2);
    assert_eq!(frame["active_turn"], serde_json::Value::Null);
    assert_eq!(frame["pending_queue"].as_array().unwrap().len(), 0);
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn sse_routes_deny_when_disabled() {
    let _g = env_lock().await;
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db, Some(empty_runtime())).await;
    let (cookie, _uid) = signup_and_login(&app, "oxpf@example.com", "oxpf").await;

    let chat = Request::builder()
        .method("POST")
        .uri("/api/oxpilot/sessions/x/chat")
        .header("content-type", "application/json")
        .header("cookie", &cookie)
        .body(Body::from(r#"{"content":"hi"}"#))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "oxpilot.disabled");

    let get = Request::builder()
        .method("GET")
        .uri("/api/oxpilot/sessions/x/chat/stream")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(get).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn provider_failure_yields_error_terminal_and_error_row() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let runtime: SharedOxpilot = OxpilotRuntime::from_slot(OxpilotRuntimeSlot {
        provider: Some(Arc::new(StubProvider { fail: true })),
        model: Some("stub-1".to_string()),
        disabled_reason: None,
        turns: Arc::new(ActiveTurns::default()),
        built_from: None,
        budget_caps: BudgetCaps::default(),
        tools: None,
    });
    let app = test_app(db.clone(), Some(runtime)).await;
    let (cookie, uid) = signup_and_login(&app, "oxpg@example.com", "oxpg").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "pf").await;
    let create = create_session(&app, &cookie, "oxpg", "pf", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    let chat = Request::builder()
        .method("POST")
        .uri(chat_uri(&session_id, ""))
        .header("content-type", "application/json")
        .header("cookie", &cookie)
        .body(Body::from(r#"{"content":"boom"}"#))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    let text = sse_text(res).await;
    let frames = sse_frames(&text);
    let terminal = frames.iter().find(|f| f["type"] == "terminal").unwrap();
    assert_eq!(terminal["result"], "error");
    assert_eq!(terminal["code"], "provider_error");
    let message_id = terminal["message_id"].as_str().unwrap();
    let row = db
        .find_oxpilot_message_by_id(message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "error");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn client_msg_id_resend_returns_snapshot_no_duplicate() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxph@example.com", "oxph").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "pm").await;
    let create = create_session(&app, &cookie, "oxph", "pm", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    let post = |_app: &axum::Router| {
        Request::builder()
            .method("POST")
            .uri(chat_uri(&session_id, ""))
            .header("content-type", "application/json")
            .header("cookie", &cookie)
            .body(Body::from(r#"{"content":"once","client_msg_id":"dup-1"}"#))
            .unwrap()
    };
    let res = app.clone().oneshot(post(&app)).await.unwrap();
    let _ = sse_text(res).await;
    let res = app.clone().oneshot(post(&app)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let text = sse_text(res).await;
    let frames = sse_frames(&text);
    assert_eq!(frames[0]["type"], "snapshot");
    let messages = db.list_oxpilot_messages(&session_id).await.unwrap();
    assert_eq!(
        messages.iter().filter(|m| m.role == "user").count(),
        1,
        "idempotent resend must not duplicate the user row"
    );
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

// ---------- U2 — session-core procs ----------

const U2_PROCS: &[&str] = &[
    "oxpilot.session_get",
    "oxpilot.session_delete",
    "oxpilot.message_queue",
    "oxpilot.queue_list",
    "oxpilot.usage_get",
    // U3 — C1's previously-orphaned abort proc now dispatches.
    "oxpilot.turn_cancel",
    // U8 — the session-scoped grant read + the inline queued-row cancel
    // ride the identical SessionOnly / require_enabled gates.
    "oxpilot.queue_delete",
    "oxpilot.grant_list",
];

/// Occupy the turn map — `message_queue` only exists under a live turn.
async fn claim_turn(runtime: &SharedOxpilot, session_id: &str) {
    runtime
        .turns()
        .await
        .claim(session_id, || {
            let (fanout, _) = tokio::sync::broadcast::channel(256);
            oxidean_api_core::oxpilot::ActiveTurn {
                session_id: session_id.to_string(),
                turn_id: "t-test".to_string(),
                message_id: "m-test".to_string(),
                started_seq: std::sync::atomic::AtomicI64::new(9),
                pending_client_msg_id: None,
                pending_content: String::new(),
                fanout,
                emitted_deltas: tokio::sync::Mutex::new(Vec::new()),
                cancel: tokio_util::sync::CancellationToken::new(),
                admission_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
                terminal_sent: std::sync::atomic::AtomicBool::new(false),
                generation: 0,
            }
        })
        .await
        .expect("claim turn");
}

#[tokio::test]
async fn u2_procs_anonymous_and_pat_denied() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpm@example.com", "oxpm").await;
    verify_user(&db, &uid).await;

    let pat = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"pat.createClassic","input":{"name":"cli","scopes":["repo"]}}"#,
    )
    .await;
    let token = pat["data"]["token"].as_str().unwrap().to_string();

    for proc in U2_PROCS {
        let body = format!(r#"{{"procedure":"{proc}","input":{{"session_id":"s"}}}}"#);
        // anonymous → auth.unauthenticated
        let anon = rpc_json(&app, None, &body).await;
        assert_eq!(anon["ok"], false, "{proc} anon — {anon}");
        assert_eq!(anon["error"]["code"], "auth.unauthenticated", "{proc}");
        // PAT → auth.pat_scope (SessionOnly denial upstream of dispatch)
        let req = Request::builder()
            .method("POST")
            .uri("/api/rpc")
            .header("content-type", "application/json")
            .header("Oxidean-RPC-Version", "1")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["ok"], false, "{proc} PAT — {v}");
        assert_eq!(v["error"]["code"], "auth.pat_scope", "{proc}");
    }
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn u2_procs_flag_off_denied() {
    let _g = env_lock().await;
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(empty_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpn@example.com", "oxpn").await;
    verify_user(&db, &uid).await;

    for proc in U2_PROCS {
        let v = rpc_json(
            &app,
            Some(&cookie),
            &format!(r#"{{"procedure":"{proc}","input":{{"session_id":"s"}}}}"#),
        )
        .await;
        assert_eq!(v["ok"], false, "{proc} flag-off — {v}");
        assert_eq!(v["error"]["code"], "oxpilot.disabled", "{proc}");
    }
}

#[tokio::test]
async fn session_get_and_delete_happy_path() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpq@example.com", "oxpq").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "gd").await;
    let create = create_session(&app, &cookie, "oxpq", "gd", "goal one").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    let get = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.session_get","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(get["ok"], true, "{get}");
    assert_eq!(get["data"]["id"], session_id.as_str());
    assert_eq!(get["data"]["title"], "goal one");
    assert_eq!(get["data"]["status"], "active");

    let del = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.session_delete","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");
    assert_eq!(del["data"]["deleted"], true);

    // Gone — get + a second delete both resolve not_found (and the
    // second attempt still writes its audit row).
    let get2 = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.session_get","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(get2["error"]["code"], "not_found");
    let del2 = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.session_delete","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(del2["error"]["code"], "not_found");

    // Audit rows: `ok` on success, `not_found` on the second try.
    let rows = db
        .list_audit_events_for_actor(&uid, Some("oxpilot.session_delete"), 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    let results: Vec<String> = rows
        .iter()
        .map(|r| {
            let d: serde_json::Value = serde_json::from_str(r.detail.as_deref().unwrap()).unwrap();
            d["result"].as_str().unwrap().to_string()
        })
        .collect();
    assert!(results.contains(&"ok".to_string()), "{results:?}");
    assert!(results.contains(&"not_found".to_string()), "{results:?}");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

struct FailingHook;

#[async_trait::async_trait]
impl oxidean_api_core::oxpilot::SessionDeleteHook for FailingHook {
    async fn on_session_delete(
        &self,
        _session_id: &str,
        _user_id: &str,
    ) -> Result<(), oxidean_core::AppError> {
        Err(oxidean_core::AppError::new("hook.boom", "refusing"))
    }
}

#[tokio::test]
async fn session_delete_hook_failed_audits_and_keeps_row() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;

    // Pre-install a service carrying a failing hook — `install_sessions`
    // is set-once, so test_app's default install can't displace it.
    let svc = oxidean_api_oxpilot::services::OxpilotSessionService::new(db.clone());
    svc.register_delete_hook(Arc::new(FailingHook));
    let runtime = stub_runtime();
    runtime.install_sessions(Arc::new(svc));
    let app = test_app(db.clone(), Some(runtime)).await;

    let (cookie, uid) = signup_and_login(&app, "oxpr@example.com", "oxpr").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "hf").await;
    let create = create_session(&app, &cookie, "oxpr", "hf", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    let del = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.session_delete","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(del["ok"], false, "{del}");
    // The hook's own code reaches the caller — `hook_index` rides `data`.
    assert_eq!(del["error"]["code"], "hook.boom");
    assert_eq!(del["error"]["data"]["hook_index"], 0);

    // The row survives — the cascade never ran.
    let get = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.session_get","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(get["ok"], true, "{get}");

    // Audit: result=hook_failed with the failing hook_index.
    let rows = db
        .list_audit_events_for_actor(&uid, Some("oxpilot.session_delete"), 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let detail: serde_json::Value =
        serde_json::from_str(rows[0].detail.as_deref().unwrap()).unwrap();
    assert_eq!(detail["result"], "hook_failed");
    assert_eq!(detail["hook_index"], 0);
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn message_queue_receipt_shapes_and_queue_list() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let runtime = stub_runtime();
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxps@example.com", "oxps").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "mq").await;
    let create = create_session(&app, &cookie, "oxps", "mq", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    // No live turn → session_active_turn_absent (the client must POST
    // /chat instead).
    let absent = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.message_queue","input":{{"session_id":"{session_id}","content":"hi"}}}}"#
        ),
    )
    .await;
    assert_eq!(absent["error"]["code"], "session_active_turn_absent");

    claim_turn(&runtime, &session_id).await;

    // Normal receipt — ordinal position.
    let q1 = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.message_queue","input":{{"session_id":"{session_id}","content":"first","client_msg_id":"cm-q1"}}}}"#
        ),
    )
    .await;
    assert_eq!(q1["ok"], true, "{q1}");
    let queued_id = q1["data"]["queued_id"].as_str().unwrap().to_string();
    assert_eq!(q1["data"]["position"], 1);
    // Idempotent resend — same receipt.
    let q1b = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.message_queue","input":{{"session_id":"{session_id}","content":"first","client_msg_id":"cm-q1"}}}}"#
        ),
    )
    .await;
    assert_eq!(q1b["data"]["queued_id"], queued_id.as_str());
    assert_eq!(q1b["data"]["position"], 1);

    let q2 = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.message_queue","input":{{"session_id":"{session_id}","content":"second","client_msg_id":"cm-q2"}}}}"#
        ),
    )
    .await;
    assert_eq!(q2["data"]["position"], 2);

    // queue_list — delivery order, pinned fields.
    let list = rpc_json(
        &app,
        Some(&cookie),
        &format!(r#"{{"procedure":"oxpilot.queue_list","input":{{"session_id":"{session_id}"}}}}"#),
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");
    let queued = list["data"]["queued"].as_array().unwrap();
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[0]["id"], queued_id.as_str());
    assert_eq!(queued[0]["content"], "first");
    assert_eq!(queued[0]["client_msg_id"], "cm-q1");
    assert_eq!(queued[1]["content"], "second");
    assert!(queued[0]["queue_seq"].as_i64().unwrap() < queued[1]["queue_seq"].as_i64().unwrap());

    // Transcript-hit: a cmid already persisted as a transcript row
    // returns the {null, null} receipt. Complete a real turn first so
    // "cm-t" lands in oxpilot_messages.
    runtime.turns().await.release(&session_id, "t-test").await;
    let chat = Request::builder()
        .method("POST")
        .uri(chat_uri(&session_id, ""))
        .header("content-type", "application/json")
        .header("cookie", &cookie)
        .body(Body::from(r#"{"content":"hi","client_msg_id":"cm-t"}"#))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    let _ = sse_text(res).await;
    // Re-occupy so the queue check passes, then dedupe → null receipt.
    claim_turn(&runtime, &session_id).await;
    let hit = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.message_queue","input":{{"session_id":"{session_id}","content":"hi","client_msg_id":"cm-t"}}}}"#
        ),
    )
    .await;
    assert_eq!(hit["ok"], true, "{hit}");
    assert_eq!(hit["data"]["queued_id"], serde_json::Value::Null);
    assert_eq!(hit["data"]["position"], serde_json::Value::Null);

    // Budget honesty — both dedupe hits answered WITHOUT admitting:
    // only q1, q2, and the cm-t POST reserved a request (BR2.2 +
    // BR6.2 take two; a resend is never charged).
    let usage = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.usage_get","input":{}}"#,
    )
    .await;
    assert_eq!(usage["ok"], true, "{usage}");
    assert_eq!(usage["data"]["requests_used"], 3);
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn usage_get_c1_payload_and_recorded_values() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let runtime = stub_runtime(); // BudgetCaps::default → unbounded
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpu@example.com", "oxpu").await;
    verify_user(&db, &uid).await;

    let v = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.usage_get","input":{}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    let d = &v["data"];
    assert_eq!(d["period"].as_str().unwrap().len(), "YYYY-MM-DD".len());
    assert_eq!(d["requests_used"], 0);
    assert_eq!(d["tokens_used"], 0);
    assert_eq!(d["requests_cap"], serde_json::Value::Null);
    assert_eq!(d["tokens_cap"], serde_json::Value::Null);

    // Record spend through the installed seam, re-read the view.
    let svc = runtime.sessions().expect("sessions");
    svc.record_usage(
        &uid,
        oxidean_api_core::oxpilot::UsageDelta {
            requests: 2,
            tokens: Some(5),
        },
    )
    .await
    .unwrap();
    let v2 = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.usage_get","input":{}}"#,
    )
    .await;
    assert_eq!(v2["data"]["requests_used"], 2);
    assert_eq!(v2["data"]["tokens_used"], 5);
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn message_queue_budget_exceeded_with_scope() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    // A runtime with a user cap of 1 — one recorded request breaches it.
    let mut slot = OxpilotRuntimeSlot {
        provider: Some(Arc::new(StubProvider { fail: false })),
        model: Some("stub-1".to_string()),
        disabled_reason: None,
        turns: Arc::new(ActiveTurns::default()),
        built_from: Some(ProviderIdentity {
            provider_id: "stub".to_string(),
            model: Some("stub-1".to_string()),
            base_url: None,
            key_source: KeySource::None,
        }),
        budget_caps: BudgetCaps::default(),
        tools: None,
    };
    slot.budget_caps = BudgetCaps {
        user: Some(1),
        instance: None,
    };
    let runtime = OxpilotRuntime::from_slot(slot);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpv@example.com", "oxpv").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "bd").await;
    let create = create_session(&app, &cookie, "oxpv", "bd", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    runtime
        .sessions()
        .unwrap()
        .record_usage(
            &uid,
            oxidean_api_core::oxpilot::UsageDelta {
                requests: 1,
                tokens: None,
            },
        )
        .await
        .unwrap();
    claim_turn(&runtime, &session_id).await;
    let v = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.message_queue","input":{{"session_id":"{session_id}","content":"over"}}}}"#
        ),
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "budget_exceeded");
    assert_eq!(v["error"]["data"]["scope"], "user");

    // The spend view still answers under breach (it never refuses).
    let usage = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.usage_get","input":{}}"#,
    )
    .await;
    assert_eq!(usage["ok"], true, "{usage}");
    assert_eq!(usage["data"]["requests_cap"], 1);
    assert_eq!(usage["data"]["requests_used"], 1);
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

/// The five U2 names ride the shell's aggregated PROCEDURES surface —
/// the manifest/spec consumers see them exactly once.
#[test]
fn u2_procs_in_aggregated_procedures() {
    for proc in U2_PROCS {
        assert!(
            oxidean_api::rpc::PROCEDURES.contains(proc),
            "{proc} missing from rpc::PROCEDURES"
        );
    }
}

#[tokio::test]
async fn chat_post_queued_client_msg_id_answers_turn_active() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpd@example.com", "oxpd").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "qdedupe").await;
    let create = create_session(&app, &cookie, "oxpd", "qdedupe", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    // An orphaned queued row (e.g. a turn that ended before its drain)
    // still holds the cmid — a resent POST /chat must NOT mint a second
    // transcript row: the duplicate would make every later
    // `drain_queue_tx` commit fail on UNIQUE(session_id, client_msg_id).
    db.enqueue_oxpilot_queued(&oxidean_db::NewOxpilotQueued {
        id: "oxpq-orphan-1",
        session_id: &session_id,
        queue_seq: 1,
        content: "queued earlier",
        client_msg_id: Some("cm-orphan"),
    })
    .await
    .expect("queue insert");

    let chat = Request::builder()
        .method("POST")
        .uri(chat_uri(&session_id, ""))
        .header("content-type", "application/json")
        .header("cookie", &cookie)
        .body(Body::from(
            r#"{"content":"queued earlier","client_msg_id":"cm-orphan"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT, "{res:?}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "turn_active");

    // No transcript row was minted — the queue is the only artifact.
    assert!(db
        .list_oxpilot_messages(&session_id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        db.list_oxpilot_queued_for_session(&session_id)
            .await
            .unwrap()
            .len(),
        1
    );
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

/// BR6.2 take two — admission runs BEFORE the residual drain: a
/// `budget_exceeded` POST releases the claim with the queue untouched,
/// so a retried send re-drains the same rows instead of finding them
/// stranded as dedupe-only transcript rows.
#[tokio::test]
async fn chat_post_budget_refusal_preserves_residual_queue() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let mut slot = OxpilotRuntimeSlot {
        provider: Some(Arc::new(StubProvider { fail: false })),
        model: Some("stub-1".to_string()),
        disabled_reason: None,
        turns: Arc::new(ActiveTurns::default()),
        built_from: Some(ProviderIdentity {
            provider_id: "stub".to_string(),
            model: Some("stub-1".to_string()),
            base_url: None,
            key_source: KeySource::None,
        }),
        budget_caps: BudgetCaps::default(),
        tools: None,
    };
    slot.budget_caps = BudgetCaps {
        user: Some(1),
        instance: None,
    };
    let runtime = OxpilotRuntime::from_slot(slot);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpw@example.com", "oxpw").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "bdq").await;
    let create = create_session(&app, &cookie, "oxpw", "bdq", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    // A residual queued row the next turn should drain…
    db.enqueue_oxpilot_queued(&oxidean_db::NewOxpilotQueued {
        id: "oxpq-resid-1",
        session_id: &session_id,
        queue_seq: 1,
        content: "residual",
        client_msg_id: Some("cm-resid"),
    })
    .await
    .expect("queue insert");
    // …and a user already at the 1-request cap → admission refuses.
    runtime
        .sessions()
        .unwrap()
        .record_usage(
            &uid,
            oxidean_api_core::oxpilot::UsageDelta {
                requests: 1,
                tokens: None,
            },
        )
        .await
        .unwrap();

    let chat = Request::builder()
        .method("POST")
        .uri(chat_uri(&session_id, ""))
        .header("content-type", "application/json")
        .header("cookie", &cookie)
        .body(Body::from(r#"{"content":"again"}"#))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS, "{res:?}");

    // The refusal must NOT have committed the drain — the row is still
    // queued and no transcript row materialized.
    let queued = db
        .list_oxpilot_queued_for_session(&session_id)
        .await
        .unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].id, "oxpq-resid-1");
    assert!(db
        .list_oxpilot_messages(&session_id)
        .await
        .unwrap()
        .is_empty());
    // …and the released claim lets the next attempt claim again.
    assert!(runtime.turns().await.get(&session_id).await.is_none());
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

// ---------- U3 — turn_cancel + live-turn abort ----------

#[tokio::test]
async fn turn_cancel_aborts_live_turn() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let runtime = stub_runtime();
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxtc@example.com", "oxtc").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "tc").await;
    let create = create_session(&app, &cookie, "oxtc", "tc", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    claim_turn(&runtime, &session_id).await;
    let v = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.turn_cancel","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["aborted"], true, "{v}");
    // The CAS landed the terminal + the token is cancelled.
    let turn = runtime.turns().await.get(&session_id).await.unwrap();
    assert!(turn.cancel.is_cancelled());
    let buf = turn.emitted_deltas.lock().await;
    assert_eq!(buf.last().unwrap()["result"], "aborted");
    drop(buf);
    // Second call — the terminal already landed ⇒ CAS-loss arm answers
    // `{aborted:false}` (the intent "turn is dead" is met either way).
    let v2 = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.turn_cancel","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(v2["ok"], true, "{v2}");
    assert_eq!(v2["data"]["aborted"], false, "{v2}");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn turn_cancel_absent_and_foreign() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxtd@example.com", "oxtd").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "td").await;
    let create = create_session(&app, &cookie, "oxtd", "td", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();
    let (cookie2, uid2) = signup_and_login(&app, "oxte@example.com", "oxte").await;
    verify_user(&db, &uid2).await;

    // No live turn on an owned session ⇒ no_active_turn (C1).
    let v = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.turn_cancel","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "no_active_turn");

    // A foreign session id is indistinguishable from absent — the proc
    // never reads turn state before ownership resolves (anti-enum).
    let v = rpc_json(
        &app,
        Some(&cookie2),
        &format!(
            r#"{{"procedure":"oxpilot.turn_cancel","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "not_found");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn session_delete_aborts_live_turn_via_hook() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let runtime = stub_runtime();
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxdh@example.com", "oxdh").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "dh").await;
    let create = create_session(&app, &cookie, "oxdh", "dh", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    claim_turn(&runtime, &session_id).await;
    let del = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.session_delete","input":{{"session_id":"{session_id}"}}}}"#
        ),
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");
    // The C8 hook fired BEFORE the cascade: the turn saw its terminal
    // abort frame + cancel token.
    let turn = runtime.turns().await.get(&session_id).await;
    if let Some(t) = &turn {
        assert!(t.cancel.is_cancelled());
        assert_eq!(
            t.emitted_deltas.lock().await.last().unwrap()["result"],
            "aborted"
        );
    }
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

/// A GET attach racing a turn whose terminal already buffered replays
/// the buffer and ENDS on the terminal frame — never a hung stream.
#[tokio::test]
async fn get_stream_late_attach_replays_buffer_to_terminal() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let runtime = stub_runtime();
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxla@example.com", "oxla").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "la").await;
    let create = create_session(&app, &cookie, "oxla", "la", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    claim_turn(&runtime, &session_id).await;
    // The turn is still in the map but its terminal already landed —
    // the attach must replay the buffer and end on the terminal frame.
    let turn = runtime.turns().await.get(&session_id).await.unwrap();
    let frame = serde_json::json!({
        "type": "terminal",
        "message_id": turn.message_id,
        "result": "done",
    });
    turn.emitted_deltas.lock().await.push(frame.clone());
    let _ = turn.fanout.send(frame);

    let get = Request::builder()
        .method("GET")
        .uri(format!("/api/oxpilot/sessions/{session_id}/chat/stream"))
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(get).await.unwrap();
    let text = sse_text(res).await; // terminates ⇒ collect finishes
    let frames = sse_frames(&text);
    let last = frames.last().expect("at least one frame");
    assert_eq!(last["type"], "terminal");
    assert_eq!(last["result"], "done");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn chat_post_budget_exceeded_is_429() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let mut slot = OxpilotRuntimeSlot {
        provider: Some(Arc::new(StubProvider { fail: false })),
        model: Some("stub-1".to_string()),
        disabled_reason: None,
        turns: Arc::new(ActiveTurns::default()),
        built_from: None,
        budget_caps: BudgetCaps::default(),
        tools: None,
    };
    slot.budget_caps = BudgetCaps {
        user: Some(1),
        instance: None,
    };
    let runtime = OxpilotRuntime::from_slot(slot);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxbp@example.com", "oxbp").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "bp").await;
    let create = create_session(&app, &cookie, "oxbp", "bp", "g").await;
    let session_id = create["data"]["id"].as_str().unwrap().to_string();

    // One recorded request breaches the user cap — the POST refuses
    // BEFORE claiming a turn.
    runtime
        .sessions()
        .unwrap()
        .record_usage(
            &uid,
            oxidean_api_core::oxpilot::UsageDelta {
                requests: 1,
                tokens: None,
            },
        )
        .await
        .unwrap();
    let chat = Request::builder()
        .method("POST")
        .uri(chat_uri(&session_id, ""))
        .header("content-type", "application/json")
        .header("cookie", &cookie)
        .body(Body::from(r#"{"content":"spend"}"#))
        .unwrap();
    let res = app.clone().oneshot(chat).await.unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], "budget_exceeded");
    // No turn was claimed — the session is free.
    assert!(runtime.turns().await.get(&session_id).await.is_none());
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

// ---- U5 (oxpilot-tools) — `oxpilot.diff_*` procs ----

const U5_PROCS: &[&str] = &[
    "oxpilot.diff_list",
    "oxpilot.diff_get",
    "oxpilot.diff_apply",
];

/// `test_app` with an isolated on-disk repos dir — the diff_apply chain
/// performs real git content ops.
async fn test_app_repos(
    db: Database,
    slot: Option<SharedOxpilot>,
    repos_dir: std::path::PathBuf,
) -> axum::Router {
    let mut state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
    if let Some(s) = slot {
        let svc = std::sync::Arc::new(oxidean_api_oxpilot::services::OxpilotSessionService::new(
            state.db.clone(),
        ));
        svc.register_delete_hook(std::sync::Arc::new(
            oxidean_api_oxpilot::runtime::OxpilotTurnAbortHook::new(s.clone()),
        ));
        s.install_sessions(svc);
        state = state.with_oxpilot(s);
    }
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

fn proposal<'a>(
    id: &'a str,
    session_id: &'a str,
    base: &'a str,
    ops_json: &'a str,
) -> oxidean_db::NewOxpilotProposal<'a> {
    oxidean_db::NewOxpilotProposal {
        id,
        session_id,
        title: "agent diff",
        base,
        // Unpinned — `diff_apply` falls back to `base` verbatim, which
        // is what the `no-such-ref` apply-failure leg relies on.
        base_sha: "",
        diff_body: "diff --git a/hello.txt b/hello.txt\nnew file mode 100644\n",
        ops_json,
    }
}

#[tokio::test]
async fn u5_procs_anonymous_and_pat_denied() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpz@example.com", "oxpz").await;
    verify_user(&db, &uid).await;

    let pat = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"pat.createClassic","input":{"name":"cli","scopes":["repo"]}}"#,
    )
    .await;
    let token = pat["data"]["token"].as_str().unwrap().to_string();

    for proc in U5_PROCS {
        let body =
            format!(r#"{{"procedure":"{proc}","input":{{"session_id":"s","proposal_id":"p"}}}}"#);
        let anon = rpc_json(&app, None, &body).await;
        assert_eq!(anon["ok"], false, "{proc} anon — {anon}");
        assert_eq!(anon["error"]["code"], "auth.unauthenticated", "{proc}");
        let req = Request::builder()
            .method("POST")
            .uri("/api/rpc")
            .header("content-type", "application/json")
            .header("Oxidean-RPC-Version", "1")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["ok"], false, "{proc} PAT — {v}");
        assert_eq!(v["error"]["code"], "auth.pat_scope", "{proc}");
    }
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

#[tokio::test]
async fn u5_procs_flag_off_denied() {
    let _g = env_lock().await;
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(empty_runtime())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpy@example.com", "oxpy").await;
    verify_user(&db, &uid).await;

    for proc in U5_PROCS {
        let v = rpc_json(
            &app,
            Some(&cookie),
            &format!(r#"{{"procedure":"{proc}","input":{{"session_id":"s","proposal_id":"p"}}}}"#),
        )
        .await;
        assert_eq!(v["ok"], false, "{proc} flag-off — {v}");
        assert_eq!(v["error"]["code"], "oxpilot.disabled", "{proc}");
    }
}

#[test]
fn u5_procs_in_aggregated_procedures() {
    for proc in U5_PROCS {
        assert!(
            oxidean_api::rpc::PROCEDURES.contains(proc),
            "{proc} missing from rpc::PROCEDURES"
        );
    }
}

/// `diff_list` is owned-session-gated and newest-first; `diff_get`
/// returns the C1 wire shape — `files[]` derived from `diff_body`,
/// `ops_json` never on the wire.
#[tokio::test]
async fn diff_list_and_get_session_scoped() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), Some(stub_runtime())).await;
    let (cookie_a, uid_a) = signup_and_login(&app, "oxpw@example.com", "oxpw").await;
    let (cookie_b, uid_b) = signup_and_login(&app, "oxpv@example.com", "oxpv").await;
    verify_user(&db, &uid_a).await;
    verify_user(&db, &uid_b).await;
    create_repo(&app, &cookie_a, "diffs").await;

    let create = create_session(&app, &cookie_a, "oxpw", "diffs", "goal").await;
    assert_eq!(create["ok"], true, "{create}");
    let sid = create["data"]["id"].as_str().unwrap().to_string();

    db.insert_oxpilot_proposal(&proposal("oxpp-int-a", &sid, "main", "[]"))
        .await
        .expect("p1");
    db.insert_oxpilot_proposal(&proposal("oxpp-int-b", &sid, "main", "[]"))
        .await
        .expect("p2");

    let list = rpc_json(
        &app,
        Some(&cookie_a),
        &format!(r#"{{"procedure":"oxpilot.diff_list","input":{{"session_id":"{sid}"}}}}"#),
    )
    .await;
    assert_eq!(list["ok"], true, "{list}");
    let diffs = list["data"]["diffs"].as_array().unwrap();
    assert_eq!(diffs.len(), 2);
    assert_eq!(diffs[0]["status"], "pending");
    assert_eq!(diffs[0]["files"][0]["path"], "hello.txt");
    assert!(
        diffs[0].get("ops_json").is_none(),
        "ops_json stays off the wire"
    );

    // Foreign session ids resolve as not_found — not an empty list.
    let foreign = rpc_json(
        &app,
        Some(&cookie_b),
        &format!(r#"{{"procedure":"oxpilot.diff_list","input":{{"session_id":"{sid}"}}}}"#),
    )
    .await;
    assert_eq!(foreign["ok"], false, "{foreign}");
    assert_eq!(foreign["error"]["code"], "not_found");

    let get = rpc_json(
        &app,
        Some(&cookie_a),
        r#"{"procedure":"oxpilot.diff_get","input":{"proposal_id":"oxpp-int-a"}}"#,
    )
    .await;
    assert_eq!(get["ok"], true, "{get}");
    assert_eq!(get["data"]["id"], "oxpp-int-a");
    assert_eq!(get["data"]["session_id"], sid.as_str());
    assert!(get["data"].get("ops_json").is_none());

    // diff_get is user-scoped (BR5.2): user B's fetch of A's proposal
    // denies identically to a missing id.
    let get_b = rpc_json(
        &app,
        Some(&cookie_b),
        r#"{"procedure":"oxpilot.diff_get","input":{"proposal_id":"oxpp-int-a"}}"#,
    )
    .await;
    assert_eq!(get_b["ok"], false, "{get_b}");
    assert_eq!(get_b["error"]["code"], "not_found");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

/// `diff_apply` happy path — branch → file ops → pull.create →
/// applied; a second apply on the same proposal is refused as
/// `diff_not_pending`.
#[tokio::test]
async fn diff_apply_chain_materializes_then_rejects_second_apply() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("oxp_apply.db").display());
    let db = support::connect_test_db(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app_repos(db.clone(), Some(stub_runtime()), dir.path().join("repos")).await;
    let (cookie, uid) = signup_and_login(&app, "oxpu@example.com", "oxpu").await;
    verify_user(&db, &uid).await;
    // stack_id seeds the initial `main` commit the apply branches from.
    let create = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"repo.create","input":{"name":"applyrepo","visibility":"public","description":"","stack_id":"rust"}}"#,
    )
    .await;
    assert_eq!(create["ok"], true, "{create}");

    let session = create_session(&app, &cookie, "oxpu", "applyrepo", "goal").await;
    assert_eq!(session["ok"], true, "{session}");
    let sid = session["data"]["id"].as_str().unwrap().to_string();
    let ops =
        r#"[{"path":"hello.txt","op":"create","new_path":null,"content_b64":"aGVsbG8gd29ybGQ="}]"#;
    db.insert_oxpilot_proposal(&proposal("oxpp-apply-1", &sid, "main", ops))
        .await
        .expect("proposal");

    let apply = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.diff_apply","input":{"proposal_id":"oxpp-apply-1"}}"#,
    )
    .await;
    assert_eq!(apply["ok"], true, "{apply}");
    assert_eq!(
        apply["data"]["pr_ref"].as_str().unwrap(),
        "oxpu/applyrepo#1"
    );

    let row = db
        .find_oxpilot_proposal("oxpp-apply-1")
        .await
        .expect("row")
        .expect("present");
    assert_eq!(row.status, "applied");
    assert_eq!(row.pr_ref.as_deref(), Some("oxpu/applyrepo#1"));

    // Double-apply refuses — status moved off pending.
    let again = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.diff_apply","input":{"proposal_id":"oxpp-apply-1"}}"#,
    )
    .await;
    assert_eq!(again["ok"], false, "{again}");
    assert_eq!(again["error"]["code"], "diff_not_pending");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

/// Fail-stop (BR5.7): a backing-proc failure lands as
/// `apply_failed:<proc>` and leaves the proposal `pending` for retry.
#[tokio::test]
async fn diff_apply_fail_stop_keeps_proposal_pending() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("oxp_fail.db").display());
    let db = support::connect_test_db(&url).await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let app = test_app_repos(db.clone(), Some(stub_runtime()), dir.path().join("repos")).await;
    let (cookie, uid) = signup_and_login(&app, "oxpt@example.com", "oxpt").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "failrepo").await;

    let session = create_session(&app, &cookie, "oxpt", "failrepo", "goal").await;
    assert_eq!(session["ok"], true, "{session}");
    let sid = session["data"]["id"].as_str().unwrap().to_string();
    let ops = r#"[{"path":"x.txt","op":"create","new_path":null,"content_b64":"eA=="}]"#;
    // `base` points at a ref that does not exist — branchCreate is the
    // first backing step and must fail before any file op runs.
    db.insert_oxpilot_proposal(&proposal("oxpp-fail-1", &sid, "no-such-ref", ops))
        .await
        .expect("proposal");

    let apply = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.diff_apply","input":{"proposal_id":"oxpp-fail-1"}}"#,
    )
    .await;
    assert_eq!(apply["ok"], false, "{apply}");
    assert_eq!(apply["error"]["code"], "apply_failed:repo.branchCreate");

    let row = db
        .find_oxpilot_proposal("oxpp-fail-1")
        .await
        .expect("row")
        .expect("present");
    assert_eq!(row.status, "pending", "failed apply must stay retryable");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

// ---------- U8 amendments — session_create{target_id}, grant_list{session_id}, queue_delete ----------
//
// The connector seam these procs ride is the REAL DB-backed gateway:
// `connected` is reachable only through mint → redeem → connect-grant
// approval — the same lifecycle the owner UI and the C3 routes drive.
// Seeding through the seam keeps lifecycle invariants honest; direct
// row writes are reserved for already-terminal grant shapes.

use oxidean_api_core::oxpilot::ConnectorGateway;
use oxidean_api_core::rpc::ClientMeta;
use oxidean_api_oxpilot::connector::OxpilotConnectorGateway;

/// The app variant with the connector-gateway seam installed — the
/// same `SessionService` Arc backs the session procs AND the gateway's
/// ownership checks (the one-gateway rule).
async fn test_app_with_gateway(
    db: Database,
    slot: SharedOxpilot,
) -> (axum::Router, Arc<OxpilotConnectorGateway>) {
    let mut state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    );
    let svc = std::sync::Arc::new(oxidean_api_oxpilot::services::OxpilotSessionService::new(
        state.db.clone(),
    ));
    svc.register_delete_hook(std::sync::Arc::new(
        oxidean_api_oxpilot::runtime::OxpilotTurnAbortHook::new(slot.clone()),
    ));
    slot.install_sessions(svc.clone());
    let gateway = Arc::new(OxpilotConnectorGateway::new(db, svc));
    slot.install_gateway(gateway.clone());
    state = state.with_oxpilot(slot);
    let cors = build_cors("development", None).expect("cors");
    (router_with_state(state, cors), gateway)
}

/// Mint → redeem through the seam — a `pending_consent` target with
/// its live connect grant, exactly what a real `/pair` leaves behind.
async fn redeemed_target_id(gw: &OxpilotConnectorGateway, uid: &str, label: &str) -> String {
    let meta = ClientMeta::default();
    let caps = vec!["exec_shell".to_string()];
    let pairing = gw
        .mint_pairing(&meta, uid, label, &caps)
        .await
        .expect("mint_pairing");
    let redeemed = gw
        .redeem_pairing(&meta, &pairing.pairing_code, label, &caps)
        .await
        .expect("redeem_pairing");
    redeemed.target_id
}

/// Approve the pending `connect` grant minted at redeem — BR3.2's one
/// path from `pending_consent` to `connected`.
async fn approve_connect_grant(gw: &OxpilotConnectorGateway, uid: &str, target_id: &str) {
    let meta = ClientMeta::default();
    let grants = gw
        .list_grants(&meta, uid, Some(target_id), None)
        .await
        .expect("list_grants");
    let connect = grants
        .iter()
        .find(|g| g.kind == "connect" && g.state == "pending")
        .expect("a pending connect grant must ride the fresh target");
    gw.respond_grant(&meta, uid, &connect.id, true, None)
        .await
        .expect("approve connect grant");
}

/// `session_create` carrying an explicit `target_id` — everything else
/// about the request mirrors the untargeted helper.
async fn create_session_targeted(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    name: &str,
    target_id: &str,
) -> serde_json::Value {
    let body = format!(
        r#"{{"procedure":"oxpilot.session_create","input":{{"repo_owner":"{owner}","repo_name":"{name}","goal":"g","target_id":"{target_id}"}}}}"#
    );
    rpc_json(app, Some(cookie), &body).await
}

/// `session_create{target_id}` — a `connected` target binds onto the
/// created row and rides both owner reads (`session_get` AND
/// `session_list`); absent and foreign ids answer `not_found`
/// identically (BR7.1), a non-`connected` lifecycle `bad_input`.
#[tokio::test]
async fn session_create_target_id_binds_persists_and_validates() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let (app, gw) = test_app_with_gateway(db.clone(), stub_runtime()).await;
    let (cookie, uid) = signup_and_login(&app, "oxpu@example.com", "oxpu").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "tgt").await;

    // A pending_consent target is known-but-not-connected — the dialog
    // never offers it, and the proc refuses it too (bad_input).
    let target_id = redeemed_target_id(&gw, &uid, "desk-box").await;
    let refused = create_session_targeted(&app, &cookie, "oxpu", "tgt", &target_id).await;
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(refused["error"]["code"], "rpc.bad_input", "{refused}");

    // Absent ⇒ not_found; foreign ⇒ not_found — identical codes keep
    // target existence unenumerable (BR7.1). The foreign row is minted
    // under a second user's id through the same seam.
    let absent = create_session_targeted(&app, &cookie, "oxpu", "tgt", "t-absent").await;
    assert_eq!(absent["ok"], false, "{absent}");
    assert_eq!(absent["error"]["code"], "not_found", "{absent}");
    let (_cookie2, uid2) = signup_and_login(&app, "oxpv@example.com", "oxpv").await;
    verify_user(&db, &uid2).await;
    let foreign = redeemed_target_id(&gw, &uid2, "other-box").await;
    let foreign_res = create_session_targeted(&app, &cookie, "oxpu", "tgt", &foreign).await;
    assert_eq!(foreign_res["ok"], false, "{foreign_res}");
    assert_eq!(foreign_res["error"]["code"], "not_found", "{foreign_res}");

    // Approve the connect grant → `connected` — now the bind persists
    // onto the row AND projects through both owner reads.
    approve_connect_grant(&gw, &uid, &target_id).await;
    let created = create_session_targeted(&app, &cookie, "oxpu", "tgt", &target_id).await;
    assert_eq!(created["ok"], true, "{created}");
    let sid = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        created["data"]["computer_use_target_id"],
        serde_json::Value::from(target_id.as_str()),
        "{created}"
    );

    let got = rpc_json(
        &app,
        Some(&cookie),
        &format!(r#"{{"procedure":"oxpilot.session_get","input":{{"session_id":"{sid}"}}}}"#),
    )
    .await;
    assert_eq!(got["ok"], true, "{got}");
    assert_eq!(
        got["data"]["computer_use_target_id"],
        serde_json::Value::from(target_id.as_str()),
        "{got}"
    );

    let listed = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.session_list","input":{}}"#,
    )
    .await;
    assert_eq!(listed["ok"], true, "{listed}");
    let row = listed["data"]["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == sid)
        .expect("created session in list");
    assert_eq!(
        row["computer_use_target_id"],
        serde_json::Value::from(target_id.as_str()),
        "{row}"
    );
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

/// `grant_list{session_id}` narrows to that session's bound grants —
/// the connect grant (`session_id NULL`) and another session's rows
/// never leak into the scoped read; foreign/absent session ids resolve
/// `not_found` identically (BR3.3).
#[tokio::test]
async fn grant_list_session_id_narrows_and_denies() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let (app, gw) = test_app_with_gateway(db.clone(), stub_runtime()).await;
    let (cookie, uid) = signup_and_login(&app, "oxpw@example.com", "oxpw").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "gl").await;

    // A connected target — leaves a resolved connect grant
    // (`session_id NULL`) in the table alongside the session-bound rows.
    let target_id = redeemed_target_id(&gw, &uid, "desk-box").await;
    approve_connect_grant(&gw, &uid, &target_id).await;

    let s1 = create_session(&app, &cookie, "oxpw", "gl", "one").await;
    let sid1 = s1["data"]["id"].as_str().unwrap().to_string();
    let s2 = create_session(&app, &cookie, "oxpw", "gl", "two").await;
    let sid2 = s2["data"]["id"].as_str().unwrap().to_string();

    // Session-bound action grants minted straight into the store (the
    // dispatch path's insert shape verbatim — kind `action`,
    // action_kind the capability wire name, far-future expiry so the
    // BR3.1 lazy-expiry read leaves them pending).
    const FAR_FUTURE: &str = "2999-01-01T00:00:00Z";
    for (gid, sid, kind) in [
        ("oxpg-u8-a1", sid1.as_str(), "exec_shell"),
        ("oxpg-u8-a2", sid2.as_str(), "fs_read"),
    ] {
        db.insert_oxpilot_grant(&oxidean_db::NewOxpilotGrant {
            id: gid,
            target_id: &target_id,
            user_id: &uid,
            session_id: Some(sid),
            kind: "action",
            action_kind: Some(kind),
            action_args: Some("{}"),
            action_summary: Some("{}"),
            expires_at: FAR_FUTURE,
        })
        .await
        .expect("seed grant");
    }

    let scoped = rpc_json(
        &app,
        Some(&cookie),
        &format!(r#"{{"procedure":"oxpilot.grant_list","input":{{"session_id":"{sid1}"}}}}"#),
    )
    .await;
    assert_eq!(scoped["ok"], true, "{scoped}");
    let grants = scoped["data"]["grants"].as_array().unwrap();
    assert_eq!(grants.len(), 1, "{scoped}");
    assert_eq!(grants[0]["id"], "oxpg-u8-a1");
    assert_eq!(
        grants[0]["session_id"],
        serde_json::Value::from(sid1.as_str())
    );
    assert_eq!(grants[0]["capability"], "exec_shell");

    let other = rpc_json(
        &app,
        Some(&cookie),
        &format!(r#"{{"procedure":"oxpilot.grant_list","input":{{"session_id":"{sid2}"}}}}"#),
    )
    .await;
    let other_grants = other["data"]["grants"].as_array().unwrap();
    assert_eq!(other_grants.len(), 1, "{other}");
    assert_eq!(other_grants[0]["id"], "oxpg-u8-a2");

    // The unscoped owner read still carries everything — the allowed
    // connect grant + both session-bound rows — so the narrowing above
    // is the filter, not the table contents.
    let all = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.grant_list","input":{}}"#,
    )
    .await;
    let all_grants = all["data"]["grants"].as_array().unwrap();
    let all_ids: Vec<&str> = all_grants.iter().filter_map(|g| g["id"].as_str()).collect();
    assert_eq!(all_ids.len(), 3, "{all}");
    assert!(all_ids.contains(&"oxpg-u8-a1"), "{all}");
    assert!(all_ids.contains(&"oxpg-u8-a2"), "{all}");
    // … and the third is the connect grant, carrying session_id NULL.
    assert!(
        all_grants
            .iter()
            .any(|g| g["kind"] == "connect" && g["session_id"].is_null()),
        "{all}"
    );

    // Foreign + absent session ids fold into the same `not_found`
    // (BR3.3) — a scoped read can never enumerate another user's
    // sessions or their grants.
    let absent = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"oxpilot.grant_list","input":{"session_id":"s-absent"}}"#,
    )
    .await;
    assert_eq!(absent["ok"], false, "{absent}");
    assert_eq!(absent["error"]["code"], "not_found", "{absent}");

    let (cookie2, uid2) = signup_and_login(&app, "oxpx@example.com", "oxpx").await;
    verify_user(&db, &uid2).await;
    create_repo(&app, &cookie2, "gl2").await;
    let foreign_session = create_session(&app, &cookie2, "oxpx", "gl2", "f").await;
    let foreign_sid = foreign_session["data"]["id"].as_str().unwrap();
    let foreign = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.grant_list","input":{{"session_id":"{foreign_sid}"}}}}"#
        ),
    )
    .await;
    assert_eq!(foreign["ok"], false, "{foreign}");
    assert_eq!(foreign["error"]["code"], "not_found", "{foreign}");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}

/// `queue_delete{session_id, queued_id}` — owned rows answer
/// `{deleted:true}` and leave `queue_list` drained; absent ids and a
/// foreign session resolve `not_found` without touching the row
/// (BR3.3/BR9.1). The `require_verified`/`require_enabled` gates ride
/// U2_PROCS above.
#[tokio::test]
async fn queue_delete_drains_and_denies_foreign_or_absent() {
    let _g = env_lock().await;
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "1");
    let db = support::connect_test_db("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let runtime = stub_runtime();
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let (cookie, uid) = signup_and_login(&app, "oxpy2@example.com", "oxpy2").await;
    verify_user(&db, &uid).await;
    create_repo(&app, &cookie, "qd").await;
    let session = create_session(&app, &cookie, "oxpy2", "qd", "g").await;
    let sid = session["data"]["id"].as_str().unwrap().to_string();

    // The queue exists only under a live turn — claim it, then enqueue.
    claim_turn(&runtime, &sid).await;
    let queue = |content: &str, cmid: &str| {
        format!(
            r#"{{"procedure":"oxpilot.message_queue","input":{{"session_id":"{sid}","content":"{content}","client_msg_id":"{cmid}"}}}}"#
        )
    };
    let q1 = rpc_json(&app, Some(&cookie), &queue("first", "cm-qd1")).await;
    assert_eq!(q1["ok"], true, "{q1}");
    let qid1 = q1["data"]["queued_id"].as_str().unwrap().to_string();
    let q2 = rpc_json(&app, Some(&cookie), &queue("second", "cm-qd2")).await;
    assert_eq!(q2["ok"], true, "{q2}");
    let qid2 = q2["data"]["queued_id"].as_str().unwrap().to_string();

    // Absent queued_id under an owned session ⇒ not_found (stale-ref
    // honesty, BR9.1); absent session ⇒ not_found before any row read.
    let missing = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.queue_delete","input":{{"session_id":"{sid}","queued_id":"q-absent"}}}}"#
        ),
    )
    .await;
    assert_eq!(missing["ok"], false, "{missing}");
    assert_eq!(missing["error"]["code"], "not_found", "{missing}");
    let no_session = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.queue_delete","input":{{"session_id":"s-absent","queued_id":"{qid1}"}}}}"#
        ),
    )
    .await;
    assert_eq!(no_session["ok"], false, "{no_session}");
    assert_eq!(no_session["error"]["code"], "not_found", "{no_session}");

    // A foreign session id folds into the same not_found — the owner's
    // queued row is untouched.
    let (cookie2, uid2) = signup_and_login(&app, "oxpz2@example.com", "oxpz2").await;
    verify_user(&db, &uid2).await;
    let foreign = rpc_json(
        &app,
        Some(&cookie2),
        &format!(
            r#"{{"procedure":"oxpilot.queue_delete","input":{{"session_id":"{sid}","queued_id":"{qid1}"}}}}"#
        ),
    )
    .await;
    assert_eq!(foreign["ok"], false, "{foreign}");
    assert_eq!(foreign["error"]["code"], "not_found", "{foreign}");

    // Owned delete → {deleted:true}; the drained row leaves queue_list.
    let deleted = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.queue_delete","input":{{"session_id":"{sid}","queued_id":"{qid1}"}}}}"#
        ),
    )
    .await;
    assert_eq!(deleted["ok"], true, "{deleted}");
    assert_eq!(deleted["data"]["deleted"], true, "{deleted}");

    let list = rpc_json(
        &app,
        Some(&cookie),
        &format!(r#"{{"procedure":"oxpilot.queue_list","input":{{"session_id":"{sid}"}}}}"#),
    )
    .await;
    let queued = list["data"]["queued"].as_array().unwrap();
    assert_eq!(queued.len(), 1, "{list}");
    assert_eq!(queued[0]["id"], qid2.as_str(), "{list}");

    // The already-drained id is a stale ref now — not_found again.
    let stale = rpc_json(
        &app,
        Some(&cookie),
        &format!(
            r#"{{"procedure":"oxpilot.queue_delete","input":{{"session_id":"{sid}","queued_id":"{qid1}"}}}}"#
        ),
    )
    .await;
    assert_eq!(stale["ok"], false, "{stale}");
    assert_eq!(stale["error"]["code"], "not_found", "{stale}");
    std::env::remove_var("OXIDEAN_OXPILOT_ENABLED");
}
