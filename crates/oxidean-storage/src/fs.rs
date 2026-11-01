//! `fs` backend — today's on-disk semantics behind [`BlobStore`]:
//! temp-write → verify → atomic rename (`lfs/store.rs::put_stream`), the
//! marker-verified root (`ObjectStore::new` construction order), and
//! same-filesystem `tmp/` staging so renames stay atomic.

use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{stream, StreamExt};
use sha2::{Digest, Sha256};
use tokio::fs::{self, File};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::error::StorageError;
use crate::key::{validate_store_name, BlobKey};
use crate::store::{
    bytes_to_hex, BlobMeta, BlobObject, BlobStore, ByteStream, PresignedUrl, PutConstraints,
    StagedUpload,
};

/// In-flight upload staging under `{root}/tmp/` — same filesystem as the
/// final path so the commit rename is atomic. Shared with `key.rs`, which
/// reserves the name from `BlobKey`s.
use crate::key::TMP_DIR;
/// `lost+found` is exempted from the foreign-content scan — a fresh
/// dedicated volume ships one (`ObjectStore` SD3.9-R9).
const LOST_FOUND: &str = "lost+found";
const READ_CHUNK: usize = 64 * 1024;

/// Filesystem [`BlobStore`] over one store root (the store's existing
/// `OXIDEAN_*_DIR` or `var/*` default).
#[derive(Debug)]
pub struct FsBlobStore {
    root: PathBuf,
}

impl FsBlobStore {
    /// Marker-gated open, `ObjectStore::new` construction order: dir-is-file
    /// ⇒ `Config` → non-empty (exempting marker + `lost+found`) without
    /// marker ⇒ `Config` → `create_dir_all` → write marker → create `tmp/`.
    pub fn open(root: impl AsRef<Path>, store: &str) -> Result<Self, StorageError> {
        Self::open_inner(root.as_ref(), store, false)
    }

    /// Marker-claiming open for roots that predate the marker rule — writes
    /// `.oxidean-{store}-marker` into an already-populated dir. Reserve for
    /// deliberate migration points; the foreign-dir refusal is what stops a
    /// sweep deleting somebody else's bytes.
    pub fn adopt(root: impl AsRef<Path>, store: &str) -> Result<Self, StorageError> {
        Self::open_inner(root.as_ref(), store, true)
    }

    fn open_inner(root: &Path, store: &str, adopt: bool) -> Result<Self, StorageError> {
        validate_store_name(store)?;
        let marker_name = format!(".oxidean-{store}-marker");
        let root = absolutize(root);
        if root.is_file() {
            return Err(StorageError::Config(format!(
                "blob store root {} is a regular file",
                root.display()
            )));
        }
        if root.is_dir() {
            let (has_marker, has_foreign) = scan_root(&root, &marker_name)?;
            if has_foreign && !has_marker && !adopt {
                return Err(StorageError::Config(format!(
                    "blob store root {} holds foreign content and no {marker_name} marker",
                    root.display()
                )));
            }
        }
        std::fs::create_dir_all(&root)?;
        let marker = root.join(&marker_name);
        if !marker.exists() {
            std::fs::write(
                &marker,
                "oxidean blob store — do not co-locate other content\n",
            )?;
        }
        let tmp = root.join(TMP_DIR);
        std::fs::create_dir_all(&tmp)?;
        set_dir_private(&tmp)?;
        Ok(Self { root })
    }

    /// The resolved (absolute) store root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `{root}/{key}` — safe join: `BlobKey` guarantees non-empty non-dot
    /// segments of `[A-Za-z0-9._~-]`, so traversal is impossible by
    /// construction.
    fn key_path(&self, key: &BlobKey) -> PathBuf {
        self.root.join(key.as_str())
    }
}

/// Relative roots anchor at the process cwd — same absolutize idiom as
/// `app.rs`/`ObjectStoreConfig` (git ops run from temp `-C` worktrees;
/// store paths must not drift with `-C`).
pub(crate) fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    }
}

