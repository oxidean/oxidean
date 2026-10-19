//! Content-addressed object store under `OXIDEAN_VCS_OBJECTS_DIR`
//! (u3-vcs-object-store; workflows WF1–WF5).
//!
//! Layout: `{objects_dir}/{hash[0:2]}/{hash[2:4]}/{hash}` — bare lowercase
//! sha256-hex, staged via `tmp/{pid}-{counter}-{nanos}.tmp` → atomic rename.
//! `.oxidean-vcs-objects` marks a dir this store owns (foreign dirs refuse);
//! `.gc-lock` is the single-node GC mutex.
//!
//! **U4 invariant (WF3/C6a):** this unit ships no standalone link-insert tx —
//! `vcs_create_changeset_tx`'s `object_links` param is the mechanism. U4 MUST
//! pass every manifest `blob_hash` (not just new blobs): a referenced-but-
//! unlinked object looks orphaned to the GC row pass while the
//! `vcs_file_changes` RESTRICT-side clause keeps `vcs_delete_object_if_unlinked`
//! `false` forever — neither reclaimed nor linkable-invisible.
//!
//! Hashes are stored/compared bare; the `sha256:` prefix is added at the U7
//! wire boundary and never enters this module.

mod config;
mod gc;
mod paths;

use std::fmt;
use std::path::Path;

use bytes::Bytes;
use futures_util::StreamExt;
use oxidean_db::Database;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub use config::ObjectStoreConfig;
use paths::{MARKER_FILE, TMP_DIR};

/// Which quota denied the write (`vcs.quota_exceeded` carries the scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaScope {
    /// Per-object streamed byte cap (`OXIDEAN_VCS_MAX_OBJECT_BYTES`).
    Object,
    /// Per-repo logical bytes (`OXIDEAN_VCS_REPO_QUOTA_BYTES`).
    Repo,
}

/// Closed failure surface for the unit — wire-code mapping lives in U7
/// (BR3.9); the enum is the contract and never stringifies into wire data.
#[derive(Debug)]
pub enum ObjectStoreError {
    /// Over `max_object_bytes` (streamed cap) or `repo_quota_bytes` (link-time
    /// charge) — `vcs.quota_exceeded`.
    QuotaExceeded(QuotaScope),
    /// No row, or the file vanished between lookup and open
    /// (`io::ErrorKind::NotFound` carve-out) — `vcs.not_found`.
    NotFound,
    /// Not bare lowercase 64-hex — `vcs.invalid_input`.
    InvalidHash,
    /// Byte-stream failure (client fault) — `vcs.invalid_input`. Constructed
    /// explicitly; never via `From` (a `String` helper must not mislabel it).
    Stream(String),
    /// Construct/boot refusal — env parse, dir-is-file, foreign-dir marker
    /// gate, sibling-dir collision. U7 surfaces as startup refusal.
    Config(String),
    /// Eager `Database::is_skipped()` fail-close — `db.not_configured`.
    DbNotConfigured,
    /// Facade `Result<_, String>` surface — genuine SQL failure, host internal.
    Db(String),
    /// Filesystem failure — host internal.
    Io(std::io::Error),
    /// Invariant violation (e.g. `verify` hash mismatch) — host internal.
    Internal(String),
}

impl fmt::Display for ObjectStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QuotaExceeded(QuotaScope::Object) => write!(f, "object exceeds max_object_bytes"),
            Self::QuotaExceeded(QuotaScope::Repo) => write!(f, "repo storage quota exceeded"),
            Self::NotFound => write!(f, "object not found"),
            Self::InvalidHash => write!(f, "invalid object hash"),
            Self::Stream(e) => write!(f, "object stream: {e}"),
            Self::Config(e) => write!(f, "object store config: {e}"),
            Self::DbNotConfigured => write!(f, "database not configured"),
            Self::Db(e) => write!(f, "object store db: {e}"),
            Self::Io(e) => write!(f, "object store io: {e}"),
            Self::Internal(e) => write!(f, "object store internal: {e}"),
        }
    }
}

impl std::error::Error for ObjectStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// `io::ErrorKind::NotFound` is a domain 404 (mid-lookup delete / vanished
/// file); every other kind is a host-internal io failure (SD3.7).
impl From<std::io::Error> for ObjectStoreError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound,
            _ => Self::Io(e),
        }
    }
}

