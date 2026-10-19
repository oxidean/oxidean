//! Interval timers for orphan reconcile + git gc + LFS GC + package blob GC (D-36 / D-37 / D-LFS / D-PKG-09).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use oxidean_db::Database;
use oxidean_git::GitBackend;
use tokio::time::{interval, MissedTickBehavior};

use super::reconcile::{orphan_reconcile, run_gc_all};

/// Background job intervals from ENV (0 disables that job).
#[derive(Debug, Clone, Copy)]
pub struct JobConfig {
    /// Default 24h. Set `OXIDEAN_ORPHAN_RECONCILE_INTERVAL_SECS=0` to disable.
    pub orphan_interval: Duration,
    /// Default 7d. Set `OXIDEAN_GIT_GC_INTERVAL_SECS=0` to disable.
    pub gc_interval: Duration,
    /// Default 24h. Set `OXIDEAN_LFS_GC_INTERVAL_SECS=0` to disable.
    pub lfs_gc_interval: Duration,
    /// Default 24h. Set `OXIDEAN_PACKAGES_GC_INTERVAL_SECS=0` to disable.
    pub packages_gc_interval: Duration,
    /// Default 30s. Set `OXIDEAN_ACTIONS_SCHEDULE_SWEEP_SECS=0` to disable the
    /// `on.schedule` cron sweeper.
    pub actions_schedule_sweep: Duration,
    /// Default 1h. Set `OXIDEAN_ACTIONS_LOG_SWEEP_SECS=0` to disable the
    /// finished-log gzip + retention sweeper.
    pub actions_log_sweep: Duration,
}

impl JobConfig {
    pub fn from_env() -> Self {
        let orphan_secs = parse_u64_env("OXIDEAN_ORPHAN_RECONCILE_INTERVAL_SECS", 86_400);
        let gc_secs = parse_u64_env("OXIDEAN_GIT_GC_INTERVAL_SECS", 604_800);
        let lfs_gc_secs = parse_u64_env(
            "OXIDEAN_LFS_GC_INTERVAL_SECS",
            super::lfs_gc::DEFAULT_GC_INTERVAL_SECS,
        );
        let packages_gc_secs = parse_u64_env("OXIDEAN_PACKAGES_GC_INTERVAL_SECS", 86_400);
        let actions_schedule_secs = parse_u64_env("OXIDEAN_ACTIONS_SCHEDULE_SWEEP_SECS", 30);
        let actions_log_secs = parse_u64_env("OXIDEAN_ACTIONS_LOG_SWEEP_SECS", 3_600);
        Self {
            orphan_interval: Duration::from_secs(orphan_secs),
            gc_interval: Duration::from_secs(gc_secs),
            lfs_gc_interval: Duration::from_secs(lfs_gc_secs),
            packages_gc_interval: Duration::from_secs(packages_gc_secs),
            actions_schedule_sweep: Duration::from_secs(actions_schedule_secs),
            actions_log_sweep: Duration::from_secs(actions_log_secs),
        }
    }
}

