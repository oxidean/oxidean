//! U6 connector-gateway integration — the real assembled router over a
//! fresh SQLite schema. Covers the C3 auth matrix (absent/unknown/
//! tombstoned verifiers, revoked exceptions, the failed-auth 429, the
//! `{error:"disabled"}` fail-closed arm, body target-mismatch), the
//! pair → consent → claim → report → heartbeat happy path, claim
//! park/wake semantics (204 + Retry-After, armed-Notify wake), the
//! consent/`Denied` shape, disconnect/revoke/session-delete close
//! paths, and audit emission (events present, secrets absent).
//!
//! Two surfaces are exercised: the `/api/oxpilot/connector/*` HTTP
//! routes (verifier Bearer, no session) and the `oxpilot.*` owner
//! procs (`/api/rpc`, session cookie). Dispatch flows call the
//! installed `ConnectorGateway` trait object directly — the same port
//! U5 consumes — so the seam stays the single state machine.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::auth::{hash_password_str, SessionService as CookieSessions};
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_api_core::oxpilot::{
    ActiveTurns, BudgetCaps, ClaimOutcome, CreateSession, LlmProvider, OxpilotRuntime,
    OxpilotRuntimeSlot, SessionService, SharedOxpilot, StubProvider,
};
use oxidean_api_core::rpc::ClientMeta;
use oxidean_api_oxpilot::connector::{ConnectorDeleteHook, OxpilotConnectorGateway};
use oxidean_api_oxpilot::runtime::OxpilotTurnAbortHook;
use oxidean_api_oxpilot::services::OxpilotSessionService;
use oxidean_db::{Database, NewOxpilotPairing};
use serde_json::{json, Value};
use tokio::sync::MutexGuard;
use tower::ServiceExt;
use uuid::Uuid;

/// Serialize the file — `OXIDEAN_OXPILOT_*` env is process-global.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().await
}

const OXPILOT_ENVS: &[&str] = &[
    "OXIDEAN_OXPILOT_ENABLED",
    "OXIDEAN_OXPILOT_PROVIDER",
    "OXIDEAN_OXPILOT_MODEL",
    "OXIDEAN_OXPILOT_BASE_URL",
    "OXIDEAN_OXPILOT_API_KEY",
];

fn clear_oxpilot_env() {
    for k in OXPILOT_ENVS {
        std::env::remove_var(k);
    }
}

async fn test_db() -> Database {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    db
}

/// A runtime whose enablement resolves ON — plus the full U6 wiring
/// (sessions seam + ordered delete hooks + gateway seam), mirroring
/// `main.rs` shell assembly. Returns the service Arc too so tests can
/// drive `delete_session` through the real C8 path.
fn enabled_runtime(db: &Database) -> (SharedOxpilot, Arc<OxpilotSessionService>) {
    let provider: Arc<dyn LlmProvider> = Arc::new(StubProvider { fail: false });
    let runtime = OxpilotRuntime::from_slot(OxpilotRuntimeSlot {
        provider: Some(provider),
        model: Some("stub-1".to_string()),
        disabled_reason: None,
        turns: Arc::new(ActiveTurns::default()),
        built_from: None,
        budget_caps: BudgetCaps::default(),
        tools: None,
    });
    let sessions = Arc::new(OxpilotSessionService::new(db.clone()));
    sessions.register_delete_hook(Arc::new(OxpilotTurnAbortHook::new(runtime.clone())));
    let gateway = Arc::new(OxpilotConnectorGateway::new(db.clone(), sessions.clone()));
    sessions.register_delete_hook(Arc::new(ConnectorDeleteHook::new(gateway.clone())));
    runtime.install_sessions(sessions.clone());
    runtime.install_gateway(gateway);
    (runtime, sessions)
}

/// A `flag_off` slot — enablement resolution fails closed.
fn disabled_runtime() -> SharedOxpilot {
    OxpilotRuntime::from_slot(OxpilotRuntimeSlot {
        provider: None,
        model: None,
        disabled_reason: Some("flag_off".to_string()),
        turns: Arc::new(ActiveTurns::default()),
        built_from: None,
        budget_caps: BudgetCaps::default(),
        tools: None,
    })
}

async fn test_app(db: Database, oxpilot: Option<SharedOxpilot>) -> axum::Router {
    let mut state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
    if let Some(r) = oxpilot {
        state = state.with_oxpilot(r);
    }
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

async fn seed_user(db: &Database, tag: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let password_hash = hash_password_str("user-pass-1").unwrap();
    db.create_user(
        &id,
        &format!("{tag}-{id}@example.com"),
        &format!("{tag}-{}", &id[..8]),
        Some(&password_hash),
        tag,
        "",
        None,
        oxidean_core::Role::User,
    )
    .await
    .expect("create user");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&id, &now).await.expect("verify");
    id
}