/// Exists solely for the facade's `Result<_, String>` surface — stream chunk
/// errors construct `Stream` explicitly so a String helper can never be
/// mislabeled as a DB failure (SD3.7).
impl From<String> for ObjectStoreError {
    fn from(e: String) -> Self {
        Self::Db(e)
    }
}

impl From<std::env::VarError> for ObjectStoreError {
    fn from(e: std::env::VarError) -> Self {
        Self::Config(e.to_string())
    }
}

/// `put` discriminant — `Stored` when the global `vcs_objects` row is new,
/// `Dedup` when it already existed (re-upload is always a safe no-op).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutKind {
    Stored,
    Dedup,
}

/// `hash` is bare 64-hex — the `sha256:` prefix is a U7 wire-boundary concern.
#[derive(Debug)]
pub struct PutOutcome {
    pub hash: String,
    pub kind: PutKind,
}

/// One GC pass's accounting (SD3.10 correction: `skipped` + `errors` added).
#[derive(Debug, Default)]
pub struct VcsGcStats {
    pub orphans_removed: u64,
    pub rowless_removed: u64,
    pub tmp_removed: u64,
    pub bytes_reclaimed: u64,
    /// A live `.gc-lock` younger than `gc_grace_secs` skipped the pass.
    pub skipped: bool,
    pub errors: u32,
}

/// `put`/`get`/`verify`/`gc_once`/`clear_stale_lock` — the U7 job/boot/route
/// hooks. Trusts an already-authorized caller; no auth/routes inside (BR3.10).
pub struct ObjectStore {
    pub(crate) db: Database,
    pub(crate) config: ObjectStoreConfig,
}

impl ObjectStore {
    /// Construct-order (SD3.8): existing regular file ⇒ `Config` → non-empty
    /// (exempting only the marker + `lost+found`) without marker ⇒ `Config` →
    /// `create_dir_all` → write marker → `create_dir_all(tmp)`. Zero writes
    /// before the refusal; marker BEFORE `tmp/` so a mid-sequence crash leaves
    /// a marked dir, not a markerless tmp-only one that refuses forever.
    pub fn new(db: Database, config: ObjectStoreConfig) -> Result<Self, ObjectStoreError> {
        let dir = &config.objects_dir;
        if dir.is_file() {
            return Err(ObjectStoreError::Config(
                "objects dir is a regular file".into(),
            ));
        }
        if dir.is_dir() {
            let (has_marker, has_foreign) = scan_objects_dir(dir)?;
            if has_foreign && !has_marker {
                return Err(ObjectStoreError::Config(
                    "objects dir holds foreign content and no .oxidean-vcs-objects marker".into(),
                ));
            }
        }
        std::fs::create_dir_all(dir)?;
        if !dir.join(MARKER_FILE).exists() {
            std::fs::write(
                dir.join(MARKER_FILE),
                "oxidean-vcs object store — do not co-locate other content\n",
            )?;
        }
        let tmp = dir.join(TMP_DIR);
        std::fs::create_dir_all(&tmp)?;
        set_dir_private(&tmp)?;
        Ok(Self { db, config })
    }

