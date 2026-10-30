//! Provider registry (BR1.1, BR1.4) — a CLOSED static set. `REGISTRY`
//! is the admin-facing surface (exactly `openai`, `anthropic`, `ollama`
//! — `providers_list` emits it verbatim); `RESOLUTION_MAP` adds the
//! internal `stub` so `OXIDEAN_OXPILOT_PROVIDER=stub` keeps building the
//! U1 provider on env paths while `settings_set` refuses it (release
//! guard lives in `settings::validate`).

use std::sync::Arc;

use oxidean_api_core::oxpilot::{LlmProvider, ProviderError, StubProvider};

use crate::settings::EffectiveProviderConfig;

/// Static registry metadata for one adapter — what `providers_list`
/// returns per entry. `configured` is computed at read time
/// (`key::key_material_resolvable`); a descriptor never holds key
/// material.
pub struct ProviderDescriptor {
    pub id: &'static str,
    /// Advertised model ids — an EMPTY list advertises "any model id"
    /// (ollama serves whatever the operator pulled; stub takes the env
    /// model verbatim), so validation binds only when non-empty (BR3.1).
    pub models: &'static [&'static str],
    /// `false` only for `ollama` + `stub` (BR4.4).
    pub requires_key: bool,
}

/// The admin-facing registry — order is the `providers_list` order.
pub const REGISTRY: &[ProviderDescriptor] = &[
    ProviderDescriptor {
        id: "openai",
        models: &[
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4o",
            "gpt-4o-mini",
            "o4-mini",
        ],
        requires_key: true,
    },
    ProviderDescriptor {
        id: "anthropic",
        models: &[
            "claude-sonnet-4-5",
            "claude-haiku-4-5",
            "claude-opus-4-1",
            "claude-sonnet-4-0",
        ],
        requires_key: true,
    },
    ProviderDescriptor {
        id: "ollama",
        // Operator-pulled models can't be enumerated statically — the
        // descriptor deliberately advertises no list (BR3.1).
        models: &[],
        requires_key: false,
    },
    ProviderDescriptor {
        id: "openai-compatible",
        // LiteLLM / vLLM / NIM-style endpoints — model ids are the
        // serving server's own names, unknowable statically (BR3.1).
        // The API key is optional: many LAN proxies take none.
        models: &[],
        requires_key: false,
    },
];

/// Internal resolution — `stub` is buildable from env (U1 parity) but
/// never listed or settable through `settings_set` (BR1.4).
const STUB_DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "stub",
    models: &[],
    requires_key: false,
};

/// Look up a provider id in the FULL resolution set (registry + stub).
pub fn resolve_descriptor(id: &str) -> Option<&'static ProviderDescriptor> {
    REGISTRY
        .iter()
        .find(|d| d.id == id)
        .or((id == STUB_DESCRIPTOR.id).then_some(&STUB_DESCRIPTOR))
}

/// Construct the adapter a resolved config selects (BR1.3). Callers
/// guarantee `cfg.provider_id` names a resolvable id — anything else is
/// `invalid_provider` (defensive; `validate` normally catches it first).
pub async fn build(cfg: &EffectiveProviderConfig) -> Result<Arc<dyn LlmProvider>, ProviderError> {
    match cfg.provider_id.as_deref() {
        Some("openai") => super::openai::build(cfg).await,
        Some("anthropic") => super::anthropic::build(cfg).await,
        Some("ollama") => super::ollama::build(cfg).await,
        Some("openai-compatible") => super::openai::build_compat(cfg).await,
        // U1 parity — the env path's canned provider; not reachable via
        // settings_set (release-shaped guard in `settings::validate`).
        Some("stub") => Ok(Arc::new(StubProvider { fail: false })),
        Some(other) => Err(ProviderError {
            code: "invalid_provider".to_string(),
            message: format!("unknown provider `{other}`"),
        }),
        None => Err(ProviderError {
            code: "invalid_provider".to_string(),
            message: "no provider resolved".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::EffectiveProviderConfig;

    #[test]
    fn registry_is_exactly_the_admin_surface() {
        let ids: Vec<&str> = REGISTRY.iter().map(|d| d.id).collect();
        assert_eq!(ids, ["openai", "anthropic", "ollama", "openai-compatible"]);
    }

    #[test]
    fn resolution_map_adds_stub_but_never_lists_it() {
        assert!(resolve_descriptor("stub").is_some());
        assert!(REGISTRY.iter().all(|d| d.id != "stub"));
        for id in ["openai", "anthropic", "ollama", "openai-compatible", "stub"] {
            assert!(resolve_descriptor(id).is_some(), "{id}");
        }
        assert!(resolve_descriptor("gemini").is_none());
        // Only ollama + stub + the compat endpoint are keyless (BR4.4).
        assert!(resolve_descriptor("openai").unwrap().requires_key);
        assert!(resolve_descriptor("anthropic").unwrap().requires_key);
        assert!(!resolve_descriptor("ollama").unwrap().requires_key);
        assert!(
            !resolve_descriptor("openai-compatible")
                .unwrap()
                .requires_key
        );
        assert!(!resolve_descriptor("stub").unwrap().requires_key);
    }

    #[tokio::test]
    async fn build_stub_returns_the_u1_provider() {
        let cfg = EffectiveProviderConfig {
            enabled: true,
            provider_id: Some("stub".to_string()),
            model: Some("stub-1".to_string()),
            ..Default::default()
        };
        let p = build(&cfg).await.unwrap();
        assert_eq!(p.id(), "stub");
    }

    #[tokio::test]
    async fn build_compat_accepts_private_url_without_key() {
        // LiteLLM-style LAN endpoint: key optional, private target exempt
        // from the public-egress bound (same trust level as ollama).
        let cfg = EffectiveProviderConfig {
            enabled: true,
            provider_id: Some("openai-compatible".to_string()),
            model: Some("llama-3.3-70b".to_string()),
            base_url: Some("http://10.0.0.9:4000".to_string()),
            ..Default::default()
        };
        let p = build(&cfg).await.expect("compat provider must build");
        assert_eq!(p.id(), "openai");
    }

    #[tokio::test]
    async fn build_compat_requires_base_url() {
        let cfg = EffectiveProviderConfig {
            enabled: true,
            provider_id: Some("openai-compatible".to_string()),
            ..Default::default()
        };
        let e = build(&cfg)
            .await
            .err()
            .expect("compat without url must fail");
        assert_eq!(e.code, "invalid_provider");
    }

    #[tokio::test]
    async fn build_openai_still_requires_key() {
        let cfg = EffectiveProviderConfig {
            enabled: true,
            provider_id: Some("openai".to_string()),
            ..Default::default()
        };
        let e = build(&cfg)
            .await
            .err()
            .expect("openai without key must fail");
        assert_eq!(e.code, "invalid_provider");
    }

    #[tokio::test]
    async fn build_unknown_and_absent_reject() {
        let cfg = EffectiveProviderConfig {
            enabled: true,
            provider_id: Some("gemini".to_string()),
            ..Default::default()
        };
        let e = build(&cfg).await.err().expect("gemini must fail");
        assert_eq!(e.code, "invalid_provider");
        let cfg = EffectiveProviderConfig {
            enabled: true,
            ..Default::default()
        };
        let e = build(&cfg).await.err().expect("absent provider must fail");
        assert_eq!(e.code, "invalid_provider");
    }
}
