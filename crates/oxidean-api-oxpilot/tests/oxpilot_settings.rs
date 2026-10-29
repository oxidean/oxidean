//! U4 `admin.oxpilot.*` integration — the real router over a fresh
//! SQLite schema. Covers the session-only rejection matrix, the
//! serialized write path (resolve → validate → build → persist → swap
//! → audit), failed-write preservation (nothing persisted, old slot
//! intact), provider-less writes, flag-off→on Arc carry, custody
//! surfaces (`key_set`/`key_last4` only), and the debug-shaped `stub`
//! gate. `OXIDEAN_OXPILOT_*` env is process-global — every test holds
//! `ENV_LOCK` for its whole run.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use oxidean_api::auth::{hash_password_str, SessionService};
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_api_core::oxpilot::{
    ActiveTurns, BudgetCaps, KeySource, OxpilotRuntime, OxpilotRuntimeSlot, ProviderIdentity,
    SharedOxpilot, StubProvider,
};
use oxidean_db::Database;
use serde_json::Value;
use tokio::sync::MutexGuard;
use tower::ServiceExt;
use uuid::Uuid;

/// Serialize the whole file — OXIDEAN_OXPILOT_* env is process-global.
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
    "OXIDEAN_OXPILOT_USER_BUDGET",
    "OXIDEAN_OXPILOT_INSTANCE_BUDGET",
    "OXIDEAN_OXPILOT_SECRETS_KEY",
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

/// Fresh app; returns (router, runtime) — the runtime handle stays
/// inspectable after `router_with_state` consumes `AppState`.
async fn test_app(db: Database, oxpilot: Option<SharedOxpilot>) -> (axum::Router, SharedOxpilot) {
    let mut state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development");
    if let Some(r) = oxpilot {
        state = state.with_oxpilot(r);
    }
    let runtime = state.oxpilot.clone();
    let cors = build_cors("development", None).expect("cors");
    (router_with_state(state, cors), runtime)
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

/// First user → SysAdmin directly (mirrors `support::unlock_signup`).
async fn seed_admin(db: &Database) -> String {
    let id = Uuid::new_v4().to_string();
    let password_hash = hash_password_str("admin-pass-1").unwrap();
    db.create_user(
        &id,
        &format!("admin-{id}@example.com"),
        &format!("adm-{}", &id[..8]),
        Some(&password_hash),
        "admin",
        "",
        None,
        oxidean_core::Role::SysAdmin,
    )
    .await
    .expect("create sys-admin");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&id, &now).await.expect("verify");
    id
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

/// Mint a session cookie directly (skip the login flow — these tests
/// exercise the admin surface, not auth).
async fn session_cookie(db: &Database, user_id: &str) -> String {
    let sessions = SessionService::new("development");
    let (raw, _cookie) = sessions
        .create(db, user_id, false, None, None)
        .await
        .expect("create session");
    format!("oxidean_session={raw}")
}

/// A parked stub-provider slot — flag_off carrying a built provider
/// whose identity matches an env `OXIDEAN_OXPILOT_PROVIDER=stub`
/// resolution (model None, keyless).
fn parked_stub_runtime() -> (
    SharedOxpilot,
    Arc<dyn oxidean_api_core::oxpilot::LlmProvider>,
) {
    let provider: Arc<dyn oxidean_api_core::oxpilot::LlmProvider> =
        Arc::new(StubProvider { fail: false });
    let runtime = OxpilotRuntime::from_slot(OxpilotRuntimeSlot {
        provider: Some(provider.clone()),
        model: None,
        disabled_reason: Some("flag_off".to_string()),
        turns: Arc::new(ActiveTurns::default()),
        built_from: Some(ProviderIdentity {
            provider_id: "stub".to_string(),
            model: None,
            base_url: None,
            key_source: KeySource::None,
        }),
        budget_caps: BudgetCaps::default(),
        tools: None,
    });
    (runtime, provider)
}

async fn settings_set(app: &axum::Router, cookie: &str, input: Value) -> Value {
    rpc_json(
        app,
        Some(cookie),
        &serde_json::json!({
            "procedure": "admin.oxpilot.settings_set",
            "input": input,
        })
        .to_string(),
    )
    .await
}

async fn settings_get(app: &axum::Router, cookie: &str) -> Value {
    rpc_json(
        app,
        Some(cookie),
        r#"{"procedure":"admin.oxpilot.settings_get","input":{}}"#,
    )
    .await
}

// ---------- rejection matrix (BR1.1/BR5.1) ----------

#[tokio::test]
async fn session_only_rejection_matrix() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let user_id = seed_user(&db, "user").await;
    let (app, _rt) = test_app(db.clone(), None).await;
    let admin_cookie = session_cookie(&db, &admin_id).await;
    let user_cookie = session_cookie(&db, &user_id).await;

    for proc in [
        "admin.oxpilot.settings_get",
        "admin.oxpilot.settings_set",
        "admin.oxpilot.providers_list",
    ] {
        let body = format!(r#"{{"procedure":"{proc}","input":{{}}}}"#);
        // Anonymous → auth.unauthenticated.
        let v = rpc_json(&app, None, &body).await;
        assert_eq!(v["ok"], false, "{proc} anon — {v}");
        assert_eq!(v["error"]["code"], "auth.unauthenticated", "{proc} — {v}");
        // Non-admin session → admin.forbidden.
        let v = rpc_json(&app, Some(&user_cookie), &body).await;
        assert_eq!(v["error"]["code"], "admin.forbidden", "{proc} — {v}");
        // PAT bearer → refused pre-dispatch (SessionOnly gate).
        let pat = rpc_json(
            &app,
            Some(&admin_cookie),
            r#"{"procedure":"pat.createClassic","input":{"name":"cli","scopes":["repo"]}}"#,
        )
        .await;
        let token = pat["data"]["token"].as_str().unwrap().to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/api/rpc")
            .header("content-type", "application/json")
            .header("Oxidean-RPC-Version", "1")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.clone()))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["code"], "auth.pat_scope", "{proc} PAT — {v}");
    }
}

