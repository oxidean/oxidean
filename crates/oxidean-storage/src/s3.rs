//! `s3` backend — S3-compatible object storage (AWS S3, MinIO, Garage,
//! SeaweedFS, Ceph RGW) over `object_store`'s `AmazonS3`, with presigning
//! via `object_store::signer`.
//!
//! Layout: one bucket, `{global_prefix}/{store}/` per store, keys verbatim
//! (`lfs/ab/cd/{oid}`). Uploads go through multipart (stage → scan →
//! commit: `MultipartUpload::complete` is the single visibility point),
//! buffering to [`PART_BUFFER`] so provider part-size minimums hold
//! regardless of caller chunking. Writes below the buffer land as a plain
//! `put`.
//!
//! Boot-time isolation mirrors `ObjectStoreConfig`: every store claims
//! `{prefix}/.oxidean-{store}-marker` — a foreign unmarked prefix refuses
//! `Config` — and [`crate::StorageConfig::check_store_isolation`] refuses
//! two stores on the same (bucket, prefix).
//!
//! Presigning: single-object URLs via `signed_url`, TTL-bounded by
//! [`MAX_PRESIGN_TTL`]; constraints fold into the signature via signed
//! headers where the provider enforces them (`Content-Length`,
//! `x-amz-checksum-sha256`, `Content-Type`) — a caller that mints *after*
//! the store's ACL check keeps upload integrity server-verified.

use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use base64::Engine;
use bytes::{Bytes, BytesMut};
use futures_util::{StreamExt, TryStreamExt};
use http::header::{HeaderName, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE};
use http::Method;
use object_store::aws::AmazonS3;
use object_store::path::Path as StorePath;
use object_store::signer::{SignedUrlOptions, Signer};
use object_store::{
    GetOptions, GetRange, MultipartUpload, ObjectMeta, ObjectStore, ObjectStoreExt,
    PutMultipartOptions, PutOptions, PutPayload,
};
use sha2::{Digest, Sha256};

use crate::config::S3Config;
use crate::error::StorageError;
use crate::key::BlobKey;
use crate::store::{
    bytes_to_hex, BlobMeta, BlobObject, BlobStore, ByteStream, PresignedUrl, PutConstraints,
    StagedUpload,
};

/// Hard ceiling on presigned-URL TTLs — minted URLs are short-lived by
/// contract (issue #210 security §1: bounded presign lifetime).
pub const MAX_PRESIGN_TTL: Duration = Duration::from_secs(3600);

/// Multipart part-size floor is 5 MiB on every S3-compatible provider; we
/// stage chunks to 8 MiB so small caller chunks can't fail the minimum.
/// The trailing remainder below 5 MiB is legal as the last part.
const PART_BUFFER: usize = 8 * 1024 * 1024;

const MARKER_BODY: &[u8] = b"oxidean blob store - do not co-locate other content\n";

/// S3 [`BlobStore`] over `{bucket}/{global_prefix}/{store}/`.
pub struct S3BlobStore {
    inner: Arc<AmazonS3>,
    bucket: String,
    prefix: String,
}

impl S3BlobStore {
    /// Connect + claim the store prefix: build the client, resolve
    /// `{global}/{store}`, then run the marker gate — a foreign (non-marker)
    /// key in the prefix refuses `Config` (fail closed; callers either
    /// refuse boot or disable the surface, `vcs.disabled`-style). Marker
    /// bodies are identical so concurrent first-boots are idempotent.
    pub async fn connect(config: &S3Config, store: &str) -> Result<Self, StorageError> {
        let prefix = config.store_prefix(store)?;
        let inner = Arc::new(config.build_client()?);
        claim_prefix_marker(inner.as_ref(), &prefix, store).await?;
        Ok(Self {
            inner,
            bucket: config.bucket.clone(),
            prefix,
        })
    }

    /// The bucket this store writes to.
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    /// The resolved `{global}/{store}` key prefix (never empty, no trailing `/`).
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// `{prefix}/{key}` — safe join: `BlobKey` segments are `[A-Za-z0-9._~-]`
    /// so the prefix join cannot smuggle traversal or separators.
    fn path(&self, key: &BlobKey) -> StorePath {
        StorePath::from(format!("{}/{}", self.prefix, key.as_str()))
    }
}

