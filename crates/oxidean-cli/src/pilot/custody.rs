//! Verifier custody (BR1.2, BR2.4, BR8.3/8.4, BR9) — the two-backend seam.
//!
//! Write order: OS keyring first (`pilot/<instance>/<connector_id>` via the
//! existing instance-scoped `keystore` API — the `pilot/` prefix namespaces
//! connector creds from user PATs), falling back to
//! `config_dir()/pilot-secrets.json` (0600, atomic tmp+fsync+rename) when the
//! keyring errors — the defined headless path, not an invented one. Pairing
//! aborts ONLY when both backends fail, and it aborts BEFORE the identity
//! row is written — a row without its verifier is a zombie.
//!
//! `pilot-targets.json` (0600) is the metadata map keyed by `target_id`; it
//! NEVER carries verifier material. A corrupt file on either map is a named
//! parse error — never a silent zombie diagnosis.
//!
//! Every function takes `config_dir` explicitly so the seam is testable on
//! `tempfile` dirs without env mutation.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::keystore;

/// A secret that zeroizes on drop and never prints its value. `secrecy`
/// isn't a workspace dep; `zeroize` already is — this newtype is the
/// plan's licensed "zeroize newtype" SecretString.
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// The raw bearer material — call sites pass it to auth headers and
    /// the scrub set ONLY; never log or store the return.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Clone for SecretString {
    fn clone(&self) -> Self {
        // `*self.0` derefs Zeroizing → String; re-wrap so the clone is
        // still zeroize-on-drop.
        Self::new((*self.0).clone())
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString([redacted])")
    }
}

/// Which custody backend holds the verifier — reported by `status` via the
/// same live probe that yields present/missing (never a stored attribute).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Keyring,
    File,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Keyring => "keyring",
            Self::File => "file",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CustodyError {
    /// Both backends failed at pair time (BR1.2's abort).
    #[error("no custody backend available — keyring: {keyring}; file: {file}")]
    Unavailable { keyring: String, file: String },
    #[error("{0}")]
    Io(String),
    /// Corrupt file — a named error, never a zombie (security-design).
    #[error("{0}")]
    Parse(String),
}

/// The `pilot-targets.json` row — everything the pair response carries
/// minus the verifier (entities.md ConnectorIdentity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetRow {
    pub instance: String,
    pub connector_id: String,
    pub target_id: String,
    pub device_label: String,
    pub granted_caps: Vec<String>,
    pub requested_caps: Vec<String>,
    pub paired_at: String,
}

const TARGETS_FILE: &str = "pilot-targets.json";
const SECRETS_FILE: &str = "pilot-secrets.json";

pub fn targets_path(dir: &Path) -> PathBuf {
    dir.join(TARGETS_FILE)
}

pub fn secrets_path(dir: &Path) -> PathBuf {
    dir.join(SECRETS_FILE)
}

/// Keyring composite instance key — `pilot/<instance>/<connector_id>`;
/// `keystore::*` builds service `ox/<key>` user `token`.
fn keyring_key(instance: &str, connector_id: &str) -> String {
    format!("pilot/{instance}/{connector_id}")
}

/// `pilot-secrets.json` map key — `<instance>/<connector_id>` (no `pilot/`
/// prefix; the file is already pilot-scoped).
fn secrets_key(instance: &str, connector_id: &str) -> String {
    format!("{instance}/{connector_id}")
}