/// Root identity for overlap checks: canonicalize the longest existing
/// prefix (resolving symlinks there), then fold the non-existing tail's
/// `.`/`..` components lexically. A plain `canonicalize`-or-nothing fallback
/// would keep `..` in missing roots (`Path::components` does not normalize
/// them), letting `/var/missing/../lfs` slip past `/var/lfs`.
///
/// Each appended tail component is re-canonicalized when it exists, so a
/// symlink created under a "missing" directory is still followed before the
/// next `..` folds — otherwise `dir/missing/../alias/child` (with `alias`
/// linking into another store's root) would compare non-overlapping while
/// the OS resolves it inside the other store at create time.
pub(crate) fn resolve_isolation_root(root: &Path) -> PathBuf {
    let abs = absolutize(root);
    let components: Vec<Component> = abs.components().collect();
    for split in (0..=components.len()).rev() {
        let prefix: PathBuf = components[..split].iter().collect();
        if let Ok(base) = std::fs::canonicalize(&prefix) {
            let mut resolved = base;
            for component in &components[split..] {
                match component {
                    Component::ParentDir => {
                        resolved.pop();
                    }
                    Component::Normal(name) => {
                        resolved.push(name);
                        if let Ok(target) = std::fs::canonicalize(&resolved) {
                            resolved = target;
                        }
                    }
                    _ => {}
                }
            }
            return resolved;
        }
    }
    absolutize(root)
}

