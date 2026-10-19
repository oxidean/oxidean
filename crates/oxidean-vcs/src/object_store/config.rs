//! `OXIDEAN_VCS_*` operator contract (BR3.7 / entities.md ObjectStoreConfig).
//! `from_env` parses + validates — malformed values are `Config` errors, never
//! silent defaults.

use std::env::VarError;
use std::path::PathBuf;

use super::ObjectStoreError;

pub const OBJECTS_DIR_ENV: &str = "OXIDEAN_VCS_OBJECTS_DIR";
pub const MAX_OBJECT_BYTES_ENV: &str = "OXIDEAN_VCS_MAX_OBJECT_BYTES";
pub const REPO_QUOTA_BYTES_ENV: &str = "OXIDEAN_VCS_REPO_QUOTA_BYTES";
pub const GC_GRACE_SECS_ENV: &str = "OXIDEAN_VCS_GC_GRACE_SECS";
pub const GC_INTERVAL_SECS_ENV: &str = "OXIDEAN_VCS_GC_INTERVAL_SECS";

const DEFAULT_OBJECTS_DIR: &str = "./var/vcs-objects";
const DEFAULT_MAX_OBJECT_BYTES: u64 = 268_435_456; // 256 MiB
const DEFAULT_REPO_QUOTA_BYTES: u64 = 10_737_418_240; // 10 GiB
const DEFAULT_GC_GRACE_SECS: u64 = 86_400;
const DEFAULT_GC_INTERVAL_SECS: u64 = 3_600;

/// chrono's millisecond repr overflows above `i64::MAX / 1000` (SD3.9-R10) —
/// `grace` feeds `chrono::Duration`, `interval` feeds `std::time::Duration`
/// spawn-side. Byte caps must fit `i64` so the `size: u64 -> i64` bind into
/// `vcs_insert_object_if_absent` can never wrap negative (SD3.6).
const GRACE_MAX_SECS: u64 = (i64::MAX / 1000) as u64;
const I64_MAX_U64: u64 = i64::MAX as u64;

#[derive(Debug, Clone)]
pub struct ObjectStoreConfig {
    pub objects_dir: PathBuf,
    pub max_object_bytes: u64,
    pub repo_quota_bytes: u64,
    pub gc_grace_secs: u64,
    pub gc_interval_secs: u64,
}

impl Default for ObjectStoreConfig {
    fn default() -> Self {
        Self {
            objects_dir: PathBuf::from(DEFAULT_OBJECTS_DIR),
            max_object_bytes: DEFAULT_MAX_OBJECT_BYTES,
            repo_quota_bytes: DEFAULT_REPO_QUOTA_BYTES,
            gc_grace_secs: DEFAULT_GC_GRACE_SECS,
            gc_interval_secs: DEFAULT_GC_INTERVAL_SECS,
        }
    }
}

impl ObjectStoreConfig {
    /// Parse the five `OXIDEAN_VCS_*` vars. Unset ⇒ frozen default; malformed
    /// or out-of-range ⇒ `Config` (fail closed, never silent).
    pub fn from_env() -> Result<Self, ObjectStoreError> {
        let objects_dir = env_path(OBJECTS_DIR_ENV, DEFAULT_OBJECTS_DIR)?;
        let max_object_bytes = env_u64(
            MAX_OBJECT_BYTES_ENV,
            DEFAULT_MAX_OBJECT_BYTES,
            0,
            I64_MAX_U64,
        )?;
        let repo_quota_bytes = env_u64(
            REPO_QUOTA_BYTES_ENV,
            DEFAULT_REPO_QUOTA_BYTES,
            0,
            I64_MAX_U64,
        )?;
        let gc_grace_secs = env_u64(GC_GRACE_SECS_ENV, DEFAULT_GC_GRACE_SECS, 1, GRACE_MAX_SECS)?;
        let gc_interval_secs = env_u64(
            GC_INTERVAL_SECS_ENV,
            DEFAULT_GC_INTERVAL_SECS,
            0,
            I64_MAX_U64,
        )?;

        // SD3.9-R7: refuse when the resolved dir equals another content
        // store's resolved dir — an empty `OXIDEAN_LFS_DIR`/
        // `OXIDEAN_PACKAGES_DIR` would take our marker and lose its
        // later-arriving blobs to the row-less sweep.
        for (name, default) in [
            ("OXIDEAN_LFS_DIR", "var/lfs"),
            ("OXIDEAN_PACKAGES_DIR", "var/packages"),
        ] {
            let sibling = env_path(name, default)?;
            if sibling == objects_dir {
                return Err(ObjectStoreError::Config(format!(
                    "{OBJECTS_DIR_ENV} resolves to the same directory as {name}"
                )));
            }
        }

        Ok(Self {
            objects_dir,
            max_object_bytes,
            repo_quota_bytes,
            gc_grace_secs,
            gc_interval_secs,
        })
    }
}

fn env_u64(name: &str, default: u64, min: u64, max: u64) -> Result<u64, ObjectStoreError> {
    let raw = match std::env::var(name) {
        Ok(v) => v,
        Err(VarError::NotPresent) => return Ok(default),
        Err(e) => return Err(e.into()),
    };
    let value: u64 = raw
        .parse()
        .map_err(|_| ObjectStoreError::Config(format!("{name}: not a u64")))?;
    if value < min || value > max {
        return Err(ObjectStoreError::Config(format!(
            "{name}: out of range (allowed {min}..={max})"
        )));
    }
    Ok(value)
}

