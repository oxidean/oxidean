//! Repo Actions secrets crypto (D-ACT-17) — AES-256-GCM at rest; never echo plaintext on GET.

use std::collections::HashMap;

use aes_gcm::aead::consts::U12;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use oxidean_db::Database;
use sha2::{Digest, Sha256};

fn secrets_key_bytes() -> Result<[u8; 32], String> {
    let raw = std::env::var("OXIDEAN_ACTIONS_SECRETS_KEY")
        .or_else(|_| std::env::var("OXIDEAN_SESSION_SECRET"))
        .map_err(|_| {
            "OXIDEAN_ACTIONS_SECRETS_KEY (or OXIDEAN_SESSION_SECRET) is required to encrypt secrets at rest"
                .to_string()
        })?;
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    Ok(h.finalize().into())
}

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
    let key = Aes256Gcm::new_from_slice(&secrets_key_bytes()?).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 12];
    getrandom::fill(&mut nonce_bytes).map_err(|e| e.to_string())?;
    let nonce: &Nonce<U12> = nonce_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "invalid nonce length")?;
    let ct = key
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(hex_encode(&out))
}

pub fn decrypt_secret(blob: &str) -> Result<String, String> {
    let raw = hex_decode(blob)?;
    if raw.len() < 13 {
        return Err("ciphertext too short".into());
    }
    let (nonce_bytes, ct) = raw.split_at(12);
    let key = Aes256Gcm::new_from_slice(&secrets_key_bytes()?).map_err(|e| e.to_string())?;
    let nonce: &Nonce<U12> = nonce_bytes.try_into().map_err(|_| "invalid nonce length")?;
    let pt = key.decrypt(nonce, ct).map_err(|e| e.to_string())?;
    String::from_utf8(pt).map_err(|e| e.to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("bad hex ciphertext".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
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

    #[test]
    fn encrypt_roundtrip() {
        std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "unit-test-key");
        let ct = encrypt_secret("super-secret").unwrap();
        assert!(!ct.contains("super-secret"));
        assert_eq!(decrypt_secret(&ct).unwrap(), "super-secret");
    }

    #[test]
    fn encrypt_fails_closed_without_key() {
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
