//! The [`BlobStore`] contract every consumer migrates behind — put/get/
//! get_range/head/delete/exists plus presigning — and the [`StagedUpload`]
//! stage → scan → commit seam (issue #210 §8).

use std::fmt::Write as _;
use std::ops::Range;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::BoxStream;
use futures_util::{Stream, StreamExt, TryStreamExt};

use crate::error::StorageError;
use crate::key::BlobKey;

/// Byte-stream contract every backend accepts and returns — callers never
/// branch on where blob bytes live.
pub type ByteStream = BoxStream<'static, Result<Bytes, StorageError>>;

/// Adapt a caller-owned chunk stream into [`ByteStream`].
pub fn into_byte_stream<S, E>(stream: S) -> ByteStream
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::fmt::Display,
{
    stream
        .map_err(|e| StorageError::Stream(e.to_string()))
        .boxed()
}

/// Upload-time integrity constraints, checked at commit (and eagerly
/// mid-stream for `expected_size`, preserving `lfs/store.rs` behavior).
#[derive(Debug, Default, Clone)]
pub struct PutConstraints {
    /// Declared total bytes — a commit-time `SizeMismatch` when the staged
    /// count differs, and an eager mid-stream abort once it exceeds.
    /// `presign_put` folds it into a signed `Content-Length` header.
    pub expected_size: Option<u64>,
    /// Declared lowercase-hex sha256 — `HashMismatch` at commit when the
    /// received bytes disagree. `presign_put` folds it into a signed
    /// `x-amz-checksum-sha256` header so enforcing providers verify it too.
    pub expected_sha256: Option<String>,
    /// Content-Type the object must carry — folded into the presigned PUT
    /// signature so the client cannot send another type.
    pub content_type: Option<String>,
}

impl PutConstraints {
    /// Shape-check the constraints themselves — a malformed declared digest
    /// is `Config`, never a false `HashMismatch`.
    pub fn validate(&self) -> Result<(), StorageError> {
        if let Some(digest) = &self.expected_sha256 {
            if !is_hex_64(digest) {
                return Err(StorageError::Config(
                    "expected_sha256 must be 64 lowercase hex".into(),
                ));
            }
        }
        Ok(())
    }
}

