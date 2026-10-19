//! Grace-windowed GC (WF4/WF5; SD3.4 lock + SD3.5 reclaim order).
//!
//! Pass order per pin: absent dir ⇒ zero stats → marker gate → `.gc-lock`
//! (the lock never precedes the gates) → orphan-row pass (conditional row
//! delete BEFORE file unlink) → shard-shaped row-less sweep → stale `*.tmp`
//! sweep. Per-object failures are `%hash`/`%error` warn logs — never paths,
//! env values, or content (SR3.5).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::paths::{self, GC_LOCK_FILE, TMP_DIR};
use super::{scan_objects_dir, ObjectStore, ObjectStoreError, VcsGcStats};

impl ObjectStore {
    /// Run one GC pass (WF4). Ordering (pinned): fail-close on skipped db →
    /// absent dir ⇒ `Ok(zero stats)` → marker gate (`Config` on a markerless
    /// non-empty dir — runtime marker deletion must not turn into a sweep) →
    /// lock acquire → orphan-row pass → row-less sweep → `*.tmp` sweep.
    pub async fn gc_once(&self) -> Result<VcsGcStats, ObjectStoreError> {
        let mut stats = VcsGcStats::default();
        if self.db.is_skipped() {
            return Err(ObjectStoreError::DbNotConfigured);
        }
        let dir = self.config.objects_dir.clone();
        if !dir.is_dir() {
            return Ok(stats);
        }
        // Marker gate — same shape as `new()`: foreign content without the
        // marker means this is not our dir (or the marker was deleted
        // mid-run); either way refuse before touching anything.
        let (has_marker, has_foreign) = scan_objects_dir(&dir)?;
        if has_foreign && !has_marker {
            return Err(ObjectStoreError::Config(
                "objects dir missing .oxidean-vcs-objects marker".into(),
            ));
        }

        let grace = Duration::from_secs(self.config.gc_grace_secs);
        let lock_path = dir.join(GC_LOCK_FILE);
        let _guard = match acquire_lock(&lock_path, grace)? {
            LockOutcome::Acquired(g) => g,
            LockOutcome::Busy => {
                stats.skipped = true;
                return Ok(stats);
            }
        };

        // Pass 1 — orphan rows: conditional delete BEFORE the file unlink, so
        // a crash leaves a sweepable file, never a live row over missing
        // bytes (SD3.5).
        let grace_ts = grace_cutoff_ts(self.config.gc_grace_secs);
        let orphans = self.db.vcs_list_orphaned_objects_before(&grace_ts).await?;
        for orphan in &orphans {
            heartbeat(&lock_path);
            match self.db.vcs_delete_object_if_unlinked(&orphan.hash).await {
                Ok(true) => {
                    match remove_file_tolerant(&paths::shard_path(&dir, &orphan.hash)?).await {
                        Ok(()) => {
                            stats.orphans_removed += 1;
                            stats.bytes_reclaimed += u64::try_from(orphan.size).unwrap_or(0);
                        }
                        Err(e) => {
                            tracing::warn!(hash = %orphan.hash, error = %e, "vcs gc file unlink failed");
                            stats.errors += 1;
                        }
                    }
                }
                // `false` = protected — a racing link or a `vcs_file_changes`
                // blob reference. Silent skip, not an error.
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(hash = %orphan.hash, error = %e, "vcs gc orphan delete failed");
                    stats.errors += 1;
                }
            }
        }

        // Pass 2 — row-less shard sweep + stray `*.tmp` under the shard tree.
        let canonical_root = tokio::fs::canonicalize(&dir).await?;
        self.sweep_rowless(&dir, &canonical_root, grace, &mut stats, &lock_path)
            .await;

        // Pass 3 — `{objects_dir}/tmp/` stale staging files.
        self.sweep_tmp(&dir.join(TMP_DIR), grace, &mut stats, &lock_path)
            .await;

