//! Filesystem-backed comment attachments: quota accounting + orphan GC.
//!
//! Uploads live at `{uploads_dir}/comment-attachments/{uuid}` with a
//! `{uuid}.json` sidecar. There is no DB row — the UUID is the capability —
//! so quota is computed from sidecar metadata and GC keeps any id still
//! referenced by persisted markdown bodies. Reference scope is the DB body
//! columns scanned by `oxidean_db::comment_attachments` — markdown living
//! only in git (wiki pages, READMEs) does not protect an attachment.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use oxidean_db::Database;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Subdirectory under `uploads_dir`.
pub const COMMENT_ATTACHMENTS_DIR: &str = "comment-attachments";
/// URL path prefix embedded in markdown bodies.
pub const ATTACHMENT_URL_PREFIX: &str = "/uploads/comment-attachments/";

/// Per-user stored-byte quota (256 MiB). `0` disables the byte quota.
pub const DEFAULT_MAX_BYTES_PER_USER: u64 = 256 * 1024 * 1024;
/// Per-user attachment count quota. `0` disables the count quota.
pub const DEFAULT_MAX_COUNT_PER_USER: u64 = 100;
/// Default orphan grace before an unreferenced upload may be collected.
pub const DEFAULT_ORPHAN_GRACE_SECS: u64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentAttachmentMeta {
    pub name: String,
    pub content_type: String,
    pub size: u64,
    pub uploader_id: String,
    pub created_at: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CommentAttachmentUsage {
    pub count: u64,
    pub bytes: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CommentAttachmentGcStats {
    pub scanned: u64,
    pub removed: u64,
    pub kept_referenced: u64,
    pub kept_recent: u64,
    pub errors: u64,
}

pub fn attachments_dir(uploads_dir: &Path) -> PathBuf {
    uploads_dir.join(COMMENT_ATTACHMENTS_DIR)
}

pub fn data_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(id)
}

pub fn meta_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

pub fn max_bytes_per_user() -> u64 {
    env_u64(
        "OXIDEAN_COMMENT_ATTACHMENT_MAX_BYTES_PER_USER",
        DEFAULT_MAX_BYTES_PER_USER,
    )
}

pub fn max_count_per_user() -> u64 {
    env_u64(
        "OXIDEAN_COMMENT_ATTACHMENT_MAX_COUNT_PER_USER",
        DEFAULT_MAX_COUNT_PER_USER,
    )
}

pub fn orphan_grace_from_env() -> Duration {
    Duration::from_secs(env_u64(
        "OXIDEAN_COMMENT_ATTACHMENT_ORPHAN_GRACE_SECS",
        DEFAULT_ORPHAN_GRACE_SECS,
    ))
}

/// Sum sidecar-declared size/count for one uploader. Malformed or foreign
/// sidecars are ignored — they cannot be attributed to this user.
pub fn user_attachment_usage(dir: &Path, user_id: &str) -> std::io::Result<CommentAttachmentUsage> {
    let mut usage = CommentAttachmentUsage::default();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        // Missing dir = nothing stored yet; other I/O errors must fail the
        // quota check closed rather than undercount usage.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(usage),
        Err(e) => return Err(e),
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if Uuid::parse_str(id).is_err() {
            continue;
        }
        let Ok(bytes) = fs::read(entry.path()) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<CommentAttachmentMeta>(&bytes) else {
            continue;
        };
        if meta.uploader_id == user_id {
            usage.count += 1;
            usage.bytes = usage.bytes.saturating_add(meta.size);
        }
    }
    Ok(usage)
}

/// Reject when `additional` bytes would push `user_id` over either quota.
pub fn check_upload_quota(dir: &Path, user_id: &str, additional: u64) -> Result<(), String> {
    let usage = user_attachment_usage(dir, user_id)
        .map_err(|e| format!("comment attachment quota scan failed: {e}"))?;
    let max_count = max_count_per_user();
    if max_count > 0 && usage.count + 1 > max_count {
        return Err(format!(
            "comment attachment count quota exceeded ({}/{max_count})",
            usage.count + 1
        ));
    }
    let max_bytes = max_bytes_per_user();
    if max_bytes > 0 && usage.bytes.saturating_add(additional) > max_bytes {
        return Err(format!(
            "comment attachment storage quota exceeded ({}/{max_bytes} bytes)",
            usage.bytes.saturating_add(additional)
        ));
    }
    Ok(())
}

/// Extract normalized attachment ids referenced by persisted markdown bodies.
pub fn referenced_attachment_ids(bodies: &[String]) -> HashSet<String> {
    let mut ids = HashSet::new();
    for body in bodies {
        for (idx, _) in body.match_indices(ATTACHMENT_URL_PREFIX) {
            let rest = &body[idx + ATTACHMENT_URL_PREFIX.len()..];
            let candidate: String = rest.chars().take(36).collect();
            if let Ok(id) = Uuid::parse_str(&candidate) {
                ids.insert(id.to_string());
            }
        }
    }
    ids
}

fn created_at(
    meta: Option<&CommentAttachmentMeta>,
    data_path: &Path,
    meta_path: &Path,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Some(meta) = meta {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&meta.created_at) {
            return Some(dt.with_timezone(&chrono::Utc));
        }
    }
    for path in [data_path, meta_path] {
        if let Ok(modified) = fs::metadata(path).and_then(|m| m.modified()) {
            return Some(modified.into());
        }
    }
    None
}

