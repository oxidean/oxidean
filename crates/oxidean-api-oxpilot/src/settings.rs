//! `admin.oxpilot.settings_*` + `providers_list` (WF2/WF5) — sparse
//! row-over-env resolution (WF1), the mutex-serialized
//! `resolve → validate → build → persist → swap → audit` write path
//! (BR4.3), and `rebuild` — the shared boot/write slot installer.
//!
//! Request payload nullability is tri-state (BR2.3): absent = keep the
//! stored override, explicit `null` = clear it, value = set — decoded as
//! `Option<Option<T>>` (a deliberate deviation from `admin.mcp`'s
//! null-collapse). Key material enters only as `api_key` plaintext in
//! the payload, is encrypted into the candidate row before build, and
//! never leaves the providers tree again.

use std::sync::Arc;

use oxidean_api_core::oxpilot::{
    disabled_slot, BudgetCaps, KeySource, OxpilotRuntime, OxpilotRuntimeSlot, ProviderIdentity,
};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::AppError;
use oxidean_db::{Database, OxpilotSettingsRow};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::providers::{egress, key, registry};

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "oxpilot settings db error");
        AppError::new("admin.internal", "settings operation failed")
    }
}

// ---------- env snapshot ----------

/// The `OXIDEAN_OXPILOT_*` env floor — snapshotted once per resolve so
/// resolution is a pure merge (testable without touching `std::env` in
/// the matrix tests).
#[derive(Debug, Clone, Default)]
pub struct EnvDefaults {
    pub enabled: bool,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// Plaintext — lives only in this in-memory shape, never persisted
    /// or emitted (entities.md `api_key_plaintext` constraint).
    pub api_key: Option<String>,
    pub user_budget: Option<i64>,
    pub instance_budget: Option<i64>,
}

fn env_str(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_flag_on() -> bool {
    std::env::var("OXIDEAN_OXPILOT_ENABLED")
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
        })
        .unwrap_or(false)
}

pub fn load_env() -> EnvDefaults {
    EnvDefaults {
        enabled: env_flag_on(),
        provider_id: env_str("OXIDEAN_OXPILOT_PROVIDER"),
        model: env_str("OXIDEAN_OXPILOT_MODEL"),
        base_url: env_str("OXIDEAN_OXPILOT_BASE_URL"),
        api_key: env_str("OXIDEAN_OXPILOT_API_KEY"),
        user_budget: env_str("OXIDEAN_OXPILOT_USER_BUDGET").and_then(|v| v.parse::<i64>().ok()),
        instance_budget: env_str("OXIDEAN_OXPILOT_INSTANCE_BUDGET")
            .and_then(|v| v.parse::<i64>().ok()),
    }
}

// ---------- effective config (WF1) ----------

/// Row-over-env resolution result — THE validation target (BR3.x binds
/// this merged shape, never raw request fields) and the input to
/// `registry::build`. `key_ciphertext`/`api_key_env` are the build-time
/// key sources; plaintext materializes inside provider construction
/// only (`key::resolve_material`, `pub(in crate::providers)`).
#[derive(Debug, Clone, Default)]
pub struct EffectiveProviderConfig {
    pub enabled: bool,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub key_ciphertext: Option<String>,
    pub api_key_env: Option<String>,
    /// Resolved last4 display hint — stored last4 when DB-sourced,
    /// computed against the env key when env-sourced (BR2.4).
    pub key_last4: Option<String>,
    pub user_budget: Option<i64>,
    pub instance_budget: Option<i64>,
    /// `flag_off | misconfigured | null` — construction can still
    /// downgrade to `misconfigured` (entities.md).
    pub disabled_reason: Option<String>,
}

/// Merge the sparse override row over the env snapshot — each field
/// takes the DB value when non-NULL, else the env default (BR2.1).
/// `disabled_reason` resolves `flag_off` > `misconfigured` (U1
/// precedence): flag-off wins even with a broken provider id and
/// unknown-but-present ids land `misconfigured`. An unset provider id
/// defaults to `stub` — the sys-admin-only preview mode (BR2.6) — so
/// `no_provider` never reaches this resolution anymore.
pub fn resolve_over_env(
    row: Option<&OxpilotSettingsRow>,
    env: &EnvDefaults,
) -> EffectiveProviderConfig {
    let pick = |db: &Option<String>, env: &Option<String>| db.clone().or_else(|| env.clone());
    let empty = OxpilotSettingsRow::default();
    let row = row.unwrap_or(&empty);
    let enabled = row.enabled_override.unwrap_or(env.enabled);
    let provider_id = pick(&row.provider_id, &env.provider_id)
        .or_else(|| Some(oxidean_api_core::oxpilot::STUB_PROVIDER_ID.to_string()));
    let key_ciphertext = row.key_ciphertext.clone();
    let api_key_env = env.api_key.clone();
    let key_source = if key_ciphertext.is_some() {
        KeySource::Db
    } else if api_key_env.is_some() {
        KeySource::Env
    } else {
        KeySource::None
    };
    let key_last4 = match key_source {
        KeySource::Db => row.key_last4.clone(),
        KeySource::Env => api_key_env.as_deref().map(key::key_last4),
        KeySource::None => None,
    };
    let disabled_reason = if !enabled {
        Some("flag_off".to_string())
    } else if provider_id
        .as_deref()
        .is_some_and(|id| registry::resolve_descriptor(id).is_none())
    {
        Some("misconfigured".to_string())
    } else {
        None
    };
    EffectiveProviderConfig {
        enabled,
        provider_id,
        model: pick(&row.model, &env.model),
        base_url: pick(&row.base_url, &env.base_url),
        key_ciphertext,
        api_key_env,
        key_last4,
        user_budget: row.user_budget.or(env.user_budget),
        instance_budget: row.instance_budget.or(env.instance_budget),
        disabled_reason,
    }
}

impl EffectiveProviderConfig {
    pub fn key_source(&self) -> KeySource {
        if self.key_ciphertext.is_some() {
            KeySource::Db
        } else if self.api_key_env.is_some() {
            KeySource::Env
        } else {
            KeySource::None
        }
    }

    /// The build-origin tuple — a custody change (env→db, db→cleared)
    /// is an identity change and never carries a stale Arc (NFR4.3).
    /// `None` when no provider resolves.
    pub fn identity(&self) -> Option<ProviderIdentity> {
        self.provider_id.as_ref().map(|id| ProviderIdentity {
            provider_id: id.clone(),
            model: self.resolved_model(),
            base_url: self.base_url.clone(),
            key_source: self.key_source(),
        })
    }