    /// Stream → tmp → hash+count → rename → row → link (SD3.3). Over
    /// `max_object_bytes` aborts mid-stream with the tmp removed; repo quota
    /// is charged only on a NEW link (`vcs_get_object_link` probe — a
    /// re-reference adds no bytes, BR3.4).
    pub async fn put<S, E>(
        &self,
        mut stream: S,
        repo_id: &str,
    ) -> Result<PutOutcome, ObjectStoreError>
    where
        S: StreamExt<Item = Result<Bytes, E>> + Unpin,
        E: fmt::Display,
    {
        if self.db.is_skipped() {
            return Err(ObjectStoreError::DbNotConfigured);
        }
        let tmp_dir = self.config.objects_dir.join(TMP_DIR);
        tokio::fs::create_dir_all(&tmp_dir).await?;
        let tmp_path = paths::tmp_path(&self.config.objects_dir);
        let mut file = match tokio::fs::File::create(&tmp_path).await {
            Ok(f) => f,
            Err(e) => return Err(e.into()),
        };
        let mut hasher = Sha256::new();
        let mut written: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    return Err(ObjectStoreError::Stream(e.to_string()));
                }
            };
            written = written.saturating_add(chunk.len() as u64);
            if written > self.config.max_object_bytes {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(ObjectStoreError::QuotaExceeded(QuotaScope::Object));
            }
            hasher.update(&chunk);
            if let Err(e) = file.write_all(&chunk).await {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(e.into());
            }
        }
        if let Err(e) = file.flush().await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(e.into());
        }
        drop(file);

        let hash = bytes_to_hex(hasher.finalize().as_slice());
        let final_path = paths::shard_path(&self.config.objects_dir, &hash)?;
        if let Some(parent) = final_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(e.into());
            }
            if let Err(e) = set_dir_private_async(parent).await {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(e);
            }
        }
        if final_path.exists() {
            // Identical content already on disk — drop the staging file.
            let _ = tokio::fs::remove_file(&tmp_path).await;
        } else if let Err(e) = tokio::fs::rename(&tmp_path, &final_path).await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(e.into());
        } else {
            set_blob_private(&final_path).await?;
        }

        let kind = match self
            .db
            // `written <= max_object_bytes <= i64::MAX` (SD3.6) — the bind
            // can't wrap; the checked cast keeps that honest if config ever
            // bypasses `from_env`.
            .vcs_insert_object_if_absent(
                &hash,
                i64::try_from(written)
                    .map_err(|_| ObjectStoreError::Internal("size over i64".into()))?,
            )
            .await?
        {
            true => PutKind::Stored,
            false => PutKind::Dedup,
        };

        // Membership probe: an existing (repo, hash) link skips the charge —
        // re-referencing adds no bytes.
        if self.db.vcs_get_object_link(repo_id, &hash).await? {
            return Ok(PutOutcome { hash, kind });
        }
        let counts = self.db.vcs_repo_object_counts(repo_id).await?;
        // i64 rows vs u64 config — checked cast: a corrupt-negative row fails
        // closed instead of wrapping to a pass (SD3.6).
        let used = u64::try_from(counts.storage_bytes)
            .map_err(|_| ObjectStoreError::Internal("negative storage_bytes row".into()))?;
        // `repo_quota_bytes = 0` still admits size-0 objects (`0 + 0 > 0` is
        // false — the pinned empty-object carve-out).
        if used.saturating_add(written) > self.config.repo_quota_bytes {
            if kind == PutKind::Stored {
                // Eager cleanup: conditional delete first; unlink the file
                // only when the row really went away (false = a link raced
                // in — the bytes are protected).
                if self.db.vcs_delete_object_if_unlinked(&hash).await? {
                    let _ = tokio::fs::remove_file(&final_path).await;
                }
            }
            return Err(ObjectStoreError::QuotaExceeded(QuotaScope::Repo));
        }
        self.db
            .vcs_insert_object_link_if_absent(repo_id, &hash)
            .await?;
        Ok(PutOutcome { hash, kind })
    }

    /// Validate → row check → open. The row implies the bytes; io NotFound on
    /// open (mid-lookup delete) maps to `NotFound` (WF2, BR3.9).
    pub async fn get(&self, hash: &str) -> Result<tokio::fs::File, ObjectStoreError> {
        if self.db.is_skipped() {
            return Err(ObjectStoreError::DbNotConfigured);
        }
        paths::validate_hash(hash)?;
        if self.db.vcs_get_object(hash).await?.is_none() {
            return Err(ObjectStoreError::NotFound);
        }
        let path = paths::shard_path(&self.config.objects_dir, hash)?;
        Ok(tokio::fs::File::open(&path).await?)
    }

    /// Re-hash the stored bytes and compare — GC integrity check + tests.
    /// Mismatch ⇒ `Internal` (bytes are never served under a false hash).
    pub async fn verify(&self, hash: &str) -> Result<(), ObjectStoreError> {
        let mut file = self.get(hash).await?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let digest = bytes_to_hex(hasher.finalize().as_slice());
        if digest != hash {
            return Err(ObjectStoreError::Internal(
                "stored bytes hash mismatch".into(),
            ));
        }
        Ok(())
    }
}

