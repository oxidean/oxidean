//! Config persistence and instance/token resolution.
//!
//! Resolution order (highest wins):
//! - instance: `--instance` flag → `OXIDEAN_INSTANCE` env → config default
//! - token:    `OXIDEAN_TOKEN` env → per-instance token in config
//!
//! Config lives at `$OXIDEAN_CONFIG_DIR/config.json`, else
//! `$XDG_CONFIG_HOME/ox/config.json`, else `~/.config/ox/config.json`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const ENV_INSTANCE: &str = "OXIDEAN_INSTANCE";
pub const ENV_TOKEN: &str = "OXIDEAN_TOKEN";
pub const ENV_CONFIG_DIR: &str = "OXIDEAN_CONFIG_DIR";

const APP_DIR: &str = "ox";
const FILE_NAME: &str = "config.json";

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Instance used when neither `--instance` nor `OXIDEAN_INSTANCE` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_instance: Option<String>,
    /// Per-instance credentials keyed by normalized base URL.
    #[serde(default)]
    pub instances: BTreeMap<String, InstanceEntry>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstanceEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Neither `OXIDEAN_CONFIG_DIR`, `XDG_CONFIG_HOME`, nor `HOME` is set.
    #[error("cannot locate a config directory (set OXIDEAN_CONFIG_DIR, XDG_CONFIG_HOME, or HOME)")]
    NoConfigDir,
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Parse(String),
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_var(name: &str) -> Option<String> {
    non_empty(std::env::var(name).ok())
}

/// Pure resolution so precedence is unit-testable without env mutation.
pub fn config_dir_from(
    override_dir: Option<&str>,
    xdg: Option<&str>,
    home: Option<&str>,
) -> Option<PathBuf> {
    if let Some(d) = override_dir {
        return Some(PathBuf::from(d));
    }
    if let Some(d) = xdg {
        return Some(PathBuf::from(d).join(APP_DIR));
    }
    home.map(|h| PathBuf::from(h).join(".config").join(APP_DIR))
}

pub fn config_dir() -> Result<PathBuf, ConfigError> {
    config_dir_from(
        env_var(ENV_CONFIG_DIR).as_deref(),
        env_var("XDG_CONFIG_HOME").as_deref(),
        env_var("HOME").as_deref(),
    )
    .ok_or(ConfigError::NoConfigDir)
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    Ok(config_dir()?.join(FILE_NAME))
}

/// Missing file → default config; malformed file → error (never silently
/// discarding a user's tokens).
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| {
            ConfigError::Parse(format!("{}: invalid config JSON: {e}", path.display()))
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(ConfigError::Io(format!("{}: {e}", path.display()))),
    }
}

pub fn save(path: &Path, cfg: &Config) -> Result<(), ConfigError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| ConfigError::Io(format!("{}: {e}", dir.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    let mut json = serde_json::to_string_pretty(cfg)
        .map_err(|e| ConfigError::Io(format!("serialize config: {e}")))?;
    json.push('\n');
    std::fs::write(path, json).map_err(|e| ConfigError::Io(format!("{}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Normalize a base instance URL: trimmed, trailing slashes stripped, and
/// required to carry an explicit `http(s)://` scheme.
pub fn normalize_instance(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("empty instance URL".to_string());
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(format!("instance URL needs an http(s) scheme, got '{raw}'"));
    }
    Ok(trimmed.to_string())
}

/// `--instance` → `OXIDEAN_INSTANCE` → config default. A present-but-invalid
/// flag/env value is an error rather than silently falling through.
pub fn resolve_instance(
    flag: Option<&str>,
    env: Option<&str>,
    cfg: &Config,
) -> Result<Option<String>, String> {
    if let Some(raw) = flag {
        return normalize_instance(raw).map(Some);
    }
    if let Some(raw) = env {
        return normalize_instance(raw).map(Some);
    }
    Ok(cfg.default_instance.clone())
}

/// `OXIDEAN_TOKEN` env → stored per-instance token.
pub fn resolve_token(env: Option<&str>, cfg: &Config, instance: &str) -> Option<String> {
    env.map(str::to_string)
        .or_else(|| cfg.instances.get(instance).and_then(|e| e.token.clone()))
}

/// Read `OXIDEAN_INSTANCE` / `OXIDEAN_TOKEN` for callers (kept thin so the
/// precedence logic above stays pure).
pub fn env_instance() -> Option<String> {
    env_var(ENV_INSTANCE)
}

pub fn env_token() -> Option<String> {
    env_var(ENV_TOKEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with_default() -> Config {
        let mut instances = BTreeMap::new();
        instances.insert(
            "https://cfg.example.com".to_string(),
            InstanceEntry {
                token: Some("stored-token".to_string()),
            },
        );
        Config {
            default_instance: Some("https://cfg.example.com".to_string()),
            instances,
        }
    }

    #[test]
    fn instance_precedence_flag_env_config() {
        let cfg = cfg_with_default();
        assert_eq!(
            resolve_instance(
                Some("https://flag.example.com"),
                Some("https://env.example.com"),
                &cfg
            )
            .unwrap()
            .as_deref(),
            Some("https://flag.example.com")
        );
        assert_eq!(
            resolve_instance(None, Some("https://env.example.com"), &cfg)
                .unwrap()
                .as_deref(),
            Some("https://env.example.com")
        );
        assert_eq!(
            resolve_instance(None, None, &cfg).unwrap().as_deref(),
            Some("https://cfg.example.com")
        );
        assert_eq!(
            resolve_instance(None, None, &Config::default()).unwrap(),
            None
        );
    }

    #[test]
    fn instance_normalization() {
        assert_eq!(
            normalize_instance(" https://f.example.com/ ").unwrap(),
            "https://f.example.com"
        );
        assert!(normalize_instance("f.example.com").is_err());
        assert!(normalize_instance("").is_err());
    }

    #[test]
    fn token_env_beats_stored() {
        let cfg = cfg_with_default();
        assert_eq!(
            resolve_token(Some("env-token"), &cfg, "https://cfg.example.com").as_deref(),
            Some("env-token")
        );
        assert_eq!(
            resolve_token(None, &cfg, "https://cfg.example.com").as_deref(),
            Some("stored-token")
        );
        assert_eq!(resolve_token(None, &cfg, "https://other.example.com"), None);
    }

    #[test]
    fn config_dir_precedence() {
        assert_eq!(
            config_dir_from(Some("/tmp/oxcfg"), Some("/xdg"), Some("/home/u")).unwrap(),
            PathBuf::from("/tmp/oxcfg")
        );
        assert_eq!(
            config_dir_from(None, Some("/xdg"), Some("/home/u")).unwrap(),
            PathBuf::from("/xdg/ox")
        );
        assert_eq!(
            config_dir_from(None, None, Some("/home/u")).unwrap(),
            PathBuf::from("/home/u/.config/ox")
        );
        assert!(config_dir_from(None, None, None).is_none());
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let cfg = cfg_with_default();
        save(&path, &cfg).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn load_missing_is_default() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(&dir.path().join("nope.json")).unwrap();
        assert_eq!(loaded, Config::default());
    }

    #[test]
    fn load_malformed_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{not json").unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse(_))));
    }
}