    /// Effective model — resolved override/env, else the descriptor's
    /// advertised default. `None` only for no-list providers (ollama)
    /// without an explicit model.
    pub fn resolved_model(&self) -> Option<String> {
        self.model.clone().or_else(|| {
            self.provider_id
                .as_deref()
                .and_then(registry::resolve_descriptor)
                .and_then(|d| d.models.first())
                .map(|m| m.to_string())
        })
    }
}

// ---------- sparse apply (BR2.3, BR3.4) ----------

/// Tri-state decode — `Option<Option<T>>` alone collapses `null` to
/// `None` (outer), indistinguishable from absent. With this
/// deserializer: absent → `None` (keep), `null` → `Some(None)` (clear),
/// value → `Some(Some(v))` (set) — BR2.3's deliberate deviation from
/// `admin.mcp`'s null-collapse.
fn de_tri_state<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// `settings_set` payload. Key material is `api_key` plaintext —
/// encrypted into the candidate row by `apply`, never persisted raw.
#[derive(Debug, Default, Deserialize)]
pub struct SettingsSetRequest {
    #[serde(default, deserialize_with = "de_tri_state")]
    pub enabled: Option<Option<bool>>,
    #[serde(default, deserialize_with = "de_tri_state")]
    pub provider_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_tri_state")]
    pub model: Option<Option<String>>,
    #[serde(default, deserialize_with = "de_tri_state")]
    pub base_url: Option<Option<String>>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_key: bool,
    #[serde(default)]
    pub clear_overrides: bool,
    #[serde(default, deserialize_with = "de_tri_state")]
    pub user_budget: Option<Option<i64>>,
    #[serde(default, deserialize_with = "de_tri_state")]
    pub instance_budget: Option<Option<i64>>,
}

fn bad_input(msg: impl Into<String>) -> AppError {
    AppError::new("rpc.bad_input", msg.into())
}