/// Marker gate shared by `connect` and tests: head the marker → present ⇒
/// owned ⇒ pass; else bound-list the prefix (any key ⇒ foreign, refuse);
/// else write the marker.
async fn claim_prefix_marker<S: ObjectStore>(
    inner: &S,
    prefix: &str,
    store: &str,
) -> Result<(), StorageError> {
    let marker = StorePath::from(format!("{prefix}/.oxidean-{store}-marker"));
    match inner.head(&marker).await {
        Ok(_) => return Ok(()),
        Err(object_store::Error::NotFound { .. }) => {}
        Err(e) => return Err(e.into()),
    }
    let mut listing = inner.list(Some(&StorePath::from(format!("{prefix}/"))));
    match listing.next().await {
        Some(Ok(_)) => {
            return Err(StorageError::Config(format!(
                "store prefix `{prefix}` holds foreign content and no {marker} marker — \
                 refusing to claim it"
            )));
        }
        Some(Err(e)) => return Err(e.into()),
        None => {}
    }
    drop(listing);
    inner
        .put(&marker, PutPayload::from(Bytes::from_static(MARKER_BODY)))
        .await?;
    Ok(())
}

fn object_meta_to_blob(meta: &ObjectMeta) -> BlobMeta {
    let last_modified = u64::try_from(meta.last_modified.timestamp())
        .ok()
        .and_then(|s| SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(s)));
    BlobMeta {
        size: meta.size,
        last_modified,
        e_tag: meta.e_tag.clone(),
    }
}

async fn os_head<S: ObjectStore>(inner: &S, path: &StorePath) -> Result<BlobMeta, StorageError> {
    Ok(object_meta_to_blob(&inner.head(path).await?))
}

