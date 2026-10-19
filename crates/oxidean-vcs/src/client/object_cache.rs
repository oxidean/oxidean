//! Bounded content-addressed blob cache shared across checkouts and repos
//! (FR8.2, AC6.2).
//!
//! Layout: `<root>/<first-2-hex>/<rest>.bin` keyed by the blob's `sha256:`
//! object id. Every read re-verifies the content hash — a corrupted entry is
//! evicted and reported as a miss (AC6.2.2), never served. Writes verify the
//! bytes against the wanted hash before the entry commits (AC3.3.3): nothing
//! corrupt ever lands in the cache. Total size is bounded (`max_bytes`,
//! `OXIDEAN_VCS_CACHE_MAX_BYTES`, default 1 GiB) with oldest-mtime eviction.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::transport::CliFault;

/// Default cache bound — 1 GiB.
pub const DEFAULT_CACHE_MAX_BYTES: u64 = 1 << 30;

/// `OXIDEAN_VCS_CACHE_DIR` — overrides the whole cache root.
pub const CACHE_DIR_ENV: &str = "OXIDEAN_VCS_CACHE_DIR";
/// `OXIDEAN_VCS_CACHE_MAX_BYTES` — bound override.
pub const CACHE_MAX_BYTES_ENV: &str = "OXIDEAN_VCS_CACHE_MAX_BYTES";

/// Resolve the cache root: env override, then `$XDG_CACHE_HOME/ox/vcs-objects`,
/// then `~/.cache/ox/vcs-objects`.
pub fn default_cache_root() -> PathBuf {
    if let Ok(dir) = std::env::var(CACHE_DIR_ENV) {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("ox/vcs-objects");
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".cache/ox/vcs-objects")
}

fn max_bytes_from_env() -> u64 {
    std::env::var(CACHE_MAX_BYTES_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_CACHE_MAX_BYTES)
}

/// The `sha256:<64 lowercase hex>` object id for `bytes`.
pub fn object_hash(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

/// `hash` is a wire `sha256:<hex>` object id.
fn hex_of(hash: &str) -> Option<&str> {
    let hex_part = hash.strip_prefix("sha256:")?;
    if hex_part.len() == 64 && hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(hex_part)
    } else {
        None
    }
}

/// Bounded, verify-on-read object cache.
pub struct ObjectCache {
    root: PathBuf,
    max_bytes: u64,
}

impl ObjectCache {
    pub fn new(root: impl Into<PathBuf>, max_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_bytes,
        }
    }

    /// Env-driven construction — `OXIDEAN_VCS_CACHE_DIR`,
    /// `OXIDEAN_VCS_CACHE_MAX_BYTES`.
    pub fn from_env() -> Self {
        Self::new(default_cache_root(), max_bytes_from_env())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn entry_path(&self, hash: &str) -> Result<PathBuf, CliFault> {
        let hex_part =
            hex_of(hash).ok_or_else(|| CliFault::Cache(format!("invalid object id {hash}")))?;
        Ok(self
            .root
            .join(&hex_part[..2])
            .join(format!("{}.bin", &hex_part[2..])))
    }

    /// Verified read: hit → `Ok(Some(bytes))`; absent or corrupt → `Ok(None)`
    /// (corrupt entries are evicted, AC6.2.2).
    pub fn get(&self, hash: &str) -> Result<Option<Vec<u8>>, CliFault> {
        let path = self.entry_path(hash)?;
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(CliFault::Fs(format!(
                    "reading cache entry {}: {e}",
                    path.display()
                )))
            }
        };
        if object_hash(&bytes) == hash {
            Ok(Some(bytes))
        } else {
            let _ = fs::remove_file(&path);
            Ok(None)
        }
    }

    /// Verified write: bytes must hash to `hash` before anything lands
    /// (AC3.3.3). Temp-write-then-rename, then evict to the bound.
    pub fn put(&mut self, hash: &str, bytes: &[u8]) -> Result<(), CliFault> {
        let actual = object_hash(bytes);
        if actual != hash {
            return Err(CliFault::Cache(format!(
                "object hash mismatch: wanted {hash}, got {actual}"
            )));
        }
        let path = self.entry_path(hash)?;
        let dir = path.parent().expect("entry path has a parent");
        fs::create_dir_all(dir)
            .map_err(|e| CliFault::Fs(format!("creating cache dir {}: {e}", dir.display())))?;
        let tmp = dir.join(format!(".tmp-{}", std::process::id()));
        fs::write(&tmp, bytes)
            .and_then(|()| fs::rename(&tmp, &path))
            .map_err(|e| CliFault::Fs(format!("writing cache entry {}: {e}", path.display())))?;
        self.evict_to_bound()
    }

    /// Drop oldest-mtime entries until total size ≤ `max_bytes`.
    fn evict_to_bound(&self) -> Result<(), CliFault> {
        let mut entries: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
        let mut total = 0u64;
        collect_entries(&self.root, &mut entries, &mut total)?;
        if total <= self.max_bytes {
            return Ok(());
        }
        entries.sort_by_key(|(_, _, mtime)| *mtime);
        for (path, size, _) in entries {
            if total <= self.max_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }
}