async fn session_cookie(db: &Database, user_id: &str) -> String {
    let sessions = CookieSessions::new("development");
    let (raw, _cookie) = sessions
        .create(db, user_id, false, None, None)
        .await
        .expect("create session");
    format!("oxidean_session={raw}")
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

async fn rpc_json(app: &axum::Router, cookie: Option<&str>, body: &str) -> Value {
    let res = app.clone().oneshot(rpc_req(body, cookie)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// The C3 connector edge — verifier Bearer optional, no cookies.
async fn conn_post(
    app: &axum::Router,
    path: &str,
    bearer: Option<&str>,
    body: Value,
) -> (StatusCode, HeaderMap, Value) {
    let mut b = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, v)
}

async fn owner_pairing(app: &axum::Router, cookie: &str, name: &str, caps: &[&str]) -> Value {
    rpc_json(
        app,
        Some(cookie),
        &json!({
            "procedure": "oxpilot.pairing_create",
            "input": { "target_name": name, "caps": caps },
        })
        .to_string(),
    )
    .await
}

async fn owner_grant_list(app: &axum::Router, cookie: &str) -> Vec<Value> {
    let v = rpc_json(
        app,
        Some(cookie),
        r#"{"procedure":"oxpilot.grant_list","input":{}}"#,
    )
    .await;
    assert_eq!(v["ok"], true, "grant_list — {v}");
    v["data"]["grants"].as_array().unwrap().clone()
}

async fn owner_respond(
    app: &axum::Router,
    cookie: &str,
    grant_id: &str,
    allow: bool,
    scope: Option<&str>,
) -> Value {
    let mut input = json!({ "grant_id": grant_id, "allow": allow });
    if let Some(s) = scope {
        input["scope"] = json!(s);
    }
    rpc_json(
        app,
        Some(cookie),
        &json!({ "procedure": "oxpilot.grant_respond", "input": input }).to_string(),
    )
    .await
}

/// Mint → pair → consent-connect. Returns `(target_id, verifier)`.
async fn paired_connected_target(
    app: &axum::Router,
    cookie: &str,
    caps: &[&str],
) -> (String, String) {
    let v = owner_pairing(app, cookie, "desk", caps).await;
    assert_eq!(v["ok"], true, "pairing_create — {v}");
    let code = v["data"]["pairing_code"].as_str().unwrap().to_string();
    let caps_json: Vec<String> = caps.iter().map(|c| c.to_string()).collect();
    let (st, _h, v) = conn_post(
        app,
        "/api/oxpilot/connector/pair",
        None,
        json!({
            "pairing_code": code,
            "device_label": "desk-box",
            "requested_caps": caps_json,
        }),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "pair — {v}");
    let target_id = v["target_id"].as_str().unwrap().to_string();
    let verifier = v["verifier"].as_str().unwrap().to_string();
    let grants = owner_grant_list(app, cookie).await;
    let cg = grants
        .iter()
        .find(|g| g["kind"] == "connect" && g["state"] == "pending")
        .expect("pending connect grant");
    let v = owner_respond(app, cookie, cg["id"].as_str().unwrap(), true, None).await;
    assert_eq!(v["ok"], true, "connect respond — {v}");
    assert_eq!(v["data"]["state"], "allowed");
    (target_id, verifier)
}

async fn make_session(
    svc: &OxpilotSessionService,
    db: &Database,
    user_id: &str,
    id: &str,
) -> String {
    // `oxpilot_sessions.repo_id` is an FK — the row needs a real
    // repository parent.
    let repo = format!("repo-{id}");
    db.insert_repository(&repo, user_id, "user", &repo, "private", "", "main")
        .await
        .expect("seed repo");
    svc.create_session(
        user_id,
        CreateSession {
            id: id.to_string(),
            repo_owner: "o".to_string(),
            repo_name: repo.clone(),
            repo_id: repo,
            branch: "main".to_string(),
            goal: None,
            title: "t".to_string(),
        },
    )
    .await
    .expect("create oxpilot session")
    .id
}

fn denied_reason(e: &oxidean_core::AppError) -> (String, Option<String>) {
    let d = e.data.as_ref().expect("denied error carries data")["denied"].clone();
    (
        d["reason"].as_str().unwrap().to_string(),
        d["grant_id"].as_str().map(|s| s.to_string()),
    )
}

// ---------- C3 auth matrix ----------

#[tokio::test]
async fn connector_auth_matrix() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;

    // Absent Bearer → 401 unauthorized (same miss path).
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/heartbeat",
        None,
        json!({ "target_id": target_id }),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["error"], "unauthorized");

    // Unknown verifier → 401.
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/heartbeat",
        Some("0000000000000000000000000000000000000000000000000000000000000000"),
        json!({ "target_id": target_id }),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "{v}");
    assert_eq!(v["error"], "unauthorized");

    // Happy 200 — liveness touch + empty cancel set.
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/heartbeat",
        Some(&verifier),
        json!({ "target_id": target_id }),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["revoked"], false);
    assert_eq!(v["cancel"], json!([]));

    // Body target-mismatch → 403 forbidden (identity binds, never
    // trusts, the body's target_id).
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/heartbeat",
        Some(&verifier),
        json!({ "target_id": "not-the-target" }),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["error"], "forbidden");

    // Tombstoned verifier — disconnect then every route 401s.
    let v = rpc_json(
        &app,
        Some(&cookie),
        &json!({ "procedure": "oxpilot.target_disconnect", "input": { "target_id": target_id } })
            .to_string(),
    )
    .await;
    assert_eq!(v["ok"], true, "disconnect — {v}");
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/heartbeat",
        Some(&verifier),
        json!({ "target_id": target_id }),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "{v}");

    // Malformed body → bad_input.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/oxpilot/connector/pair")
                .header("content-type", "application/json")
                .body(Body::from("{not json".to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn connector_disabled_fail_closed() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let _user = seed_user(&db, "owner").await;
    let app = test_app(db.clone(), Some(disabled_runtime())).await;
    for path in ["pair", "claim", "report", "heartbeat"] {
        let (st, _h, v) = conn_post(
            &app,
            &format!("/api/oxpilot/connector/{path}"),
            None,
            json!({}),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{path} — {v}");
        assert_eq!(v["error"], "disabled", "{path} — {v}");
    }
}

#[tokio::test]
async fn connector_auth_limiter_trips() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime)).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (_t, _v) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;

    // 20 failed verifier lookups trip the IP bucket (NFR1.3).
    for i in 0..20 {
        let (st, _h, v) = conn_post(
            &app,
            "/api/oxpilot/connector/heartbeat",
            Some(&format!("{i:064x}")),
            json!({ "target_id": "t" }),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED, "miss {i} — {v}");
    }
    let (st, h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/heartbeat",
        Some(&format!("{:064x}", 99)),
        json!({ "target_id": "t" }),
    )
    .await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS, "{v}");
    assert_eq!(v["error"], "rate_limited");
    assert!(h.get("retry-after").is_some(), "429 carries Retry-After");
}