// ---------- read/write round-trip ----------

#[tokio::test]
async fn settings_round_trip_get_set_get() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    std::env::set_var("OXIDEAN_OXPILOT_SECRETS_KEY", "test-secret");
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (app, _rt) = test_app(db.clone(), None).await;
    let cookie = session_cookie(&db, &admin_id).await;

    // Baseline: env-only resolution, nothing overridden.
    let g0 = settings_get(&app, &cookie).await;
    assert_eq!(g0["ok"], true, "{g0}");
    assert_eq!(g0["data"]["enabled"], false); // env flag unset → off
    assert_eq!(g0["data"]["enabled_overridden"], false);
    // Unset provider resolves the sys-admin-only stub default even while
    // the flag is off — the response reports resolution, not enablement.
    assert_eq!(g0["data"]["provider_id"], "stub");
    assert_eq!(g0["data"]["disabled_reason"], "flag_off");
    assert_eq!(g0["data"]["key_set"], false);

    // Sparse write — fields not named stay untouched.
    let s = settings_set(
        &app,
        &cookie,
        serde_json::json!({
            "enabled": true,
            "provider_id": "ollama",
            "model": "llama3.1",
            "base_url": "http://10.9.8.7:11434",
            "user_budget": 5000
        }),
    )
    .await;
    assert_eq!(s["ok"], true, "{s}");
    assert_eq!(s["data"]["enabled"], true);
    assert_eq!(s["data"]["enabled_overridden"], true);
    assert_eq!(s["data"]["provider_id"], "ollama");
    assert_eq!(s["data"]["provider_id_overridden"], true);
    assert_eq!(s["data"]["model"], "llama3.1");
    assert_eq!(s["data"]["user_budget"], 5000);
    assert_eq!(s["data"]["instance_budget_overridden"], false);
    assert_eq!(s["data"]["disabled_reason"], Value::Null);

    // Explicit-null clears one override; others persist.
    let s2 = settings_set(&app, &cookie, serde_json::json!({"model": null})).await;
    assert_eq!(s2["ok"], true, "{s2}");
    assert_eq!(s2["data"]["model"], Value::Null);
    assert_eq!(s2["data"]["model_overridden"], false);
    assert_eq!(s2["data"]["provider_id"], "ollama"); // still stored
    let row = db.get_oxpilot_settings().await.unwrap().unwrap();
    assert!(row.model.is_none());
    assert_eq!(row.provider_id.as_deref(), Some("ollama"));
}

