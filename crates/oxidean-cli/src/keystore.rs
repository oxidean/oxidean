//! Credential storage — OS keyring when available, `config.json` fallback.
//!
//! Modeled on `gh`: tokens live in the platform secret store (macOS
//! Keychain, Windows Credential Manager, Secret Service on Linux) under
//! `ox/{instance}`. Where no backend exists — headless Linux is the common
//! case — the token stays in `config.json` (0600) exactly as before and the
//! entry records `secret_store: "file"` so `auth status` reports where the
//! credential actually lives instead of guessing.

use crate::config::InstanceEntry;

pub const SERVICE_PREFIX: &str = "ox";

/// Where a stored credential physically lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Keyring,
    File,
}

impl Store {
    pub fn label(self) -> &'static str {
        match self {
            Self::Keyring => "keyring",
            Self::File => "file",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "keyring" => Self::Keyring,
            _ => Self::File,
        }
    }
}

fn account(instance: &str) -> String {
    format!("{SERVICE_PREFIX}/{instance}")
}

/// Keyring entry for one instance. `user` is fixed (`token`) so every
/// `ox/{instance}` service holds a single credential.
fn entry(instance: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(&account(instance), "token")
        .map_err(|e| format!("keyring unavailable: {e}"))
}

/// Try the keyring: returns the token when one is stored there.
pub fn get(instance: &str) -> Option<String> {
    entry(instance).ok()?.get_password().ok()
}

/// Store `token` in the keyring. `Err` when no backend — caller falls back
/// to the config file.
pub fn set(instance: &str, token: &str) -> Result<(), String> {
    entry(instance)?
        .set_password(token)
        .map_err(|e| format!("keyring store failed: {e}"))
}

/// Remove a keyring-stored token; missing entries are not an error.
pub fn delete(instance: &str) {
    if let Ok(e) = entry(instance) {
        let _ = e.delete_credential();
    }
}

/// Resolve the effective token for `entry`: keyring when the entry says the
/// secret lives there (or when the file field is empty — older writes),
/// else the file field. Never returns the file field *and* a stale keyring
/// copy ambiguously — `secret_store` is authoritative when recorded.
pub fn resolve(instance: &str, entry_cfg: &InstanceEntry) -> Option<String> {
    match entry_cfg.secret_store.as_deref().map(Store::parse) {
        Some(Store::Keyring) => get(instance).or_else(|| entry_cfg.token.clone()),
        Some(Store::File) => entry_cfg.token.clone(),
        // Legacy entry with no marker: prefer keyring, fall back to file.
        None => get(instance).or_else(|| entry_cfg.token.clone()),
    }
}

/// Which store `resolve` would draw from for this entry. Mirrors `resolve`'s
/// priority so `auth status` reports where the credential actually lives —
/// legacy entries (no marker) probe the keyring, else the file field.
pub fn store_used(instance: &str, entry_cfg: &InstanceEntry) -> &'static str {
    match entry_cfg.secret_store.as_deref().map(Store::parse) {
        Some(s) => s.label(),
        None => {
            if get(instance).is_some() {
                Store::Keyring.label()
            } else if entry_cfg.token.is_some() {
                Store::File.label()
            } else {
                "none"
            }
        }
    }
}

/// Persist `token` for `instance`, returning the store actually used and an
/// updated `InstanceEntry` for the config file. Keyring first; file fallback
/// keeps headless-Linux logins working unchanged.
pub fn persist(instance: &str, token: &str) -> InstanceEntry {
    match set(instance, token) {
        Ok(()) => InstanceEntry {
            token: None,
            secret_store: Some(Store::Keyring.label().to_string()),
        },
        Err(_) => InstanceEntry {
            token: Some(token.to_string()),
            secret_store: Some(Store::File.label().to_string()),
        },
    }
}

/// Drop every stored copy (keyring + file marker) for `instance`.
pub fn remove(instance: &str) {
    delete(instance);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InstanceEntry;

    #[test]
    fn file_entry_resolves_file_token() {
        let e = InstanceEntry {
            token: Some("t".into()),
            secret_store: Some("file".into()),
        };
        assert_eq!(resolve("https://nope.invalid", &e).as_deref(), Some("t"));
    }

    #[test]
    fn store_label_roundtrip() {
        assert_eq!(Store::parse("keyring"), Store::Keyring);
        assert_eq!(Store::parse("file"), Store::File);
        assert_eq!(Store::parse("anything-else"), Store::File);
    }
}