#[tokio::test]
async fn pairing_mint_limiter_trips() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime)).await;
    let cookie = session_cookie(&db, &user_id).await;

    for _ in 0..10 {
        let v = owner_pairing(&app, &cookie, "desk", &["exec_shell"]).await;
        assert_eq!(v["ok"], true, "mint — {v}");
    }
    let v = owner_pairing(&app, &cookie, "desk", &["exec_shell"]).await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "auth.rate_limited", "{v}");
}

// ---------- pair + consent flows ----------

#[tokio::test]
async fn pair_single_use_expired_and_caps_bound() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime)).await;
    let cookie = session_cookie(&db, &user_id).await;

    // Mint validation — bad cap name and empty set both refuse; the
    // gateway's `bad_input` code passes through the proc envelope.
    let v = owner_pairing(&app, &cookie, "x", &["gui_click"]).await;
    assert_eq!(v["error"]["code"], "bad_input", "{v}");
    let v = owner_pairing(&app, &cookie, "x", &[]).await;
    assert_eq!(v["error"]["code"], "bad_input", "{v}");

    // Mint + redeem.
    let v = owner_pairing(&app, &cookie, "desk", &["exec_shell", "fs_read"]).await;
    assert_eq!(v["ok"], true, "{v}");
    let code = v["data"]["pairing_code"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 64, "256-bit code as hex");

    // Requested caps outside the minted set → 403 forbidden.
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/pair",
        None,
        json!({
            "pairing_code": code,
            "device_label": "box",
            "requested_caps": ["fs_write"],
        }),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["error"], "forbidden");

    // The same forbidden attempt did NOT consume the code — redeem
    // within the minted set still succeeds.
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/pair",
        None,
        json!({
            "pairing_code": code,
            "device_label": "box",
            "requested_caps": ["exec_shell"],
        }),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let verifier = v["verifier"].as_str().unwrap();
    assert_eq!(verifier.len(), 64, "256-bit verifier as hex");
    assert_eq!(v["connector_id"], v["target_id"], "v1 ids coincide");

    // Single-use — the second redeem reads `pairing_used`.
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/pair",
        None,
        json!({
            "pairing_code": code,
            "device_label": "box2",
            "requested_caps": ["exec_shell"],
        }),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    assert_eq!(v["error"], "pairing_used");

    // Unknown-but-wellformed folds into `used` (no existence oracle).
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/pair",
        None,
        json!({
            "pairing_code": "f".repeat(64),
            "device_label": "box",
            "requested_caps": ["exec_shell"],
        }),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    assert_eq!(v["error"], "pairing_used");

    // Expired code → 410 `pairing_expired` — seeded directly so the
    // row's `expires_at` is already past; the redeem lazily stamps it.
    let expired = Uuid::new_v4().to_string();
    let code_hash = oxidean_api_core::auth::session::sha256_hex("e".repeat(64).as_bytes());
    let past = (chrono::Utc::now() - chrono::Duration::seconds(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.insert_oxpilot_pairing(&NewOxpilotPairing {
        id: &expired,
        user_id: &user_id,
        code_hash: &code_hash,
        target_name: "old",
        requested_caps: "[\"exec_shell\"]",
        expires_at: &past,
    })
    .await
    .unwrap();
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/pair",
        None,
        json!({
            "pairing_code": "e".repeat(64),
            "device_label": "box",
            "requested_caps": ["exec_shell"],
        }),
    )
    .await;
    assert_eq!(st, StatusCode::GONE, "{v}");
    assert_eq!(v["error"], "pairing_expired");
    // Lazy stamp landed.
    let row = db
        .find_oxpilot_pairing_by_code_hash(&code_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.state, "expired");
}