#[tokio::test]
async fn invalid_writes_persist_nothing_and_keep_old_slot() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    std::env::set_var("OXIDEAN_OXPILOT_PROVIDER", "stub");
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (rt, stub_arc) = parked_stub_runtime();
    let (app, _rt) = test_app(db.clone(), Some(rt.clone())).await;
    let cookie = session_cookie(&db, &admin_id).await;

    let cases: [(Value, &str); 4] = [
        (
            serde_json::json!({"provider_id": "gemini"}),
            "invalid_provider",
        ),
        (
            serde_json::json!({"provider_id": "openai", "model": "not-a-model"}),
            "invalid_provider",
        ),
        (
            serde_json::json!({"base_url": "ftp://x", "provider_id": "ollama", "model": "m"}),
            "invalid_provider",
        ),
        (serde_json::json!({"user_budget": 0}), "rpc.bad_input"),
    ];
    for (input, code) in cases {
        let v = settings_set(&app, &cookie, input.clone()).await;
        assert_eq!(v["ok"], false, "{input} — {v}");
        assert_eq!(v["error"]["code"], code, "{input} — {v}");
        // Nothing persisted.
        let row = db.get_oxpilot_settings().await.unwrap();
        assert!(
            row.is_none() || row.as_ref().unwrap().provider_id.is_none(),
            "{input} persisted provider_id: {row:?}"
        );
        // Old slot untouched — same parked stub Arc.
        let slot = rt.slot.read().await;
        assert!(slot.provider.is_some(), "{input} dropped provider");
        assert!(Arc::ptr_eq(slot.provider.as_ref().unwrap(), &stub_arc));
        assert_eq!(slot.disabled_reason.as_deref(), Some("flag_off"));
        drop(slot);
    }
}

#[tokio::test]
async fn build_failure_is_invalid_provider_nothing_persisted() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    // openai resolves but key material is absent everywhere → the
    // write-path build fails BEFORE persist (BR4.3). Enabled resolves
    // true — a flag-off write skips the build by design (R-01 fix).
    std::env::set_var("OXIDEAN_OXPILOT_ENABLED", "true");
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (rt, stub_arc) = parked_stub_runtime();
    let (app, _rt) = test_app(db.clone(), Some(rt.clone())).await;
    let cookie = session_cookie(&db, &admin_id).await;

    let v = settings_set(
        &app,
        &cookie,
        serde_json::json!({"provider_id": "openai", "model": "gpt-4o"}),
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "invalid_provider", "{v}");
    // The migration seeds an all-NULL singleton — "nothing persisted"
    // means every override still NULL (updated_at is server-set).
    let row = db.get_oxpilot_settings().await.unwrap().unwrap();
    assert!(
        row.provider_id.is_none()
            && row.model.is_none()
            && row.enabled_override.is_none()
            && row.key_ciphertext.is_none(),
        "failed write persisted: {row:?}"
    );
    let slot = rt.slot.read().await;
    assert!(Arc::ptr_eq(slot.provider.as_ref().unwrap(), &stub_arc));
}

#[tokio::test]
async fn providerless_writes_succeed_and_install_stub_default() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (app, rt) = test_app(db.clone(), None).await;
    let cookie = session_cookie(&db, &admin_id).await;

    // Budget-only — valid with no provider resolved (BR3.1).
    let v = settings_set(&app, &cookie, serde_json::json!({"user_budget": 1000})).await;
    assert_eq!(v["ok"], true, "{v}");
    // Flag-only ON — no provider → the sys-admin-only stub default
    // builds a live stub slot (BR2.6).
    let v = settings_set(&app, &cookie, serde_json::json!({"enabled": true})).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["disabled_reason"], serde_json::Value::Null);
    assert_eq!(v["data"]["provider_id"], "stub");
    {
        let slot = rt.slot.read().await;
        assert_eq!(
            slot.provider.as_ref().map(|p| p.id()),
            Some("stub"),
            "provider-less resolution should build the stub default"
        );
        assert_eq!(slot.disabled_reason, None);
    }
    // clear_overrides wipes the row — still a valid provider-less write.
    let v = settings_set(
        &app,
        &cookie,
        serde_json::json!({"clear_overrides": true, "enabled": true}),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    let row = db.get_oxpilot_settings().await.unwrap().unwrap();
    assert!(row.enabled_override.is_none() && row.provider_id.is_none());
    let slot = rt.slot.read().await;
    assert_eq!(slot.disabled_reason.as_deref(), Some("flag_off"));
}