/// Trim a tri-state string write — stored == validated == built; a
/// whitespace-padded value must not pass checks but install a broken
/// config (paste artifacts are never meaningful edge content).
fn trim_nonempty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// The value a payload field selects — trimmed; a whitespace-only or
/// explicit-null field is a CLEAR, not a selection (`None`).
fn named_trimmed(v: &Option<Option<String>>) -> Option<&str> {
    v.as_ref()
        .and_then(|o| o.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The provider-affecting payload fields — touching ANY of them forces
/// the write-path build (BR4.3); flag/budget-only writes skip it
/// (BR3.1 recovery exemption).
fn touches_provider_fields(req: &SettingsSetRequest) -> bool {
    req.provider_id.is_some()
        || req.model.is_some()
        || req.base_url.is_some()
        || req.api_key.is_some()
        || req.clear_key
        || req.clear_overrides
}

/// Apply a payload onto the stored row → candidate row + touched field
/// names (audit names-only, BR5.2). `clear_overrides` nulls the whole
/// row and wins over same-payload field writes (BR3.4).
fn apply(
    stored: Option<&OxpilotSettingsRow>,
    req: &SettingsSetRequest,
) -> Result<(OxpilotSettingsRow, Vec<String>), AppError> {
    if req.api_key.is_some() && req.clear_key {
        return Err(bad_input("api_key and clear_key conflict"));
    }
    if let Some(k) = &req.api_key {
        if k.trim().is_empty() {
            return Err(bad_input("api_key must not be empty"));
        }
    }
    let mut touched: Vec<String> = Vec::new();
    let mut candidate = if req.clear_overrides {
        touched.push("clear_overrides".to_string());
        OxpilotSettingsRow::default()
    } else {
        stored.cloned().unwrap_or_default()
    };
    // Tri-state fields — absent keeps the stored override untouched.
    if !req.clear_overrides {
        if let Some(v) = &req.enabled {
            candidate.enabled_override = *v;
            touched.push("enabled".to_string());
        }
        if let Some(v) = &req.provider_id {
            candidate.provider_id = v.clone().and_then(trim_nonempty);
            touched.push("provider_id".to_string());
        }
        if let Some(v) = &req.model {
            candidate.model = v.clone().and_then(trim_nonempty);
            touched.push("model".to_string());
        }
        if let Some(v) = &req.base_url {
            candidate.base_url = v.clone().and_then(trim_nonempty);
            touched.push("base_url".to_string());
        }
        if let Some(v) = &req.user_budget {
            candidate.user_budget = *v;
            touched.push("user_budget".to_string());
        }
        if let Some(v) = &req.instance_budget {
            candidate.instance_budget = *v;
            touched.push("instance_budget".to_string());
        }
    }
    // Key custody — ciphertext+last4 always move as ONE pair (NFR1.5).
    if req.clear_key && !req.clear_overrides {
        candidate.key_ciphertext = None;
        candidate.key_last4 = None;
        touched.push("clear_key".to_string());
    }
    if let Some(k) = &req.api_key {
        if !req.clear_overrides {
            let (ct, last4) = key::encrypt_key(k.trim()).map_err(|e| {
                if e.contains("is required to encrypt") {
                    AppError::new("crypto.unavailable", e)
                } else {
                    AppError::new("admin.internal", "key encryption failed")
                }
            })?;
            candidate.key_ciphertext = Some(ct);
            candidate.key_last4 = Some(last4);
            touched.push("api_key".to_string());
        }
    }
    Ok((candidate, touched))
}

// ---------- validation (BR3.x — binds the RESOLVED config) ----------

/// Can `settings_set` select this id? `stub` is release-refused (BR1.4)
/// — `debug_build` lets dev/test builds set it deliberately.
fn provider_settable(id: &str, debug_build: bool) -> bool {
    debug_build || id != "stub"
}

/// Validate the write-chosen fields against the RESOLVED config
/// (BR3.1–BR3.3): `invalid_provider`/`rpc.bad_input` refuse values THIS
/// write selects; problems arriving via env defaults or a stale stored
/// row degrade to a reason-bearing disabled slot instead — a write that
/// never chose a provider can never be wedged by one (flag-off is the
/// un-wedgeable kill switch).
async fn validate(
    cfg: &EffectiveProviderConfig,
    req: &SettingsSetRequest,
    debug_build: bool,
) -> Result<(), AppError> {
    for (name, b) in [
        ("user_budget", &req.user_budget),
        ("instance_budget", &req.instance_budget),
    ] {
        if let Some(Some(v)) = b {
            if *v <= 0 {
                return Err(bad_input(format!("{name} must be a positive integer")));
            }
        }
    }
    // Provider membership + release guard (BR1.4, BR3.1) — binds only a
    // provider_id named in this payload. Env `stub` stays legal in all
    // builds; a stored-but-unwritten id degrades rather than refusing.
    if let Some(id) = named_trimmed(&req.provider_id) {
        if registry::resolve_descriptor(id).is_none() || !provider_settable(id, debug_build) {
            return Err(AppError::new(
                "invalid_provider",
                format!("unknown provider `{id}`"),
            ));
        }
    }
    // Advertised-model check: binds the RESOLVED model whenever the
    // write names `provider_id` OR `model` (a provider switch must not
    // leave a stale stored model the new provider never advertised —
    // BR3.1 merge binding). Only when the resolved provider advertises
    // a list; ollama/stub take any id. A provider with NO resolved
    // model is legal — `slot.model` stays None, failing at call time.
    if req.provider_id.is_some() || req.model.is_some() {
        if let (Some(desc), Some(m)) = (
            cfg.provider_id
                .as_deref()
                .and_then(registry::resolve_descriptor),
            cfg.model.as_deref(),
        ) {
            if !desc.models.is_empty() && !desc.models.contains(&m) {
                return Err(AppError::new(
                    "invalid_provider",
                    format!("model `{m}` not advertised by `{}`", desc.id),
                ));
            }
        }
    }
    // base_url — only a write-chosen value is refused; the private-target
    // bound keys on the resolved provider — skipped for None and
    // exempted for ollama inside `validate_base_url` (BR3.2).
    if let Some(url) = named_trimmed(&req.base_url) {
        let bound_provider = cfg
            .provider_id
            .as_deref()
            .filter(|id| registry::resolve_descriptor(id).is_some());
        // The settings path validates only — the provider client
        // re-validates each fresh answer set on connect.
        egress::validate_base_url(bound_provider, url)
            .await
            .map_err(|e| AppError::new("invalid_provider", e))?;
    }
    // CWE-319 — key-bearing cleartext: a resolved `openai`,
    // `openai-compatible`, or `anthropic` over http:// must not hold key
    // material unless the operator opted in (OXIDEAN_OXPILOT_INSECURE_HTTP=1).
    // Binds only
    // writes that name a provider-relevant field — env-sourced combos
    // degrade to a misconfigured slot at build, never wedged here.
    let provider_write =
        req.provider_id.is_some() || req.base_url.is_some() || req.api_key.is_some();
    if provider_write
        && matches!(
            cfg.provider_id.as_deref(),
            Some("openai") | Some("openai-compatible") | Some("anthropic")
        )
        && cfg.base_url.as_deref().is_some_and(egress::cleartext_http)
        && (cfg.key_ciphertext.is_some() || cfg.api_key_env.is_some())
        && !egress::http_key_opted_in()
    {
        return Err(AppError::new(
            "invalid_provider",
            "key material over cleartext http:// refused — use https, drop the key, or set OXIDEAN_OXPILOT_INSECURE_HTTP=1".to_string(),
        ));
    }
    Ok(())
}

// ---------- slot resolution (WF3 step 4, NFR4.3/4.5) ----------

/// What a resolved config needs the slot to become — the pure
/// carry/drop/build decision, fully testable without touching locks.
pub enum SlotPlan {
    /// Install this slot as-is — disabled, or flag-off-with-carry.
    Slot(OxpilotRuntimeSlot),
    /// The prior provider still matches the resolved identity — carry
    /// it into an enabled slot without contacting the provider (NFR4.3).
    CarryProvider,
    /// Provider construction is required for this identity.
    Build(ProviderIdentity),
}

/// `resolve_slot(&resolved, &prior)` — the approved `built_from`-carry
/// design:
/// - flag-off retains the provider ONLY when `built_from` still matches
///   (a stale Arc is dropped, never parked);
/// - enabled + no resolved provider installs a reason-bearing disabled
///   slot — `no_provider`/`misconfigured` per the resolved reason;
/// - enabled + provider: unchanged identity carries the installed Arc
///   (no rebuild, no network); anything else requires a build.
pub fn resolve_slot(cfg: &EffectiveProviderConfig, prior: &OxpilotRuntimeSlot) -> SlotPlan {
    let turns = prior.turns.clone();
    let identity = cfg.identity();
    let caps = BudgetCaps::from(cfg);
    if !cfg.enabled {
        let carry = identity.is_some() && prior.built_from.as_ref() == identity.as_ref();
        return SlotPlan::Slot(OxpilotRuntimeSlot {
            provider: if carry { prior.provider.clone() } else { None },
            model: if carry { prior.model.clone() } else { None },
            disabled_reason: Some("flag_off".to_string()),
            turns,
            built_from: if carry {
                prior.built_from.clone()
            } else {
                None
            },
            budget_caps: caps,
            // The C6 executor is capability config, not provider config —
            // it survives every slot swap like `turns` does (NFR4.5).
            tools: prior.tools.clone(),
        });
    }
    match identity {
        None => SlotPlan::Slot(disabled_slot(
            turns,
            cfg.disabled_reason.as_deref().unwrap_or("no_provider"),
            caps,
            prior.tools.clone(),
        )),
        Some(id) => {
            if prior.built_from.as_ref() == Some(&id) && prior.provider.is_some() {
                SlotPlan::CarryProvider
            } else {
                SlotPlan::Build(id)
            }
        }
    }
}

/// Install the carried provider into an enabled slot (the
/// `CarryProvider` arm — prior Arc + model + budget caps move over
/// unchanged; `resolve_slot` already stamped the new caps on every
/// fresh-slot arm, and a pure carry means the resolution didn't move
/// the budget axes in a way that needs re-stamping — the NEXT
/// `resolve_slot` pass re-stamps from the resolved cfg anyway. To stay
/// exact we re-stamp here from the cfg the caller resolved).
fn carried_slot(prior: &OxpilotRuntimeSlot, cfg: &EffectiveProviderConfig) -> OxpilotRuntimeSlot {
    OxpilotRuntimeSlot {
        provider: prior.provider.clone(),
        model: prior.model.clone(),
        disabled_reason: None,
        turns: prior.turns.clone(),
        built_from: prior.built_from.clone(),
        budget_caps: BudgetCaps::from(cfg),
        tools: prior.tools.clone(),
    }
}

/// Enabled slot for a freshly built provider.
fn built_slot(
    turns: Arc<oxidean_api_core::oxpilot::ActiveTurns>,
    provider: Arc<dyn oxidean_api_core::oxpilot::LlmProvider>,
    cfg: &EffectiveProviderConfig,
    identity: ProviderIdentity,
    tools: Option<Arc<dyn oxidean_api_core::oxpilot::ToolExecutor>>,
) -> OxpilotRuntimeSlot {
    OxpilotRuntimeSlot {
        provider: Some(provider),
        model: cfg.resolved_model(),
        disabled_reason: None,
        turns,
        built_from: Some(identity),
        budget_caps: BudgetCaps::from(cfg),
        tools,
    }
}

// ---------- rebuild — the shared boot/write installer (WF3) ----------

/// Resolve env+row and swap the slot (BR4.3, NFR4.11): unconditional
/// after `AppState::new` so the env-only boot slot upgrades to the
/// resolved config. Row-read failure degrades to env-only (warn, never
/// fatal); build failure degrades to a `misconfigured` disabled slot —
/// boot always reaches `listen` (NFR4.12). Serialized under
/// `rebuild_lock` so a concurrent `settings_set` can't interleave.
pub async fn rebuild(db: &Database, oxpilot: &OxpilotRuntime) {
    let _guard = oxpilot.rebuild_lock.lock().await;
    let row = match db.get_oxpilot_settings().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "oxpilot settings row unreadable; resolving env-only");
            None
        }
    };
    let cfg = resolve_over_env(row.as_ref(), &load_env());
    let prior = oxpilot.slot.read().await.clone();
    match resolve_slot(&cfg, &prior) {
        SlotPlan::Slot(slot) => *oxpilot.slot.write().await = slot,
        SlotPlan::CarryProvider => *oxpilot.slot.write().await = carried_slot(&prior, &cfg),
        SlotPlan::Build(identity) => match registry::build(&cfg).await {
            Ok(provider) => {
                *oxpilot.slot.write().await = built_slot(
                    prior.turns.clone(),
                    provider,
                    &cfg,
                    identity,
                    prior.tools.clone(),
                );
            }
            Err(e) => {
                tracing::warn!(
                    code = %e.code,
                    "oxpilot provider build failed; installing misconfigured slot"
                );
                *oxpilot.slot.write().await = disabled_slot(
                    prior.turns.clone(),
                    "misconfigured",
                    BudgetCaps::from(&cfg),
                    prior.tools.clone(),
                );
            }
        },
    }
}