#[tokio::test]
async fn anti_enumeration_owner_reads() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let owner = seed_user(&db, "owner").await;
    let other = seed_user(&db, "other").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime)).await;
    let cookie = session_cookie(&db, &owner).await;
    let other_cookie = session_cookie(&db, &other).await;
    let (target_id, _v) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let grants = owner_grant_list(&app, &cookie).await;
    let grant_id = grants[0]["id"].as_str().unwrap().to_string();

    // Cross-user ids resolve `not_found`, never `forbidden` (BR7.1).
    for (proc, id) in [
        ("oxpilot.target_get", target_id.clone()),
        ("oxpilot.target_disconnect", target_id.clone()),
        ("oxpilot.target_revoke", target_id.clone()),
    ] {
        let v = rpc_json(
            &app,
            Some(&other_cookie),
            &json!({ "procedure": proc, "input": { "target_id": id } }).to_string(),
        )
        .await;
        assert_eq!(v["error"]["code"], "not_found", "{proc} — {v}");
    }
    for proc in ["oxpilot.grant_get", "oxpilot.grant_respond"] {
        let mut input = json!({ "grant_id": grant_id });
        if proc == "oxpilot.grant_respond" {
            input["allow"] = json!(true);
        }
        let v = rpc_json(
            &app,
            Some(&other_cookie),
            &json!({ "procedure": proc, "input": input }).to_string(),
        )
        .await;
        assert_eq!(v["error"]["code"], "not_found", "{proc} — {v}");
    }
    // The owner's own grant_list scoped to a foreign-looking filter
    // answers the owned view — never leaks.
    let v = rpc_json(
        &app,
        Some(&other_cookie),
        &json!({ "procedure": "oxpilot.grant_list", "input": { "target_id": target_id } })
            .to_string(),
    )
    .await;
    assert_eq!(v["data"]["grants"], json!([]), "{v}");
}

// ---------- dispatch + consent ----------

#[tokio::test]
async fn dispatch_consent_claim_report_heartbeat() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let gw = runtime.gateway().unwrap();
    let meta = ClientMeta::default();
    let session_id =
        make_session(&OxpilotSessionService::new(db.clone()), &db, &user_id, "s1").await;

    // Liveness: heartbeat marks the target inside the window BEFORE
    // dispatch's liveness check can pass.
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    gw.heartbeat(&meta, &id).await.unwrap();

    // (1) no live grant → denied{consent_required, grant_id}.
    let err = gw
        .dispatch_action(
            &meta,
            &user_id,
            &session_id,
            &target_id,
            "exec_shell",
            "echo hi ghp_0123456789abcdef",
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "oxpilot.action_denied");
    let (reason, grant_id) = denied_reason(&err);
    assert_eq!(reason, "consent_required");
    let grant_id = grant_id.unwrap();

    // (2) identical args joins the SAME pending card (BR4.5 dedupe).
    let err = gw
        .dispatch_action(
            &meta,
            &user_id,
            &session_id,
            &target_id,
            "exec_shell",
            "echo hi ghp_0123456789abcdef",
        )
        .await
        .unwrap_err();
    let (_, grant_id2) = denied_reason(&err);
    assert_eq!(grant_id2.as_deref(), Some(grant_id.as_str()));

    // The consent card shows the bounded REDACTED summary, never args.
    let v = rpc_json(
        &app,
        Some(&cookie),
        &json!({ "procedure": "oxpilot.grant_get", "input": { "grant_id": grant_id } }).to_string(),
    )
    .await;
    assert_eq!(v["data"]["kind"], "action");
    assert_eq!(v["data"]["capability"], "exec_shell");
    let summary = v["data"]["args_summary"].as_str().unwrap();
    assert!(summary.contains("echo hi"), "{summary}");
    assert!(
        !summary.contains("ghp_"),
        "args summary redacted — {summary}"
    );

    // Respond allow (default once) → action materializes `queued`.
    let v = owner_respond(&app, &cookie, &grant_id, true, None).await;
    assert_eq!(v["data"]["state"], "allowed", "{v}");

    // Resolved grant is dead — re-respond refuses `grant_not_pending`.
    let v = owner_respond(&app, &cookie, &grant_id, true, None).await;
    assert_eq!(v["error"]["code"], "grant_not_pending", "{v}");

    // Claim delivers the action with the stored (redacted) args.
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    let out = gw
        .claim(&meta, &id, 0, &["exec_shell".to_string()])
        .await
        .unwrap();
    let (action_id, args) = match out {
        ClaimOutcome::Batch {
            actions, cancel, ..
        } => {
            assert_eq!(actions.len(), 1, "{actions:?}");
            assert!(cancel.is_empty());
            (actions[0].action_id.clone(), actions[0].args.clone())
        }
        ClaimOutcome::Timeout { .. } => panic!("expected a batch"),
    };
    assert!(!args.contains("ghp_"), "delivered args redacted — {args}");
    let row = db.find_oxpilot_action(&action_id).await.unwrap().unwrap();
    assert_eq!(row.state, "claimed");
    assert_eq!(row.grant_id, grant_id);

    // The NEXT authenticated touch proves prior delivery ran — flip.
    let out = gw
        .claim(&meta, &id, 0, &["exec_shell".to_string()])
        .await
        .unwrap();
    assert!(matches!(out, ClaimOutcome::Timeout { .. }), "{out:?}");
    let row = db.find_oxpilot_action(&action_id).await.unwrap().unwrap();
    assert_eq!(row.state, "running");

    // Report done — output is redacted + tail-bounded server-side.
    gw.report(
        &meta,
        &id,
        &action_id,
        "done",
        Some(0),
        "out sk_live_0123456789abcdef done",
    )
    .await
    .unwrap();
    let row = db.find_oxpilot_action(&action_id).await.unwrap().unwrap();
    assert_eq!(row.state, "done");
    let rs = row.result_summary.unwrap();
    assert!(!rs.contains("sk_live_"), "result redacted — {rs}");

    // Idempotent re-report on a terminal row → the same ack.
    gw.report(&meta, &id, &action_id, "done", Some(0), "again")
        .await
        .unwrap();

    // Heartbeat — live, nothing to cancel.
    let hb = gw.heartbeat(&meta, &id).await.unwrap();
    assert!(!hb.revoked);
    assert!(hb.cancel.is_empty());

    // action_status — the owner's view of the terminal row.
    let view = gw.action_status(&meta, &user_id, &action_id).await.unwrap();
    assert_eq!(view.state, "done");
    assert!(view.result_summary.is_some());

    // A fresh dispatch needs consent again — `once` consumed the grant.
    let err = gw
        .dispatch_action(
            &meta,
            &user_id,
            &session_id,
            &target_id,
            "exec_shell",
            "echo two",
        )
        .await
        .unwrap_err();
    let (reason, grant_id3) = denied_reason(&err);
    assert_eq!(reason, "consent_required");
    assert_ne!(grant_id3.as_deref(), Some(grant_id.as_str()));
}