fn parse_u64_env(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Spawn in-process orphan reconcile + scheduled gc + LFS GC + package blob GC + VCS object GC + webhook delivery loops.
pub fn spawn_background_jobs(
    db: Database,
    git: Arc<dyn GitBackend>,
    repos_dir: PathBuf,
    lfs_dir: PathBuf,
    packages_dir: PathBuf,
    config: JobConfig,
    vcs: Option<crate::vcs::SharedVcs>,
) {
    let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
    crate::webhook::worker::spawn_webhook_worker(db.clone(), env_name.clone());
    crate::mirror::spawn_mirror_poller(db.clone(), git.clone(), repos_dir.clone());
    if config.orphan_interval.as_secs() > 0 {
        let db_o = db.clone();
        let repos_o = repos_dir.clone();
        let period = config.orphan_interval;
        tokio::spawn(async move {
            let mut ticker = interval(period);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            // First tick completes immediately — run once after boot, then on interval.
            loop {
                ticker.tick().await;
                match orphan_reconcile(&db_o, &repos_o).await {
                    Ok(stats) => {
                        if stats.orphans_removed > 0
                            || stats.purged_soft_deleted > 0
                            || stats.purged_expired_redirects > 0
                            || stats.errors > 0
                        {
                            tracing::info!(
                                orphans = stats.orphans_removed,
                                purged = stats.purged_soft_deleted,
                                redirects = stats.purged_expired_redirects,
                                errors = stats.errors,
                                "orphan reconcile completed"
                            );
                        } else {
                            tracing::debug!("orphan reconcile: nothing to do");
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "orphan reconcile failed"),
                }
            }
        });
        tracing::info!(
            secs = config.orphan_interval.as_secs(),
            "orphan reconcile job scheduled"
        );
    } else {
        tracing::info!("orphan reconcile job disabled (interval 0)");
    }

    if config.actions_schedule_sweep.as_secs() > 0 {
        let db_c = db.clone();
        let git_c = git.clone();
        let repos_c = repos_dir.clone();
        let env_c = env_name.clone();
        let period = config.actions_schedule_sweep;
        tokio::spawn(async move {
            let mut ticker = interval(period);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            // Skip the immediate boot tick so a restart mid-minute cannot
            // claim a tick the booting sweeper never observed; dedupe still
            // guards the real double-fire window.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                match crate::actions::cron::sweep_scheduled_workflows(
                    &db_c,
                    git_c.clone(),
                    &repos_c,
                )
                .await
                {
                    Ok(n) => {
                        if n > 0 {
                            tracing::info!(enqueued = n, "actions schedule sweep fired");
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "actions schedule sweep failed"),
                }
                // Environment wait timers — promote legs whose deadline
                // lapsed (the same 30s granularity; GitHub's timers are
                // minute-granularity anyway).
                let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                match db_c.promote_due_deployment_jobs(&now).await {
                    Ok(ids) if !ids.is_empty() => {
                        for job_id in &ids {
                            crate::webhook::dispatch::notify_workflow_job(&db_c, job_id, "queued")
                                .await;
                            crate::webhook::dispatch::notify_deployment_job_status(&db_c, job_id)
                                .await;
                        }
                        crate::actions::task_wake::wake();
                        tracing::info!(promoted = ids.len(), "deployment wait timer released jobs");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "deployment wait timer sweep failed")
                    }
                    _ => {}
                }
                // Stale claims + job timeouts — requeue dead runners' legs,
                // fail legs past `timeout_at` (Phase 8).
                let (timed_out, requeued) = crate::actions::sweeps::sweep_stale_action_jobs(
                    &db_c,
                    git_c.as_ref(),
                    &repos_c,
                    crate::actions::sweeps::stale_job_secs(),
                    &env_c,
                )
                .await;
                if timed_out > 0 || requeued > 0 {
                    tracing::info!(timed_out, requeued, "stale-claim sweep reaped jobs");
                }
                // actions/cache upkeep — expired runtime tokens + abandoned
                // `reserved` rows (their `.part` payloads get unlinked).
                match crate::actions::cache::sweep_action_caches(
                    &db_c,
                    &crate::actions::cache::resolve_cache_dir(),
                )
                .await
                {
                    Ok(n) if n > 0 => {
                        tracing::info!(reaped = n, "actions cache sweep reaped rows");
                    }
                    Err(e) => tracing::error!(error = %e, "actions cache sweep failed"),
                    _ => {}
                }
                // Twirp artifact reservations abandoned between
                // CreateArtifact and FinalizeArtifact (job died mid-upload).
                match crate::actions::artifacts_v2::sweep_pending_artifacts(
                    &db_c,
                    &crate::actions::artifacts::resolve_artifact_dir(),
                )
                .await
                {
                    Ok(n) if n > 0 => {
                        tracing::info!(reaped = n, "pending artifact sweep reaped rows");
                    }
                    Err(e) => tracing::error!(error = %e, "pending artifact sweep failed"),
                    _ => {}
                }
            }
        });
        tracing::info!(
            secs = config.actions_schedule_sweep.as_secs(),
            "actions schedule sweeper scheduled"
        );
    } else {
        tracing::info!("actions schedule sweeper disabled (interval 0)");
    }

    if config.actions_log_sweep.as_secs() > 0 {
        let db_l = db.clone();
        let period = config.actions_log_sweep;
        tokio::spawn(async move {
            let mut ticker = interval(period);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let log_dir = crate::actions::logs::resolve_log_dir();
                let (compressed, deleted) = crate::actions::sweeps::sweep_action_logs(
                    &db_l,
                    &log_dir,
                    crate::actions::sweeps::log_retention_days(),
                )
                .await;
                if compressed > 0 || deleted > 0 {
                    tracing::info!(compressed, deleted, "actions log maintenance pass done");
                }
            }
        });
        tracing::info!(
            secs = config.actions_log_sweep.as_secs(),
            "actions log maintenance sweeper scheduled"
        );
    } else {
        tracing::info!("actions log maintenance sweeper disabled (interval 0)");
    }

    if config.gc_interval.as_secs() > 0 {
        let db_g = db.clone();
        let git_g = git;
        let repos_g = repos_dir;
        let period = config.gc_interval;
        tokio::spawn(async move {
            let mut ticker = interval(period);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match run_gc_all(&db_g, git_g.as_ref(), &repos_g).await {
                    Ok((ok, err)) => {
                        if ok > 0 || err > 0 {
                            tracing::info!(gc_ok = ok, gc_err = err, "scheduled git gc completed");
                        } else {
                            tracing::debug!("scheduled git gc: no active repos");
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "scheduled git gc failed"),
                }
            }
        });
        tracing::info!(secs = config.gc_interval.as_secs(), "git gc job scheduled");
    } else {
        tracing::info!("git gc job disabled (interval 0)");
    }

    if config.lfs_gc_interval.as_secs() > 0 {
        let db_l = db.clone();
        let lfs_l = lfs_dir;
        let period = config.lfs_gc_interval;
        let grace = super::lfs_gc::gc_grace_from_env();
        tokio::spawn(async move {
            let mut ticker = interval(period);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match super::lfs_gc::run_lfs_gc(&db_l, &lfs_l, grace).await {
                    Ok(stats) => {
                        if stats.deleted > 0 || stats.errors > 0 || stats.skipped_locked {
                            tracing::info!(
                                deleted = stats.deleted,
                                errors = stats.errors,
                                skipped_locked = stats.skipped_locked,
                                "lfs gc completed"
                            );
                        } else {
                            tracing::debug!("lfs gc: nothing to do");
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "lfs gc failed"),
                }
            }
        });
        tracing::info!(
            secs = config.lfs_gc_interval.as_secs(),
            "lfs gc job scheduled"
        );
    } else {
        tracing::info!("lfs gc job disabled (interval 0)");
    }

    if config.packages_gc_interval.as_secs() > 0 {
        let db_p = db;
        let pkg_dir = packages_dir;
        let period = config.packages_gc_interval;
        tokio::spawn(async move {
            let mut ticker = interval(period);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match crate::packages::quota::gc_unref_blobs(&db_p, &pkg_dir).await {
                    Ok(n) => {
                        if n > 0 {
                            tracing::info!(removed = n, "package blob GC completed");
                        } else {
                            tracing::debug!("package blob GC: nothing to do");
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "package blob GC failed"),
                }
            }
        });
        tracing::info!(
            secs = config.packages_gc_interval.as_secs(),
            "package blob GC job scheduled"
        );
    } else {
        tracing::info!("package blob GC job disabled (interval 0)");
    }

    // VCS object-store GC — the interval is `OXIDEAN_VCS_GC_INTERVAL_SECS`
    // (frozen default 3600; `0` disables). A `None` slot means VCS bootstrap
    // refused — no objects dir exists to sweep, so the job simply isn't
    // spawned. `gc_once` honors the `.gc-lock` and `gc_grace_secs` itself;
    // stats/logs carry counts+bytes only, never object bytes (SD3.x).
    match vcs {
        Some(slot) if slot.gc_interval_secs > 0 => {
            let period = Duration::from_secs(slot.gc_interval_secs);
            let secs = slot.gc_interval_secs;
            tokio::spawn(async move {
                let mut ticker = interval(period);
                ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
                loop {
                    ticker.tick().await;
                    match slot.api.objects().gc_once().await {
                        Ok(stats) => {
                            if stats.orphans_removed > 0
                                || stats.rowless_removed > 0
                                || stats.tmp_removed > 0
                                || stats.errors > 0
                                || stats.skipped
                            {
                                tracing::info!(
                                    orphans = stats.orphans_removed,
                                    rowless = stats.rowless_removed,
                                    tmp = stats.tmp_removed,
                                    bytes = stats.bytes_reclaimed,
                                    skipped = stats.skipped,
                                    errors = stats.errors,
                                    "vcs object gc completed"
                                );
                            } else {
                                tracing::debug!("vcs object gc: nothing to do");
                            }
                        }
                        Err(e) => tracing::error!(error = %e, "vcs object gc failed"),
                    }
                }
            });
            tracing::info!(secs, "vcs object gc job scheduled");
        }
        Some(_) => tracing::info!("vcs object gc job disabled (interval 0)"),
        None => {}
    }
}