#[tokio::test]
async fn flag_off_on_carries_provider_arc_without_rebuild() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    std::env::set_var("OXIDEAN_OXPILOT_PROVIDER", "stub");
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (rt, stub_arc) = parked_stub_runtime();
    let (app, _rt) = test_app(db.clone(), Some(rt.clone())).await;
    let cookie = session_cookie(&db, &admin_id).await;

    // Flag-on write (flag-only → build skipped): the parked Arc carries.
    let v = settings_set(&app, &cookie, serde_json::json!({"enabled": true})).await;
    assert_eq!(v["ok"], true, "{v}");
    {
        let slot = rt.slot.read().await;
        assert!(
            Arc::ptr_eq(slot.provider.as_ref().unwrap(), &stub_arc),
            "flag-on must carry the same provider Arc (NFR4.5)"
        );
        assert_eq!(slot.disabled_reason, None);
    }
    // Flag-off keeps the provider parked (identity still matches).
    let v = settings_set(&app, &cookie, serde_json::json!({"enabled": false})).await;
    assert_eq!(v["ok"], true, "{v}");
    let slot = rt.slot.read().await;
    assert!(Arc::ptr_eq(slot.provider.as_ref().unwrap(), &stub_arc));
    assert_eq!(slot.disabled_reason.as_deref(), Some("flag_off"));
}

#[tokio::test]
async fn flag_off_combined_write_never_installs_enabled_slot() {
    // R-01 regression: a provider-field write under an effective
    // flag-off must NOT build — the response's flag_off and the
    // installed slot agree (kill switch is never silently ignored).
    let _g = env_lock().await;
    clear_oxpilot_env();
    std::env::set_var("OXIDEAN_OXPILOT_PROVIDER", "stub");
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (rt, _stub_arc) = parked_stub_runtime();
    let (app, _r) = test_app(db.clone(), Some(rt.clone())).await;
    let cookie = session_cookie(&db, &admin_id).await;

    let v = settings_set(
        &app,
        &cookie,
        serde_json::json!({"enabled": false, "provider_id": "stub", "model": "stub-2"}),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["enabled"], false);
    assert_eq!(v["data"]["disabled_reason"], "flag_off");
    // The installed slot matches the response — flag_off, no build.
    let slot = rt.slot.read().await;
    assert_eq!(slot.disabled_reason.as_deref(), Some("flag_off"));
    // Identity changed (model stub-2 ≠ parked None) → no stale carry.
    drop(slot);
    // The write persisted: a flag-on write builds the stored identity
    // best-effort — the slot must not wedge `misconfigured` waiting on a
    // rebuild nothing triggers (a build failure still degrades, never
    // refuses the flag write — BR3.1 recovery path).
    let v = settings_set(&app, &cookie, serde_json::json!({"enabled": true})).await;
    assert_eq!(v["ok"], true, "{v}");
    let slot = rt.slot.read().await;
    assert_eq!(slot.disabled_reason, None);
    assert_eq!(slot.provider.as_ref().map(|p| p.id()), Some("stub"));
}

#[tokio::test]
async fn key_write_surfaces_metadata_only() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    std::env::set_var("OXIDEAN_OXPILOT_SECRETS_KEY", "test-secret");
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (app, _rt) = test_app(db.clone(), None).await;
    let cookie = session_cookie(&db, &admin_id).await;

    let v = settings_set(
        &app,
        &cookie,
        serde_json::json!({"api_key": "sk-live-abcdef"}),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["key_set"], true);
    assert_eq!(v["data"]["key_last4"], "cdef");
    assert_eq!(v["data"]["key_source"], "db");
    // No key material leaks anywhere in the response.
    let text = v.to_string();
    assert!(!text.contains("sk-live"), "{text}");
    // The row holds ciphertext+last4 as a pair.
    let row = db.get_oxpilot_settings().await.unwrap().unwrap();
    assert!(row.key_ciphertext.is_some());
    assert_eq!(row.key_last4.as_deref(), Some("cdef"));
    assert!(!row.key_ciphertext.as_deref().unwrap().contains("sk-live"));
    // providers_list reports configured for key-required adapters.
    let p = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"admin.oxpilot.providers_list","input":{}}"#,
    )
    .await;
    assert_eq!(p["ok"], true, "{p}");
    let providers = p["data"]["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 4); // stub omitted (BR1.4)
    assert_eq!(providers[0]["id"], "openai");
    assert_eq!(providers[0]["configured"], true);
    assert_eq!(providers[0]["requires_key"], true);
    assert_eq!(providers[1]["id"], "anthropic");
    assert_eq!(providers[1]["configured"], true);
    assert_eq!(providers[2]["id"], "ollama");
    assert_eq!(providers[2]["configured"], true);
    assert_eq!(providers[2]["requires_key"], false);
    assert_eq!(providers[3]["id"], "openai-compatible");
    assert_eq!(providers[3]["configured"], true);
    assert_eq!(providers[3]["requires_key"], false);
    // clear_key erases the pair.
    let v = settings_set(&app, &cookie, serde_json::json!({"clear_key": true})).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["key_set"], false);
    let row = db.get_oxpilot_settings().await.unwrap().unwrap();
    assert!(row.key_ciphertext.is_none() && row.key_last4.is_none());
}

