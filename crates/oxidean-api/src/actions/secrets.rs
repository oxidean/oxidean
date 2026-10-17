//! Repo Actions secrets crypto (D-ACT-17) — AES-256-GCM at rest; never echo plaintext on GET.

use std::collections::HashMap;

use oxidean_db::Database;

/// Actions secret key chain — dedicated var first, session secret fallback.
const KEY_ENVS: &[&str] = &["OXIDEAN_ACTIONS_SECRETS_KEY", "OXIDEAN_SESSION_SECRET"];

/// Validate Actions secret name (alphanumeric + `_`).
pub fn validate_secret_name(name: &str) -> Result<(), String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("secret name is required".into());
    }
    if n.len() > 128 {
        return Err("secret name too long".into());
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("secret name must be alphanumeric, underscore, or hyphen".into());
    }
    Ok(())
}

pub fn encrypt_secret(plaintext: &str) -> Result<String, String> {
    crate::crypto::encrypt(KEY_ENVS, plaintext)
}

pub fn decrypt_secret(blob: &str) -> Result<String, String> {
    crate::crypto::decrypt(KEY_ENVS, blob)
}

/// Decrypt all secrets for a repo (FetchTask injection).
pub async fn decrypted_secrets_for_repo(
    db: &Database,
    repository_id: &str,
) -> Result<HashMap<String, String>, String> {
    let rows = db.list_action_secret_ciphertexts(repository_id).await?;
    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        out.insert(row.name, decrypt_secret(&row.ciphertext)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env-var-mutating tests serialize on this — `set_var`/`remove_var` are
    /// process-global and parallel test threads otherwise race each other.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn encrypt_roundtrip() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "unit-test-key");
        let ct = encrypt_secret("super-secret").unwrap();
        assert!(!ct.contains("super-secret"));
        assert_eq!(decrypt_secret(&ct).unwrap(), "super-secret");
    }

    #[test]
    fn encrypt_fails_closed_without_key() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("OXIDEAN_ACTIONS_SECRETS_KEY");
        std::env::remove_var("OXIDEAN_SESSION_SECRET");
        let err = encrypt_secret("x").unwrap_err();
        assert!(err.contains("OXIDEAN_ACTIONS_SECRETS_KEY"), "{err}");
    }

    #[test]
    fn validate_name_rejects_empty() {
        assert!(validate_secret_name("").is_err());
        assert!(validate_secret_name("API_TOKEN").is_ok());
    }
}
