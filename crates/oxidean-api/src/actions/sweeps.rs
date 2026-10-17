//! Stale-claim + job-timeout sweeper (Phase 8).
//!
//! Every runner contact (`update_log`/`update_task`) bumps the job's
//! `heartbeat_at`; a runner that dies mid-job stops bumping. The sweeper:
//!
//! - `timeout_at` lapsed while `in_progress` → `failure` (GitHub's
//!   `timeout-minutes` kill) + `::error::` log line + job-token revoke.
//! - `COALESCE(heartbeat_at, started_at)` older than `stale_secs` → the job
//!   requeues (`attempt + 1`, runner binding cleared) + `::warning::` log +
//!   job-token revoke — a zombie's later calls 403 on `runner_id` mismatch.
//!
//! Runs on the 30 s actions sweep tick in `jobs/schedule.rs`.

use oxidean_db::{Database, StaleJobRow};

/// Stale-claim threshold — `OXIDEAN_ACTIONS_STALE_JOB_SECS` (default 300 s).
/// Runners heartbeat on every log chunk; five silent minutes means dead.
pub fn stale_job_secs() -> u64 {
    std::env::var("OXIDEAN_ACTIONS_STALE_JOB_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300)
}

/// Lenient timestamp parse — `heartbeat_at`/`timeout_at` are RFC3339 we
/// write; `started_at` is dialect-native (`YYYY-MM-DD HH:MM:SS[.f][+00]`).
/// Missing/unparseable → `None` (treated as never-contacted for heartbeat,
/// never-deadline for `timeout_at`).
fn parse_ts(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(d.with_timezone(&chrono::Utc));
    }
    // Dialect-native `YYYY-MM-DD HH:MM:SS[.ffffff][+00]` → RFC3339 shape.
    let mut s = raw.trim().to_string();
    if s.len() > 10 && s.as_bytes()[10] == b' ' {
        s.replace_range(10..11, "T");
    }
    for suffix in ["+00", "-00", " UTC"] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            s = stripped.to_string();
            break;
        }
    }
    if !s.ends_with('Z') {
        s.push('Z');
    }
    chrono::DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .ok()
}

/// Sweep one pass: deadline-fail + stale-requeue. Returns
/// `(timed_out, requeued)` counts for logging.
pub async fn sweep_stale_action_jobs(db: &Database, stale_secs: u64) -> (u64, u64) {
    let jobs = match db.list_in_progress_action_jobs().await {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!(error = %e, "stale-claim sweep: list failed");
            return (0, 0);
        }
    };
    let now = chrono::Utc::now();
    let stale_cutoff = now - chrono::Duration::seconds(stale_secs as i64);
    let mut timed_out = 0u64;
    let mut requeued = 0u64;
    for job in jobs {
        if let Some(expired) = deadline_lapsed(&job, now) {
            if expired {
                timed_out += fail_deadline_job(db, &job).await as u64;
                continue;
            }
        }
        let last_contact = job
            .heartbeat_at
            .as_deref()
            .or(job.started_at.as_deref())
            .and_then(parse_ts);
        let stale = match last_contact {
            Some(t) => t < stale_cutoff,
            // Never stamped — can only happen on pre-Phase-8 rows; started_at
            // is set at claim, so treat missing as stale to recover them.
            None => true,
        };
        if stale {
            requeued += requeue_dead_runner_job(db, &job, stale_secs).await as u64;
        }
    }
    (timed_out, requeued)
}

fn deadline_lapsed(job: &StaleJobRow, now: chrono::DateTime<chrono::Utc>) -> Option<bool> {
    job.timeout_at
        .as_deref()
        .and_then(parse_ts)
        .map(|deadline| deadline < now)
}

/// Fail one deadline-lapsed job — writes the `::error::` notice into its log
/// first so the log explains the kill even when the DB write lands last.
async fn fail_deadline_job(db: &Database, job: &StaleJobRow) -> bool {
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let _ = crate::actions::logs::append_job_log(
        &crate::actions::logs::resolve_log_dir(),
        &job.run_id,
        &job.id,
        b"::error::job timed out \xe2\x80\x94 timeout-minutes deadline lapsed\n",
    )
    .await;
    let landed = db
        .fail_action_job_deadline(&job.id, &at)
        .await
        .unwrap_or(false);
    if landed {
        if let Err(e) = db.revoke_action_job_token_for_job(&job.id).await {
            tracing::warn!(error = %e, job_id = %job.id, "deadline kill: token revoke failed");
        }
        if let Err(e) = db.recompute_action_run_status(&job.run_id).await {
            tracing::warn!(error = %e, run_id = %job.run_id, "deadline kill: run rollup failed");
        }
        if let Err(e) = Box::pin(crate::actions::scheduler::recompute_ready_jobs(
            db,
            &job.run_id,
        ))
        .await
        {
            tracing::warn!(error = %e, run_id = %job.run_id, "deadline kill: recompute failed");
        }
        tracing::info!(job_id = %job.id, job_key = %job.job_key, "job failed: timeout-minutes deadline");
    }
    landed
}