        tracing::info!(
            orphans_removed = stats.orphans_removed,
            rowless_removed = stats.rowless_removed,
            tmp_removed = stats.tmp_removed,
            bytes_reclaimed = stats.bytes_reclaimed,
            errors = stats.errors,
            "vcs object gc pass"
        );
        Ok(stats)
    }

    /// Boot hook (WF5): remove any `.gc-lock` unconditionally — a fresh
    /// process holds none (single-node pin, TD3.6).
    pub fn clear_stale_lock(&self) -> Result<(), ObjectStoreError> {
        match std::fs::remove_file(self.config.objects_dir.join(GC_LOCK_FILE)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Walk `{2hex}/{2hex}/{64hex}` candidates only — the name-shape filter
    /// runs BEFORE stat, so the marker, `.gc-lock`, `tmp/`, `lost+found`, and
    /// dotfiles can't match by construction (SD3.2.3). Symlinks are skipped
    /// no-follow; every candidate canonicalizes under the canonical root.
    /// Stray `*.tmp` names at shard level ride the tmp predicate.
    async fn sweep_rowless(
        &self,
        dir: &Path,
        canonical_root: &Path,
        grace: Duration,
        stats: &mut VcsGcStats,
        lock_path: &Path,
    ) {
        let mut top = match tokio::fs::read_dir(dir).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "vcs gc read objects dir failed");
                stats.errors += 1;
                return;
            }
        };
        while let Some(entry) = next_entry(&mut top, stats).await {
            heartbeat(lock_path);
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            match entry.file_type().await {
                Ok(ft) if ft.is_symlink() => continue,
                Ok(ft) if name.ends_with(".tmp") && ft.is_file() => {
                    self.sweep_one_tmp(&entry.path(), grace, stats).await;
                    continue;
                }
                Ok(ft) if paths::is_hex_shaped(&name, 2) && ft.is_dir() => {}
                _ => continue,
            }
            let mut mid = match tokio::fs::read_dir(entry.path()).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(error = %e, "vcs gc shard dir read failed");
                    stats.errors += 1;
                    continue;
                }
            };
            while let Some(e2) = next_entry(&mut mid, stats).await {
                heartbeat(lock_path);
                let Some(name2) = e2.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                match e2.file_type().await {
                    Ok(ft) if ft.is_symlink() => continue,
                    Ok(ft) if name2.ends_with(".tmp") && ft.is_file() => {
                        self.sweep_one_tmp(&e2.path(), grace, stats).await;
                        continue;
                    }
                    Ok(ft) if paths::is_hex_shaped(&name2, 2) && ft.is_dir() => {}
                    _ => continue,
                }
                let mut inner = match tokio::fs::read_dir(e2.path()).await {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!(error = %e, "vcs gc shard dir read failed");
                        stats.errors += 1;
                        continue;
                    }
                };
                while let Some(e3) = next_entry(&mut inner, stats).await {
                    heartbeat(lock_path);
                    let Some(name3) = e3.file_name().to_str().map(str::to_owned) else {
                        continue;
                    };
                    match e3.file_type().await {
                        Ok(ft) if ft.is_symlink() => continue,
                        Ok(ft) if name3.ends_with(".tmp") && ft.is_file() => {
                            self.sweep_one_tmp(&e3.path(), grace, stats).await;
                            continue;
                        }
                        Ok(ft) if paths::is_hex_shaped(&name3, 64) && ft.is_file() => {}
                        _ => continue,
                    }
                    self.sweep_candidate(&e3.path(), &name3, canonical_root, grace, stats)
                        .await;
                }
            }
        }
    }

    /// One `{64hex}` file candidate: canonicalize under root → mtime past
    /// grace → `vcs_get_object` probe → row-less ⇒ unlink. A probe `Err`
    /// keeps the file and counts an error — an unconfirmable probe is never
    /// row-less (SD3.5).
    async fn sweep_candidate(
        &self,
        path: &Path,
        hash: &str,
        canonical_root: &Path,
        grace: Duration,
        stats: &mut VcsGcStats,
    ) {
        let canon = match tokio::fs::canonicalize(path).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(hash = %hash, error = %e, "vcs gc canonicalize failed");
                stats.errors += 1;
                return;
            }
        };
        if !canon.starts_with(canonical_root) {
            // Escapes the store root — never touch it.
            tracing::warn!(hash = %hash, "vcs gc candidate escapes objects dir");
            return;
        }
        let meta = match tokio::fs::metadata(path).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(hash = %hash, error = %e, "vcs gc stat failed");
                stats.errors += 1;
                return;
            }
        };
        if !is_stale(meta.modified().ok(), grace) {
            return;
        }
        match self.db.vcs_get_object(hash).await {
            Ok(Some(_)) => {} // rowed object — bytes stay
            Ok(None) => match remove_file_tolerant(path).await {
                Ok(()) => {
                    stats.rowless_removed += 1;
                    stats.bytes_reclaimed += meta.len();
                }
                Err(e) => {
                    tracing::warn!(hash = %hash, error = %e, "vcs gc rowless unlink failed");
                    stats.errors += 1;
                }
            },
            Err(e) => {
                tracing::warn!(hash = %hash, error = %e, "vcs gc row probe failed");
                stats.errors += 1;
            }
        }
    }

    /// `{objects_dir}/tmp/` — `*.tmp` staging files past grace.
    async fn sweep_tmp(
        &self,
        tmp_dir: &Path,
        grace: Duration,
        stats: &mut VcsGcStats,
        lock_path: &Path,
    ) {
        let mut rd = match tokio::fs::read_dir(tmp_dir).await {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                tracing::warn!(error = %e, "vcs gc tmp dir read failed");
                stats.errors += 1;
                return;
            }
        };
        while let Some(entry) = next_entry(&mut rd, stats).await {
            heartbeat(lock_path);
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            match entry.file_type().await {
                Ok(ft) if ft.is_symlink() || !name.ends_with(".tmp") => continue,
                Ok(ft) if ft.is_file() => {
                    self.sweep_one_tmp(&entry.path(), grace, stats).await;
                }
                _ => continue,
            }
        }
    }

    async fn sweep_one_tmp(&self, path: &Path, grace: Duration, stats: &mut VcsGcStats) {
        let meta = match tokio::fs::metadata(path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                tracing::warn!(error = %e, "vcs gc tmp stat failed");
                stats.errors += 1;
                return;
            }
        };
        if !is_stale(meta.modified().ok(), grace) {
            return;
        }
        match remove_file_tolerant(path).await {
            Ok(()) => {
                stats.tmp_removed += 1;
                stats.bytes_reclaimed += meta.len();
            }
            Err(e) => {
                tracing::warn!(error = %e, "vcs gc tmp unlink failed");
                stats.errors += 1;
            }
        }
    }
}

