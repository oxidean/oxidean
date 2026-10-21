//! Job log files under OXIDEAN_ACTIONS_LOG_DIR (D-ACT-13).

use std::path::{Path, PathBuf};

use tokio::fs;

/// `OXIDEAN_ACTIONS_LOG_DIR` (default `var/actions-logs`, relative to cwd) —
/// same contract `AppState::new` applies; shared by enqueue-side writers that
/// don't carry `AppState` (push dispatch over SSH/RPC).
pub fn resolve_log_dir() -> PathBuf {
    let dir = std::env::var("OXIDEAN_ACTIONS_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/actions-logs"));
    if dir.is_absolute() {
        dir
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(dir)
    }
}

/// `{log_dir}/{run_id}/{job_id}.log` — IDs must not contain path separators (T-19-05).
pub fn job_log_path(log_dir: &Path, run_id: &str, job_id: &str) -> Result<PathBuf, String> {
    reject_id(run_id)?;
    reject_id(job_id)?;
    Ok(log_dir.join(run_id).join(format!("{job_id}.log")))
}

/// `{log_dir}/{run_id}/{job_id}.log.gz` — compressed-at-rest twin written by
/// the log-maintenance sweep once the run goes terminal.
fn job_log_gz_path(log_dir: &Path, run_id: &str, job_id: &str) -> Result<PathBuf, String> {
    reject_id(run_id)?;
    reject_id(job_id)?;
    Ok(log_dir.join(run_id).join(format!("{job_id}.log.gz")))
}

fn reject_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err("invalid actions log id".into());
    }
    Ok(())
}

/// Live-tail wake channel — one broadcast for every job; each successful
/// append publishes the job_id so SSE tailers re-read the file immediately
/// instead of waiting out a poll interval. A lagging subscriber simply
/// re-reads on its next tick — the file cursor, not the channel, is the
/// source of truth for what bytes are new.
fn log_wakes() -> &'static tokio::sync::broadcast::Sender<String> {
    static WAKES: std::sync::OnceLock<tokio::sync::broadcast::Sender<String>> =
        std::sync::OnceLock::new();
    WAKES.get_or_init(|| tokio::sync::broadcast::channel(1024).0)
}

/// Subscribe to append notifications — payload is the appended job_id.
pub(crate) fn subscribe_job_log_wakes() -> tokio::sync::broadcast::Receiver<String> {
    log_wakes().subscribe()
}

pub async fn append_job_log(
    log_dir: &Path,
    run_id: &str,
    job_id: &str,
    chunk: &[u8],
) -> Result<(), String> {
    let path = job_log_path(log_dir, run_id, job_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("create log dir: {e}"))?;
    }
    use tokio::io::AsyncWriteExt;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .map_err(|e| format!("open log: {e}"))?;
    f.write_all(chunk)
        .await
        .map_err(|e| format!("write log: {e}"))?;
    // Flush before drop so a following read_job_log in the same process
    // cannot observe an empty file (tokio File drop alone is racy under load).
    f.flush().await.map_err(|e| format!("flush log: {e}"))?;
    let _ = log_wakes().send(job_id.to_string());
    Ok(())
}

/// Delete the plain and compressed-at-rest log files for a job — rerun and
/// stale-claim requeues call this so a new attempt starts with an empty log
/// instead of appending to the dead attempt's output. Missing files are fine.
pub async fn delete_job_log(log_dir: &Path, run_id: &str, job_id: &str) -> Result<(), String> {
    for path in [
        job_log_path(log_dir, run_id, job_id)?,
        job_log_gz_path(log_dir, run_id, job_id)?,
    ] {
        match fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("remove log {}: {e}", path.display())),
        }
    }
    Ok(())
}