/// tmp staging holds pre-commit bytes — lock it down like the vcs
/// object store does (non-unix inherits the umask, same documented hole).
#[cfg(unix)]
fn set_dir_private(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// Non-unix fallback — see `set_dir_private`.
#[cfg(not(unix))]
fn set_dir_private(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}

/// `(has_marker, has_foreign)` — foreign = any entry besides our marker and
/// `lost+found` (`scan_objects_dir` precedent).
fn scan_root(dir: &Path, marker_name: &str) -> Result<(bool, bool), StorageError> {
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
        if name == marker_name {
            has_marker = true;
        } else if name != LOST_FOUND {
            has_foreign = true;
        }
    }
    Ok((has_marker, has_foreign))
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// `{tmp_dir}/{pid}-{counter}-{nanos}.tmp` — unique per attempt so
/// concurrent same-key uploads never share a staging file.
fn tmp_path(tmp_dir: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    tmp_dir.join(format!("{}-{seq}-{nanos}.tmp", std::process::id()))
}

/// Chunked file reads as a [`ByteStream`].
fn blob_stream<R>(reader: R) -> ByteStream
where
    R: AsyncRead + Send + Unpin + 'static,
{
    stream::try_unfold(reader, |mut r| async move {
        let mut buf = vec![0u8; READ_CHUNK];
        match r.read(&mut buf).await {
            Ok(0) => Ok(None),
            Ok(n) => Ok(Some((Bytes::copy_from_slice(&buf[..n]), r))),
            Err(e) => Err(StorageError::from(e)),
        }
    })
    .boxed()
}

async fn file_meta(file: &File) -> Result<BlobMeta, StorageError> {
    let meta = file.metadata().await?;
    Ok(BlobMeta {
        size: meta.len(),
        last_modified: meta.modified().ok(),
        e_tag: None,
    })
}

/// Tmp-file staged upload — the rename into `final_path` is the atomic
/// commit point.
struct FsUpload {
    tmp_path: PathBuf,
    final_path: PathBuf,
    file: File,
    hasher: Sha256,
    written: u64,
}

#[async_trait]
impl StagedUpload for FsUpload {
    async fn write_chunk(&mut self, chunk: Bytes) -> Result<(), StorageError> {
        self.file.write_all(&chunk).await?;
        self.hasher.update(&chunk);
        self.written = self.written.saturating_add(chunk.len() as u64);
        Ok(())
    }

    fn staged_len(&self) -> u64 {
        self.written
    }

    async fn commit(mut self: Box<Self>, constraints: PutConstraints) -> Result<u64, StorageError> {
        constraints.validate()?;
        // shutdown flushes + closes — the lfs put_stream flush+drop idiom.
        if let Err(e) = self.file.shutdown().await {
            let _ = fs::remove_file(&self.tmp_path).await;
            return Err(e.into());
        }
        if let Some(expected) = constraints.expected_size {
            if self.written != expected {
                let _ = fs::remove_file(&self.tmp_path).await;
                return Err(StorageError::SizeMismatch(format!(
                    "got {}, expected {expected}",
                    self.written
                )));
            }
        }
        if let Some(expected) = &constraints.expected_sha256 {
            let digest = bytes_to_hex(self.hasher.finalize_reset().as_slice());
            if &digest != expected {
                let _ = fs::remove_file(&self.tmp_path).await;
                return Err(StorageError::HashMismatch(format!(
                    "got {digest}, expected {expected}"
                )));
            }
        }
        if let Some(parent) = self.final_path.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                let _ = fs::remove_file(&self.tmp_path).await;
                return Err(e.into());
            }
        }
        if let Err(e) = fs::rename(&self.tmp_path, &self.final_path).await {
            let _ = fs::remove_file(&self.tmp_path).await;
            return Err(e.into());
        }
        Ok(self.written)
    }

    async fn abort(self: Box<Self>) -> Result<(), StorageError> {
        match fs::remove_file(&self.tmp_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[async_trait]
impl BlobStore for FsBlobStore {
    fn backend_name(&self) -> &'static str {
        "fs"
    }

    async fn begin_upload(&self, key: &BlobKey) -> Result<Box<dyn StagedUpload>, StorageError> {
        let tmp_dir = self.root.join(TMP_DIR);
        fs::create_dir_all(&tmp_dir).await?;
        let tmp = tmp_path(&tmp_dir);
        let file = File::create(&tmp).await?;
        Ok(Box::new(FsUpload {
            tmp_path: tmp,
            final_path: self.key_path(key),
            file,
            hasher: Sha256::new(),
            written: 0,
        }))
    }

    async fn get(&self, key: &BlobKey) -> Result<BlobObject, StorageError> {
        let path = self.key_path(key);
        let stat = fs::metadata(&path).await?;
        if !stat.is_file() {
            return Err(StorageError::NotFound);
        }
        let file = File::open(&path).await?;
        Ok(BlobObject {
            meta: file_meta(&file).await?,
            stream: blob_stream(file),
        })
    }

    async fn get_range(
        &self,
        key: &BlobKey,
        range: Range<u64>,
    ) -> Result<BlobObject, StorageError> {
        if range.start >= range.end {
            return Err(StorageError::InvalidRange(format!(
                "{}..{}",
                range.start, range.end
            )));
        }
        let path = self.key_path(key);
        let stat = fs::metadata(&path).await?;
        if !stat.is_file() {
            return Err(StorageError::NotFound);
        }
        let mut file = File::open(&path).await?;
        let meta = file_meta(&file).await?;
        let start = range.start.min(meta.size);
        let end = range.end.min(meta.size);
        file.seek(std::io::SeekFrom::Start(start)).await?;
        Ok(BlobObject {
            meta,
            stream: blob_stream(file.take(end - start)),
        })
    }

    async fn head(&self, key: &BlobKey) -> Result<BlobMeta, StorageError> {
        let stat = fs::metadata(self.key_path(key)).await?;
        if !stat.is_file() {
            return Err(StorageError::NotFound);
        }
        Ok(BlobMeta {
            size: stat.len(),
            last_modified: stat.modified().ok(),
            e_tag: None,
        })
    }

    async fn exists(&self, key: &BlobKey) -> Result<bool, StorageError> {
        match fs::metadata(self.key_path(key)).await {
            Ok(m) => Ok(m.is_file()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn delete(&self, key: &BlobKey) -> Result<(), StorageError> {
        match fs::remove_file(self.key_path(key)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn presign_get(
        &self,
        _key: &BlobKey,
        _ttl: Duration,
    ) -> Result<PresignedUrl, StorageError> {
        Err(StorageError::PresignUnsupported)
    }

    async fn presign_put(
        &self,
        _key: &BlobKey,
        _ttl: Duration,
        _constraints: PutConstraints,
    ) -> Result<PresignedUrl, StorageError> {
        Err(StorageError::PresignUnsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::into_byte_stream;
    use futures_util::stream as fstream;

    fn stream_of(chunks: Vec<Bytes>) -> ByteStream {
        fstream::iter(chunks.into_iter().map(Ok::<Bytes, StorageError>)).boxed()
    }

    fn sha256_hex(data: &[u8]) -> String {
        bytes_to_hex(Sha256::digest(data).as_slice())
    }

    async fn collect(mut object: BlobObject) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(chunk) = object.stream.next().await {
            out.extend_from_slice(&chunk.unwrap());
        }
        out
    }

    #[tokio::test]
    async fn open_creates_marker_and_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lfs");
        let store = FsBlobStore::open(&root, "lfs").unwrap();
        assert_eq!(store.root(), root.as_path());
        assert!(root.join(".oxidean-lfs-marker").is_file());
        assert!(root.join("tmp").is_dir());
        // Re-open on a marked root succeeds.
        FsBlobStore::open(&root, "lfs").unwrap();
    }

    #[tokio::test]
    async fn open_refuses_foreign_root_then_adopts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("store");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("stray.bin"), b"x").unwrap();
        let err = FsBlobStore::open(&root, "lfs").unwrap_err();
        assert!(matches!(err, StorageError::Config(_)), "{err:?}");
        // lost+found alone does not count as foreign.
        let root2 = dir.path().join("store2");
        std::fs::create_dir_all(root2.join("lost+found")).unwrap();
        FsBlobStore::open(&root2, "lfs").unwrap();
        // adopt marks the populated root deliberately.
        FsBlobStore::adopt(&root, "lfs").unwrap();
        assert!(root.join(".oxidean-lfs-marker").is_file());
        // Another store's marker still reads as foreign.
        let root3 = dir.path().join("store3");
        std::fs::create_dir_all(&root3).unwrap();
        std::fs::write(root3.join(".oxidean-packages-marker"), b"x").unwrap();
        assert!(FsBlobStore::open(&root3, "lfs").is_err());
    }

    #[tokio::test]
    async fn open_refuses_regular_file_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("afile");
        std::fs::write(&root, b"x").unwrap();
        assert!(matches!(
            FsBlobStore::open(&root, "lfs").unwrap_err(),
            StorageError::Config(_)
        ));
    }

    #[tokio::test]
    async fn put_get_head_exists_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path().join("s"), "lfs").unwrap();
        let key = BlobKey::new("ab/cd/payload.bin").unwrap();
        let data = Bytes::from_static(b"hello blob world");
        let digest = sha256_hex(&data);

        let written = store
            .put_stream(
                &key,
                stream_of(vec![data.slice(..5), data.slice(5..)]),
                PutConstraints {
                    expected_size: Some(data.len() as u64),
                    expected_sha256: Some(digest),
                    content_type: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(written, data.len() as u64);

        let meta = store.head(&key).await.unwrap();
        assert_eq!(meta.size, data.len() as u64);
        assert!(store.exists(&key).await.unwrap());

        let object = store.get(&key).await.unwrap();
        assert_eq!(collect(object).await, data.as_ref());

        let ranged = store.get_range(&key, 6..10).await.unwrap();
        assert_eq!(ranged.meta.size, data.len() as u64);
        assert_eq!(collect(ranged).await, b"blob".to_vec());

        // Whole-tail clamp: end past size returns the remainder.
        let tail = store.get_range(&key, 12..10_000).await.unwrap();
        assert_eq!(collect(tail).await, b"orld".to_vec());

        store.delete(&key).await.unwrap();
        assert!(!store.exists(&key).await.unwrap());
        assert!(matches!(
            store.head(&key).await.unwrap_err(),
            StorageError::NotFound
        ));
        // Delete is idempotent.
        store.delete(&key).await.unwrap();
        // And tmp/ holds nothing after a clean commit.
        assert_eq!(
            std::fs::read_dir(dir.path().join("s/tmp")).unwrap().count(),
            0
        );
    }

    #[tokio::test]
    async fn commit_rejects_wrong_size_and_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path().join("s"), "lfs").unwrap();
        let key = BlobKey::new("k").unwrap();
        let data = Bytes::from_static(b"abc");

        let err = store
            .put_stream(
                &key,
                stream_of(vec![data.clone()]),
                PutConstraints {
                    expected_size: Some(4),
                    expected_sha256: None,
                    content_type: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::SizeMismatch(_)), "{err:?}");

        let err = store
            .put_stream(
                &key,
                stream_of(vec![data.clone()]),
                PutConstraints {
                    expected_size: None,
                    expected_sha256: Some("0".repeat(64)),
                    content_type: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::HashMismatch(_)), "{err:?}");

        // Failures left no object and no staging file.
        assert!(!store.exists(&key).await.unwrap());
        assert_eq!(
            std::fs::read_dir(dir.path().join("s/tmp")).unwrap().count(),
            0
        );
    }

    #[tokio::test]
    async fn put_stream_aborts_eagerly_over_declared_size() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path().join("s"), "lfs").unwrap();
        let key = BlobKey::new("k").unwrap();
        let err = store
            .put_stream(
                &key,
                stream_of(vec![
                    Bytes::from_static(b"aaaa"),
                    Bytes::from_static(b"bbbb"),
                ]),
                PutConstraints {
                    expected_size: Some(5),
                    expected_sha256: None,
                    content_type: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::SizeMismatch(_)), "{err:?}");
        assert!(!store.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn stream_error_aborts_the_upload() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path().join("s"), "lfs").unwrap();
        let key = BlobKey::new("k").unwrap();
        let stream = into_byte_stream(fstream::iter(vec![
            Ok(Bytes::from_static(b"ok")),
            Err(std::fmt::Error),
        ]));
        let err = store
            .put_stream(&key, stream, PutConstraints::default())
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::Stream(_)), "{err:?}");
        assert!(!store.exists(&key).await.unwrap());
        assert_eq!(
            std::fs::read_dir(dir.path().join("s/tmp")).unwrap().count(),
            0
        );
    }

    #[tokio::test]
    async fn staged_upload_scan_before_commit() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path().join("s"), "lfs").unwrap();
        let key = BlobKey::new("k").unwrap();
        let mut upload = store.begin_upload(&key).await.unwrap();
        upload
            .write_chunk(Bytes::from_static(b"staged"))
            .await
            .unwrap();
        assert_eq!(upload.staged_len(), 6);
        // Staged bytes are invisible until commit — the scanning seam.
        assert!(!store.exists(&key).await.unwrap());
        upload.commit(PutConstraints::default()).await.unwrap();
        assert!(store.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn overwrite_replaces_keyed_object() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path().join("s"), "lfs").unwrap();
        let key = BlobKey::new("k").unwrap();
        store
            .put_stream(
                &key,
                stream_of(vec![Bytes::from_static(b"v1")]),
                PutConstraints::default(),
            )
            .await
            .unwrap();
        store
            .put_stream(
                &key,
                stream_of(vec![Bytes::from_static(b"v22")]),
                PutConstraints::default(),
            )
            .await
            .unwrap();
        let object = store.get(&key).await.unwrap();
        assert_eq!(collect(object).await, b"v22".to_vec());
    }

    #[tokio::test]
    async fn presign_is_unsupported_on_fs() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path().join("s"), "lfs").unwrap();
        let key = BlobKey::new("k").unwrap();
        assert!(matches!(
            store
                .presign_get(&key, Duration::from_secs(60))
                .await
                .unwrap_err(),
            StorageError::PresignUnsupported
        ));
        assert!(matches!(
            store
                .presign_put(&key, Duration::from_secs(60), PutConstraints::default())
                .await
                .unwrap_err(),
            StorageError::PresignUnsupported
        ));
    }

    #[tokio::test]
    async fn invalid_range_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsBlobStore::open(dir.path().join("s"), "lfs").unwrap();
        let key = BlobKey::new("k").unwrap();
        store
            .put_stream(
                &key,
                stream_of(vec![Bytes::from_static(b"abc")]),
                PutConstraints::default(),
            )
            .await
            .unwrap();
        assert!(matches!(
            store.get_range(&key, 3..3).await,
            Err(StorageError::InvalidRange(_))
        ));
    }
}