/// `modified()` may fail on odd filesystems; a file we cannot age is kept
/// (not stale). `age >= grace` — a zero grace marks everything stale, the
/// file-side mirror of the `9999-12-31` row cutoff.
fn is_stale(mtime: Option<SystemTime>, grace: Duration) -> bool {
    match mtime.and_then(|t| t.elapsed().ok()) {
        Some(age) => age >= grace,
        None => false,
    }
}

/// Orphan-row cutoff as ISO-8601 UTC for `vcs_list_orphaned_objects_before`.
/// `grace = 0` ⇒ far-future cutoff — sweep everything (lfs_gc idiom).
fn grace_cutoff_ts(grace_secs: u64) -> String {
    if grace_secs == 0 {
        return "9999-12-31 23:59:59".to_string();
    }
    let cutoff = match i64::try_from(grace_secs) {
        Ok(secs) => chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::seconds(secs))
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC),
        Err(_) => chrono::DateTime::<chrono::Utc>::MIN_UTC,
    };
    cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// `remove_file` where NotFound is a no-op, not an error (lfs store.rs:97
/// precedent — a file that vanished between stat and unlink is fine).
async fn remove_file_tolerant(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

async fn next_entry(
    rd: &mut tokio::fs::ReadDir,
    stats: &mut VcsGcStats,
) -> Option<tokio::fs::DirEntry> {
    match rd.next_entry().await {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "vcs gc dir entry read failed");
            stats.errors += 1;
            None
        }
    }
}