/// Byte size of the job log on disk — 0 when absent. Stat-only, unlike
/// [`read_job_log`] which materializes the file.
pub async fn job_log_size(log_dir: &Path, run_id: &str, job_id: &str) -> Result<u64, String> {
    let path = job_log_path(log_dir, run_id, job_id)?;
    match fs::metadata(&path).await {
        Ok(m) => Ok(m.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(format!("stat log: {e}")),
    }
}

/// Bounded append (Phase 8): chunks past `max_bytes` land a single truncation
/// marker instead of growing the file — a runaway/stolen runner can no longer
/// fill the disk. Returns `true` when the chunk actually appended.
pub async fn append_job_log_bounded(
    log_dir: &Path,
    run_id: &str,
    job_id: &str,
    chunk: &[u8],
    max_bytes: u64,
) -> Result<bool, String> {
    let size = job_log_size(log_dir, run_id, job_id).await?;
    if size >= max_bytes {
        return Ok(false);
    }
    if size + chunk.len() as u64 > max_bytes {
        append_job_log(
            log_dir,
            run_id,
            job_id,
            format!("::warning::job log truncated at {max_bytes} bytes\n").as_bytes(),
        )
        .await?;
        return Ok(false);
    }
    append_job_log(log_dir, run_id, job_id, chunk).await?;
    Ok(true)
}

/// Compress `<job_id>.log` to `<job_id>.log.gz` and remove the plain file —
/// called by the maintenance sweep for terminal runs. A late-arriving `.log`
/// beside an existing `.gz` (zombie append post-compress) is merged: the `.gz`
/// is decompressed, the stray bytes appended, and the `.gz` rewritten.
/// Idempotent: no `.log` is a no-op. Returns whether a file was compressed
/// or merged this call.
pub async fn compress_job_log(log_dir: &Path, run_id: &str, job_id: &str) -> Result<bool, String> {
    let path = job_log_path(log_dir, run_id, job_id)?;
    let gz_path = job_log_gz_path(log_dir, run_id, job_id)?;
    let gz_exists = fs::metadata(&gz_path).await.is_ok();
    let bytes = match fs::read(&path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("read log for compress: {e}")),
    };
    let prior = if gz_exists {
        match fs::read(&gz_path).await {
            Ok(g) => Some(g),
            Err(e) => return Err(format!("read compressed log: {e}")),
        }
    } else {
        None
    };
    let gz = tokio::task::spawn_blocking(move || {
        use flate2::{read::GzDecoder, write::GzEncoder, Compression};
        use std::io::{Read, Write};
        let mut plain = Vec::with_capacity(bytes.len());
        if let Some(g) = prior {
            GzDecoder::new(g.as_slice())
                .read_to_end(&mut plain)
                .map_err(|e| e.to_string())?;
        }
        plain.extend_from_slice(&bytes);
        let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
        enc.write_all(&plain)
            .and_then(|_| enc.finish())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("compress log task: {e}"))??;
    fs::write(&gz_path, &gz)
        .await
        .map_err(|e| format!("write compressed log: {e}"))?;
    fs::remove_file(&path)
        .await
        .map_err(|e| format!("remove plain log: {e}"))?;
    Ok(true)
}

/// One `##[ann]` marker decoded at ingest — a GitHub `::error|warning|
/// notice::` workflow command carried out-of-band by the runner.
/// Field names follow GitHub's check-run annotation shape.
#[derive(Debug, Clone)]
pub struct JobAnnotation {
    pub level: String,
    pub message: String,
    pub path: Option<String>,
    pub start_line: Option<i64>,
    pub end_line: Option<i64>,
    pub start_column: Option<i64>,
    pub end_column: Option<i64>,
    pub title: Option<String>,
}

/// Split `##[ann]{json}` marker lines out of an incoming log chunk: the
/// returned text drops markers (they are runner-side metadata, not log
/// content) and the returned list carries the parsed annotations to persist.
/// A marker whose JSON fails to parse stays in the text — never lose log
/// content to a malformed line.
pub fn extract_annotations(chunk: &[u8]) -> (Vec<u8>, Vec<JobAnnotation>) {
    let text = match std::str::from_utf8(chunk) {
        Ok(t) => t,
        // Binary chunks can't carry markers — pass through untouched.
        Err(_) => return (chunk.to_vec(), Vec::new()),
    };
    let mut out = String::with_capacity(text.len());
    let mut anns = Vec::new();
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        match body
            .strip_prefix("##[ann]")
            .and_then(|j| parse_annotation(j).ok())
        {
            Some(a) => anns.push(a),
            None => out.push_str(line),
        }
    }
    (out.into_bytes(), anns)
}

fn parse_annotation(json: &str) -> Result<JobAnnotation, ()> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| ())?;
    let get_s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(String::from);
    let get_n = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_u64().or_else(|| x.as_i64().map(|i| i as u64)))
            .map(|n| n as i64)
    };
    let level = get_s("level").filter(|l| matches!(l.as_str(), "error" | "warning" | "notice"));
    let message = get_s("message");
    match (level, message) {
        (Some(level), Some(message)) => Ok(JobAnnotation {
            level,
            message,
            path: get_s("path"),
            start_line: get_n("line"),
            end_line: get_n("end_line"),
            start_column: get_n("col"),
            end_column: get_n("end_col"),
            title: get_s("title"),
        }),
        _ => Err(()),
    }
}