/// `(has_marker, has_foreign)` — foreign = any entry besides the marker and
/// `lost+found` (SD3.9-R9: a fresh dedicated volume ships one).
fn scan_objects_dir(dir: &Path) -> Result<(bool, bool), ObjectStoreError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((false, false)),
        Err(e) => return Err(e.into()),
    };
    let mut has_marker = false;
    let mut has_foreign = false;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if name == MARKER_FILE {
            has_marker = true;
        } else if name != paths::LOST_FOUND {
            has_foreign = true;
        }
    }
    Ok((has_marker, has_foreign))
}

#[cfg(unix)]
fn set_dir_private(path: &Path) -> Result<(), ObjectStoreError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_dir_private(_path: &Path) -> Result<(), ObjectStoreError> {
    // Non-unix inherits the umask — documented v1 hole (SD3.8).
    Ok(())
}

#[cfg(unix)]
async fn set_dir_private_async(path: &Path) -> Result<(), ObjectStoreError> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn set_dir_private_async(_path: &Path) -> Result<(), ObjectStoreError> {
    Ok(())
}

#[cfg(unix)]
async fn set_blob_private(path: &Path) -> Result<(), ObjectStoreError> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn set_blob_private(_path: &Path) -> Result<(), ObjectStoreError> {
    Ok(())
}

/// Local `bytes_to_hex` clone (api's is unreachable under the dependency
/// direction; six private clones already exist in-tree — TD3.2).
fn bytes_to_hex(bytes: &[u8]) -> String {
    use fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(all(test, feature = "server"))]
pub(crate) mod testutil {
    //! Shared fixtures: real migrated sqlite `Database` + `tempfile` object
    //! dirs (zero mocks — the facade contract runs against the real schema).

    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    use bytes::Bytes;
    use futures_util::{stream, Stream};
    use oxidean_db::Database;

    use super::{ObjectStore, ObjectStoreConfig};