fn collect_entries(
    dir: &Path,
    entries: &mut Vec<(PathBuf, u64, std::time::SystemTime)>,
    total: &mut u64,
) -> Result<(), CliFault> {
    let Ok(read) = fs::read_dir(dir) else {
        return Ok(()); // absent root — nothing to evict
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            collect_entries(&path, entries, total)?;
        } else if meta.is_file() {
            *total += meta.len();
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            entries.push((path, meta.len(), mtime));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_of(s: &str) -> String {
        object_hash(s.as_bytes())
    }

    #[test]
    fn put_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = ObjectCache::new(dir.path(), DEFAULT_CACHE_MAX_BYTES);
        let hash = hash_of("hello");
        cache.put(&hash, b"hello").unwrap();
        assert_eq!(
            cache.get(&hash).unwrap().as_deref(),
            Some(b"hello".as_slice())
        );
        assert_eq!(cache.get(&hash_of("absent")).unwrap(), None);
    }

    #[test]
    fn put_rejects_hash_mismatch_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = ObjectCache::new(dir.path(), DEFAULT_CACHE_MAX_BYTES);
        let err = cache
            .put(&hash_of("expected"), b"actual")
            .expect_err("mismatched bytes must be refused");
        assert!(err.to_string().contains("hash mismatch"));
        // Nothing landed under the wanted hash.
        assert_eq!(cache.get(&hash_of("expected")).unwrap(), None);
        assert!(fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    #[test]
    fn get_evicts_corrupt_entry_as_miss() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = ObjectCache::new(dir.path(), DEFAULT_CACHE_MAX_BYTES);
        let hash = hash_of("good");
        cache.put(&hash, b"good").unwrap();
        // Corrupt the on-disk entry — the read must evict + miss, never serve.
        let hex = &hash["sha256:".len()..];
        let path = dir
            .path()
            .join(&hex[..2])
            .join(format!("{}.bin", &hex[2..]));
        fs::write(&path, b"corrupted").unwrap();
        assert_eq!(cache.get(&hash).unwrap(), None);
        assert!(!path.exists());
    }

    #[test]
    fn evicts_oldest_entries_to_bound() {
        let dir = tempfile::tempdir().unwrap();
        // Bound fits two 4-byte blobs but not three.
        let mut cache = ObjectCache::new(dir.path(), 9);
        let h1 = hash_of("aaaa");
        let h2 = hash_of("bbbb");
        let h3 = hash_of("cccc");
        cache.put(&h1, b"aaaa").unwrap();
        // Distinct mtimes — the oldest is the eviction candidate.
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.put(&h2, b"bbbb").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.put(&h3, b"cccc").unwrap();
        assert_eq!(cache.get(&h1).unwrap(), None, "oldest evicted");
        assert_eq!(cache.get(&h2).unwrap().as_deref(), Some(b"bbbb".as_slice()));
        assert_eq!(cache.get(&h3).unwrap().as_deref(), Some(b"cccc".as_slice()));
    }

    #[test]
    fn entry_path_shards_by_hex_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ObjectCache::new(dir.path(), DEFAULT_CACHE_MAX_BYTES);
        let hash = hash_of("x");
        let path = cache.entry_path(&hash).unwrap();
        let hex = &hash["sha256:".len()..];
        assert_eq!(
            path,
            dir.path()
                .join(&hex[..2])
                .join(format!("{}.bin", &hex[2..]))
        );
        assert!(cache.entry_path("not-a-hash").is_err());
        assert!(cache.entry_path("sha256:short").is_err());
    }

    #[test]
    fn object_hash_is_sha256_prefixed() {
        let h = object_hash(b"");
        assert_eq!(
            h,
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