pub(crate) fn is_hex_64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Lowercase-hex encode — the LFS `bytes_to_hex` idiom kept in-crate.
pub(crate) fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Object metadata — `size` is the whole object (a `get_range` head still
/// reports total length, matching `object_store::ObjectMeta` semantics).
#[derive(Debug, Clone)]
pub struct BlobMeta {
    pub size: u64,
    pub last_modified: Option<SystemTime>,
    pub e_tag: Option<String>,
}

/// A readable object — metadata plus the byte stream.
pub struct BlobObject {
    pub meta: BlobMeta,
    pub stream: ByteStream,
}

/// A minted presigned URL — always single-object and TTL-bounded
/// (`s3` enforces `ttl <= MAX_PRESIGN_TTL`).
#[derive(Debug)]
pub struct PresignedUrl {
    pub url: String,
    pub expires_at: SystemTime,
    /// Headers bound into the signature — the client MUST send them
    /// verbatim or the signature fails.
    pub required_headers: Vec<(String, String)>,
}

/// Staged write — the stage → scan → commit seam.
///
/// Bytes land in the backend's staging area (a `tmp/` file or multipart
/// parts) and become visible to readers only at [`commit`]. A caller that
/// scans or re-hashes staged bytes does so between `write_chunk` and
/// `commit`; `abort` discards them. Commit is the single visibility point —
/// the natural hook for later content scanning (ClamAV-style) without an
/// API change.
///
/// [`commit`]: StagedUpload::commit
#[async_trait]
pub trait StagedUpload: Send {
    /// Append the next chunk to the staged bytes.
    async fn write_chunk(&mut self, chunk: Bytes) -> Result<(), StorageError>;
    /// Bytes staged so far — feeds the eager `expected_size` abort.
    fn staged_len(&self) -> u64;
    /// Verify `constraints`, then publish the staged bytes atomically at the
    /// key. A violated constraint aborts — a partial commit is never visible.
    async fn commit(self: Box<Self>, constraints: PutConstraints) -> Result<u64, StorageError>;
    /// Discard the staged bytes (tmp file / un-completed multipart parts).
    async fn abort(self: Box<Self>) -> Result<(), StorageError>;
}

/// One non-git persistent store (lfs / packages / release-assets / …).
///
/// Implementations: [`crate::FsBlobStore`] (default, today's on-disk
/// semantics) and [`crate::S3BlobStore`] (S3-compatible object storage).
/// Held as `Arc<dyn BlobStore>` by consumers.
///
/// Presigning is a capability, not a guarantee: `fs` answers
/// [`StorageError::PresignUnsupported`]. Callers mint URLs only *after*
/// the store's own ACL check — presign changes who serves the bytes,
/// never who decides (ADR-0001).
#[async_trait]
pub trait BlobStore: Send + Sync + 'static {
    /// Backend identifier for diagnostics (`"fs"` / `"s3"`).
    fn backend_name(&self) -> &'static str;

    /// Stream → stage → constraint-check → commit; returns bytes committed.
    /// The default drives [`begin_upload`] → [`commit`]; `expected_size`
    /// aborts eagerly once the staged count exceeds it — the
    /// `lfs/store.rs::put_stream` floor every backend must keep.
    ///
    /// [`begin_upload`]: BlobStore::begin_upload
    /// [`commit`]: StagedUpload::commit
    async fn put_stream(
        &self,
        key: &BlobKey,
        mut stream: ByteStream,
        constraints: PutConstraints,
    ) -> Result<u64, StorageError> {
        constraints.validate()?;
        let mut upload = self.begin_upload(key).await?;
        while let Some(item) = stream.next().await {
            let chunk = match item {
                Ok(c) => c,
                Err(e) => {
                    let _ = upload.abort().await;
                    return Err(e);
                }
            };
            if let Err(e) = upload.write_chunk(chunk).await {
                let _ = upload.abort().await;
                return Err(e);
            }
            if let Some(max) = constraints.expected_size {
                if upload.staged_len() > max {
                    let _ = upload.abort().await;
                    return Err(StorageError::SizeMismatch(format!(
                        "body exceeds declared size of {max} bytes"
                    )));
                }
            }
        }
        upload.commit(constraints).await
    }

    /// Open a staged write at `key` — see [`StagedUpload`].
    async fn begin_upload(&self, key: &BlobKey) -> Result<Box<dyn StagedUpload>, StorageError>;

    /// Open the whole object for streaming.
    async fn get(&self, key: &BlobKey) -> Result<BlobObject, StorageError>;

    /// Open `[start, end)` of the object for streaming. `start >= end` is
    /// [`StorageError::InvalidRange`]; an end past the object length
    /// clamps, HTTP-Range style.
    async fn get_range(&self, key: &BlobKey, range: Range<u64>)
        -> Result<BlobObject, StorageError>;

    /// Object metadata — [`StorageError::NotFound`] when absent.
    async fn head(&self, key: &BlobKey) -> Result<BlobMeta, StorageError>;

    /// Cheap existence probe.
    async fn exists(&self, key: &BlobKey) -> Result<bool, StorageError>;

    /// Idempotent delete — absent is success.
    async fn delete(&self, key: &BlobKey) -> Result<(), StorageError>;

    /// Mint a single-object presigned GET — [`StorageError::PresignUnsupported`]
    /// on backends without a signer.
    async fn presign_get(&self, key: &BlobKey, ttl: Duration)
        -> Result<PresignedUrl, StorageError>;

    /// Mint a single-object presigned PUT with `constraints` folded into the
    /// signature where the provider supports it.
    async fn presign_put(
        &self,
        key: &BlobKey,
        ttl: Duration,
        constraints: PutConstraints,
    ) -> Result<PresignedUrl, StorageError>;
}