/// Requeue one stale job — log notice + token revoke so the dead runner's
/// credentials die with it.
async fn requeue_dead_runner_job(db: &Database, job: &StaleJobRow, stale_secs: u64) -> bool {
    let _ = crate::actions::logs::append_job_log(
        &crate::actions::logs::resolve_log_dir(),
        &job.run_id,
        &job.id,
        format!(
            "::warning::runner lost contact for over {stale_secs}s — job requeued (attempt {})\n",
            job.attempt + 1
        )
        .as_bytes(),
    )
    .await;
    let landed = db.requeue_stale_action_job(&job.id).await.unwrap_or(false);
    if landed {
        if let Err(e) = db.revoke_action_job_token_for_job(&job.id).await {
            tracing::warn!(error = %e, job_id = %job.id, "stale requeue: token revoke failed");
        }
        tracing::info!(
            job_id = %job.id,
            job_key = %job.job_key,
            runner = ?job.runner_id,
            attempt = job.attempt + 1,
            "stale claim requeued — runner heartbeat lost"
        );
    }
    landed
}

/// Log retention — `OXIDEAN_ACTIONS_LOG_RETENTION_DAYS` (default 90, GitHub's
/// retention). `0` keeps logs forever. Compression runs regardless — a
/// forever-keep instance still wants gzipped cold storage.
pub fn log_retention_days() -> u64 {
    std::env::var("OXIDEAN_ACTIONS_LOG_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(90)
}

/// Terminal run statuses — only finished runs get compressed/expired logs.
fn run_is_terminal(status: &str) -> bool {
    matches!(status, "success" | "failure" | "cancelled")
}

/// Log maintenance pass over `log_dir`: for every run directory whose run is
/// terminal, gzip each `*.log` to `*.log.gz`; when `retention_days` is nonzero
/// and every file in the directory is older than the cutoff, delete the
/// directory. Returns `(compressed, deleted_runs)`.
///
/// Newest-file-mtime drives the retention cutoff rather than the directory's
/// own mtime — a stray late append (zombie runner) keeps the directory alive.
pub async fn sweep_action_logs(
    db: &Database,
    log_dir: &std::path::Path,
    retention_days: u64,
) -> (u64, u64) {
    let mut runs = match tokio::fs::read_dir(log_dir).await {
        Ok(r) => r,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, "actions log sweep: read_dir failed");
            }
            return (0, 0);
        }
    };
    let cutoff = (retention_days > 0).then(|| {
        std::time::SystemTime::now() - std::time::Duration::from_secs(retention_days * 86_400)
    });
    let mut compressed = 0u64;
    let mut deleted = 0u64;
    loop {
        let ent = match runs.next_entry().await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(error = %e, "actions log sweep: entry failed");
                break;
            }
        };
        let is_dir = ent.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
        if !is_dir {
            continue;
        }
        let run_id = ent.file_name().to_string_lossy().to_string();
        let terminal = db
            .find_action_run_by_id(&run_id)
            .await
            .ok()
            .flatten()
            .is_some_and(|r| run_is_terminal(&r.status));
        if !terminal {
            continue;
        }
        // Collect job log files + the newest mtime in one pass.
        let mut newest: Option<std::time::SystemTime> = None;
        let mut job_ids: Vec<String> = Vec::new();
        let mut files = match tokio::fs::read_dir(ent.path()).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(error = %e, run_id = %run_id, "actions log sweep: run dir failed");
                continue;
            }
        };
        while let Ok(Some(f)) = files.next_entry().await {
            if let Ok(m) = f.metadata().await {
                if let Ok(mtime) = m.modified() {
                    newest = Some(newest.map_or(mtime, |n: std::time::SystemTime| n.max(mtime)));
                }
            }
            if let Some(stem) = f.file_name().to_str().and_then(|n| n.strip_suffix(".log")) {
                job_ids.push(stem.to_string());
            }
        }
        if cutoff.is_some_and(|c| newest.is_some_and(|m| m < c)) {
            if let Err(e) = tokio::fs::remove_dir_all(ent.path()).await {
                tracing::warn!(error = %e, run_id = %run_id, "actions log retention: delete failed");
            } else {
                deleted += 1;
            }
            continue;
        }
        for job_id in job_ids {
            match crate::actions::logs::compress_job_log(log_dir, &run_id, &job_id).await {
                Ok(true) => compressed += 1,
                Ok(false) => {}
                Err(e) => tracing::warn!(
                    error = %e,
                    run_id = %run_id,
                    job_id = %job_id,
                    "actions log compress failed"
                ),
            }
        }
    }
    (compressed, deleted)
}

#[cfg(test)]
mod tests {
    use super::parse_ts;

    #[test]
    fn parses_every_dialect_timestamp_shape() {
        // RFC3339 (what we write into heartbeat_at/timeout_at).
        let a = parse_ts("2026-10-17T10:30:00Z").unwrap();
        // SQLite/MySQL CURRENT_TIMESTAMP shape.
        let b = parse_ts("2026-10-17 10:30:00").unwrap();
        // Postgres ::text shape (fractional + offset) — microseconds survive.
        let c = parse_ts("2026-10-17 10:30:00.123456+00").unwrap();
        let d = parse_ts("2026-10-17T10:30:00.123456Z").unwrap();
        assert_eq!(a, b);
        assert_eq!(c, d);
        assert!(parse_ts("garbage").is_none());
    }
}