#[tokio::test]
async fn concurrent_writes_serialize_coherently() {
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (app, _rt) = test_app(db.clone(), None).await;
    let cookie = session_cookie(&db, &admin_id).await;

    // Two whole-row-coherent writes racing — the rebuild mutex orders
    // them so the persisted pair is never interleaved (BR4.3).
    let a = settings_set(
        &app,
        &cookie,
        serde_json::json!({"user_budget": 11, "instance_budget": 11}),
    );
    let b = settings_set(
        &app,
        &cookie,
        serde_json::json!({"user_budget": 22, "instance_budget": 22}),
    );
    let (va, vb) = tokio::join!(a, b);
    assert_eq!(va["ok"], true, "{va}");
    assert_eq!(vb["ok"], true, "{vb}");
    let row = db.get_oxpilot_settings().await.unwrap().unwrap();
    assert!(
        (row.user_budget, row.instance_budget) == (Some(11), Some(11))
            || (row.user_budget, row.instance_budget) == (Some(22), Some(22)),
        "interleaved budgets: {row:?}"
    );
}

#[tokio::test]
async fn stub_write_debug_shaped_succeeds() {
    // Test binaries are debug builds — `cfg!(debug_assertions)` is true,
    // so the BR1.4 gate allows `stub` here; the release-shaped refusal is
    // pinned by `settings::tests::provider_settable` (debug=false arm).
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (app, rt) = test_app(db.clone(), None).await;
    let cookie = session_cookie(&db, &admin_id).await;

    let v = settings_set(
        &app,
        &cookie,
        serde_json::json!({
            "enabled": true,
            "provider_id": "stub",
            "model": "stub-1"
        }),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    let slot = rt.slot.read().await;
    assert!(
        slot.provider.is_some(),
        "stub build must install a provider"
    );
    assert_eq!(slot.provider.as_ref().unwrap().id(), "stub");
    assert_eq!(slot.disabled_reason, None);
    // providers_list still never shows stub.
    let p = rpc_json(
        &app,
        Some(&cookie),
        r#"{"procedure":"admin.oxpilot.providers_list","input":{}}"#,
    )
    .await;
    let ids: Vec<&str> = p["data"]["providers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["openai", "anthropic", "ollama", "openai-compatible"]);
}

#[tokio::test]
async fn openai_compatible_write_builds_keyless_slot() {
    // LiteLLM-style endpoint: provider_id + base_url with no key —
    // validate passes (self-host exemption) and the slot builds.
    let _g = env_lock().await;
    clear_oxpilot_env();
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (app, rt) = test_app(db.clone(), None).await;
    let cookie = session_cookie(&db, &admin_id).await;

    let v = settings_set(
        &app,
        &cookie,
        serde_json::json!({
            "enabled": true,
            "provider_id": "openai-compatible",
            "model": "llama-3.3-70b",
            "base_url": "http://litellm.local:4000"
        }),
    )
    .await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["data"]["provider_id"], "openai-compatible");
    let slot = rt.slot.read().await;
    assert!(
        slot.provider.is_some(),
        "compat build must install a provider"
    );
    assert_eq!(slot.disabled_reason, None);

    // Missing base_url is refused up front — the write fails typed.
    let v = settings_set(
        &app,
        &cookie,
        serde_json::json!({
            "provider_id": "openai-compatible",
            "base_url": null
        }),
    )
    .await;
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["error"]["code"], "invalid_provider");
}

#[tokio::test]
async fn admin_procs_work_while_oxpilot_disabled() {
    // Gate-exempt: admin settings procs are the recovery path — they
    // must not answer oxpilot.disabled even flag-off (BR1.1 negative).
    let _g = env_lock().await;
    clear_oxpilot_env(); // flag off → everything else gated
    let db = test_db().await;
    let admin_id = seed_admin(&db).await;
    let (app, _rt) = test_app(db.clone(), None).await;
    let cookie = session_cookie(&db, &admin_id).await;
    let g = settings_get(&app, &cookie).await;
    assert_eq!(g["ok"], true, "{g}");
    let s = settings_set(&app, &cookie, serde_json::json!({"enabled": false})).await;
    assert_eq!(s["ok"], true, "{s}");
}
