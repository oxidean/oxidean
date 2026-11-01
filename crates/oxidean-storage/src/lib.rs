//! `oxidean-storage` — the [`BlobStore`] abstraction every non-git
//! persistent store migrates behind (issue #210, spec
//! `agent-os/specs/2026-11-01-1425-storage-blob-backend`).
//!
//! Two backends, one trait:
//!
//! - **fs** ([`FsBlobStore`]) — today's on-disk semantics: marker-verified
//!   root, temp-write → verify → atomic rename under a same-filesystem
//!   `tmp/` stage. `OXIDEAN_STORAGE_BACKEND` unset is byte-identical
//!   behavior — the single-node / dev-compose default.
//! - **s3** ([`S3BlobStore`]) — generic S3-compatible object storage (AWS
//!   S3, MinIO, Garage, SeaweedFS, Ceph RGW) via `object_store`'s
//!   `AmazonS3`, with ambient-credential-chain support (env vars,
//!   web-identity/IRSA, ECS, IMDS) when no static `OXIDEAN_S3_*` keys are
//!   set. Uploads stage through multipart; `complete` is the visibility
//!   point.
//!
//! Cross-cutting invariants baked in here (spec §security invariants):
//!
//! - keys are [`BlobKey`]s — validated identifiers only, never raw client
//!   input;
//! - writes are stage → scan → commit ([`StagedUpload`]): bytes are
//!   visible to readers only at commit — the natural hook for later
//!   content scanning without an API change;
//! - every store root/prefix carries an ownership marker
//!   (`.oxidean-{store}-marker`) checked fail-closed at open/connect, and
//!   [`StorageConfig::check_store_isolation`] refuses colliding
//!   roots/(bucket, prefix) pairs;
//! - `presign_*` is a capability, not a guarantee — [`FsBlobStore`]
//!   answers [`StorageError::PresignUnsupported`]; on s3, URLs are
//!   single-object and TTL-bounded ([`MAX_PRESIGN_TTL`]), and callers
//!   must mint *after* the store's ACL check;
//! - credentials come from env/Kubernetes secrets via [`S3Config`] and
//!   are never logged ([`S3Credentials`] redacts its `Debug` output).
//!
//! Consumer migration (lfs / packages / release-assets / template-packs /
//! vcs-objects / uploads, then actions artifacts+cache+logs and pages) is
//! follow-up work — this crate is the foundation they land on.

#![forbid(unsafe_code)]

mod config;
mod error;
mod fs;
mod key;
mod s3;
mod store;

pub use config::{
    BackendKind, S3Config, S3Credentials, StorageConfig, BACKEND_ENV, S3_ACCESS_KEY_ID_ENV,
    S3_BUCKET_ENV, S3_ENDPOINT_ENV, S3_INSECURE_HTTP_ENV, S3_PATH_STYLE_ENV, S3_PREFIX_ENV,
    S3_REGION_ENV, S3_SECRET_ACCESS_KEY_ENV, S3_SESSION_TOKEN_ENV,
};
pub use error::StorageError;
pub use fs::FsBlobStore;
pub use key::{validate_store_name, BlobKey};
pub use s3::{S3BlobStore, MAX_PRESIGN_TTL};
pub use store::{
    into_byte_stream, BlobMeta, BlobObject, BlobStore, ByteStream, PresignedUrl, PutConstraints,
    StagedUpload,
};
