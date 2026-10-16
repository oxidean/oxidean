//! Shared at-rest secret box: AES-256-GCM keyed by an env-derived key
//! (SHA-256 of the first set variable from a caller-chosen preference list),
//! random 12-byte nonce, `hex(nonce || ciphertext)` storage form.
//!
//! Callers pass the env preference chain so each domain keeps its own key
//! (`OXIDEAN_ACTIONS_SECRETS_KEY`, `OXIDEAN_SECRETS_KEY`, …) while all fall
//! back to `OXIDEAN_SESSION_SECRET` for simple deployments. Resolution is
//! deterministic — changing env configuration invalidates old ciphertexts,
//! same contract as `actions::secrets` today.

use aes_gcm::aead::consts::U12;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use sha2::{Digest, Sha256};

/// Resolve the 32-byte key from the first set var in `env_keys`.
/// Fails closed listing every accepted variable when none is configured.
fn key_bytes(env_keys: &[&str]) -> Result<[u8; 32], String> {
    let raw = env_keys
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .ok_or_else(|| {
            format!(
                "{} is required to encrypt secrets at rest",
                env_keys.join(" (or ") + ")"
            )
        })?;
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    Ok(h.finalize().into())
}

/// Encrypt UTF-8 plaintext → `hex(nonce || ciphertext)`.
pub fn encrypt(env_keys: &[&str], plaintext: &str) -> Result<String, String> {
    let key = Aes256Gcm::new_from_slice(&key_bytes(env_keys)?).map_err(|e| e.to_string())?;
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

/// Decrypt a `hex(nonce || ciphertext)` blob → UTF-8 plaintext.
pub fn decrypt(env_keys: &[&str], blob: &str) -> Result<String, String> {
    let raw = hex_decode(blob)?;
    if raw.len() < 13 {
        return Err("ciphertext too short".into());
    }
    let (nonce_bytes, ct) = raw.split_at(12);
    let key = Aes256Gcm::new_from_slice(&key_bytes(env_keys)?).map_err(|e| e.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    // Env is process-global — each test gets its own var so parallel runs in
    // one binary can never observe the other's remove.
    #[test]
    fn encrypt_decrypt_roundtrip() {
        std::env::set_var("OXIDEAN_TEST_SECRET_KEY_ROUNDTRIP", "crypto-unit-key");
        let keys = &["OXIDEAN_TEST_SECRET_KEY_ROUNDTRIP"];
        let ct = encrypt(keys, "plain").unwrap();
        assert!(!ct.contains("plain"));
        assert_eq!(decrypt(keys, &ct).unwrap(), "plain");
        std::env::remove_var("OXIDEAN_TEST_SECRET_KEY_ROUNDTRIP");
    }

    #[test]
    fn decrypt_rejects_tampered() {
        std::env::set_var("OXIDEAN_TEST_SECRET_KEY_TAMPER", "crypto-unit-key");
        let keys = &["OXIDEAN_TEST_SECRET_KEY_TAMPER"];
        let mut ct = encrypt(keys, "data").unwrap();
        ct.replace_range(0..2, "ff");
        assert!(decrypt(keys, &ct).is_err());
        std::env::remove_var("OXIDEAN_TEST_SECRET_KEY_TAMPER");
    }
}