#[tokio::test]
async fn dispatch_denial_reasons_and_target_checks() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let other_id = seed_user(&db, "other").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let gw = runtime.gateway().unwrap();
    let meta = ClientMeta::default();
    let session_id =
        make_session(&OxpilotSessionService::new(db.clone()), &db, &user_id, "s1").await;

    // Unknown session → `not_found` passthrough (not denied-shaped).
    let err = gw
        .dispatch_action(&meta, &user_id, "no-session", &target_id, "exec_shell", "x")
        .await
        .unwrap_err();
    assert_eq!(err.code, "not_found");

    // Cross-user session → `not_found` passthrough.
    let err = gw
        .dispatch_action(&meta, &other_id, &session_id, &target_id, "exec_shell", "x")
        .await
        .unwrap_err();
    assert_eq!(err.code, "not_found");

    // Unknown target → denied{not_connected} (no existence leak).
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, "nope", "exec_shell", "x")
        .await
        .unwrap_err();
    assert_eq!(denied_reason(&err).0, "not_connected");

    // A target owned by another user → the same `not_connected` fold
    // (the session is the caller's own, so the session check passes
    // and the target's `user_id` mismatch is what folds).
    let other_cookie = session_cookie(&db, &other_id).await;
    let (foreign_target, _fv) = paired_connected_target(&app, &other_cookie, &["exec_shell"]).await;
    let err = gw
        .dispatch_action(
            &meta,
            &user_id,
            &session_id,
            &foreign_target,
            "exec_shell",
            "x",
        )
        .await
        .unwrap_err();
    assert_eq!(denied_reason(&err).0, "not_connected");

    // Connected but never-seen → `offline` (last_seen_at NULL).
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "x")
        .await
        .unwrap_err();
    assert_eq!(denied_reason(&err).0, "offline");

    // Heartbeat liveness → capability bound next.
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    gw.heartbeat(&meta, &id).await.unwrap();
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "fs_write", "x")
        .await
        .unwrap_err();
    assert_eq!(denied_reason(&err).0, "capability");
}

#[tokio::test]
async fn claim_parks_then_wakes_on_enqueue() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let gw = runtime.gateway().unwrap();
    let meta = ClientMeta::default();
    let session_id =
        make_session(&OxpilotSessionService::new(db.clone()), &db, &user_id, "s1").await;
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    gw.heartbeat(&meta, &id).await.unwrap();

    // wait_seconds=0 → immediate Timeout (HTTP maps it to 204).
    let out = gw
        .claim(&meta, &id, 0, &["exec_shell".to_string()])
        .await
        .unwrap();
    assert!(matches!(out, ClaimOutcome::Timeout { .. }));

    // Park a claim (wait 30) — dispatch+allow wakes it via Notify.
    let id2 = id.clone();
    let gw2 = gw.clone();
    let meta2 = meta.clone();
    let parked = tokio::spawn(async move {
        gw2.claim(&meta2, &id2, 30, &["exec_shell".to_string()])
            .await
    });
    tokio::time::sleep(Duration::from_millis(120)).await;
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "ls")
        .await
        .unwrap_err();
    let (_, grant_id) = denied_reason(&err);
    let v = owner_respond(&app, &cookie, &grant_id.unwrap(), true, None).await;
    assert_eq!(v["data"]["state"], "allowed");
    let out = parked.await.unwrap().unwrap();
    match out {
        ClaimOutcome::Batch { actions, .. } => {
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].args, "ls");
        }
        ClaimOutcome::Timeout { .. } => panic!("parked claim must wake on enqueue"),
    }
}

#[tokio::test]
async fn claim_http_204_and_retry_after() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime)).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;

    let (st, h, _v) = conn_post(
        &app,
        "/api/oxpilot/connector/claim",
        Some(&verifier),
        json!({ "target_id": target_id, "wait_seconds": 0, "capabilities": ["exec_shell"] }),
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert!(h.get("retry-after").is_some());
}

