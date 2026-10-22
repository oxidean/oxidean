//! Content-addressed OID store under `OXIDEAN_LFS_DIR` (D-LFS-01, D-LFS-03).

use std::path::{Path, PathBuf};

use axum::body::Bytes;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::auth::session::bytes_to_hex;

/// Reject anything that is not lowercase 64-hex before path join (T-14-04).
pub fn validate_oid(oid: &str) -> Result<(), String> {
    if oid.len() != 64 {
        return Err("oid must be 64 hex characters".into());
    }
    if !oid.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err("oid must be lowercase hex".into());
    }
    Ok(())
}

/// `{LFS_DIR}/{oid[0:2]}/{oid[2:4]}/{oid}`
pub fn shard_path(lfs_dir: &Path, oid: &str) -> Result<PathBuf, String> {
    validate_oid(oid)?;
    Ok(lfs_dir.join(&oid[0..2]).join(&oid[2..4]).join(oid))
}

pub fn object_exists(lfs_dir: &Path, oid: &str) -> Result<bool, String> {
    let path = shard_path(lfs_dir, oid)?;
    Ok(path.is_file())
}

/// Stream body to a temp file beside the shard, hash, rename on match.
pub async fn put_stream<S, E>(
    lfs_dir: &Path,
    oid: &str,
    expected_size: Option<u64>,
    mut stream: S,
) -> Result<u64, String>
where
    S: StreamExt<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    validate_oid(oid)?;
    let final_path = shard_path(lfs_dir, oid)?;
    if let Some(parent) = final_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("create shard dir: {e}"))?;
    }

    let tmp_path = final_path.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| format!("create temp: {e}"))?;
    let mut hasher = Sha256::new();
    let mut written: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("body stream: {e}"))?;
        written = written.saturating_add(chunk.len() as u64);
        if let Some(max) = expected_size {
            if written > max {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err("body exceeds declared size".into());
            }
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("write temp: {e}"))?;
    }
    file.flush().await.map_err(|e| format!("flush temp: {e}"))?;
    drop(file);

    let digest = bytes_to_hex(hasher.finalize().as_slice());
    if digest != oid {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(format!("hash mismatch: got {digest}, expected {oid}"));
    }
    if let Some(max) = expected_size {
        if written != max {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(format!("size mismatch: got {written}, expected {max}"));
        }
    }

    tokio::fs::rename(&tmp_path, &final_path)
        .await
        .map_err(|e| format!("rename into place: {e}"))?;
    Ok(written)
}

pub async fn delete_object(lfs_dir: &Path, oid: &str) -> Result<(), String> {
    let path = shard_path(lfs_dir, oid)?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("delete object: {e}")),
    }
}

pub async fn read_object(lfs_dir: &Path, oid: &str) -> Result<Vec<u8>, String> {
    let path = shard_path(lfs_dir, oid)?;
    tokio::fs::read(&path)
        .await
        .map_err(|e| format!("read object: {e}"))
}