pub(crate) enum LockOutcome {
    Acquired(LockGuard),
    /// Lock held by a pass younger than `gc_grace_secs` — or we raced a
    /// stale-lock replacement. Either way this pass skips.
    Busy,
}

/// Drop = sync `std::fs::remove_file` — Drop can't await, and a panic unwind
/// or task abort must still release the lock (strictly better than the LFS
/// explicit release, which strands the lock on panic).
pub(crate) struct LockGuard {
    path: PathBuf,
    _file: std::fs::File,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// `create_new` acquire → on `AlreadyExists`, mtime age `< gc_grace_secs` ⇒
/// `Busy`; older ⇒ stale — remove and retry once (a second `AlreadyExists`
/// means a contender raced in — Busy, not a lock-steal loop).
fn acquire_lock(lock_path: &Path, grace: Duration) -> Result<LockOutcome, ObjectStoreError> {
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut f) => {
                let _ = f.write_all(format!("{}\n", std::process::id()).as_bytes());
                return Ok(LockOutcome::Acquired(LockGuard {
                    path: lock_path.to_path_buf(),
                    _file: f,
                }));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                match std::fs::metadata(lock_path).and_then(|m| m.modified()) {
                    Ok(mtime) => {
                        let stale = is_stale(Some(mtime), grace);
                        if !stale {
                            return Ok(LockOutcome::Busy);
                        }
                        match std::fs::remove_file(lock_path) {
                            Ok(()) => continue,
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                            Err(e) => return Err(e.into()),
                        }
                    }
                    // Vanished between open and stat — retry the acquire.
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(LockOutcome::Busy)
}

/// Heartbeat — `OpenOptions::write` + `set_modified`, NEVER `File::create`
/// (that would truncate the lock content). Re-opens by path so a stolen and
/// recreated lock gets the bump, not our dead inode (SD3.4/R8).
fn heartbeat(lock_path: &Path) {
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(lock_path) {
        let _ = f.set_modified(SystemTime::now());
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use std::time::Duration;

    use super::super::testutil::*;
    use super::*;
    use crate::object_store::{ObjectStoreConfig, PutKind};
    use oxidean_db::{VcsChangesetInsert, VcsFileChangeInsert};

    const HASH_ORPHAN: &str = "aa00000000000000000000000000000000000000000000000000000000000001";
    const HASH_PROTECTED: &str = "bb00000000000000000000000000000000000000000000000000000000000002";
    const MANIFEST: &str = "cc0000000000000000000000000000000000000000000000000000000000000c";

    fn changeset<'a>(id: &'a str, repo_id: &'a str) -> VcsChangesetInsert<'a> {
        VcsChangesetInsert {
            id,
            repo_id,
            parents: "[]",
            author: "Vcs U <u@example.com>",
            message: "t",
            committed_at: "2026-01-02T03:04:05Z",
            manifest_hash: MANIFEST,
            idem_key: None,
        }
    }

    fn add_change<'a>(path: &'a str, blob: &'a str) -> VcsFileChangeInsert<'a> {
        VcsFileChangeInsert {
            path,
            kind: "add",
            mode: 0o100644,
            old_path: None,
            blob_hash: Some(blob),
        }
    }

    #[tokio::test]
    async fn gc_absent_dir_zero_stats() {
        let (_db_dir, db) = test_db().await;
        let tmp = tempfile::tempdir().unwrap();
        // Absent objects dir ⇒ Ok(zero stats), `skipped = false` — precedes
        // the marker gate and the lock.
        let dir = tmp.path().join("objects");
        let store = store_unchecked(db, cfg(&dir));
        let stats = store.gc_once().await.unwrap();
        assert!(!stats.skipped);
        assert_eq!(stats.orphans_removed, 0);
        assert_eq!(stats.rowless_removed, 0);
        assert_eq!(stats.tmp_removed, 0);
        assert_eq!(stats.bytes_reclaimed, 0);
        assert_eq!(stats.errors, 0);
    }

    #[tokio::test]
    async fn gc_foreign_dir_marker_gate() {
        let (_db_dir, db) = test_db().await;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("objects");
        std::fs::create_dir_all(&dir).unwrap();
        let foreign = dir.join("someone-elses.dat");
        std::fs::write(&foreign, b"foreign").unwrap();
        // Non-empty markerless dir ⇒ Config, nothing touched.
        let store = store_unchecked(db, cfg(&dir));
        let err = store.gc_once().await.unwrap_err();
        assert!(matches!(err, ObjectStoreError::Config(_)));
        assert_eq!(std::fs::read(&foreign).unwrap(), b"foreign");
    }

    #[tokio::test]
    async fn gc_lock_skip_and_stale() {
        let (_db_dir, db) = test_db().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = ObjectStoreConfig {
            gc_grace_secs: 3600,
            ..cfg(tmp.path())
        };
        let store = ObjectStore::new(db, config).unwrap();
        let lock = tmp.path().join(GC_LOCK_FILE);

        // Live lock (mtime now << grace) ⇒ skipped pass.
        std::fs::write(&lock, b"999\n").unwrap();
        let stats = store.gc_once().await.unwrap();
        assert!(stats.skipped);
        assert_eq!(stats.orphans_removed, 0);
        assert!(lock.is_file());

        // Stale lock (mtime older than grace) ⇒ replaced, pass runs, and the
        // guard's Drop removes the lockfile at pass end.
        age_file(&lock, 7200);
        let stats = store.gc_once().await.unwrap();
        assert!(!stats.skipped);
        assert!(!lock.exists());
    }

    #[test]
    fn gc_lock_guard_drops() {
        let tmp = tempfile::tempdir().unwrap();
        let lock = tmp.path().join(GC_LOCK_FILE);
        let grace = Duration::from_secs(60);
        let guard = match acquire_lock(&lock, grace).unwrap() {
            LockOutcome::Acquired(g) => g,
            LockOutcome::Busy => panic!("first acquire must succeed"),
        };
        // Heartbeat bumps mtime via OpenOptions::write + set_modified —
        // no truncate, pid content survives.
        std::fs::write(&lock, b"sentinel\n").unwrap();
        heartbeat(&lock);
        assert_eq!(std::fs::read_to_string(&lock).unwrap(), "sentinel\n");
        // A live lock blocks a second acquire.
        assert!(matches!(
            acquire_lock(&lock, grace).unwrap(),
            LockOutcome::Busy
        ));
        drop(guard);
        assert!(!lock.exists());
        // And a post-drop acquire succeeds.
        match acquire_lock(&lock, grace).unwrap() {
            LockOutcome::Acquired(g) => drop(g),
            LockOutcome::Busy => panic!("acquire after drop must succeed"),
        }
        assert!(!lock.exists());
    }

    #[tokio::test]
    async fn gc_orphans_row_then_file() {
        let (_db_dir, db) = test_db().await;
        let rid = seed_repo(&db, "u-gc", "repo-gc").await;
        let tmp = tempfile::tempdir().unwrap();
        // grace = 0 ⇒ far-future row cutoff — every orphan row qualifies.
        let config = ObjectStoreConfig {
            gc_grace_secs: 0,
            ..cfg(tmp.path())
        };
        let store = ObjectStore::new(db.clone(), config).unwrap();

        // Orphan: row + file, zero references.
        db.vcs_insert_object_if_absent(HASH_ORPHAN, 10)
            .await
            .unwrap();
        let file_orphan = shard_file(tmp.path(), HASH_ORPHAN, b"0123456789");
        // Protected: a `vcs_file_changes.blob_hash` reference with NO link —
        // listed as an orphan row, but the conditional delete returns false.
        db.vcs_insert_object_if_absent(HASH_PROTECTED, 20)
            .await
            .unwrap();
        let cs = changeset("cs_01J000000000000000000000AA", &rid);
        let files = [add_change("a.txt", HASH_PROTECTED)];
        db.vcs_create_changeset_tx(&cs, &files, &[], &[])
            .await
            .unwrap();
        let file_protected = shard_file(tmp.path(), HASH_PROTECTED, b"01234567890123456789");

        let stats = store.gc_once().await.unwrap();
        assert_eq!(stats.orphans_removed, 1);
        assert_eq!(stats.bytes_reclaimed, 10);
        assert_eq!(stats.errors, 0);
        // Row deleted → file unlinked.
        assert!(!file_orphan.exists());
        assert!(db.vcs_get_object(HASH_ORPHAN).await.unwrap().is_none());
        // delete=false ⇒ protected — file kept, not an error.
        assert!(file_protected.is_file());
        assert!(db.vcs_get_object(HASH_PROTECTED).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn gc_rowless_sweep() {
        let (_db_dir, db) = test_db().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = ObjectStoreConfig {
            gc_grace_secs: 3600,
            ..cfg(tmp.path())
        };
        let store = ObjectStore::new(db.clone(), config).unwrap();
        let dir = tmp.path();

        // Shape-matching file, no row, stale mtime → removed.
        let stale_hash = "11".repeat(32);
        let stale = shard_file(dir, &stale_hash, b"stale-bytes");
        age_file(&stale, 7200);
        // Shape-matching, no row, but fresh mtime → kept (inside grace).
        let fresh_hash = "22".repeat(32);
        let fresh = shard_file(dir, &fresh_hash, b"fresh");
        // Shape-matching WITH a row → probe finds it → kept.
        let rowed_hash = "33".repeat(32);
        db.vcs_insert_object_if_absent(&rowed_hash, 5)
            .await
            .unwrap();
        let rowed = shard_file(dir, &rowed_hash, b"rowed");
        age_file(&rowed, 7200);
        // Non-shape names must never be touched.
        let stray_dir = dir.join("lost+found");
        std::fs::create_dir_all(&stray_dir).unwrap();
        let stray = stray_dir.join("rescued");
        std::fs::write(&stray, b"keep").unwrap();
        let dotfile = dir.join(".hidden-object");
        std::fs::write(&dotfile, b"keep").unwrap();
        let odd_name = dir.join("not-hex-dir").join("still-not-hex");
        std::fs::create_dir_all(odd_name.parent().unwrap()).unwrap();
        std::fs::write(&odd_name, b"keep").unwrap();
        // A mid-shard file with a non-64 name is also not a candidate.
        let short = dir.join("44").join("55").join("tooshort");
        std::fs::create_dir_all(short.parent().unwrap()).unwrap();
        std::fs::write(&short, b"keep").unwrap();

        // Symlinked candidate — skipped no-follow; the target is never
        // touched even when it escapes the store root.
        #[cfg(unix)]
        {
            let outside = tmp.path().join("outside.bin");
            std::fs::write(&outside, b"outside").unwrap();
            let link_dir = dir.join("66").join("77");
            std::fs::create_dir_all(&link_dir).unwrap();
            let link = link_dir.join("66".repeat(32));
            std::os::unix::fs::symlink(&outside, &link).unwrap();
        }

        let stats = store.gc_once().await.unwrap();
        assert_eq!(stats.rowless_removed, 1);
        assert_eq!(stats.errors, 0);
        assert!(!stale.exists());
        assert!(fresh.is_file());
        assert!(rowed.is_file());
        assert!(stray.is_file());
        assert!(dotfile.is_file());
        assert!(odd_name.is_file());
        assert!(short.is_file());
        #[cfg(unix)]
        assert!(tmp.path().join("outside.bin").is_file());
        // Marker + lock land nowhere near the shape predicate.
        assert!(dir.join(paths::MARKER_FILE).is_file());
    }

    #[tokio::test]
    async fn gc_tmp_sweep() {
        let (_db_dir, db) = test_db().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = ObjectStoreConfig {
            gc_grace_secs: 3600,
            ..cfg(tmp.path())
        };
        let store = ObjectStore::new(db.clone(), config).unwrap();
        let dir = tmp.path();

        // Stale staging file under tmp/ → removed.
        let old_tmp = dir.join(TMP_DIR).join("1-0-12345.tmp");
        std::fs::write(&old_tmp, b"partial").unwrap();
        age_file(&old_tmp, 7200);
        // Fresh staging file inside grace → kept.
        let new_tmp = dir.join(TMP_DIR).join("1-1-99999.tmp");
        std::fs::write(&new_tmp, b"inflight").unwrap();
        // A stray `*.tmp` at shard level rides the same predicate.
        let stray_tmp = dir.join("aa").join("bb").join("stray.tmp");
        std::fs::create_dir_all(stray_tmp.parent().unwrap()).unwrap();
        std::fs::write(&stray_tmp, b"stray").unwrap();
        age_file(&stray_tmp, 7200);

        let stats = store.gc_once().await.unwrap();
        assert_eq!(stats.tmp_removed, 2);
        assert_eq!(stats.errors, 0);
        assert!(!old_tmp.exists());
        assert!(new_tmp.is_file());
        assert!(!stray_tmp.exists());

        // Probe `Err` on a row-less candidate → file kept + errors += 1
        // (an unconfirmable probe is never row-less).
        let (_bad_dir, broken) = unmigrated_db().await;
        let dir2 = tempfile::tempdir().unwrap();
        let store2 = store_unchecked(
            broken,
            ObjectStoreConfig {
                gc_grace_secs: 0,
                ..cfg(dir2.path())
            },
        );
        std::fs::write(dir2.path().join(paths::MARKER_FILE), b"").unwrap();
        let hash = "77".repeat(32);
        let candidate = shard_file(dir2.path(), &hash, b"keep-me");
        age_file(&candidate, 7200);
        let canonical = std::fs::canonicalize(dir2.path()).unwrap();
        let mut stats = VcsGcStats::default();
        let lock_path = dir2.path().join(GC_LOCK_FILE);
        store2
            .sweep_rowless(
                dir2.path(),
                &canonical,
                Duration::from_secs(0),
                &mut stats,
                &lock_path,
            )
            .await;
        assert_eq!(stats.rowless_removed, 0);
        assert_eq!(stats.errors, 1);
        assert!(candidate.is_file());
    }

    #[tokio::test]
    async fn gc_full_pass_via_public_api() {
        // End-to-end smoke: put → repo delete cascade → object goes orphan →
        // gc reclaims row + file (referenced-ness computed at GC time).
        let (_db_dir, db) = test_db().await;
        let rid = seed_repo(&db, "u-full", "repo-full").await;
        let tmp = tempfile::tempdir().unwrap();
        let config = ObjectStoreConfig {
            gc_grace_secs: 0,
            ..cfg(tmp.path())
        };
        let store = ObjectStore::new(db.clone(), config).unwrap();
        let out = store
            .put(stream_bytes(&[b"ephemeral"]), &rid)
            .await
            .unwrap();
        assert_eq!(out.kind, PutKind::Stored);
        let file = paths::shard_path(tmp.path(), &out.hash).unwrap();
        assert!(file.is_file());

        db.hard_delete_repository(&rid).await.unwrap();
        let stats = store.gc_once().await.unwrap();
        assert_eq!(stats.orphans_removed, 1);
        assert!(!file.exists());
        assert!(db.vcs_get_object(&out.hash).await.unwrap().is_none());
    }
}