#[tokio::test]
async fn session_scope_grants_and_revoke_arm() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let gw = runtime.gateway().unwrap();
    let meta = ClientMeta::default();
    let session_id =
        make_session(&OxpilotSessionService::new(db.clone()), &db, &user_id, "s1").await;
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    gw.heartbeat(&meta, &id).await.unwrap();

    // Allow with scope=session → the live action_session grant covers
    // subsequent dispatches without further consent.
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "a")
        .await
        .unwrap_err();
    let (_, g1) = denied_reason(&err);
    let v = owner_respond(&app, &cookie, &g1.unwrap(), true, Some("session")).await;
    assert_eq!(v["data"]["state"], "allowed");

    let a2 = gw
        .dispatch_action(
            &meta,
            &user_id,
            &session_id,
            &target_id,
            "exec_shell",
            "two",
        )
        .await
        .unwrap();
    assert_eq!(a2.state, "queued");
    let a3 = gw
        .dispatch_action(
            &meta,
            &user_id,
            &session_id,
            &target_id,
            "exec_shell",
            "three",
        )
        .await
        .unwrap();

    // The FIRST consent's action also queued under the session grant —
    // materialized at allow time.
    let out = gw
        .claim(&meta, &id, 0, &["exec_shell".to_string()])
        .await
        .unwrap();
    match out {
        ClaimOutcome::Batch { actions, .. } => {
            assert_eq!(actions.len(), 3, "{actions:?}");
        }
        ClaimOutcome::Timeout { .. } => panic!("expected a batch"),
    }

    // BR6.4 — `deny` on the ALLOWED action_session grant is the revoke
    // arm: claimed rows move to `cancel_requested`, queued to
    // `cancelled`.
    let grant_row = db.find_oxpilot_grant(&a2.grant_id).await.unwrap().unwrap();
    assert_eq!(grant_row.kind, "action_session");
    let v = owner_respond(&app, &cookie, &a2.grant_id, false, None).await;
    assert_eq!(v["data"]["state"], "revoked", "{v}");

    // The claimed rows ride the cancel set (heartbeat re-announces).
    let hb = gw.heartbeat(&meta, &id).await.unwrap();
    assert!(!hb.revoked);
    assert!(hb.cancel.contains(&a3.id), "cancel set — {:?}", hb.cancel);

    // A report on a cancel-requested row closes it `cancelled`.
    gw.report(&meta, &id, &a3.id, "done", Some(0), "partial")
        .await
        .unwrap();
    let row = db.find_oxpilot_action(&a3.id).await.unwrap().unwrap();
    assert_eq!(row.state, "cancelled");
    let hb = gw.heartbeat(&meta, &id).await.unwrap();
    assert!(!hb.cancel.contains(&a3.id));

    // Fresh dispatch needs consent again.
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "z")
        .await
        .unwrap_err();
    assert_eq!(denied_reason(&err).0, "consent_required");
}

// ---------- lifecycle ----------

#[tokio::test]
async fn disconnect_tombstones_and_cancels() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let gw = runtime.gateway().unwrap();
    let meta = ClientMeta::default();
    let session_id =
        make_session(&OxpilotSessionService::new(db.clone()), &db, &user_id, "s1").await;
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    gw.heartbeat(&meta, &id).await.unwrap();

    // Queue an action first so disconnect's bulk-cancel has work.
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "x")
        .await
        .unwrap_err();
    let (_, gid) = denied_reason(&err);
    let v = owner_respond(&app, &cookie, &gid.unwrap(), true, Some("session")).await;
    assert_eq!(v["data"]["state"], "allowed");

    gw.disconnect_target(&meta, &user_id, &target_id)
        .await
        .unwrap();

    // Tombstoned verifier authenticates NOTHING.
    let err = gw.resolve_verifier(&meta, &verifier).await.unwrap_err();
    assert_eq!(err.code, "unauthorized");

    // Grants + queued action landed `cancelled` directly.
    let grants = db
        .list_oxpilot_grants_for_user(&user_id, Some(&target_id))
        .await
        .unwrap();
    assert!(grants.iter().all(|g| g.state == "cancelled"), "{grants:?}");
    let actions = db
        .queued_oxpilot_action_candidates(&target_id, 50)
        .await
        .unwrap();
    assert!(actions.is_empty());

    // Dispatch on a dead target → denied{not_connected}.
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "y")
        .await
        .unwrap_err();
    assert_eq!(denied_reason(&err).0, "not_connected");

    // Idempotent on terminal targets (BR6.7).
    gw.disconnect_target(&meta, &user_id, &target_id)
        .await
        .unwrap();
    // Target view: lifecycle disconnected, status offline.
    let v = rpc_json(
        &app,
        Some(&cookie),
        &json!({ "procedure": "oxpilot.target_get", "input": { "target_id": target_id } })
            .to_string(),
    )
    .await;
    assert_eq!(v["data"]["lifecycle"], "disconnected");
    assert_eq!(v["data"]["status"], "offline");
}