/// Serialize + write `path` atomically: tmp sibling → chmod 0600 → fsync →
/// rename. Strictly stronger than `config::save`'s in-place write.
fn write_atomic(path: &Path, body: &str) -> Result<(), CustodyError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)
        .map_err(|e| CustodyError::Io(format!("{}: {e}", dir.display())))?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("pilot"),
        std::process::id()
    ));
    let write_res = (|| -> Result<(), CustodyError> {
        let mut f = std::fs::File::create(&tmp)
            .map_err(|e| CustodyError::Io(format!("{}: {e}", tmp.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // A chmod failure must abort the write — otherwise the file
            // lands at 0666 & ~umask and the 0600 pin quietly weakens.
            f.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|e| CustodyError::Io(format!("chmod {}: {e}", tmp.display())))?;
        }
        f.write_all(body.as_bytes())
            .map_err(|e| CustodyError::Io(format!("{}: {e}", tmp.display())))?;
        f.sync_all()
            .map_err(|e| CustodyError::Io(format!("{}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| CustodyError::Io(format!("rename to {}: {e}", path.display())))?;
        // fsync the containing directory so the rename itself is durable
        // — a crash between row write and verifier write must not leave
        // an undiscoverable orphan.
        #[cfg(unix)]
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if write_res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    write_res
}

/// Read + parse a JSON map file. Missing file → empty map; unparseable →
/// a NAMED corrupt-file error carrying the path (never silently discarded).
fn load_map(path: &Path) -> Result<BTreeMap<String, String>, CustodyError> {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw)
            .map_err(|e| CustodyError::Parse(format!("{} is corrupt — {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(CustodyError::Io(format!("{}: {e}", path.display()))),
    }
}

fn save_map(path: &Path, map: &BTreeMap<String, String>) -> Result<(), CustodyError> {
    let mut body = serde_json::to_string_pretty(map)
        .map_err(|e| CustodyError::Io(format!("serialize {}: {e}", path.display())))?;
    body.push('\n');
    write_atomic(path, &body)
}

/// Keyring first; on Err the 0600 file map. Both-fail → `Unavailable` —
/// pairing aborts and NO target row is written (BR1.2/BR2.4).
pub fn write_verifier(
    dir: &Path,
    instance: &str,
    connector_id: &str,
    verifier: &SecretString,
) -> Result<Backend, CustodyError> {
    match keystore::set(&keyring_key(instance, connector_id), verifier.expose()) {
        Ok(()) => Ok(Backend::Keyring),
        Err(keyring_err) => {
            let path = secrets_path(dir);
            let mut map = load_map(&path)?;
            map.insert(
                secrets_key(instance, connector_id),
                verifier.expose().to_string(),
            );
            save_map(&path, &map).map_err(|file_err| CustodyError::Unavailable {
                keyring: keyring_err,
                file: file_err.to_string(),
            })?;
            Ok(Backend::File)
        }
    }
}

/// Read order mirrors write order: keyring, then the file map. `None` is
/// the zombie signal (row present, verifier absent in BOTH backends);
/// corrupt-file surfaces as `Parse`, not `None`.
pub fn read_verifier(
    dir: &Path,
    instance: &str,
    connector_id: &str,
) -> Result<Option<(Backend, SecretString)>, CustodyError> {
    if let Some(v) = keystore::get(&keyring_key(instance, connector_id)) {
        return Ok(Some((Backend::Keyring, SecretString::new(v))));
    }
    let map = load_map(&secrets_path(dir))?;
    Ok(map
        .get(&secrets_key(instance, connector_id))
        .map(|v| (Backend::File, SecretString::new(v.clone()))))
}

/// Best-effort BOTH backends — a credential in either must die; failures
/// log to stderr, never fatal (the row delete is the identity of record).
pub fn delete_verifier(dir: &Path, instance: &str, connector_id: &str) {
    keystore::delete(&keyring_key(instance, connector_id));
    let path = secrets_path(dir);
    match load_map(&path) {
        Ok(mut map) => {
            if map.remove(&secrets_key(instance, connector_id)).is_some()
                && save_map(&path, &map).is_err()
            {
                eprintln!("ox: warning — could not update {}", path.display());
            }
        }
        Err(e) => eprintln!("ox: warning — {e}"),
    }
}

/// Secrets-map values for the output-scrub set (NFR5.4). Corrupt file →
/// Err (caller treats scrub-secrets collection as best-effort, but the
/// row/custody paths surface the same error as named).
pub fn load_map_for_scrub(dir: &Path) -> Result<BTreeMap<String, String>, CustodyError> {
    load_map(&secrets_path(dir))
}

/// The whole `pilot-targets.json` map keyed by `target_id`. Corrupt file →
/// named parse error; `run`/`disconnect`/`status` all refuse on it rather
/// than silently discarding identity rows.
pub fn load_targets(dir: &Path) -> Result<BTreeMap<String, TargetRow>, CustodyError> {
    let path = targets_path(dir);
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw)
            .map_err(|e| CustodyError::Parse(format!("{} is corrupt — {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(CustodyError::Io(format!("{}: {e}", path.display()))),
    }
}

pub fn save_targets(dir: &Path, map: &BTreeMap<String, TargetRow>) -> Result<(), CustodyError> {
    let mut body = serde_json::to_string_pretty(map)
        .map_err(|e| CustodyError::Io(format!("serialize {}: {e}", targets_path(dir).display())))?;
    body.push('\n');
    write_atomic(&targets_path(dir), &body)
}

/// Insert/replace a row — the pair path's second custody write (the
/// verifier is already safe before this is called).
pub fn upsert_target(dir: &Path, row: &TargetRow) -> Result<(), CustodyError> {
    let mut map = load_targets(dir)?;
    map.insert(row.target_id.clone(), row.clone());
    save_targets(dir, &map)
}

/// Remove a row — disconnect + the auth-death/revoked terminals.
pub fn delete_target(dir: &Path, target_id: &str) -> Result<(), CustodyError> {
    let mut map = load_targets(dir)?;
    map.remove(target_id);
    save_targets(dir, &map)
}

/// RFC 3339 `YYYY-MM-DDTHH:MM:SSZ` for `paired_at` — the crate has no
/// chrono dep, so this is the smallest civil-from-days conversion.
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // civil_from_days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        tod % 3600 / 60,
        tod % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn secrets_file_roundtrip_delete() {
        let d = dir();
        let v = SecretString::new("verifier-material-123".into());
        // Write through the file backend directly (keyring may exist on
        // dev machines — exercise the file map itself).
        let path = secrets_path(d.path());
        let mut map = BTreeMap::new();
        map.insert(
            secrets_key("https://f.example", "c1"),
            v.expose().to_string(),
        );
        save_map(&path, &map).unwrap();
        let loaded = load_map(&path).unwrap();
        assert_eq!(
            loaded.get("https://f.example/c1").map(String::as_str),
            Some("verifier-material-123")
        );
        // 0600 on unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        map.clear();
        save_map(&path, &map).unwrap();
        assert!(load_map(&path).unwrap().is_empty());
    }

    #[test]
    fn missing_secrets_file_is_none_not_zombie_error() {
        let d = dir();
        assert!(load_map(&secrets_path(d.path())).unwrap().is_empty());
        assert!(load_targets(d.path()).unwrap().is_empty());
    }

    #[test]
    fn corrupt_secrets_file_is_named_error() {
        let d = dir();
        std::fs::write(secrets_path(d.path()), "{not json").unwrap();
        match load_map(&secrets_path(d.path())) {
            Err(CustodyError::Parse(m)) => {
                assert!(m.contains("pilot-secrets.json"), "{m}");
                assert!(m.contains("corrupt"), "{m}");
            }
            other => panic!("expected named parse error, got {other:?}"),
        }
    }

    #[test]
    fn corrupt_targets_file_is_named_error() {
        let d = dir();
        std::fs::write(targets_path(d.path()), "garbage!!").unwrap();
        match load_targets(d.path()) {
            Err(CustodyError::Parse(m)) => assert!(m.contains("pilot-targets.json"), "{m}"),
            other => panic!("expected named parse error, got {other:?}"),
        }
    }

    #[test]
    fn targets_upsert_delete_roundtrip() {
        let d = dir();
        let row = TargetRow {
            instance: "https://f.example".into(),
            connector_id: "c1".into(),
            target_id: "t1".into(),
            device_label: "dev-box".into(),
            granted_caps: vec!["exec_shell".into()],
            requested_caps: vec!["exec_shell".into()],
            paired_at: now_rfc3339(),
        };
        upsert_target(d.path(), &row).unwrap();
        let map = load_targets(d.path()).unwrap();
        assert_eq!(map.get("t1"), Some(&row));
        // Serialized rows carry NO verifier field.
        let raw = std::fs::read_to_string(targets_path(d.path())).unwrap();
        assert!(!raw.contains("verifier"), "{raw}");
        delete_target(d.path(), "t1").unwrap();
        assert!(load_targets(d.path()).unwrap().is_empty());
    }

    #[test]
    fn now_rfc3339_shape() {
        let ts = now_rfc3339();
        assert_eq!(ts.len(), 20, "{ts}");
        assert!(ts.ends_with('Z') && ts.contains('T'), "{ts}");
    }
}
