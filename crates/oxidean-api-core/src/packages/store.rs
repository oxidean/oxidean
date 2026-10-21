//! Content-addressed package blob store (D-PKG-07 / D-PKG-08).
//!
//! Layout: `{OXIDEAN_PACKAGES_DIR}/sha256/{aa}/{bb}/{digest_hex}`
//! Digests are stored/passed as `sha256:<hex>` (OCI-style).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use thiserror::Error;

/// Default max blob size when env unset (2 GiB).
pub const DEFAULT_MAX_BLOB_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("invalid digest: {0}")]
    InvalidDigest(String),
    #[error("blob too large: {size} > {max}")]
    TooLarge { size: u64, max: u64 },
    #[error("blob not found: {0}")]
    NotFound(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Parse `sha256:<64 hex>` → lowercase hex without prefix.
pub fn parse_sha256_digest(digest: &str) -> Result<String, StoreError> {
    let rest = digest
        .strip_prefix("sha256:")
        .ok_or_else(|| StoreError::InvalidDigest(digest.into()))?;
    if rest.len() != 64 || !rest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(StoreError::InvalidDigest(digest.into()));
    }
    Ok(rest.to_ascii_lowercase())
}

pub fn format_sha256_digest(hex: &str) -> String {
    format!("sha256:{}", hex.to_ascii_lowercase())
}

pub fn digest_of_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format_sha256_digest(&hex_encode(&h.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Absolute path for a digest under `root`.
pub fn blob_path(root: &Path, digest: &str) -> Result<PathBuf, StoreError> {
    let hex = parse_sha256_digest(digest)?;
    let aa = &hex[0..2];
    let bb = &hex[2..4];
    Ok(root.join("sha256").join(aa).join(bb).join(&hex))
}

pub fn max_blob_bytes_from_env() -> u64 {
    std::env::var("OXIDEAN_PACKAGES_MAX_BLOB_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_BLOB_BYTES)
}

/// Stage bytes via tempfile then atomically rename into the CA path.
/// Returns `(digest, size)`. Does **not** touch DB refcounts.
pub fn put_blob(root: &Path, bytes: &[u8], max_bytes: u64) -> Result<(String, u64), StoreError> {
    let size = bytes.len() as u64;
    if size > max_bytes {
        return Err(StoreError::TooLarge {
            size,
            max: max_bytes,
        });
    }
    let digest = digest_of_bytes(bytes);
    let dest = blob_path(root, &digest)?;
    if dest.exists() {
        return Ok((digest, size));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = NamedTempFile::new_in(dest.parent().unwrap_or(root))?;
    use std::io::Write;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    tmp.persist(&dest).map_err(|e| StoreError::Io(e.error))?;
    Ok((digest, size))
}

pub fn get_blob(root: &Path, digest: &str) -> Result<Vec<u8>, StoreError> {
    let path = blob_path(root, digest)?;
    if !path.exists() {
        return Err(StoreError::NotFound(digest.into()));
    }
    Ok(std::fs::read(path)?)
}

pub fn blob_exists(root: &Path, digest: &str) -> Result<bool, StoreError> {
    Ok(blob_path(root, digest)?.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn package_store_put_get_roundtrip() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let (digest, size) = put_blob(root, b"hello packages", 1024).unwrap();
        assert!(digest.starts_with("sha256:"));
        assert_eq!(size, 14);
        let got = get_blob(root, &digest).unwrap();
        assert_eq!(got, b"hello packages");
        // Idempotent put
        let (d2, _) = put_blob(root, b"hello packages", 1024).unwrap();
        assert_eq!(d2, digest);
    }

    #[test]
    fn package_store_rejects_path_traversal_digest() {
        assert!(parse_sha256_digest("sha256:../evil").is_err());
        assert!(parse_sha256_digest("md5:abc").is_err());
        assert!(parse_sha256_digest(&format!("sha256:{}", "a".repeat(64))).is_ok());
    }

    #[test]
    fn package_store_missing_blob() {
        let dir = tempdir().unwrap();
        let digest = format!("sha256:{}", "0".repeat(64));
        assert!(matches!(
            get_blob(dir.path(), &digest),
            Err(StoreError::NotFound(_))
        ));
    }

    #[test]
    fn package_store_too_large() {
        let dir = tempdir().unwrap();
        let err = put_blob(dir.path(), b"abcdef", 4).unwrap_err();
        assert!(matches!(err, StoreError::TooLarge { .. }));
    }
}