fn env_path(name: &str, default: &str) -> Result<PathBuf, ObjectStoreError> {
    let raw = match std::env::var(name) {
        Ok(v) => v,
        Err(VarError::NotPresent) => default.to_string(),
        Err(e) => return Err(e.into()),
    };
    if raw.is_empty() {
        return Err(ObjectStoreError::Config(format!("{name}: empty path")));
    }
    Ok(absolutize(PathBuf::from(raw)))
}

/// Relative dirs resolve against cwd — same absolutize idiom as `app.rs`
/// (git ops run from temp worktrees; store paths must not drift with `-C`).
fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    struct EnvGuard(Vec<(&'static str, Option<String>)>);

    impl EnvGuard {
        fn snapshot(names: &[&'static str]) -> Self {
            Self(names.iter().map(|n| (*n, std::env::var(n).ok())).collect())
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.0 {
                match value {
                    Some(v) => std::env::set_var(name, v),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    const ENV_NAMES: &[&str] = &[
        OBJECTS_DIR_ENV,
        MAX_OBJECT_BYTES_ENV,
        REPO_QUOTA_BYTES_ENV,
        GC_GRACE_SECS_ENV,
        GC_INTERVAL_SECS_ENV,
        "OXIDEAN_LFS_DIR",
        "OXIDEAN_PACKAGES_DIR",
    ];

    #[test]
    fn from_env_ranges() {
        let _guard = EnvGuard::snapshot(ENV_NAMES);
        for name in ENV_NAMES {
            std::env::remove_var(name);
        }
        // Defaults resolve.
        let cfg = ObjectStoreConfig::from_env().unwrap();
        assert_eq!(cfg.max_object_bytes, DEFAULT_MAX_OBJECT_BYTES);
        assert_eq!(cfg.repo_quota_bytes, DEFAULT_REPO_QUOTA_BYTES);
        assert_eq!(cfg.gc_grace_secs, DEFAULT_GC_GRACE_SECS);
        assert_eq!(cfg.gc_interval_secs, DEFAULT_GC_INTERVAL_SECS);
        assert!(cfg.objects_dir.is_absolute());

        let is_config = |r: Result<ObjectStoreConfig, ObjectStoreError>| {
            matches!(r, Err(ObjectStoreError::Config(_)))
        };

        // grace: 0 and > i64::MAX/1000 are refused.
        std::env::set_var(GC_GRACE_SECS_ENV, "0");
        assert!(is_config(ObjectStoreConfig::from_env()));
        std::env::set_var(GC_GRACE_SECS_ENV, (GRACE_MAX_SECS + 1).to_string());
        assert!(is_config(ObjectStoreConfig::from_env()));
        std::env::set_var(GC_GRACE_SECS_ENV, GRACE_MAX_SECS.to_string());
        assert!(ObjectStoreConfig::from_env().is_ok());
        std::env::set_var(GC_GRACE_SECS_ENV, "not-a-number");
        assert!(is_config(ObjectStoreConfig::from_env()));
        std::env::remove_var(GC_GRACE_SECS_ENV);

        // interval: > i64::MAX refused, 0 (disable) allowed.
        std::env::set_var(GC_INTERVAL_SECS_ENV, u64::MAX.to_string());
        assert!(is_config(ObjectStoreConfig::from_env()));
        std::env::set_var(GC_INTERVAL_SECS_ENV, "0");
        assert!(ObjectStoreConfig::from_env().is_ok());
        std::env::remove_var(GC_INTERVAL_SECS_ENV);

        // byte caps: > i64::MAX refused.
        std::env::set_var(MAX_OBJECT_BYTES_ENV, u64::MAX.to_string());
        assert!(is_config(ObjectStoreConfig::from_env()));
        std::env::remove_var(MAX_OBJECT_BYTES_ENV);
        std::env::set_var(REPO_QUOTA_BYTES_ENV, u64::MAX.to_string());
        assert!(is_config(ObjectStoreConfig::from_env()));
        std::env::remove_var(REPO_QUOTA_BYTES_ENV);

        // sibling-dir equality: resolved OXIDEAN_LFS_DIR / OXIDEAN_PACKAGES_DIR
        // equal to the resolved objects dir is refused.
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var(OBJECTS_DIR_ENV, dir.path());
        assert!(ObjectStoreConfig::from_env().is_ok());
        std::env::set_var("OXIDEAN_LFS_DIR", dir.path());
        assert!(is_config(ObjectStoreConfig::from_env()));
        std::env::remove_var("OXIDEAN_LFS_DIR");
        std::env::set_var("OXIDEAN_PACKAGES_DIR", dir.path());
        assert!(is_config(ObjectStoreConfig::from_env()));
        std::env::remove_var("OXIDEAN_PACKAGES_DIR");

        // empty objects dir var is refused.
        std::env::set_var(OBJECTS_DIR_ENV, "");
        assert!(is_config(ObjectStoreConfig::from_env()));
        std::env::remove_var(OBJECTS_DIR_ENV);
    }
}