#[tokio::test]
async fn revoke_preserves_verifier_and_cancel_flow() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let gw = runtime.gateway().unwrap();
    let meta = ClientMeta::default();
    let session_id =
        make_session(&OxpilotSessionService::new(db.clone()), &db, &user_id, "s1").await;
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    gw.heartbeat(&meta, &id).await.unwrap();
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "x")
        .await
        .unwrap_err();
    let (_, gid) = denied_reason(&err);
    let v = owner_respond(&app, &cookie, &gid.unwrap(), true, None).await;
    assert_eq!(v["data"]["state"], "allowed");
    // Claim the action so revoke has a `cancel_requested` row.
    let out = gw
        .claim(&meta, &id, 0, &["exec_shell".to_string()])
        .await
        .unwrap();
    let action_id = match out {
        ClaimOutcome::Batch { actions, .. } => actions[0].action_id.clone(),
        _ => panic!("expected batch"),
    };

    gw.revoke_target(&meta, &user_id, &target_id).await.unwrap();

    // Verifier STILL resolves — revoked rides the identity.
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    assert_eq!(id.status, "revoked");

    // Heartbeat → {revoked:true} + the cancel set.
    let hb = gw.heartbeat(&meta, &id).await.unwrap();
    assert!(hb.revoked);
    assert!(hb.cancel.contains(&action_id));

    // Claim on a revoked target → {error:revoked} mapped at the route.
    let err = gw
        .claim(&meta, &id, 0, &["exec_shell".to_string()])
        .await
        .unwrap_err();
    assert_eq!(err.code, "revoked");

    // The HTTP matrix — heartbeat 200 revoked:true; claim 403 revoked.
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/heartbeat",
        Some(&verifier),
        json!({ "target_id": target_id }),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["revoked"], true);
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/claim",
        Some(&verifier),
        json!({ "target_id": target_id, "wait_seconds": 0, "capabilities": ["exec_shell"] }),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(v["error"], "revoked");

    // A cancel-requested row's report still closes — `cancelled`.
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/report",
        Some(&verifier),
        json!({ "action_id": action_id, "status": "done", "exit_code": 0, "output": "killed" }),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["ack"], true);
    let row = db.find_oxpilot_action(&action_id).await.unwrap().unwrap();
    assert_eq!(row.state, "cancelled");

    // Grants revoked; dispatch → not_connected; view → revoked.
    let grants = db
        .list_oxpilot_grants_for_user(&user_id, Some(&target_id))
        .await
        .unwrap();
    assert!(grants.iter().all(|g| g.state == "revoked"), "{grants:?}");
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "y")
        .await
        .unwrap_err();
    assert_eq!(denied_reason(&err).0, "not_connected");
    let v = rpc_json(
        &app,
        Some(&cookie),
        &json!({ "procedure": "oxpilot.target_get", "input": { "target_id": target_id } })
            .to_string(),
    )
    .await;
    assert_eq!(v["data"]["status"], "revoked");

    // Idempotent second revoke.
    gw.revoke_target(&meta, &user_id, &target_id).await.unwrap();
}

#[tokio::test]
async fn session_delete_cancels_rows_keeps_connect_grants() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let gw = runtime.gateway().unwrap();
    let meta = ClientMeta::default();
    let session_id = make_session(&svc, &db, &user_id, "s1").await;
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    gw.heartbeat(&meta, &id).await.unwrap();
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "x")
        .await
        .unwrap_err();
    let (_, gid) = denied_reason(&err);
    let grant_id = gid.unwrap();
    let v = owner_respond(&app, &cookie, &grant_id, true, Some("session")).await;
    assert_eq!(v["data"]["state"], "allowed");
    let action = gw
        .dispatch_action(&meta, &user_id, &session_id, &target_id, "exec_shell", "y")
        .await
        .unwrap();

    // Connect grant exists (session_id NULL) and must SURVIVE.
    let connect_grant = owner_grant_list(&app, &cookie)
        .await
        .into_iter()
        .find(|g| g["kind"] == "connect")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // C8 — session delete runs hook #2 BEFORE the cascade.
    svc.delete_session(&user_id, &session_id).await.unwrap();

    // Session-scoped rows cascade away entirely (the hook's cancel is
    // what the audit trail records — the rows themselves DELETE).
    assert!(db.find_oxpilot_grant(&grant_id).await.unwrap().is_none());
    assert!(db.find_oxpilot_action(&action.id).await.unwrap().is_none());
    let cg = db.find_oxpilot_grant(&connect_grant).await.unwrap();
    assert!(cg.is_some(), "connect grant survives session delete");

    // Post-commit audits prove hook #2 ran the cancel transition.
    let events = db
        .list_audit_events_for_actor(&user_id, None, 500)
        .await
        .unwrap();
    let cancels: Vec<_> = events
        .iter()
        .filter(|e| {
            e.event_type == "oxpilot.action_cancelled" || e.event_type == "oxpilot.grant_cancelled"
        })
        .collect();
    assert!(
        cancels.iter().any(|e| {
            e.detail
                .as_deref()
                .map(|d| d.contains("session_deleted"))
                .unwrap_or(false)
        }),
        "session_deleted audits — {events:?}"
    );
}

// ---------- report semantics ----------

