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
