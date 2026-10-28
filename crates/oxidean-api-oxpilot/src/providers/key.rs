//! Provider-key custody (BR1.2, NFR1.4–1.6): write-only — the settings
//! write path encrypts once and stores `hex(nonce||ciphertext)`+`last4`;
//! provider CONSTRUCTION is the only code that ever opens the blob.
//!
//! `decrypt_key` is `pub(in crate::providers)` on purpose: `settings.rs`
//! is the same crate, so `pub(crate)` would not enforce the boundary —
//! a read-path decrypt is a compile failure, not a remembered rule.
//!
//! Key source chain: `OXIDEAN_OXPILOT_SECRETS_KEY` →
//! `OXIDEAN_SESSION_SECRET` (shared api-core crypto helper; SHA-256 of
//! the first set var). A missing source fails closed — callers map the
//! error to `crypto.unavailable` (BR5.1).

const KEY_ENVS: &[&str] = &["OXIDEAN_OXPILOT_SECRETS_KEY", "OXIDEAN_SESSION_SECRET"];

/// Encrypt a plaintext API key → `(ciphertext_hex, last4)` — the pair
/// always travels together into the same row write (NFR1.5).
pub(crate) fn encrypt_key(plain: &str) -> Result<(String, String), String> {
    let ciphertext = oxidean_api_core::crypto::encrypt(KEY_ENVS, plain)?;
    Ok((ciphertext, key_last4(plain)))
}

/// Open a stored ciphertext — CONSTRUCTION PATH ONLY
/// (`providers::build`). Visibility is the custody boundary.
pub(in crate::providers) fn decrypt_key(ciphertext: &str) -> Result<String, String> {
    oxidean_api_core::crypto::decrypt(KEY_ENVS, ciphertext)
}

/// Trailing-4 display hint — the ONLY key material a read may surface.
/// A ≤4-char plaintext would expose the whole secret through this hint,
/// so it collapses to a fixed mask instead.
pub(crate) fn key_last4(plain: &str) -> String {
    let chars: Vec<char> = plain.chars().collect();
    if chars.len() <= 4 {
        return "····".to_string();
    }
    chars[chars.len() - 4..].iter().collect()
}

/// Resolve construction-time plaintext for an adapter — stored
/// ciphertext first (decrypted in place), else the env var. `Ok(None)`
/// = no material exists at all (keyless adapters don't care; keyed
/// adapters turn it into `invalid_provider`). A present-but-undecryptable
/// ciphertext is a REAL failure (`invalid_provider` at write,
/// `misconfigured` at boot), never a silent fall-through to env — the
/// stored key is meant to override it.
pub(in crate::providers) fn resolve_material(
    ciphertext: Option<&str>,
    env_key: Option<&str>,
) -> Result<Option<String>, oxidean_api_core::oxpilot::ProviderError> {
    use oxidean_api_core::oxpilot::ProviderError;
    match ciphertext {
        Some(ct) => decrypt_key(ct).map(Some).map_err(|e| {
            if e.contains("is required to encrypt") {
                ProviderError {
                    code: "crypto.unavailable".to_string(),
                    message: e,
                }
            } else {
                ProviderError {
                    code: "invalid_provider".to_string(),
                    message: "stored API key cannot be decrypted".to_string(),
                }
            }
        }),
        None => Ok(env_key.map(str::to_string).filter(|k| !k.trim().is_empty())),
    }
}

/// Does the row/env pair resolve key material WITHOUT decrypting?
/// (`providers_list.configured` — presence check only, NFR1.14.)
pub(crate) fn key_material_resolvable(ciphertext: Option<&str>, env_key: Option<&str>) -> bool {
    ciphertext.is_some_and(|c| !c.is_empty()) || env_key.is_some_and(|k| !k.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env mutation is process-global — serialize the custody tests.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvGuard;
    impl EnvGuard {
        fn acquire() -> (std::sync::MutexGuard<'static, ()>, Self) {
            let g = ENV_LOCK.lock().unwrap();
            (g, Self)
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for k in KEY_ENVS {
                std::env::remove_var(k);
            }
        }
    }

    #[test]
    fn key_last4_masks_short_keys() {
        assert_eq!(key_last4("abcd"), "····");
        assert_eq!(key_last4("x"), "····");
        assert_eq!(key_last4("sk-12345"), "2345");
        assert_eq!(key_last4("key-abcdefgh"), "efgh");
    }

    #[test]
    fn custody_roundtrip_encrypt_decrypt_last4() {
        let (_lock, _env) = EnvGuard::acquire();
        std::env::set_var("OXIDEAN_OXPILOT_SECRETS_KEY", "unit-test-secret");
        let (ct, last4) = encrypt_key("sk-live-abcdef").unwrap();
        assert_ne!(ct, "sk-live-abcdef");
        assert!(!ct.contains("sk-live"));
        assert_eq!(last4, "cdef");
        assert_eq!(decrypt_key(&ct).unwrap(), "sk-live-abcdef");
    }

    #[test]
    fn missing_key_source_fails_closed() {
        let (_lock, _env) = EnvGuard::acquire();
        let err = encrypt_key("sk-x").unwrap_err();
        assert!(err.contains("is required to encrypt"), "{err}");
    }

    #[test]
    fn resolve_material_prefers_db_then_env() {
        let (_lock, _env) = EnvGuard::acquire();
        std::env::set_var("OXIDEAN_SESSION_SECRET", "fallback-secret");
        let (ct, _) = encrypt_key("db-key-material").unwrap();
        // Stored ciphertext wins over env plaintext.
        assert_eq!(
            resolve_material(Some(&ct), Some("env-key")).unwrap(),
            Some("db-key-material".to_string())
        );
        // No ciphertext → env plaintext passes through.
        assert_eq!(
            resolve_material(None, Some("env-key")).unwrap(),
            Some("env-key".to_string())
        );
        // Nothing configured → Ok(None), never a decrypt attempt.
        assert_eq!(resolve_material(None, None).unwrap(), None);
        // Corrupt ciphertext is a REAL failure (invalid_provider), not a
        // silent fall-through to env.
        let e = resolve_material(Some("deadbeef"), Some("env-key")).unwrap_err();
        assert_eq!(e.code, "invalid_provider", "{e:?}");
    }
}
