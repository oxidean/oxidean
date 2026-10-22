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
use base64::Engine as _;
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

// ---- libsodium `crypto_box_seal` receiver ----------------------------------
//
// GitHub's secrets endpoints serve `{key_id, key}` (an X25519 public key) and
// clients sealed-box encrypt the value client-side before PUT — `gh secret
// set`, `actions/github-script`, and web forms all rely on it. The keypair is
// derived deterministically from the same env key material used for at-rest
// encryption plus a scope tag, so repo/env/org surfaces get distinct keys
// without storing anything new, and the pair rotates whenever the operator
// rotates the at-rest key.

/// Deterministic X25519 secret key for `env_keys` + `scope`.
fn seal_secret_key(env_keys: &[&str], scope: &str) -> Result<crypto_box::SecretKey, String> {
    let base = key_bytes(env_keys)?;
    let mut h = Sha256::new();
    h.update(b"oxidean-secrets-seal-v1:");
    h.update(scope.as_bytes());
    h.update(base);
    Ok(crypto_box::SecretKey::from(<[u8; 32]>::from(h.finalize())))
}

/// `key_id` for a scope's derived key — stable while the key material is.
fn seal_key_id(env_keys: &[&str], scope: &str) -> Result<String, String> {
    let pk = seal_secret_key(env_keys, scope)?.public_key();
    let mut h = Sha256::new();
    h.update(b"oxidean-secrets-keyid-v1:");
    h.update(pk.as_bytes());
    Ok(hex_encode(&h.finalize()[..16]))
}

/// `GET …/secrets/public-key` payload: `{key_id, key}` (base64 X25519).
pub fn seal_public_key(env_keys: &[&str], scope: &str) -> Result<(String, String), String> {
    let pk = seal_secret_key(env_keys, scope)?.public_key();
    let key = base64::engine::general_purpose::STANDARD.encode(pk.as_bytes());
    Ok((seal_key_id(env_keys, scope)?, key))
}

/// Open a base64 `crypto_box_seal` blob against a scope's derived key.
/// Returns the UTF-8 plaintext.
pub fn seal_open(env_keys: &[&str], scope: &str, sealed_b64: &str) -> Result<String, String> {
    let sealed = base64::engine::general_purpose::STANDARD
        .decode(sealed_b64.trim())
        .map_err(|e| format!("bad base64 sealed box: {e}"))?;
    let sk = seal_secret_key(env_keys, scope)?;
    let pt = sk
        .unseal(&sealed)
        .map_err(|_| "sealed box did not open".to_string())?;
    String::from_utf8(pt).map_err(|e| e.to_string())
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
        let ct = encrypt(keys, "data").unwrap();
        // Flip a bit in the ciphertext, not a fixed overwrite — `hex(nonce ||
        // ciphertext)` means a constant write is a no-op ~1/256 of the time
        // (the byte already holds it), which read as "tamper accepted" and
        // flaked this assertion.
        let mut raw = hex_decode(&ct).unwrap();
        raw[12] ^= 0x01;
        let ct = hex_encode(&raw);
        assert!(decrypt(keys, &ct).is_err());
        std::env::remove_var("OXIDEAN_TEST_SECRET_KEY_TAMPER");
    }

    #[test]
    fn sealed_box_roundtrip_per_scope() {
        std::env::set_var("OXIDEAN_TEST_SECRET_KEY_SEAL", "crypto-unit-key");
        let keys = &["OXIDEAN_TEST_SECRET_KEY_SEAL"];

        let (key_id, key_b64) = seal_public_key(keys, "repo:r1").unwrap();
        assert_eq!(key_id.len(), 32);
        let pk_raw = base64::engine::general_purpose::STANDARD
            .decode(&key_b64)
            .unwrap();
        let pk = crypto_box::PublicKey::try_from(pk_raw.as_slice()).unwrap();
        let sealed = pk
            .seal(&mut crypto_box::aead::OsRng, b"hunter2")
            .expect("seal");
        let sealed_b64 = base64::engine::general_purpose::STANDARD.encode(&sealed);
        assert_eq!(seal_open(keys, "repo:r1", &sealed_b64).unwrap(), "hunter2");

        // Distinct scopes derive distinct keypairs — a repo box does not
        // open under the environment scope.
        assert!(seal_open(keys, "env:r1:prod", &sealed_b64).is_err());
        let (other_id, other_key) = seal_public_key(keys, "org:acme").unwrap();
        assert_ne!(key_id, other_id);
        assert_ne!(key_b64, other_key);
        std::env::remove_var("OXIDEAN_TEST_SECRET_KEY_SEAL");
    }
}
