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
        Self {
            orphan_interval: Duration::from_secs(orphan_secs),
            gc_interval: Duration::from_secs(gc_secs),
            lfs_gc_interval: Duration::from_secs(lfs_gc_secs),
            packages_gc_interval: Duration::from_secs(packages_gc_secs),
        }
    }
}

fn parse_u64_env(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Spawn in-process orphan reconcile + scheduled gc + LFS GC + package blob GC + webhook delivery loops.
pub fn spawn_background_jobs(
    db: Database,
    git: Arc<dyn GitBackend>,
    repos_dir: PathBuf,
    lfs_dir: PathBuf,
    packages_dir: PathBuf,
    config: JobConfig,
) {
    let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
    crate::webhook::worker::spawn_webhook_worker(db.clone(), env_name);
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
}