/// Persist annotations extracted by [`extract_annotations`] — annotation
/// loss is tolerable (the `##[level]` text is already in the log), so an
/// insert failure logs a warning rather than failing the log append.
pub async fn persist_annotations(
    db: &oxidean_db::Database,
    run_id: &str,
    job_id: &str,
    anns: Vec<JobAnnotation>,
) {
    for a in anns {
        let row = oxidean_db::actions::ActionAnnotationRow {
            id: uuid::Uuid::new_v4().to_string(),
            run_id: run_id.to_string(),
            job_id: job_id.to_string(),
            annotation_level: a.level,
            message: a.message,
            path: a.path,
            start_line: a.start_line,
            end_line: a.end_line,
            start_column: a.start_column,
            end_column: a.end_column,
            title: a.title,
            created_at: String::new(),
        };
        if let Err(e) = db.insert_action_annotation(&row).await {
            tracing::warn!("action annotation insert failed for job {job_id}: {e}");
        }
    }
}

/// Read the job log tail starting at `offset` bytes. Returns
/// `(content_from_offset, total_size)` — `total_size` doubles as the caller's
/// next offset (`next_offset`).
///
/// `offset` past EOF clamps to empty; a mid-UTF-8 `offset` snaps forward to the
/// next char boundary (offsets handed out as `next_offset` are already
/// boundaries because runner appends are whole UTF-8 strings).
///
/// Compressed-at-rest logs (`<job_id>.log.gz`, terminal runs) are decompressed
/// whole — `offset` still applies to the decompressed bytes so offset-based
/// tails keep working across the compress boundary.
pub async fn read_job_log(
    log_dir: &Path,
    run_id: &str,
    job_id: &str,
    offset: u64,
) -> Result<(Vec<u8>, u64), String> {
    let path = job_log_path(log_dir, run_id, job_id)?;
    let mut bytes = match fs::read(&path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let gz_path = job_log_gz_path(log_dir, run_id, job_id)?;
            let gz = fs::read(&gz_path)
                .await
                .map_err(|e| format!("read log: {e}"))?;
            tokio::task::spawn_blocking(move || {
                use flate2::read::GzDecoder;
                use std::io::Read;
                let mut out = Vec::new();
                GzDecoder::new(gz.as_slice())
                    .read_to_end(&mut out)
                    .map(|_| out)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("decompress log task: {e}"))??
        }
        Err(e) => return Err(format!("read log: {e}")),
    };
    let size = bytes.len() as u64;
    let mut start = offset.min(size) as usize;
    while start < bytes.len() && (bytes[start] & 0xC0) == 0x80 {
        start += 1;
    }
    Ok((bytes.split_off(start), size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_annotations_strips_markers_and_parses_fields() {
        let chunk = b"step out\n##[ann]{\"level\":\"error\",\"message\":\"boom\",\"path\":\"src/main.rs\",\"line\":3,\"end_line\":4,\"col\":1,\"end_col\":9,\"title\":\"Lint\"}\n##[error]boom\n";
        let (text, anns) = extract_annotations(chunk);
        assert_eq!(
            String::from_utf8(text).unwrap(),
            "step out\n##[error]boom\n"
        );
        assert_eq!(anns.len(), 1);
        let a = &anns[0];
        assert_eq!(a.level, "error");
        assert_eq!(a.message, "boom");
        assert_eq!(a.path.as_deref(), Some("src/main.rs"));
        assert_eq!(a.start_line, Some(3));
        assert_eq!(a.end_line, Some(4));
        assert_eq!(a.start_column, Some(1));
        assert_eq!(a.end_column, Some(9));
        assert_eq!(a.title.as_deref(), Some("Lint"));
    }

    #[test]
    fn extract_annotations_keeps_malformed_markers_in_text() {
        let chunk = b"##[ann]{not json}\n##[ann]{\"level\":\"bogus\"}\nok\n";
        let (text, anns) = extract_annotations(chunk);
        assert_eq!(
            String::from_utf8(text).unwrap(),
            "##[ann]{not json}\n##[ann]{\"level\":\"bogus\"}\nok\n"
        );
        assert!(anns.is_empty());
    }

    #[test]
    fn extract_annotations_passes_binary_through() {
        let chunk: &[u8] = b"##[ann]{\"level\":\"notice\",\"message\":\"x\"}\xff\xfe\n";
        let (text, anns) = extract_annotations(chunk);
        assert_eq!(text, chunk);
        assert!(anns.is_empty());
    }

    #[test]
    fn extract_annotations_minimal_marker() {
        let chunk = b"##[ann]{\"level\":\"notice\",\"message\":\"hi\"}\n";
        let (text, anns) = extract_annotations(chunk);
        assert!(text.is_empty());
        assert_eq!(anns[0].level, "notice");
        assert_eq!(anns[0].message, "hi");
        assert!(anns[0].path.is_none());
    }
}