    /// File-backed sqlite (never `:memory:` — a real file exercises the pool
    /// the way the facade uses it) with all migrations applied.
    pub(crate) async fn test_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", dir.path().join("vcs.db").display());
        let db = Database::connect(&url).await.unwrap();
        db.migrate().await.unwrap();
        (dir, db)
    }

    /// Connected, unmigrated — every facade call errors ("no such table").
    /// Feeds the row-less sweep's probe-`Err` branch deterministically.
    pub(crate) async fn unmigrated_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", dir.path().join("vcs.db").display());
        let db = Database::connect(&url).await.unwrap();
        (dir, db)
    }

    /// `vcs_object_links.repo_id` is FK'd to `repositories` — link-bearing
    /// tests need a real repo row (which needs a user row).
    pub(crate) async fn seed_repo(db: &Database, uid: &str, rid: &str) -> String {
        db.create_user(
            uid,
            &format!("{uid}@example.com"),
            uid,
            None,
            "Vcs U",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .unwrap();
        db.insert_repository(rid, uid, "user", rid, "private", "", "main")
            .await
            .unwrap();
        rid.to_string()
    }

    pub(crate) fn cfg(dir: &Path) -> ObjectStoreConfig {
        ObjectStoreConfig {
            objects_dir: dir.to_path_buf(),
            ..ObjectStoreConfig::default()
        }
    }

    /// Construct without `new()`'s dir bootstrap — for absent-dir /
    /// foreign-dir GC cases that must not get a marker written.
    pub(crate) fn store_unchecked(db: Database, config: ObjectStoreConfig) -> ObjectStore {
        ObjectStore { db, config }
    }

    pub(crate) fn stream_bytes(
        chunks: &[&'static [u8]],
    ) -> impl Stream<Item = Result<Bytes, String>> + Unpin {
        stream::iter(
            chunks
                .iter()
                .map(|c| Ok::<Bytes, String>(Bytes::from_static(c)))
                .collect::<Vec<_>>(),
        )
    }

    pub(crate) fn shard_file(objects_dir: &Path, hash: &str, bytes: &[u8]) -> PathBuf {
        let path = objects_dir.join(&hash[0..2]).join(&hash[2..4]).join(hash);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// Backdate a file's mtime (std `File::set_modified` — arbitrary past).
    pub(crate) fn age_file(path: &Path, secs_ago: u64) {
        let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(secs_ago))
            .unwrap();
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::testutil::*;
    use super::*;
    use futures_util::stream;

    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn new_refuses_regular_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("objects");
        std::fs::write(&file, b"not a dir").unwrap();
        let err = ObjectStore::new(Database::skipped(), cfg(&file))
            .err()
            .unwrap();
        assert!(matches!(err, ObjectStoreError::Config(_)));
        assert!(file.is_file());
    }

    #[test]
    fn new_refuses_foreign_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("objects");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("stray.txt"), b"foreign").unwrap();
        let err = ObjectStore::new(Database::skipped(), cfg(&dir))
            .err()
            .unwrap();
        assert!(matches!(err, ObjectStoreError::Config(_)));
        // A marker claims the dir — construction then succeeds.
        std::fs::write(dir.join(MARKER_FILE), b"").unwrap();
        ObjectStore::new(Database::skipped(), cfg(&dir)).unwrap();
    }

    #[test]
    fn new_allows_fresh_and_lostfound() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("objects");
        ObjectStore::new(Database::skipped(), cfg(&dir)).unwrap();
        assert!(dir.join(MARKER_FILE).is_file());
        assert!(dir.join(TMP_DIR).is_dir());

        // A dedicated volume's `lost+found` alone does not make it foreign.
        let dir2 = tmp.path().join("vol-mount");
        std::fs::create_dir_all(dir2.join(paths::LOST_FOUND)).unwrap();
        ObjectStore::new(Database::skipped(), cfg(&dir2)).unwrap();
        assert!(dir2.join(MARKER_FILE).is_file());
    }

    #[test]
    fn new_marker_before_tmp() {
        let tmp = tempfile::tempdir().unwrap();
        // Crash-ordering invariant: after `new`, the marker exists whenever
        // `tmp/` does.
        let dir = tmp.path().join("a");
        ObjectStore::new(Database::skipped(), cfg(&dir)).unwrap();
        assert!(dir.join(MARKER_FILE).is_file());
        assert!(dir.join(TMP_DIR).is_dir());

        // Crash point 1 (marker written, tmp not yet): re-`new` heals it.
        let dir2 = tmp.path().join("b");
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(dir2.join(MARKER_FILE), b"").unwrap();
        ObjectStore::new(Database::skipped(), cfg(&dir2)).unwrap();
        assert!(dir2.join(TMP_DIR).is_dir());

        // Crash point 2 — impossible under the pinned order (tmp without
        // marker): the dir reads foreign and refuses.
        let dir3 = tmp.path().join("c");
        std::fs::create_dir_all(dir3.join(TMP_DIR)).unwrap();
        let err = ObjectStore::new(Database::skipped(), cfg(&dir3))
            .err()
            .unwrap();
        assert!(matches!(err, ObjectStoreError::Config(_)));
    }

    #[tokio::test]
    async fn put_roundtrip_dedup() {
        let (_db_dir, db) = test_db().await;
        let rid = seed_repo(&db, "u-rt", "repo-rt").await;
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(db.clone(), cfg(tmp.path())).unwrap();

        let out = store
            .put(stream_bytes(&[b"hello ", b"vcs"]), &rid)
            .await
            .unwrap();
        assert_eq!(out.kind, PutKind::Stored);
        // Bare 64-hex hash — the wire adds `sha256:` at the U7 boundary.
        assert_eq!(out.hash.len(), 64);
        assert!(!out.hash.contains(':'));
        let expected = bytes_to_hex(Sha256::digest(b"hello vcs").as_slice());
        assert_eq!(out.hash, expected);
        let file = paths::shard_path(tmp.path(), &out.hash).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"hello vcs");

        // Re-upload: same hash, Dedup — a safe no-op.
        let out2 = store
            .put(stream_bytes(&[b"hello vcs"]), &rid)
            .await
            .unwrap();
        assert_eq!(out2.kind, PutKind::Dedup);
        assert_eq!(out2.hash, out.hash);

        let mut f = store.get(&out.hash).await.unwrap();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"hello vcs");
    }

    #[tokio::test]
    async fn put_empty_object() {
        let (_db_dir, db) = test_db().await;
        let rid = seed_repo(&db, "u-empty", "repo-empty").await;
        let tmp = tempfile::tempdir().unwrap();
        // `repo_quota_bytes = 0` still admits the size-0 object (pinned
        // carve-out: `0 + 0 > 0` is false).
        let config = ObjectStoreConfig {
            repo_quota_bytes: 0,
            ..cfg(tmp.path())
        };
        let store = ObjectStore::new(db.clone(), config).unwrap();
        let empty: &[&'static [u8]] = &[];
        let out = store.put(stream_bytes(empty), &rid).await.unwrap();
        assert_eq!(out.kind, PutKind::Stored);
        assert_eq!(out.hash, EMPTY_SHA256);
        let row = db.vcs_get_object(&out.hash).await.unwrap().unwrap();
        assert_eq!(row.size, 0);
        assert!(paths::shard_path(tmp.path(), &out.hash).unwrap().is_file());
    }

    #[tokio::test]
    async fn put_object_quota_aborts_tmp() {
        let (_db_dir, db) = test_db().await;
        let rid = seed_repo(&db, "u-cap", "repo-cap").await;
        let tmp = tempfile::tempdir().unwrap();
        let config = ObjectStoreConfig {
            max_object_bytes: 4,
            ..cfg(tmp.path())
        };
        let store = ObjectStore::new(db.clone(), config).unwrap();
        let err = store
            .put(stream_bytes(&[b"12345", b"67890"]), &rid)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ObjectStoreError::QuotaExceeded(QuotaScope::Object)
        ));
        // Staging file removed; nothing landed.
        assert!(std::fs::read_dir(tmp.path().join(TMP_DIR))
            .unwrap()
            .next()
            .is_none());
        let hash = bytes_to_hex(Sha256::digest(b"1234567890").as_slice());
        assert!(db.vcs_get_object(&hash).await.unwrap().is_none());
        assert!(!paths::shard_path(tmp.path(), &hash).unwrap().exists());

        // A mid-stream chunk error is a Stream fault, and the tmp still dies.
        let failing = stream::iter([
            Ok::<Bytes, String>(Bytes::from_static(b"ab")),
            Err("boom".to_string()),
        ]);
        let err = store.put(failing, &rid).await.unwrap_err();
        assert!(matches!(err, ObjectStoreError::Stream(_)));
        assert!(std::fs::read_dir(tmp.path().join(TMP_DIR))
            .unwrap()
            .next()
            .is_none());
    }

    #[tokio::test]
    async fn put_repo_quota_denies_link_keeps_object() {
        let (_db_dir, db) = test_db().await;
        let rid_a = seed_repo(&db, "u-qa", "repo-qa").await;
        let rid_b = seed_repo(&db, "u-qb", "repo-qb").await;
        let tmp = tempfile::tempdir().unwrap();

        // New-object PUT over repo quota → eager cleanup: row + file gone.
        let config0 = ObjectStoreConfig {
            repo_quota_bytes: 0,
            ..cfg(tmp.path())
        };
        let store0 = ObjectStore::new(db.clone(), config0).unwrap();
        let err = store0
            .put(stream_bytes(&[b"over quota"]), &rid_a)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ObjectStoreError::QuotaExceeded(QuotaScope::Repo)
        ));
        let hash0 = bytes_to_hex(Sha256::digest(b"over quota").as_slice());
        assert!(db.vcs_get_object(&hash0).await.unwrap().is_none());
        assert!(!paths::shard_path(tmp.path(), &hash0).unwrap().exists());

        // Object already linked by repo A: repo B's denied PUT must not
        // touch the bytes (the conditional delete returns false — protected).
        let config_big = ObjectStoreConfig {
            repo_quota_bytes: u64::MAX,
            ..cfg(tmp.path())
        };
        let store_big = ObjectStore::new(db.clone(), config_big).unwrap();
        let out = store_big
            .put(stream_bytes(&[b"shared bytes"]), &rid_a)
            .await
            .unwrap();
        assert_eq!(out.kind, PutKind::Stored);
        let err = store0
            .put(stream_bytes(&[b"shared bytes"]), &rid_b)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ObjectStoreError::QuotaExceeded(QuotaScope::Repo)
        ));
        let file = paths::shard_path(tmp.path(), &out.hash).unwrap();
        assert!(file.is_file());
        assert!(db.vcs_get_object(&out.hash).await.unwrap().is_some());
        assert!(db.vcs_get_object_link(&rid_a, &out.hash).await.unwrap());
        assert!(!db.vcs_get_object_link(&rid_b, &out.hash).await.unwrap());
    }

    #[tokio::test]
    async fn put_rereference_no_quota_charge() {
        let (_db_dir, db) = test_db().await;
        let rid_a = seed_repo(&db, "u-ra", "repo-ra").await;
        let rid_b = seed_repo(&db, "u-rb", "repo-rb").await;
        let tmp = tempfile::tempdir().unwrap();
        // Quota == object size: each repo can link it exactly once — any
        // second charge would tip `storage + size > quota`.
        let config = ObjectStoreConfig {
            repo_quota_bytes: 5,
            ..cfg(tmp.path())
        };
        let store = ObjectStore::new(db.clone(), config).unwrap();
        store.put(stream_bytes(&[b"12345"]), &rid_a).await.unwrap();
        // Repo B links the same global object — Dedup + its own charge.
        let out_b = store.put(stream_bytes(&[b"12345"]), &rid_b).await.unwrap();
        assert_eq!(out_b.kind, PutKind::Dedup);
        assert!(db.vcs_get_object_link(&rid_b, &out_b.hash).await.unwrap());
        // Repo B re-uploads: the membership probe short-circuits — no
        // count_for_repo denial path even though B is now exactly at quota.
        let out_b2 = store.put(stream_bytes(&[b"12345"]), &rid_b).await.unwrap();
        assert_eq!(out_b2.kind, PutKind::Dedup);
    }

    #[tokio::test]
    async fn get_streams_and_404s() {
        let (_db_dir, db) = test_db().await;
        let rid = seed_repo(&db, "u-get", "repo-get").await;
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(db.clone(), cfg(tmp.path())).unwrap();
        let out = store.put(stream_bytes(&[b"payload"]), &rid).await.unwrap();

        let mut f = store.get(&out.hash).await.unwrap();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"payload");

        // Unknown hash (no row) → NotFound.
        let missing = "f".repeat(64);
        assert!(matches!(
            store.get(&missing).await.unwrap_err(),
            ObjectStoreError::NotFound
        ));
        // Row present but the file vanished → io NotFound → domain NotFound.
        let file = paths::shard_path(tmp.path(), &out.hash).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert!(matches!(
            store.get(&out.hash).await.unwrap_err(),
            ObjectStoreError::NotFound
        ));
        // Malformed hash → InvalidHash before any fs touch.
        for bad in ["../escape", "short", &format!("sha256:{missing}")] {
            assert!(matches!(
                store.get(bad).await.unwrap_err(),
                ObjectStoreError::InvalidHash
            ));
        }
    }

    #[tokio::test]
    async fn verify_detects_corruption() {
        let (_db_dir, db) = test_db().await;
        let rid = seed_repo(&db, "u-vfy", "repo-vfy").await;
        let tmp = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(db.clone(), cfg(tmp.path())).unwrap();
        let out = store
            .put(stream_bytes(&[b"integrity"]), &rid)
            .await
            .unwrap();
        store.verify(&out.hash).await.unwrap();

        // Flip the stored bytes — re-hash must fail as Internal, not serve.
        let file = paths::shard_path(tmp.path(), &out.hash).unwrap();
        std::fs::write(&file, b"!ntegrity").unwrap();
        let err = store.verify(&out.hash).await.unwrap_err();
        assert!(matches!(err, ObjectStoreError::Internal(_)));
    }

    #[tokio::test]
    async fn skipped_db_fails_closed() {
        let tmp = tempfile::tempdir().unwrap();
        // `new` is fs-only — the fail-close gate is at put/get/gc_once entry.
        let store = ObjectStore::new(Database::skipped(), cfg(tmp.path())).unwrap();
        let err = store
            .put(stream_bytes(&[b"x"]), "repo-x")
            .await
            .unwrap_err();
        assert!(matches!(err, ObjectStoreError::DbNotConfigured));
        let hash = "a".repeat(64);
        assert!(matches!(
            store.get(&hash).await.unwrap_err(),
            ObjectStoreError::DbNotConfigured
        ));
        assert!(matches!(
            store.gc_once().await.unwrap_err(),
            ObjectStoreError::DbNotConfigured
        ));
    }
}
