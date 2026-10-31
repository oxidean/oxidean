//! Closed failure surface for the crate — mirrors the `ObjectStoreError`
//! contract in `oxidean-vcs`: variants never stringify into wire data and
//! never carry credentials, endpoints, or bucket internals.

use thiserror::Error;

/// Every failure a [`crate::BlobStore`] or the config layer can produce.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Key rejected before any path/prefix join — never raw client input.
    #[error("invalid blob key: {0}")]
    InvalidKey(String),
    /// Named-store id rejected — must match `[a-z0-9][a-z0-9-]*`.
    #[error("invalid store name: {0}")]
    InvalidStoreName(String),
    /// No object at the key — domain 404.
    #[error("object not found")]
    NotFound,
    /// Declared-vs-actual size disagreement (eager stream cap or commit check).
    #[error("blob size mismatch: {0}")]
    SizeMismatch(String),
    /// Declared sha256 disagreed with the received bytes.
    #[error("blob hash mismatch: {0}")]
    HashMismatch(String),
    /// Byte-range ask rejected before the fetch (`start >= end`).
    #[error("invalid byte range: {0}")]
    InvalidRange(String),
    /// Caller stream yielded an error mid-upload (client fault).
    #[error("blob stream: {0}")]
    Stream(String),
    /// Env parse or boot-time refusal — foreign root/prefix, colliding
    /// (bucket, prefix) pairs, half-set credentials, http without opt-in.
    #[error("storage config: {0}")]
    Config(String),
    /// Backend cannot mint presigned URLs (`fs`, or a signerless store).
    #[error("backend does not support presigned URLs")]
    PresignUnsupported,
    /// Filesystem failure.
    #[error("blob store io: {0}")]
    Io(#[source] std::io::Error),
    /// Remote object-store failure — `source` keeps the typed error for
    /// callers that need to match on it.
    #[error("object store backend: {0}")]
    Backend(#[source] object_store::Error),
}

/// `io::ErrorKind::NotFound` is a domain 404 (file vanished mid-race); every
/// other kind is a host-internal io failure.
impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound,
            _ => Self::Io(e),
        }
    }
}

impl From<object_store::Error> for StorageError {
    fn from(e: object_store::Error) -> Self {
        match e {
            object_store::Error::NotFound { .. } => Self::NotFound,
            other => Self::Backend(other),
        }
    }
}

impl From<std::env::VarError> for StorageError {
    fn from(e: std::env::VarError) -> Self {
        match e {
            // VarError::NotUnicode's Display embeds the raw value — for a
            // credential variable that would leak secret bytes into logs.
            std::env::VarError::NotPresent => {
                Self::Config("required environment variable is not set".into())
            }
            std::env::VarError::NotUnicode(_) => {
                Self::Config("environment variable was not valid unicode".into())
            }
        }
    }
}
