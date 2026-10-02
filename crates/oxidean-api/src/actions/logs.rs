//! Job log files under OXIDEAN_ACTIONS_LOG_DIR (D-ACT-13).

use std::path::{Path, PathBuf};

use tokio::fs;

/// `{log_dir}/{run_id}/{job_id}.log` — IDs must not contain path separators (T-19-05).
pub fn job_log_path(log_dir: &Path, run_id: &str, job_id: &str) -> Result<PathBuf, String> {
    reject_id(run_id)?;
    reject_id(job_id)?;
    Ok(log_dir.join(run_id).join(format!("{job_id}.log")))
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
    Ok(())
}

/// Read the job log tail starting at `offset` bytes. Returns
/// `(content_from_offset, total_size)` — `total_size` doubles as the caller's
/// next offset (`next_offset`).
///
/// `offset` past EOF clamps to empty; a mid-UTF-8 `offset` snaps forward to the
/// next char boundary (offsets handed out as `next_offset` are already
/// boundaries because runner appends are whole UTF-8 strings).
pub async fn read_job_log(
    log_dir: &Path,
    run_id: &str,
    job_id: &str,
    offset: u64,
) -> Result<(Vec<u8>, u64), String> {
    let path = job_log_path(log_dir, run_id, job_id)?;
    let mut bytes = fs::read(&path)
        .await
        .map_err(|e| format!("read log: {e}"))?;
    let size = bytes.len() as u64;
    let mut start = offset.min(size) as usize;
    while start < bytes.len() && (bytes[start] & 0xC0) == 0x80 {
        start += 1;
    }
    Ok((bytes.split_off(start), size))
}
