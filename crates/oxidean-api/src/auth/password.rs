//! Argon2id password hashing helpers (PHC strings). Never log password or hash.

use argon2::{
    password_hash::{
        phc::{Salt, SaltString},
        PasswordHasher, PasswordVerifier,
    },
    Argon2,
};

/// Minimum password length enforced by [`hash_password_str`]. Callers of
/// [`hash_password`] should enforce the same minimum (≥ 8 characters).
pub const MIN_PASSWORD_LEN: usize = 8;

/// Hash a password with Argon2id (default params) to a PHC string.
///
/// Callers must enforce a minimum length of [`MIN_PASSWORD_LEN`] (or use
/// [`hash_password_str`], which rejects shorter passwords).
pub fn hash_password(password: &[u8]) -> Result<String, password_hash::Error> {
    let salt = Salt::from(SaltString::generate());
    Ok(Argon2::default()
        .hash_password_with_salt(password, salt.as_ref())?
        .to_string())
}

/// Hash a UTF-8 password string, rejecting passwords shorter than [`MIN_PASSWORD_LEN`].
pub fn hash_password_str(password: &str) -> Result<String, PasswordError> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(PasswordError::TooShort);
    }
    hash_password(password.as_bytes()).map_err(PasswordError::Hash)
}

/// Verify a password against a stored Argon2id PHC hash.
///
/// Returns `false` for malformed hashes or mismatches. Never panics.
pub fn verify_password(password: &[u8], password_hash: &str) -> bool {
    Argon2::default()
        .verify_password(password, password_hash)
        .is_ok()
}

/// Errors from the string wrapper around Argon2 hashing.
#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("password must be at least {MIN_PASSWORD_LEN} characters")]
    TooShort,
    #[error("password hash failed: {0}")]
    Hash(password_hash::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_true() {
        let hash = hash_password(b"correct-horse").expect("hash");
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(b"correct-horse", &hash));
    }

    #[test]
    fn verify_wrong_password_false() {
        let hash = hash_password(b"correct-horse").expect("hash");
        assert!(!verify_password(b"wrong-password", &hash));
    }

    #[test]
    fn verify_garbage_hash_false() {
        assert!(!verify_password(b"anything", "not-a-valid-phc"));
        assert!(!verify_password(
            b"anything",
            "$argon2id$v=19$m=65536,t=2,p=1$aaa$bbb"
        ));
    }

    #[test]
    fn hash_password_str_rejects_short() {
        assert!(matches!(
            hash_password_str("short"),
            Err(PasswordError::TooShort)
        ));
        let ok = hash_password_str("longenough").expect("min length 8");
        assert!(verify_password(b"longenough", &ok));
    }
}