// ---------- admin RPC procedures (WF2/WF5) ----------

/// `admin.oxpilot.settings_get` — effective resolved values + per-field
/// `*_overridden` flags + `key_set`/`key_last4` (BR2.4). Never emits
/// ciphertext or plaintext.
pub async fn settings_get(ctx: &RpcCtx) -> Result<Value, AppError> {
    oxidean_api_core::auth::admin::require_admin(ctx).await?;
    let row = ctx.db.get_oxpilot_settings().await.map_err(db_err)?;
    let cfg = resolve_over_env(row.as_ref(), &load_env());
    Ok(settings_json(row.as_ref(), &cfg))
}

fn settings_json(row: Option<&OxpilotSettingsRow>, cfg: &EffectiveProviderConfig) -> Value {
    let overridden = |f: fn(&OxpilotSettingsRow) -> bool| row.is_some_and(f);
    json!({
        "enabled": cfg.enabled,
        "enabled_overridden": overridden(|r| r.enabled_override.is_some()),
        "provider_id": cfg.provider_id,
        "provider_id_overridden": overridden(|r| r.provider_id.is_some()),
        "model": cfg.model,
        "model_overridden": overridden(|r| r.model.is_some()),
        "base_url": cfg.base_url,
        "base_url_overridden": overridden(|r| r.base_url.is_some()),
        "user_budget": cfg.user_budget,
        "user_budget_overridden": overridden(|r| r.user_budget.is_some()),
        "instance_budget": cfg.instance_budget,
        "instance_budget_overridden": overridden(|r| r.instance_budget.is_some()),
        "key_set": cfg.key_source() != KeySource::None,
        "key_last4": cfg.key_last4,
        "key_source": match cfg.key_source() {
            KeySource::Db => "db",
            KeySource::Env => "env",
            KeySource::None => "none",
        },
        "disabled_reason": cfg.disabled_reason,
        "updated_at": row.map(|r| r.updated_at.clone()),
    })
}

/// `admin.oxpilot.settings_set` — the whole write path under
/// `rebuild_lock` (BR4.3): require_admin → apply → resolve → validate
/// → build (provider-affecting writes only) → persist → swap → audit.
/// Build runs BEFORE persist — a failed build returns
/// `invalid_provider`, persists nothing, leaves the old slot untouched.
pub async fn settings_set(ctx: &mut RpcCtx, input: Value) -> Result<Value, AppError> {
    let admin = oxidean_api_core::admin::require_admin_user(ctx).await?;
    let req: SettingsSetRequest = serde_json::from_value(input)
        .map_err(|e| bad_input(format!("invalid settings_set input: {e}")))?;
    let runtime = ctx
        .oxpilot
        .clone()
        .ok_or_else(|| AppError::new("admin.internal", "oxpilot runtime unavailable"))?;
    let _guard = runtime.rebuild_lock.lock().await;

    let stored = ctx.db.get_oxpilot_settings().await.map_err(db_err)?;
    let (candidate, touched) = apply(stored.as_ref(), &req)?;
    let cfg = resolve_over_env(Some(&candidate), &load_env());
    validate(&cfg, &req, cfg!(debug_assertions)).await?;

    // Build decision (BR4.3): provider-affecting writes with a resolved
    // provider MUST build before persist; flag/budget-only writes skip
    // the build entirely (recovery path). `cfg.enabled` gates the build —
    // a combined write under an effective flag-off must NOT construct or
    // install an enabled slot; `resolve_slot` answers flag-off instead.
    let build_required = cfg.enabled && touches_provider_fields(&req) && cfg.provider_id.is_some();
    let built = if build_required {
        Some(
            registry::build(&cfg)
                .await
                .map_err(|e| AppError::new(e.code, e.message))?,
        )
    } else {
        None
    };

    // Persist — a candidate with no surviving overrides NULLs the row
    // through `clear` (one statement, same resolution as absent). On
    // failure: nothing swapped (BR4.3).
    let empty = OxpilotSettingsRow::default();
    let persist_row = if candidate == empty {
        None
    } else {
        Some(candidate)
    };
    let persisted_row = match &persist_row {
        Some(row) => Some(ctx.db.upsert_oxpilot_settings(row).await.map_err(db_err)?),
        None => {
            ctx.db.clear_oxpilot_settings().await.map_err(db_err)?;
            None
        }
    };

    // Swap — the carried `Arc<ActiveTurns>` survives; in-flight turns
    // keep their captured provider Arcs (NFR4.5).
    let prior = runtime.slot.read().await.clone();
    let new_slot = if let Some(provider) = built {
        built_slot(
            prior.turns.clone(),
            provider,
            &cfg,
            cfg.identity().expect("build implies resolved provider"),
            prior.tools.clone(),
        )
    } else {
        match resolve_slot(&cfg, &prior) {
            SlotPlan::Slot(s) => s,
            SlotPlan::CarryProvider => carried_slot(&prior, &cfg),
            // A no-build write reaching Build (e.g. flag-on with an
            // uncarried provider) builds best-effort: the write itself
            // already succeeded — the slot must not wedge on a refusal
            // meant for provider-affecting writes (BR3.1). A build
            // failure degrades to `misconfigured`, same as `rebuild`.
            SlotPlan::Build(identity) => match registry::build(&cfg).await {
                Ok(provider) => built_slot(
                    prior.turns.clone(),
                    provider,
                    &cfg,
                    identity,
                    prior.tools.clone(),
                ),
                Err(e) => {
                    tracing::warn!(
                        code = %e.code,
                        "oxpilot provider build failed on settings write; installing misconfigured slot"
                    );
                    disabled_slot(
                        prior.turns.clone(),
                        "misconfigured",
                        BudgetCaps::from(&cfg),
                        prior.tools.clone(),
                    )
                }
            },
        }
    };
    // The installed slot's reason is the runtime truth — an enable-only
    // write that installs a `misconfigured` slot must not report
    // `disabled_reason: null` just because the provider id resolved.
    let installed_reason = new_slot.disabled_reason.clone();
    *runtime.slot.write().await = new_slot;

    // Audit AFTER commit+swap — names only, fire-and-log; a failed audit
    // row can never un-commit the mutation (BR5.2).
    oxidean_api_core::audit::record(
        ctx,
        Some((&admin.id, &admin.username)),
        "admin.oxpilot.settings_set",
        None,
        Some(json!({ "fields": touched }).to_string()),
    )
    .await;

    let mut out = settings_json(persisted_row.as_ref(), &cfg);
    out["disabled_reason"] = json!(installed_reason);
    Ok(out)
}

