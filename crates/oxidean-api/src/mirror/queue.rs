//! Per-repo coalesce queue: one in-flight + one pending follow-up.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oxidean_db::Database;
use oxidean_git::GitBackend;
use tokio::sync::Notify;

use super::engine::run_mirror_sync;

struct CoalesceState {
    /// Repo IDs currently syncing.
    in_flight: HashSet<String>,
    /// Repo IDs waiting for a follow-up after the current run.
    pending: HashSet<String>,
}

impl CoalesceState {
    fn new() -> Self {
        Self {
            in_flight: HashSet::new(),
            pending: HashSet::new(),
        }
    }
}

static QUEUE: std::sync::OnceLock<Arc<Mutex<CoalesceState>>> = std::sync::OnceLock::new();
static WAKE: std::sync::OnceLock<Arc<Notify>> = std::sync::OnceLock::new();

fn queue() -> Arc<Mutex<CoalesceState>> {
    QUEUE
        .get_or_init(|| Arc::new(Mutex::new(CoalesceState::new())))
        .clone()
}

fn wake() -> Arc<Notify> {
    WAKE.get_or_init(|| Arc::new(Notify::new())).clone()
}

/// Soft-fail enqueue after a local ref mutation (receive-pack, merge, branch RPC).
pub fn notify_mirror_after_local_mutation(
    db: Database,
    git: Arc<dyn GitBackend>,
    repos_dir: PathBuf,
    repository_id: String,
) {
    tokio::spawn(async move {
        // Quiet window so multi-ref pushes coalesce into one run.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        if let Err(e) = enqueue_mirror_for_repo(db, git, repos_dir, repository_id).await {
            tracing::warn!(error = %e, "mirror enqueue after local mutation failed");
        }
    });
}

/// Enqueue a two-way sync for `repository_id` (coalesced).
pub async fn enqueue_mirror_for_repo(
    db: Database,
    git: Arc<dyn GitBackend>,
    repos_dir: PathBuf,
    repository_id: String,
) -> Result<(), String> {
    let mirror = db
        .get_mirror_by_repo(&repository_id)
        .await?
        .filter(|m| m.enabled);
    let Some(mirror) = mirror else {
        return Ok(());
    };

    let should_spawn = {
        let q_arc = queue();
        let mut q = q_arc.lock().map_err(|e| e.to_string())?;
        if q.in_flight.contains(&repository_id) {
            q.pending.insert(repository_id.clone());
            false
        } else {
            q.in_flight.insert(repository_id.clone());
            true
        }
    };

    if should_spawn {
        let db2 = db.clone();
        let git2 = git.clone();
        let repos2 = repos_dir.clone();
        let rid = repository_id.clone();
        let mid = mirror.id.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = run_mirror_sync(&db2, git2.as_ref(), &repos2, &mid).await {
                    tracing::warn!(repo = %rid, error = %e, "mirror sync failed");
                }
                let again = {
                    let q_arc = queue();
                    let mut q = match q_arc.lock() {
                        Ok(g) => g,
                        Err(_) => break,
                    };
                    if q.pending.remove(&rid) {
                        true
                    } else {
                        q.in_flight.remove(&rid);
                        false
                    }
                };
                if !again {
                    break;
                }
            }
        });
    }

    wake().notify_one();
    Ok(())
}

/// Short poll backstop — scans enabled mirrors whose poll_interval_secs > 0.
pub fn spawn_mirror_poller(db: Database, git: Arc<dyn GitBackend>, repos_dir: PathBuf) {
    let tick_secs = std::env::var("OXIDEAN_MIRROR_POLL_TICK_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    if tick_secs == 0 {
        tracing::info!("mirror poller disabled (OXIDEAN_MIRROR_POLL_TICK_SECS=0)");
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(tick_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Track last enqueue per repo to respect per-row poll_interval_secs.
        let last: Arc<Mutex<HashMap<String, std::time::Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));
        loop {
            ticker.tick().await;
            let mirrors = match db.list_enabled_mirrors().await {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(error = %e, "mirror poller list failed");
                    continue;
                }
            };
            let now = std::time::Instant::now();
            for m in mirrors {
                if m.poll_interval_secs <= 0 {
                    continue;
                }
                let due = {
                    let map = match last.lock() {
                        Ok(g) => g,
                        Err(_) => continue,
                    };
                    match map.get(&m.repository_id) {
                        Some(t) => now.duration_since(*t).as_secs() as i64 >= m.poll_interval_secs,
                        None => true,
                    }
                };
                if !due {
                    continue;
                }
                if let Ok(mut map) = last.lock() {
                    map.insert(m.repository_id.clone(), now);
                }
                let _ = enqueue_mirror_for_repo(
                    db.clone(),
                    git.clone(),
                    repos_dir.clone(),
                    m.repository_id,
                )
                .await;
            }
        }
    });
}
