//! Content-addressed instance template pack store (issue #18).
//!
//! Layout: `{OXIDEAN_TEMPLATE_PACKS_DIR}/sha256/{aa}/{bb}/{digest_hex}.zip`

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

pub const DEFAULT_MAX_PACK_BYTES: u64 = 10 * 1024 * 1024;
pub const DEFAULT_MAX_EXTRACTED_BYTES: u64 = 25 * 1024 * 1024;
pub const DEFAULT_MAX_FILE_COUNT: usize = 5_000;

#[derive(Debug, Error)]
pub enum TemplateStoreError {
    #[error("invalid digest: {0}")]
    InvalidDigest(String),
    #[error("pack too large: {size} > {max}")]
    TooLarge { size: u64, max: u64 },
    #[error("pack not found: {0}")]
    NotFound(String),
    #[error("unsafe path in archive: {0}")]
    UnsafePath(String),
    #[error("archive error: {0}")]
    Archive(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub fn max_pack_bytes_from_env() -> u64 {
    std::env::var("OXIDEAN_TEMPLATE_PACK_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_PACK_BYTES)
}

pub fn digest_of_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{}", hex_encode(&h.finalize()))
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

fn parse_sha256_digest(digest: &str) -> Result<String, TemplateStoreError> {
    let rest = digest
        .strip_prefix("sha256:")
        .ok_or_else(|| TemplateStoreError::InvalidDigest(digest.into()))?;
    if rest.len() != 64 || !rest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TemplateStoreError::InvalidDigest(digest.into()));
    }
    Ok(rest.to_ascii_lowercase())
}

pub fn pack_path(root: &Path, digest: &str) -> Result<PathBuf, TemplateStoreError> {
    let hex = parse_sha256_digest(digest)?;
    let aa = &hex[0..2];
    let bb = &hex[2..4];
    Ok(root
        .join("sha256")
        .join(aa)
        .join(bb)
        .join(format!("{hex}.zip")))
}

/// Store zip bytes; returns `(digest, size)`.
pub fn put_pack(
    root: &Path,
    bytes: &[u8],
    max_bytes: u64,
) -> Result<(String, u64), TemplateStoreError> {
    let size = bytes.len() as u64;
    if size > max_bytes {
        return Err(TemplateStoreError::TooLarge {
            size,
            max: max_bytes,
        });
    }
    // Validate zip + paths before writing.
    let _ = unzip_to_map(bytes)?;
    let digest = digest_of_bytes(bytes);
    let path = pack_path(root, &digest)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        std::fs::write(&path, bytes)?;
    }
    Ok((digest, size))
}

pub fn read_pack(root: &Path, digest: &str) -> Result<Vec<u8>, TemplateStoreError> {
    let path = pack_path(root, digest)?;
    if !path.is_file() {
        return Err(TemplateStoreError::NotFound(digest.into()));
    }
    Ok(std::fs::read(path)?)
}

pub fn delete_pack(root: &Path, digest: &str) -> Result<(), TemplateStoreError> {
    let path = pack_path(root, digest)?;
    if path.is_file() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn is_safe_entry_path(name: &str) -> bool {
    if name.is_empty() || name.ends_with('/') {
        return false;
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return false;
    }
    if name.contains('\0') {
        return false;
    }
    for part in name.split(['/', '\\']) {
        if part.is_empty() || part == "." || part == ".." {
            return false;
        }
    }
    true
}

/// Unzip pack bytes into a sorted path → content map (dirs skipped).
pub fn unzip_to_map(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, TemplateStoreError> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|e| TemplateStoreError::Archive(e.to_string()))?;
    if archive.len() > DEFAULT_MAX_FILE_COUNT {
        return Err(TemplateStoreError::Archive(format!(
            "too many files: {} > {}",
            archive.len(),
            DEFAULT_MAX_FILE_COUNT
        )));
    }
    let mut out = BTreeMap::new();
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| TemplateStoreError::Archive(e.to_string()))?;
        if file.is_dir() {
            continue;
        }
        let name = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if !is_safe_entry_path(&name) {
            return Err(TemplateStoreError::UnsafePath(name));
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        total += buf.len() as u64;
        if total > DEFAULT_MAX_EXTRACTED_BYTES {
            return Err(TemplateStoreError::TooLarge {
                size: total,
                max: DEFAULT_MAX_EXTRACTED_BYTES,
            });
        }
        out.insert(name, buf);
    }
    if out.is_empty() {
        return Err(TemplateStoreError::Archive(
            "archive contains no files".into(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn sample_zip() -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = ZipWriter::new(&mut buf);
            w.start_file("README.md", SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"# hello\n").unwrap();
            w.start_file("src/main.rs", SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"fn main() {}\n").unwrap();
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn put_and_unzip_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = sample_zip();
        let (digest, size) = put_pack(dir.path(), &bytes, DEFAULT_MAX_PACK_BYTES).unwrap();
        assert!(digest.starts_with("sha256:"));
        assert_eq!(size, bytes.len() as u64);
        let map = unzip_to_map(&read_pack(dir.path(), &digest).unwrap()).unwrap();
        assert_eq!(map.get("README.md").unwrap(), b"# hello\n");
        assert!(map.contains_key("src/main.rs"));
    }

    #[test]
    fn put_pack_dedupes_identical_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = sample_zip();
        let (d1, _) = put_pack(dir.path(), &bytes, DEFAULT_MAX_PACK_BYTES).unwrap();
        let (d2, _) = put_pack(dir.path(), &bytes, DEFAULT_MAX_PACK_BYTES).unwrap();
        assert_eq!(d1, d2);
        let path = pack_path(dir.path(), &d1).unwrap();
        assert!(path.is_file());
        // Deleting once removes the shared blob — callers must refcount via DB.
        delete_pack(dir.path(), &d1).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn rejects_path_traversal() {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = ZipWriter::new(&mut buf);
            // Some zip writers allow odd names; enclosed_name should catch ../
            w.start_file("../evil.txt", SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"x").unwrap();
            w.finish().unwrap();
        }
        let bytes = buf.into_inner();
        let err = unzip_to_map(&bytes).unwrap_err();
        match err {
            TemplateStoreError::UnsafePath(_) | TemplateStoreError::Archive(_) => {}
            other => panic!("unexpected: {other}"),
        }
    }
}