/// `admin.oxpilot.providers_list` — REGISTRY verbatim plus a computed
/// `configured` (BR1.4/WF5: key material resolvable for key-required
/// adapters — env `OXIDEAN_OXPILOT_API_KEY` or the stored ciphertext —
/// always true for `ollama`). `stub` is never listed.
pub async fn providers_list(ctx: &RpcCtx) -> Result<Value, AppError> {
    oxidean_api_core::auth::admin::require_admin(ctx).await?;
    let row = ctx.db.get_oxpilot_settings().await.map_err(db_err)?;
    let ciphertext = row.as_ref().and_then(|r| r.key_ciphertext.clone());
    let env_key = env_str("OXIDEAN_OXPILOT_API_KEY");
    let providers: Vec<Value> = registry::REGISTRY
        .iter()
        .map(|d| {
            let configured = !d.requires_key
                || key::key_material_resolvable(ciphertext.as_deref(), env_key.as_deref());
            json!({
                "id": d.id,
                "models": d.models,
                "requires_key": d.requires_key,
                "configured": configured,
            })
        })
        .collect();
    Ok(json!({ "providers": providers }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::key::tests::ENV_LOCK;
    use oxidean_api_core::oxpilot::{ActiveTurns, StubProvider};

    fn env() -> EnvDefaults {
        EnvDefaults {
            enabled: true,
            provider_id: Some("stub".to_string()),
            model: Some("stub-1".to_string()),
            base_url: None,
            api_key: Some("sk-env-9999".to_string()),
            user_budget: Some(11),
            instance_budget: Some(22),
        }
    }

    fn prior_slot(built: bool) -> OxpilotRuntimeSlot {
        OxpilotRuntimeSlot {
            provider: built.then(|| {
                Arc::new(StubProvider { fail: false })
                    as Arc<dyn oxidean_api_core::oxpilot::LlmProvider>
            }),
            model: built.then(|| "stub-1".to_string()),
            disabled_reason: None,
            turns: Arc::new(ActiveTurns::default()),
            built_from: built.then(|| ProviderIdentity {
                provider_id: "stub".to_string(),
                model: Some("stub-1".to_string()),
                base_url: None,
                key_source: KeySource::Env,
            }),
            budget_caps: BudgetCaps::default(),
            tools: None,
        }
    }

    // ---------- resolve_over_env ----------

    #[test]
    fn resolve_row_wins_field_by_field() {
        let row = OxpilotSettingsRow {
            enabled_override: Some(false),
            provider_id: Some("openai".to_string()),
            model: Some("gpt-4.1".to_string()),
            base_url: Some("https://8.8.8.8".to_string()),
            key_ciphertext: Some("ct".to_string()),
            key_last4: Some("wxyz".to_string()),
            user_budget: Some(1),
            instance_budget: None, // NULL → env
            updated_at: String::new(),
        };
        let cfg = resolve_over_env(Some(&row), &env());
        assert!(!cfg.enabled); // row wins
        assert_eq!(cfg.provider_id.as_deref(), Some("openai"));
        assert_eq!(cfg.model.as_deref(), Some("gpt-4.1"));
        assert_eq!(cfg.base_url.as_deref(), Some("https://8.8.8.8"));
        assert_eq!(cfg.user_budget, Some(1));
        assert_eq!(cfg.instance_budget, Some(22)); // env fallback
        assert_eq!(cfg.key_source(), KeySource::Db);
        assert_eq!(cfg.key_last4.as_deref(), Some("wxyz"));
        assert_eq!(cfg.disabled_reason.as_deref(), Some("flag_off"));
    }

    #[test]
    fn resolve_null_row_follows_env() {
        let cfg = resolve_over_env(None, &env());
        assert!(cfg.enabled);
        assert_eq!(cfg.provider_id.as_deref(), Some("stub"));
        assert_eq!(cfg.key_source(), KeySource::Env);
        // Env-sourced last4 is computed, never stored (BR2.4).
        assert_eq!(cfg.key_last4.as_deref(), Some("9999"));
        assert_eq!(cfg.disabled_reason, None);
        assert_eq!(cfg.user_budget, Some(11));
    }

    #[test]
    fn resolve_disabled_reason_precedence() {
        // flag_off beats a broken provider id (U1 precedence).
        let mut e = env();
        e.enabled = false;
        e.provider_id = Some("bogus".to_string());
        let cfg = resolve_over_env(None, &e);
        assert_eq!(cfg.disabled_reason.as_deref(), Some("flag_off"));
        // Unknown id on an enabled resolution → misconfigured.
        e.enabled = true;
        let cfg = resolve_over_env(None, &e);
        assert_eq!(cfg.disabled_reason.as_deref(), Some("misconfigured"));
        // No provider → the sys-admin-only stub default (BR2.6).
        e.provider_id = None;
        let cfg = resolve_over_env(None, &e);
        assert_eq!(cfg.provider_id.as_deref(), Some("stub"));
        assert_eq!(cfg.disabled_reason, None);
    }

    #[test]
    fn resolve_identity_and_model_defaults() {
        let cfg = resolve_over_env(None, &env());
        let id = cfg.identity().unwrap();
        assert_eq!(id.provider_id, "stub");
        assert_eq!(id.model.as_deref(), Some("stub-1"));
        assert_eq!(id.key_source, KeySource::Env);
        // Model falls back to the advertised default when unset.
        let mut e = env();
        e.provider_id = Some("openai".to_string());
        e.model = None;
        let cfg = resolve_over_env(None, &e);
        assert_eq!(cfg.resolved_model().as_deref(), Some("gpt-4.1"));
        // Provider-less resolves the stub default's identity (BR2.6).
        e.provider_id = None;
        let cfg = resolve_over_env(None, &e);
        assert_eq!(cfg.identity().unwrap().provider_id, "stub");
    }

    // ---------- apply ----------

    fn set_req(v: Value) -> SettingsSetRequest {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn apply_tri_state_semantics() {
        let stored = OxpilotSettingsRow {
            provider_id: Some("anthropic".to_string()),
            model: Some("claude-sonnet-4-5".to_string()),
            user_budget: Some(5),
            ..Default::default()
        };
        // absent = keep, null = clear, value = set (BR2.3).
        let req = set_req(json!({
            "model": null,
            "provider_id": "ollama",
            "instance_budget": 9
        }));
        let (cand, touched) = apply(Some(&stored), &req).unwrap();
        assert_eq!(cand.provider_id.as_deref(), Some("ollama"));
        assert_eq!(cand.model, None); // cleared
        assert_eq!(cand.user_budget, Some(5)); // untouched
        assert_eq!(cand.instance_budget, Some(9));
        assert!(touched.contains(&"model".to_string()));
        assert!(touched.contains(&"provider_id".to_string()));
    }

    #[test]
    fn apply_trims_stored_strings() {
        // R-08: stored == validated == built — whitespace-padded values
        // persist in trimmed form, never raw.
        let req = set_req(json!({
            "provider_id": " ollama ",
            "model": "  llama3.1",
            "base_url": "http://127.0.0.1:11434 "
        }));
        let (cand, _) = apply(None, &req).unwrap();
        assert_eq!(cand.provider_id.as_deref(), Some("ollama"));
        assert_eq!(cand.model.as_deref(), Some("llama3.1"));
        assert_eq!(cand.base_url.as_deref(), Some("http://127.0.0.1:11434"));
        // Whitespace-only writes clear (same as empty after trim).
        let req = set_req(json!({"provider_id": "   "}));
        let (cand, _) = apply(None, &req).unwrap();
        assert_eq!(cand.provider_id, None);
    }

    #[test]
    fn apply_clear_overrides_wins_over_field_writes() {
        let stored = OxpilotSettingsRow {
            provider_id: Some("openai".to_string()),
            model: Some("gpt-4.1".to_string()),
            key_ciphertext: Some("ct".to_string()),
            key_last4: Some("wxyz".to_string()),
            enabled_override: Some(true),
            ..Default::default()
        };
        let req = set_req(json!({
            "clear_overrides": true,
            "provider_id": "ollama",
            "enabled": false
        }));
        let (cand, touched) = apply(Some(&stored), &req).unwrap();
        assert_eq!(cand, OxpilotSettingsRow::default());
        assert_eq!(touched, vec!["clear_overrides".to_string()]);
    }

    #[test]
    fn apply_key_conflicts_and_pairs() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::set_var("OXIDEAN_OXPILOT_SECRETS_KEY", "test-secret");
        // api_key + clear_key in one payload ⇒ bad_input (BR3.4).
        let req = set_req(json!({"api_key": "sk-x", "clear_key": true}));
        assert!(apply(None, &req).is_err());
        // whitespace api_key ⇒ bad_input.
        let req = set_req(json!({"api_key": "   "}));
        assert!(apply(None, &req).is_err());
        // api_key encrypts into the pair — plaintext never lands on the row.
        let req = set_req(json!({"api_key": "sk-live-abcdef"}));
        let (cand, _) = apply(None, &req).unwrap();
        let ct = cand.key_ciphertext.as_deref().unwrap();
        assert_ne!(ct, "sk-live-abcdef");
        assert!(!ct.contains("sk-live"));
        assert_eq!(cand.key_last4.as_deref(), Some("cdef"));
        // clear_key erases the pair atomically.
        let stored = OxpilotSettingsRow {
            key_ciphertext: Some("ct".to_string()),
            key_last4: Some("wxyz".to_string()),
            ..Default::default()
        };
        let req = set_req(json!({"clear_key": true}));
        let (cand, _) = apply(Some(&stored), &req).unwrap();
        assert!(cand.key_ciphertext.is_none() && cand.key_last4.is_none());
        std::env::remove_var("OXIDEAN_OXPILOT_SECRETS_KEY");
    }

    #[test]
    fn apply_missing_crypto_source_is_crypto_unavailable() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("OXIDEAN_OXPILOT_SECRETS_KEY");
        std::env::remove_var("OXIDEAN_SESSION_SECRET");
        let req = set_req(json!({"api_key": "sk-x"}));
        let e = apply(None, &req).unwrap_err();
        assert_eq!(e.code, "crypto.unavailable", "{e:?}");
    }

    #[test]
    fn touches_provider_fields_classification() {
        assert!(touches_provider_fields(&set_req(json!({"model": "x"}))));
        assert!(touches_provider_fields(&set_req(
            json!({"clear_key": true})
        )));
        assert!(touches_provider_fields(&set_req(
            json!({"clear_overrides": true})
        )));
        assert!(!touches_provider_fields(&set_req(json!({"enabled": true}))));
        assert!(!touches_provider_fields(&set_req(
            json!({"user_budget": 5, "instance_budget": null})
        )));
        assert!(!touches_provider_fields(&set_req(json!({}))));
    }

    // ---------- resolve_slot ----------

    fn cfg_with(provider: &str, enabled: bool) -> EffectiveProviderConfig {
        let mut e = env();
        e.enabled = enabled;
        e.provider_id = Some(provider.to_string());
        resolve_over_env(None, &e)
    }

    #[test]
    fn resolve_slot_flag_off_carries_only_matching_identity() {
        let prior = prior_slot(true);
        // Matching identity → provider retained under flag_off (NFR4.5).
        let cfg = cfg_with("stub", false);
        match resolve_slot(&cfg, &prior) {
            SlotPlan::Slot(s) => {
                assert!(s.provider.is_some());
                assert_eq!(s.disabled_reason.as_deref(), Some("flag_off"));
                assert!(s.built_from.is_some());
            }
            _ => panic!("expected Slot"),
        }
        // Changed identity → stale provider dropped.
        let mut cfg = cfg_with("stub", false);
        cfg.model = Some("other".to_string());
        match resolve_slot(&cfg, &prior) {
            SlotPlan::Slot(s) => {
                assert!(s.provider.is_none());
                assert_eq!(s.disabled_reason.as_deref(), Some("flag_off"));
                assert!(s.built_from.is_none());
            }
            _ => panic!("expected Slot"),
        }
    }

    #[test]
    fn resolve_slot_enabled_unset_provider_uses_stub_default() {
        // BR2.6 — an unset provider resolves stub; a prior stub slot
        // carries its Arc (same identity, no rebuild).
        let mut e = env();
        e.provider_id = None;
        let cfg = resolve_over_env(None, &e);
        assert!(matches!(
            resolve_slot(&cfg, &prior_slot(true)),
            SlotPlan::CarryProvider
        ));
    }

    #[test]
    fn resolve_slot_enabled_carry_or_build() {
        let prior = prior_slot(true);
        // Same identity → carry, no build (NFR4.3).
        let cfg = cfg_with("stub", true);
        assert!(matches!(
            resolve_slot(&cfg, &prior),
            SlotPlan::CarryProvider
        ));
        // Any identity change → Build.
        let mut cfg = cfg_with("stub", true);
        cfg.api_key_env = None; // KeySource::None ≠ Env → identity change
        match resolve_slot(&cfg, &prior) {
            SlotPlan::Build(id) => assert_eq!(id.provider_id, "stub"),
            _ => panic!("expected Build"),
        }
        // Never-built prior → Build.
        let cfg = cfg_with("stub", true);
        assert!(matches!(
            resolve_slot(&cfg, &prior_slot(false)),
            SlotPlan::Build(_)
        ));
    }

    /// US5.2 — every slot-constructor path stamps `budget_caps` from the
    /// resolved cfg (the C5 amendment): flag-off carry, reason-bearing
    /// disabled, provider carry, fresh build.
    #[test]
    fn budget_caps_stamp_on_every_slot_path() {
        let prior = prior_slot(true);
        let cfg = cfg_with("stub", true); // env() carries budgets 11/22
        let want = BudgetCaps {
            user: Some(11),
            instance: Some(22),
        };

        // flag_off arm.
        let off = cfg_with("stub", false);
        match resolve_slot(&off, &prior) {
            SlotPlan::Slot(s) => assert_eq!(s.budget_caps, want),
            _ => panic!("expected Slot"),
        }
        // enabled with a hand-built providerless cfg → reason-bearing
        // disabled slot (defensive path — resolve_over_env's stub
        // default means this never arrives through real resolution).
        let mut none = cfg_with("stub", true);
        none.provider_id = None;
        match resolve_slot(&none, &prior) {
            SlotPlan::Slot(s) => assert_eq!(s.budget_caps, want),
            _ => panic!("expected Slot"),
        }
        // carried + built.
        assert_eq!(carried_slot(&prior, &cfg).budget_caps, want);
        let built = built_slot(
            prior.turns.clone(),
            Arc::new(StubProvider { fail: false }),
            &cfg,
            cfg.identity().unwrap(),
            prior.tools.clone(),
        );
        assert_eq!(built.budget_caps, want);
    }

    // ---------- validate ----------

    #[tokio::test]
    async fn validate_providerless_configs_pass() {
        let mut e = env();
        e.provider_id = None;
        e.base_url = Some("http://127.0.0.1".to_string()); // env-sourced, write chose nothing
        let cfg = resolve_over_env(None, &e);
        validate(&cfg, &set_req(json!({})), cfg!(debug_assertions))
            .await
            .unwrap();
        // Write-chosen shape-legal base_url passes too.
        validate(
            &cfg,
            &set_req(json!({"base_url": "http://127.0.0.1"})),
            cfg!(debug_assertions),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn validate_key_bearing_cleartext_refused_for_provider_writes() {
        // env() resolves api_key + provider over env; an http://
        // base_url + resolved key must refuse (CWE-319) — but only for
        // writes naming provider fields; a pure budget write must not
        // wedge on the env-sourced combo.
        // compat carries the self-host exemption (LAN http reaches the
        // cleartext bound); anthropic doesn't — only a *public* http
        // literal passes its egress shape check and rides cleartext.
        for (provider, url) in [
            ("openai-compatible", "http://litellm.lan:4000"),
            ("anthropic", "http://8.8.8.8"),
        ] {
            let mut cfg = cfg_with(provider, true);
            cfg.base_url = Some(url.to_string());
            assert!(
                cfg.api_key_env.is_some() || cfg.key_ciphertext.is_some(),
                "fixture must resolve key material"
            );
            let refused = validate(
                &cfg,
                &set_req(json!({"base_url": url})),
                cfg!(debug_assertions),
            )
            .await
            .unwrap_err();
            assert_eq!(refused.code, "invalid_provider", "{provider}");
            assert!(
                refused.message.contains("cleartext"),
                "{provider}: refusal must name the cleartext bound, got: {}",
                refused.message
            );
        }
        let mut cfg = cfg_with("openai-compatible", true);
        cfg.base_url = Some("http://litellm.lan:4000".to_string());
        // Budget-only write is untouched by the env-sourced combo.
        validate(
            &cfg,
            &set_req(json!({"user_budget": 5})),
            cfg!(debug_assertions),
        )
        .await
        .unwrap();
        // Keyless combo stays legal — http:// + no key is the
        // documented self-host shape.
        let mut keyless = cfg.clone();
        keyless.api_key_env = None;
        keyless.key_ciphertext = None;
        validate(
            &keyless,
            &set_req(json!({"base_url": "http://litellm.lan:4000"})),
            cfg!(debug_assertions),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn validate_budgets_must_be_positive() {
        let e = env();
        let cfg = resolve_over_env(None, &e);
        // Write-chosen non-positive budgets refuse.
        assert_eq!(
            validate(
                &cfg,
                &set_req(json!({"user_budget": 0})),
                cfg!(debug_assertions)
            )
            .await
            .unwrap_err()
            .code,
            "rpc.bad_input"
        );
        assert_eq!(
            validate(
                &cfg,
                &set_req(json!({"instance_budget": -3})),
                cfg!(debug_assertions)
            )
            .await
            .unwrap_err()
            .code,
            "rpc.bad_input"
        );
        // An env/stale-row non-positive budget degrades — it never wedges
        // an unrelated write.
        let mut bad_env = env();
        bad_env.user_budget = Some(0);
        let cfg = resolve_over_env(None, &bad_env);
        validate(
            &cfg,
            &set_req(json!({"model": "m"})),
            cfg!(debug_assertions),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn validate_unknown_and_unsettable_providers() {
        let mut e = env();
        e.provider_id = Some("gemini".to_string());
        let cfg = resolve_over_env(None, &e);
        // Env-sourced unknown ids degrade to `misconfigured`, never reject.
        validate(&cfg, &set_req(json!({})), cfg!(debug_assertions))
            .await
            .unwrap();
        // A write-chosen unknown id IS refused.
        assert_eq!(
            validate(
                &cfg,
                &set_req(json!({"provider_id": "gemini"})),
                cfg!(debug_assertions)
            )
            .await
            .unwrap_err()
            .code,
            "invalid_provider"
        );
        // Release-shaped guard: a write-chosen stub refuses outside
        // debug builds (BR1.4); env-stub stays legal (R-02).
        let cfg = cfg_with("stub", true);
        assert_eq!(
            validate(&cfg, &set_req(json!({"provider_id": "stub"})), false)
                .await
                .unwrap_err()
                .code,
            "invalid_provider"
        );
        validate(&cfg, &set_req(json!({})), false).await.unwrap(); // env-resolved stub: no wedge
                                                                   // Debug-shaped accepts stub.
        validate(&cfg, &set_req(json!({"provider_id": "stub"})), true)
            .await
            .unwrap();
        assert!(!provider_settable("stub", false));
        assert!(provider_settable("stub", true));
    }

    #[tokio::test]
    async fn validate_provider_switch_rechecks_stored_model() {
        // R-05 regression: `{provider_id:"openai"}` alone with a stored
        // anthropic model must refuse — the merge binds the RESOLVED
        // model whenever provider_id or model is named. `cfg` mirrors
        // the APPLIED candidate (production calls validate post-apply).
        let row = OxpilotSettingsRow {
            provider_id: Some("openai".to_string()),
            model: Some("claude-sonnet-4-5".to_string()),
            ..OxpilotSettingsRow::default()
        };
        let cfg = resolve_over_env(Some(&row), &env());
        assert_eq!(
            validate(&cfg, &set_req(json!({"provider_id": "openai"})), true)
                .await
                .unwrap_err()
                .code,
            "invalid_provider"
        );
        // Same-provider write keeps the stored model — no refusal.
        let row2 = OxpilotSettingsRow {
            provider_id: Some("anthropic".to_string()),
            model: Some("claude-sonnet-4-5".to_string()),
            ..OxpilotSettingsRow::default()
        };
        let cfg2 = resolve_over_env(Some(&row2), &env());
        validate(&cfg2, &set_req(json!({"provider_id": "anthropic"})), true)
            .await
            .unwrap();
        // Unrelated writes never re-check the stored model — even an
        // inconsistent stored pair degrades rather than wedging.
        let row3 = OxpilotSettingsRow {
            provider_id: Some("openai".to_string()),
            model: Some("claude-sonnet-4-5".to_string()),
            ..OxpilotSettingsRow::default()
        };
        let cfg3 = resolve_over_env(Some(&row3), &env());
        validate(&cfg3, &set_req(json!({"user_budget": 3})), true)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn validate_flag_off_writes_unwedged_by_bad_provider() {
        // The kill switch is un-wedgeable: `{enabled:false}` alone
        // resolves a `flag_off` slot even when the env/stale row carries
        // an invalid provider — the write chose none (BR3.1 exemption).
        let cfg = cfg_with("bogus-provider", false);
        validate(&cfg, &set_req(json!({"enabled": false})), false)
            .await
            .unwrap();
        // But a provider_id the write DOES choose is always refused.
        let cfg = cfg_with("openai", false);
        assert_eq!(
            validate(
                &cfg,
                &set_req(json!({"enabled": false, "provider_id": "bogus-provider"})),
                cfg!(debug_assertions)
            )
            .await
            .unwrap_err()
            .code,
            "invalid_provider"
        );
    }

    #[tokio::test]
    async fn validate_model_and_base_url() {
        // Unadvertised model on a list-having provider ⇒ invalid_provider.
        let mut e = env();
        e.provider_id = Some("openai".to_string());
        // Write-chosen unadvertised model on a list-having provider —
        // cfg mirrors the applied candidate (model = the write's pick).
        e.model = Some("not-a-model".to_string());
        let cfg = resolve_over_env(None, &e);
        assert_eq!(
            validate(
                &cfg,
                &set_req(json!({"model": "not-a-model"})),
                cfg!(debug_assertions)
            )
            .await
            .unwrap_err()
            .code,
            "invalid_provider"
        );
        // Advertised model passes (key/egress may still bind elsewhere —
        // this cfg is otherwise clean).
        e.model = Some("gpt-4o".to_string());
        let cfg = resolve_over_env(None, &e);
        validate(
            &cfg,
            &set_req(json!({"model": "gpt-4o"})),
            cfg!(debug_assertions),
        )
        .await
        .unwrap();
        // ollama with no model and no advertised list is legal — the
        // slot just keeps model None (call-time concern, not config).
        e.provider_id = Some("ollama".to_string());
        e.model = None;
        let cfg = resolve_over_env(None, &e);
        validate(&cfg, &set_req(json!({})), cfg!(debug_assertions))
            .await
            .unwrap();
        // Write-chosen non-http base_url ⇒ invalid_provider even
        // provider-less.
        e.provider_id = None;
        let cfg = resolve_over_env(None, &e);
        assert_eq!(
            validate(
                &cfg,
                &set_req(json!({"base_url": "ftp://x"})),
                cfg!(debug_assertions)
            )
            .await
            .unwrap_err()
            .code,
            "invalid_provider"
        );
    }

    #[tokio::test]
    async fn validate_ssrf_binds_on_resolved_provider() {
        let mut e = env();
        e.provider_id = Some("openai".to_string());
        e.model = Some("gpt-4o".to_string());
        let cfg = resolve_over_env(None, &e);
        assert_eq!(
            validate(
                &cfg,
                &set_req(json!({"base_url": "http://169.254.169.254/"})),
                cfg!(debug_assertions)
            )
            .await
            .unwrap_err()
            .code,
            "invalid_provider"
        );
        // Same URL is legal for ollama (the blessed self-host exception).
        e.provider_id = Some("ollama".to_string());
        e.model = Some("llama3.1".to_string());
        let cfg = resolve_over_env(None, &e);
        validate(
            &cfg,
            &set_req(json!({"base_url": "http://169.254.169.254/"})),
            cfg!(debug_assertions),
        )
        .await
        .unwrap();
    }
}