async fn os_exists<S: ObjectStore>(inner: &S, path: &StorePath) -> Result<bool, StorageError> {
    match inner.head(path).await {
        Ok(_) => Ok(true),
        Err(object_store::Error::NotFound { .. }) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

async fn os_get<S: ObjectStore>(
    inner: &S,
    path: &StorePath,
    range: Option<Range<u64>>,
) -> Result<BlobObject, StorageError> {
    let options = GetOptions {
        range: range.map(GetRange::Bounded),
        ..Default::default()
    };
    let result = inner.get_opts(path, options).await?;
    let meta = object_meta_to_blob(&result.meta);
    let stream: ByteStream = result.into_stream().map_err(StorageError::from).boxed();
    Ok(BlobObject { meta, stream })
}

fn check_ttl(ttl: Duration) -> Result<(), StorageError> {
    if ttl.is_zero() || ttl > MAX_PRESIGN_TTL {
        return Err(StorageError::Config(format!(
            "presign ttl {ttl:?} out of range (1s..={MAX_PRESIGN_TTL:?})"
        )));
    }
    Ok(())
}

fn hex_decode_32(hex: &str) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// Multipart-backed staged upload — parts flush at [`PART_BUFFER`] so
/// provider part-size minimums hold; the trailing buffer lands as the
/// final part or, when no part ever flushed, a single `put`.
struct S3Upload {
    inner: Arc<dyn ObjectStore>,
    path: StorePath,
    upload: Option<Box<dyn MultipartUpload>>,
    buffered: BytesMut,
    hasher: Sha256,
    written: u64,
}

impl S3Upload {
    async fn upload_part_payload(&mut self, part: Bytes) -> Result<(), StorageError> {
        if self.upload.is_none() {
            self.upload = Some(
                self.inner
                    .put_multipart_opts(&self.path, PutMultipartOptions::default())
                    .await?,
            );
        }
        let Some(upload) = self.upload.as_mut() else {
            return Err(StorageError::Backend(object_store::Error::NotSupported {
                source: "multipart upload init returned no handle".into(),
            }));
        };
        upload.put_part(PutPayload::from(part)).await?;
        Ok(())
    }

    async fn abort_inner(&mut self) -> Result<(), StorageError> {
        match self.upload.take() {
            Some(mut upload) => upload.abort().await.map_err(StorageError::from),
            None => Ok(()),
        }
    }
}

#[async_trait]
impl StagedUpload for S3Upload {
    async fn write_chunk(&mut self, chunk: Bytes) -> Result<(), StorageError> {
        self.hasher.update(&chunk);
        self.written = self.written.saturating_add(chunk.len() as u64);
        self.buffered.extend_from_slice(&chunk);
        while self.buffered.len() >= PART_BUFFER {
            let part = self.buffered.split_to(PART_BUFFER).freeze();
            self.upload_part_payload(part).await?;
        }
        Ok(())
    }

    fn staged_len(&self) -> u64 {
        self.written
    }

    async fn commit(mut self: Box<Self>, constraints: PutConstraints) -> Result<u64, StorageError> {
        constraints.validate()?;
        if let Some(expected) = constraints.expected_size {
            if self.written != expected {
                let _ = self.abort_inner().await;
                return Err(StorageError::SizeMismatch(format!(
                    "got {}, expected {expected}",
                    self.written
                )));
            }
        }
        if let Some(expected) = &constraints.expected_sha256 {
            let digest = bytes_to_hex(self.hasher.finalize_reset().as_slice());
            if &digest != expected {
                let _ = self.abort_inner().await;
                return Err(StorageError::HashMismatch(format!(
                    "got {digest}, expected {expected}"
                )));
            }
        }
        match self.upload.take() {
            Some(mut upload) => {
                if !self.buffered.is_empty() {
                    let part = self.buffered.split().freeze();
                    if let Err(e) = upload.put_part(PutPayload::from(part)).await {
                        let _ = upload.abort().await;
                        return Err(e.into());
                    }
                }
                upload.complete().await?;
                Ok(self.written)
            }
            None => {
                // Never crossed the part buffer — a plain put is cheaper and
                // works on providers without multipart. (`put_opts` — the
                // required-method form — is callable on `&dyn ObjectStore`.)
                self.inner
                    .put_opts(
                        &self.path,
                        PutPayload::from(self.buffered.split().freeze()),
                        PutOptions::default(),
                    )
                    .await?;
                Ok(self.written)
            }
        }
    }

    async fn abort(mut self: Box<Self>) -> Result<(), StorageError> {
        self.abort_inner().await
    }
}

#[async_trait]
impl BlobStore for S3BlobStore {
    fn backend_name(&self) -> &'static str {
        "s3"
    }

    async fn begin_upload(&self, key: &BlobKey) -> Result<Box<dyn StagedUpload>, StorageError> {
        Ok(Box::new(S3Upload {
            inner: self.inner.clone(),
            path: self.path(key),
            upload: None,
            buffered: BytesMut::new(),
            hasher: Sha256::new(),
            written: 0,
        }))
    }

    async fn get(&self, key: &BlobKey) -> Result<BlobObject, StorageError> {
        os_get(self.inner.as_ref(), &self.path(key), None).await
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
        // Clamp like the fs backend: providers reject a bounded range whose
        // start passes the object end, so head first and normalize — a
        // whole-tail miss streams zero bytes with full-object meta.
        let meta = os_head(self.inner.as_ref(), &self.path(key)).await?;
        if range.start >= meta.size {
            return Ok(BlobObject {
                meta,
                stream: futures_util::stream::empty().boxed(),
            });
        }
        os_get(
            self.inner.as_ref(),
            &self.path(key),
            Some(range.start..range.end.min(meta.size)),
        )
        .await
    }

    async fn head(&self, key: &BlobKey) -> Result<BlobMeta, StorageError> {
        os_head(self.inner.as_ref(), &self.path(key)).await
    }

    async fn exists(&self, key: &BlobKey) -> Result<bool, StorageError> {
        os_exists(self.inner.as_ref(), &self.path(key)).await
    }

    async fn delete(&self, key: &BlobKey) -> Result<(), StorageError> {
        self.inner.delete(&self.path(key)).await?;
        Ok(())
    }

    async fn presign_get(
        &self,
        key: &BlobKey,
        ttl: Duration,
    ) -> Result<PresignedUrl, StorageError> {
        check_ttl(ttl)?;
        let url = self
            .inner
            .signed_url(Method::GET, &self.path(key), ttl)
            .await?;
        Ok(PresignedUrl {
            url: url.to_string(),
            expires_at: SystemTime::now() + ttl,
            required_headers: Vec::new(),
        })
    }

    async fn presign_put(
        &self,
        key: &BlobKey,
        ttl: Duration,
        constraints: PutConstraints,
    ) -> Result<PresignedUrl, StorageError> {
        check_ttl(ttl)?;
        constraints.validate()?;
        let mut options = SignedUrlOptions::new();
        let mut required_headers: Vec<(String, String)> = Vec::new();
        if let Some(ct) = &constraints.content_type {
            let header = HeaderValue::from_str(ct).map_err(|_| {
                StorageError::Config("content_type is not a valid header value".into())
            })?;
            options = options.with_signed_header(CONTENT_TYPE, header);
            required_headers.push((CONTENT_TYPE.to_string(), ct.clone()));
        }
        if let Some(size) = constraints.expected_size {
            let value = size.to_string();
            let header = HeaderValue::from_str(&value).map_err(|_| {
                StorageError::Config("expected_size is not a valid header value".into())
            })?;
            options = options.with_signed_header(CONTENT_LENGTH, header);
            required_headers.push((CONTENT_LENGTH.to_string(), value));
        }
        if let Some(digest) = &constraints.expected_sha256 {
            let raw = hex_decode_32(digest).ok_or_else(|| {
                StorageError::Config("expected_sha256 must be 64 lowercase hex".into())
            })?;
            let b64 = base64::prelude::BASE64_STANDARD.encode(raw);
            let name = HeaderName::from_static("x-amz-checksum-sha256");
            let header = HeaderValue::from_str(&b64).map_err(|_| {
                StorageError::Config("sha256 checksum is not a valid header value".into())
            })?;
            options = options.with_signed_header(name.clone(), header);
            required_headers.push((name.to_string(), b64));
        }
        let url = if required_headers.is_empty() {
            self.inner
                .signed_url(Method::PUT, &self.path(key), ttl)
                .await?
        } else {
            self.inner
                .signed_url_opts(Method::PUT, &self.path(key), ttl, &options)
                .await?
        };
        Ok(PresignedUrl {
            url: url.to_string(),
            expires_at: SystemTime::now() + ttl,
            required_headers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    async fn collect(mut object: BlobObject) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(chunk) = object.stream.next().await {
            out.extend_from_slice(&chunk.unwrap());
        }
        out
    }

    #[tokio::test]
    async fn marker_claim_writes_marker_on_empty_prefix() {
        let mem = InMemory::new();
        claim_prefix_marker(&mem, "oxidean/lfs", "lfs")
            .await
            .unwrap();
        mem.head(&StorePath::from("oxidean/lfs/.oxidean-lfs-marker"))
            .await
            .unwrap();
        // Re-claim on a marked prefix succeeds.
        claim_prefix_marker(&mem, "oxidean/lfs", "lfs")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn marker_claim_refuses_foreign_prefix() {
        let mem = InMemory::new();
        mem.put(
            &StorePath::from("oxidean/lfs/some-object"),
            PutPayload::from(Bytes::from_static(b"x")),
        )
        .await
        .unwrap();
        let err = claim_prefix_marker(&mem, "oxidean/lfs", "lfs")
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::Config(_)), "{err:?}");
        // Sibling prefixes are unaffected.
        claim_prefix_marker(&mem, "oxidean/packages", "packages")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn marker_claim_accepts_marked_populated_prefix() {
        let mem = InMemory::new();
        mem.put(
            &StorePath::from("oxidean/lfs/.oxidean-lfs-marker"),
            PutPayload::from(Bytes::from_static(b"x")),
        )
        .await
        .unwrap();
        mem.put(
            &StorePath::from("oxidean/lfs/blob"),
            PutPayload::from(Bytes::from_static(b"y")),
        )
        .await
        .unwrap();
        claim_prefix_marker(&mem, "oxidean/lfs", "lfs")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn s3_upload_roundtrip_via_inmemory() {
        let mem = Arc::new(InMemory::new());
        let path = StorePath::from("oxidean/lfs/ab/cd/oid");
        let mut upload = S3Upload {
            inner: mem.clone(),
            path: path.clone(),
            upload: None,
            buffered: BytesMut::new(),
            hasher: Sha256::new(),
            written: 0,
        };
        upload
            .write_chunk(Bytes::from_static(b"hello "))
            .await
            .unwrap();
        upload.write_chunk(Bytes::from_static(b"s3")).await.unwrap();
        let digest = bytes_to_hex(Sha256::digest(b"hello s3").as_slice());
        let written = Box::new(upload)
            .commit(PutConstraints {
                expected_size: Some(9),
                expected_sha256: Some(digest),
                content_type: None,
            })
            .await
            .unwrap();
        assert_eq!(written, 9);
        let object = os_get(mem.as_ref(), &path, None).await.unwrap();
        assert_eq!(collect(object).await, b"hello s3".to_vec());
        assert!(os_exists(mem.as_ref(), &path).await.unwrap());
        let meta = os_head(mem.as_ref(), &path).await.unwrap();
        assert_eq!(meta.size, 9);
    }

    #[tokio::test]
    async fn s3_upload_multipart_over_part_buffer() {
        let mem = Arc::new(InMemory::new());
        let path = StorePath::from("oxidean/lfs/big");
        let mut upload = S3Upload {
            inner: mem.clone(),
            path: path.clone(),
            upload: None,
            buffered: BytesMut::new(),
            hasher: Sha256::new(),
            written: 0,
        };
        let data = vec![7u8; PART_BUFFER + 1024];
        upload.write_chunk(Bytes::from(data.clone())).await.unwrap();
        assert!(upload.upload.is_some());
        Box::new(upload)
            .commit(PutConstraints::default())
            .await
            .unwrap();
        let object = os_get(mem.as_ref(), &path, None).await.unwrap();
        assert_eq!(collect(object).await, data);
    }

    #[tokio::test]
    async fn s3_upload_abort_removes_parts() {
        let mem = Arc::new(InMemory::new());
        let path = StorePath::from("oxidean/lfs/aborted");
        let mut upload = S3Upload {
            inner: mem.clone(),
            path: path.clone(),
            upload: None,
            buffered: BytesMut::new(),
            hasher: Sha256::new(),
            written: 0,
        };
        upload
            .write_chunk(Bytes::from(vec![1u8; PART_BUFFER + 1]))
            .await
            .unwrap();
        Box::new(upload).abort().await.unwrap();
        assert!(!os_exists(mem.as_ref(), &path).await.unwrap());
    }

    #[tokio::test]
    async fn get_range_and_delete_via_inmemory() {
        let mem = Arc::new(InMemory::new());
        let path = StorePath::from("p/k");
        mem.put(&path, PutPayload::from(Bytes::from_static(b"0123456789")))
            .await
            .unwrap();
        let ranged = os_get(mem.as_ref(), &path, Some(2..5)).await.unwrap();
        assert_eq!(ranged.meta.size, 10);
        assert_eq!(collect(ranged).await, b"234".to_vec());
        mem.delete(&path).await.unwrap();
        assert!(!os_exists(mem.as_ref(), &path).await.unwrap());
    }

    #[test]
    fn ttl_bounds() {
        assert!(check_ttl(Duration::from_secs(300)).is_ok());
        assert!(check_ttl(Duration::from_secs(0)).is_err());
        assert!(check_ttl(MAX_PRESIGN_TTL + Duration::from_secs(1)).is_err());
    }

    #[test]
    fn hex_decode() {
        let h = "ab".repeat(32);
        assert_eq!(hex_decode_32(&h), Some([0xab; 32]));
        assert_eq!(hex_decode_32("zz"), None);
        assert_eq!(hex_decode_32(&"ab".repeat(31)), None);
    }
}