#[tokio::test]
async fn report_validation_and_scope() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;
    let (_target_id, verifier) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let gw = runtime.gateway().unwrap();
    let meta = ClientMeta::default();
    let _id = gw.resolve_verifier(&meta, &verifier).await.unwrap();

    // Unknown action → 404 not_found (kill-local signal, BR5.2).
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/report",
        Some(&verifier),
        json!({ "action_id": "nope", "status": "done", "output": "x" }),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND, "{v}");

    // A cross-target action id also 404s — mint a second connector's
    // action through its own consent flow, then report it under the
    // FIRST verifier.
    let (target2, verifier2) = paired_connected_target(&app, &cookie, &["exec_shell"]).await;
    let session_id =
        make_session(&OxpilotSessionService::new(db.clone()), &db, &user_id, "s1").await;
    let id2 = gw.resolve_verifier(&meta, &verifier2).await.unwrap();
    gw.heartbeat(&meta, &id2).await.unwrap();
    let err = gw
        .dispatch_action(&meta, &user_id, &session_id, &target2, "exec_shell", "x")
        .await
        .unwrap_err();
    let (_, gid) = denied_reason(&err);
    let v = owner_respond(&app, &cookie, &gid.unwrap(), true, None).await;
    assert_eq!(v["data"]["state"], "allowed");
    let out = gw
        .claim(&meta, &id2, 0, &["exec_shell".to_string()])
        .await
        .unwrap();
    let foreign_action = match out {
        ClaimOutcome::Batch { actions, .. } => actions[0].action_id.clone(),
        _ => panic!("expected batch"),
    };
    let (st, _h, _v) = conn_post(
        &app,
        "/api/oxpilot/connector/report",
        Some(&verifier),
        json!({ "action_id": foreign_action, "status": "done", "output": "x" }),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // Bad status → bad_input at the gateway.
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/report",
        Some(&verifier2),
        json!({ "action_id": foreign_action, "status": "bogus", "output": "x" }),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
}

// ---------- audit spy ----------

#[tokio::test]
async fn audit_events_carry_no_secret_material() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let user_id = seed_user(&db, "owner").await;
    let (runtime, _svc) = enabled_runtime(&db);
    let app = test_app(db.clone(), Some(runtime.clone())).await;
    let cookie = session_cookie(&db, &user_id).await;

    let v = owner_pairing(&app, &cookie, "desk", &["exec_shell"]).await;
    let code = v["data"]["pairing_code"].as_str().unwrap().to_string();
    let (st, _h, v) = conn_post(
        &app,
        "/api/oxpilot/connector/pair",
        None,
        json!({
            "pairing_code": code,
            "device_label": "box",
            "requested_caps": ["exec_shell"],
        }),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let verifier = v["verifier"].as_str().unwrap().to_string();
    let target_id = v["target_id"].as_str().unwrap().to_string();
    let grants = owner_grant_list(&app, &cookie).await;
    let cg = grants
        .iter()
        .find(|g| g["kind"] == "connect" && g["state"] == "pending")
        .unwrap();
    owner_respond(&app, &cookie, cg["id"].as_str().unwrap(), true, None).await;

    let gw = runtime.gateway().unwrap();
    let meta = ClientMeta::default();
    let session_id =
        make_session(&OxpilotSessionService::new(db.clone()), &db, &user_id, "s1").await;
    let id = gw.resolve_verifier(&meta, &verifier).await.unwrap();
    gw.heartbeat(&meta, &id).await.unwrap();
    let err = gw
        .dispatch_action(
            &meta,
            &user_id,
            &session_id,
            &target_id,
            "exec_shell",
            "run oxidean_pat_0123456789abcdef",
        )
        .await
        .unwrap_err();
    let (_, gid) = denied_reason(&err);
    owner_respond(&app, &cookie, &gid.unwrap(), true, None).await;
    let out = gw
        .claim(&meta, &id, 0, &["exec_shell".to_string()])
        .await
        .unwrap();
    let action_id = match out {
        ClaimOutcome::Batch { actions, .. } => actions[0].action_id.clone(),
        _ => panic!("expected batch"),
    };
    gw.report(
        &meta,
        &id,
        &action_id,
        "done",
        Some(0),
        "ok oxidean_pat_0123456789abcdef",
    )
    .await
    .unwrap();

    let events = db
        .list_audit_events_for_actor(&user_id, None, 500)
        .await
        .unwrap();
    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    for want in [
        "oxpilot.pairing_created",
        "oxpilot.pairing_claimed",
        "oxpilot.connect_grant_allowed",
        "oxpilot.target_connected",
        "oxpilot.action_grant_pending",
        "oxpilot.action_grant_allowed",
        "oxpilot.action_dispatched",
        "oxpilot.action_result",
    ] {
        assert!(types.contains(&want), "missing audit {want} — {types:?}");
    }

    // No persisted/audited secret material anywhere in the feed —
    // not the code, not the verifier, not their hashes, not the PAT
    // that rode the args/output.
    let code_hash = oxidean_api_core::auth::session::sha256_hex(code.as_bytes());
    let verifier_hash = oxidean_api_core::auth::session::sha256_hex(verifier.as_bytes());
    for e in &events {
        let hay = format!(
            "{}{}{}{}",
            e.event_type,
            e.detail.as_deref().unwrap_or_default(),
            e.target_id.as_deref().unwrap_or_default(),
            e.target_type.as_deref().unwrap_or_default()
        );
        for secret in [
            &code,
            &verifier,
            &code_hash,
            &verifier_hash,
            &"oxidean_pat_0123456789abcdef".to_string(),
        ] {
            assert!(
                !hay.contains(secret.as_str()),
                "audit detail leaked secret material — {e:?}"
            );
        }
    }
    // The stored args/output the connector saw are the redacted form.
    let row = db.find_oxpilot_action(&action_id).await.unwrap().unwrap();
    assert!(!row.args.contains("oxidean_pat_"));
    assert!(!row
        .result_summary
        .as_deref()
        .unwrap_or_default()
        .contains("oxidean_pat_"));
}