/// Delete unreferenced attachment pairs older than `grace`.
///
/// Safety rules:
/// - a DB error aborts the pass before any delete (fail closed);
/// - malformed sidecars and unparseable ages are kept;
/// - ids found in `comment_attachment_reference_bodies` are always kept;
/// - data files without sidecars and sidecars without data are both swept
///   once they age past the grace window.
pub async fn gc_orphan_comment_attachments(
    db: &Database,
    uploads_dir: &Path,
    grace: Duration,
) -> Result<CommentAttachmentGcStats, String> {
    let mut stats = CommentAttachmentGcStats::default();
    if db.is_skipped() {
        return Ok(stats);
    }
    let dir = attachments_dir(uploads_dir);
    if !dir.is_dir() {
        return Ok(stats);
    }
    // A zero/near-zero grace would race in-flight uploads (data file lands
    // before its sidecar) — keep a floor so fresh pairs always survive a pass.
    let grace = grace.max(Duration::from_secs(60));
    let cutoff = chrono::Utc::now()
        - chrono::Duration::from_std(grace).unwrap_or_else(|_| chrono::Duration::days(3650));
    let bodies = db.comment_attachment_reference_bodies().await?;
    let referenced = referenced_attachment_ids(&bodies);

    let mut ids = HashSet::new();
    let entries =
        fs::read_dir(&dir).map_err(|e| format!("comment attachment GC read_dir failed: {e}"))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some(id) = name.strip_suffix(".json") {
            ids.insert(id.to_string());
        } else if Uuid::parse_str(name).is_ok() {
            ids.insert(name.to_string());
        }
    }

    let mut candidates: Vec<(String, PathBuf, PathBuf, bool, bool)> = Vec::new();
    for id in ids {
        stats.scanned += 1;
        if Uuid::parse_str(&id).is_err() {
            stats.errors += 1;
            continue;
        }
        let data = data_path(&dir, &id);
        let meta_file = meta_path(&dir, &id);
        let meta_exists = meta_file.is_file();
        let data_exists = data.is_file();
        if !meta_exists && !data_exists {
            continue;
        }
        if referenced.contains(&id) {
            stats.kept_referenced += 1;
            continue;
        }
        let meta: Option<CommentAttachmentMeta> = if meta_exists {
            match fs::read(&meta_file) {
                Ok(bytes) => match serde_json::from_slice(&bytes) {
                    Ok(meta) => Some(meta),
                    Err(_) => {
                        // Malformed metadata is ambiguous — keep the pair.
                        stats.errors += 1;
                        continue;
                    }
                },
                Err(e) => {
                    stats.errors += 1;
                    tracing::warn!(error = %e, id, "comment attachment meta read failed");
                    None
                }
            }
        } else {
            None
        };
        let Some(created) = created_at(meta.as_ref(), &data, &meta_file) else {
            stats.errors += 1;
            continue;
        };
        if created > cutoff {
            stats.kept_recent += 1;
            continue;
        }
        candidates.push((id, data, meta_file, meta_exists, data_exists));
    }

    if candidates.is_empty() {
        return Ok(stats);
    }

    // Phase 2: references are re-scanned immediately before deleting — a
    // comment/reply saved while phase 1 ran must keep its attachment.
    let bodies = db.comment_attachment_reference_bodies().await?;
    let referenced = referenced_attachment_ids(&bodies);
    for (id, data, meta_file, meta_exists, data_exists) in candidates {
        if referenced.contains(&id) {
            stats.kept_referenced += 1;
            continue;
        }
        let mut removed_pair = true;
        if data_exists {
            if let Err(e) = fs::remove_file(&data) {
                stats.errors += 1;
                tracing::warn!(error = %e, id, "comment attachment data delete failed");
                removed_pair = false;
            }
        }
        if meta_exists {
            if let Err(e) = fs::remove_file(&meta_file) {
                stats.errors += 1;
                tracing::warn!(error = %e, id, "comment attachment meta delete failed");
                removed_pair = false;
            }
        }
        if removed_pair {
            stats.removed += 1;
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referenced_ids_extracts_and_normalizes_uuid() {
        let upper = "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA";
        let lower = upper.to_lowercase();
        let bodies = vec![format!("x ![a](/uploads/comment-attachments/{upper}) y")];
        let ids = referenced_attachment_ids(&bodies);
        assert!(ids.contains(&lower));
    }

    #[test]
    fn quota_counts_only_matching_uploader() {
        let dir = tempfile::tempdir().expect("tempdir");
        let id = Uuid::new_v4().to_string();
        let meta = CommentAttachmentMeta {
            name: "a.png".into(),
            content_type: "image/png".into(),
            size: 10,
            uploader_id: "u1".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        fs::write(
            meta_path(dir.path(), &id),
            serde_json::to_vec(&meta).expect("meta"),
        )
        .expect("write meta");
        let usage = user_attachment_usage(dir.path(), "u1").expect("usage");
        assert_eq!(usage.count, 1);
        assert_eq!(usage.bytes, 10);
        assert_eq!(
            user_attachment_usage(dir.path(), "u2")
                .expect("usage")
                .count,
            0
        );
    }
}
