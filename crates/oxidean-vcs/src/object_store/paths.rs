//! Path safety for the object store (SD3.2): hash validation runs before ANY
//! `Path::join` so caller text can never traverse; the shard layout mirrors
//! the LFS store; tmp staging names are unique per attempt.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::ObjectStoreError;

/// Ownership marker written by `ObjectStore::new` and required by `gc_once`'s
/// marker gate (a markerless non-empty dir is foreign — refuse).
pub(crate) const MARKER_FILE: &str = ".oxidean-vcs-objects";
/// Single-node GC mutex (age-stale by mtime, removed by `LockGuard` on drop).
pub(crate) const GC_LOCK_FILE: &str = ".gc-lock";
/// In-flight PUT staging — `{objects_dir}/tmp/` stays same-filesystem so the
/// rename into the shard is atomic (SD3.8 / R5).
pub(crate) const TMP_DIR: &str = "tmp";
/// `lost+found` is exempted from the "foreign content" checks: a fresh
/// dedicated volume ships one and must not be refused (SD3.9-R9).
pub(crate) const LOST_FOUND: &str = "lost+found";

/// Reject anything that is not bare lowercase 64-hex — before any path join
/// (`validate_oid` precedent; `sha256:` prefixes are stripped at the U7 wire
/// boundary and never reach here).
pub(crate) fn validate_hash(hash: &str) -> Result<(), ObjectStoreError> {
    if hash.len() != 64 {
        return Err(ObjectStoreError::InvalidHash);
    }
    if !hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(ObjectStoreError::InvalidHash);
    }
    Ok(())
}

/// `len`-long lowercase-hex name-shape predicate — the row-less sweep filter
/// (SD3.2.3): applied to directory names BEFORE stat so `.gc-lock`, the
/// marker, `tmp/`, `lost+found`, and dotfiles can never match by construction.
pub(crate) fn is_hex_shaped(name: &str, len: usize) -> bool {
    name.len() == len && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `{dir}/{h[0:2]}/{h[2:4]}/{h}` — identical shape to LFS `store.rs`.
pub(crate) fn shard_path(dir: &Path, hash: &str) -> Result<PathBuf, ObjectStoreError> {
    validate_hash(hash)?;
    Ok(dir.join(&hash[0..2]).join(&hash[2..4]).join(hash))
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// `{objects_dir}/tmp/{pid}-{counter}-{nanos}.tmp` — unique per attempt, so
/// concurrent same-hash PUTs never share a staging file.
pub(crate) fn tmp_path(objects_dir: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    objects_dir
        .join(TMP_DIR)
        .join(format!("{}-{seq}-{nanos}.tmp", std::process::id()))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn validate_hash_accepts_bare_hex() {
        let ok = "a".repeat(64);
        validate_hash(&ok).unwrap();
        validate_hash("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap();
        // Rejected before any path join: wrong length, uppercase, non-hex,
        // traversal, `sha256:`-prefixed.
        let bad = [
            "a".repeat(63),
            "a".repeat(65),
            "A".repeat(64),
            "g".repeat(64),
            "../".repeat(21) + "abc",
            format!("sha256:{}", "a".repeat(64)),
            "a".repeat(63) + "/",
        ];
        for b in &bad {
            assert!(
                matches!(validate_hash(b), Err(ObjectStoreError::InvalidHash)),
                "expected InvalidHash for {b:?}"
            );
        }
    }

    #[test]
    fn shard_path_layout() {
        let h = "0123456789abcdef".repeat(4);
        let dir = Path::new("/objects");
        let p = shard_path(dir, &h).unwrap();
        assert_eq!(p, dir.join("01").join("23").join(&h));
        // tmp staging lives under {dir}/tmp/ — same filesystem for rename.
        let t = tmp_path(dir);
        assert_eq!(t.parent().unwrap(), dir.join("tmp"));
        let name = t.file_name().unwrap().to_str().unwrap();
        assert!(name.ends_with(".tmp"));
        // Two tmp names never collide.
        assert_ne!(tmp_path(dir), tmp_path(dir));
        // Bad hash never reaches a join.
        assert!(matches!(
            shard_path(dir, "../escape"),
            Err(ObjectStoreError::InvalidHash)
        ));
    }
}
